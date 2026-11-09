//! Where Elden Ring keeps its managers (WorldChrMan, CSEventFlagMan, CSSessionManager ...): each is one
//! static pointer in the exe's .data, found from the game's own code and its run-time type information,
//! not from an address, so it holds across builds that keep the code's shape.
//!
//! The game reaches a singleton through its static, and wherever it does, it checks it for null first:
//!
//! ```text
//! mov  reg, [rip+STATIC]
//! test reg, reg
//! jnz  ok
//! lea  rcx, [rip+KEY]      ; the class, for the "singleton is null" message
//! call ...
//! ```
//!
//! KEY is a byte in .data that stands for the class: the game files each runtime class (a vtable of
//! `DLRF::DLRuntimeClassImpl<FD4::FD4Singleton<X, XImp>>`) under the addresses its vtable slots 3 to 6
//! return (`lea rax, [rip+KEY]; ret`). So the runtime class's vtable, found by its RTTI name, gives the
//! class's keys, and the null checks that name one of them give its static: the one most of them read
//! (a class can have a second static that a few checks read).

use std::collections::HashMap;

/// The exe as it is in memory (or laid out so from its file): its bytes by rva, where it's loaded, and
/// the sections that may be read (name, rva, size). Nothing outside them is read.
pub struct Image<'a> {
    bytes: &'a [u8],
    base: u64,
    sections: Vec<(String, usize, usize)>,
}

impl<'a> Image<'a> {
    pub fn new(bytes: &'a [u8], base: u64, sections: Vec<(String, usize, usize)>) -> Self {
        Self { bytes, base, sections }
    }

    /// The game's exe in this process: its code (the first .text: the second is Arxan's), .rdata and
    /// .data.
    pub fn of_game() -> Option<Image<'static>> {
        let (base, size) = kindle_core::memory::game_module()?;
        let mut sections = Vec::new();
        for (name, rva, len) in kindle_core::memory::sections() {
            if [".text", ".rdata", ".data"].contains(&name.as_str()) && !sections.iter().any(|(n, _, _)| *n == name) {
                sections.push((name, rva, len));
            }
        }
        // The exe's image is mapped for the life of the process, and only these sections of it are read.
        Some(Image::new(unsafe { std::slice::from_raw_parts(base as *const u8, size) }, base as u64, sections))
    }

    /// The first section called `name`: its rva and bytes.
    fn section(&self, name: &str) -> Option<(usize, &'a [u8])> {
        let &(_, rva, len) = self.sections.iter().find(|(n, _, _)| n == name)?;
        Some((rva, self.bytes.get(rva..rva + len)?))
    }

    /// `len` bytes at `rva`, if they're all in one of the sections.
    fn get(&self, rva: usize, len: usize) -> Option<&'a [u8]> {
        self.sections.iter().any(|&(_, r, l)| rva >= r && rva + len <= r + l).then_some(())?;
        self.bytes.get(rva..rva + len)
    }

    fn u64(&self, rva: usize) -> Option<u64> {
        Some(u64::from_le_bytes(self.get(rva, 8)?.try_into().ok()?))
    }

    /// The rva of the address `va` in the image.
    fn rva_of(&self, va: u64) -> Option<usize> {
        let rva = va.checked_sub(self.base)? as usize;
        (rva < self.bytes.len()).then_some(rva)
    }
}

/// A singleton's static: its class ("CS::WorldChrMan"), the rva of its static pointer, and how many of
/// the game's null checks read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Singleton {
    pub class: String,
    pub rva: usize,
    pub checks: usize,
}

/// Every singleton the image names, by class. A class whose statics are read by as many checks as each
/// other is left out (no guessing).
pub fn find(image: &Image) -> Vec<Singleton> {
    let keys = class_keys(image);
    let mut by_class: HashMap<&str, HashMap<usize, usize>> = HashMap::new();
    for ((at, key), n) in null_checks(image) {
        if let Some(Some(class)) = keys.get(&key) {
            *by_class.entry(class).or_default().entry(at).or_default() += n;
        }
    }
    let mut out = Vec::new();
    for (class, statics) in by_class {
        let mut statics: Vec<(usize, usize)> = statics.into_iter().collect();
        statics.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if statics.len() > 1 && statics[1].1 == statics[0].1 {
            continue;
        }
        out.push(Singleton { class: class.to_string(), rva: statics[0].0, checks: statics[0].1 });
    }
    out.sort_by(|a, b| a.class.cmp(&b.class));
    out
}

/// What the type descriptor of an FD4 singleton's runtime class is named, up to the class.
const RUNTIME_CLASS: &[u8] = b".?AV?$DLRuntimeClassImpl@V?$FD4Singleton@";

/// The class in the rest of a runtime class's decorated name (`VWorldChrMan@CS@@V...` ->
/// `CS::WorldChrMan`): its name and namespaces, up to `@@`.
fn singleton_class(rest: &[u8]) -> Option<String> {
    let rest = rest.strip_prefix(b"V").or_else(|| rest.strip_prefix(b"U"))?;
    let end = rest.windows(2).position(|w| w == b"@@")?;
    let parts: Vec<&str> = std::str::from_utf8(&rest[..end]).ok()?.split('@').collect();
    if parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')) {
        return None;
    }
    Some(parts.into_iter().rev().collect::<Vec<_>>().join("::"))
}

/// Key -> the class it stands for, from every FD4 singleton's runtime-class vtable (None for a key two
/// classes return).
fn class_keys(image: &Image) -> HashMap<usize, Option<String>> {
    let mut keys = HashMap::new();
    let (Some((data_rva, data)), Some((rdata_rva, rdata))) = (image.section(".data"), image.section(".rdata")) else { return keys };
    // The type descriptors: {type_info's vtable, spare, decorated name}.
    let mut descriptors: HashMap<u32, String> = HashMap::new();
    let mut from = 0;
    while let Some(at) = find_bytes(&data[from..], RUNTIME_CLASS) {
        let name_at = from + at;
        if let Some(class) = singleton_class(&data[name_at + RUNTIME_CLASS.len()..]) {
            descriptors.insert((data_rva + name_at - 0x10) as u32, class);
        }
        from = name_at + 1;
    }
    // Their complete object locators {signature 1, ..., +0x0C the type descriptor, +0x14 its own rva}.
    let u32_at = |off: usize| rdata.get(off..off + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut locators: HashMap<u64, &String> = HashMap::new();
    for off in (0..rdata.len().saturating_sub(24)).step_by(4) {
        if u32_at(off) == Some(1)
            && u32_at(off + 0x14) == Some((rdata_rva + off) as u32)
            && let Some(class) = u32_at(off + 0x0C).and_then(|td| descriptors.get(&td))
        {
            locators.insert(image.base + (rdata_rva + off) as u64, class);
        }
    }
    // Their vtables: the slot after a pointer to the locator.
    for off in (0..rdata.len().saturating_sub(8)).step_by(8) {
        let p = u64::from_le_bytes(rdata[off..off + 8].try_into().unwrap());
        let Some(class) = locators.get(&p) else { continue };
        let vtable = rdata_rva + off + 8;
        for slot in 3..=6 {
            let Some(key) = image.u64(vtable + 8 * slot).and_then(|f| image.rva_of(f)).and_then(|f| returned_address(image, f)) else { continue };
            match keys.get_mut(&key) {
                None => {
                    keys.insert(key, Some(class.to_string()));
                }
                Some(c) if c.as_deref() != Some(class.as_str()) => *c = None,
                Some(_) => {}
            }
        }
    }
    keys
}

/// The address the function at `rva` returns, if it's `lea rax, [rip+X]; ret`.
fn returned_address(image: &Image, rva: usize) -> Option<usize> {
    let b = image.get(rva, 8)?;
    (b[..3] == [0x48, 0x8D, 0x05] && b[7] == 0xC3).then(|| (rva + 7).wrapping_add_signed(i32::from_le_bytes(b[3..7].try_into().unwrap()) as isize))
}

/// The game's null checks of a static in .data that name a key: (the static, the key) -> how many.
fn null_checks(image: &Image) -> HashMap<(usize, usize), usize> {
    let mut out = HashMap::new();
    let (Some((rva, code)), Some((data_rva, data))) = (image.section(".text"), image.section(".data")) else { return out };
    let disp = |at: usize| i32::from_le_bytes(code[at..at + 4].try_into().unwrap()) as isize;
    let mut i = 0;
    while i + 20 <= code.len() {
        // mov r64, [rip+disp] (rax, rcx, rdx, rbx, rsi or rdi); test r64, r64; jnz; lea rcx, [rip+disp]; call
        if code[i] == 0x48
            && code[i + 1] == 0x8B
            && matches!(code[i + 2], 0x05 | 0x0D | 0x15 | 0x1D | 0x35 | 0x3D)
            && code[i + 7..i + 9] == [0x48, 0x85]
            && code[i + 9] >= 0xC0
            && code[i + 10] == 0x75
            && code[i + 12..i + 15] == [0x48, 0x8D, 0x0D]
            && code[i + 19] == 0xE8
        {
            let at = (rva + i + 7).wrapping_add_signed(disp(i + 3));
            let key = (rva + i + 19).wrapping_add_signed(disp(i + 15));
            if at >= data_rva && at < data_rva + data.len() {
                *out.entry((at, key)).or_default() += 1;
            }
        }
        i += 1;
    }
    out
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Image, Singleton, find};

    const BASE: u64 = 0x1_4000_0000;
    const TEXT: usize = 0x1000;
    const RDATA: usize = 0x2000;
    const DATA: usize = 0x3000;

    /// A made-up exe: `classes` FD4 singletons in CS (the class, its keys' rvas), each with a runtime-class
    /// type descriptor, locator, vtable and slot functions, and `checks` null checks (static, key).
    fn exe(classes: &[(&str, [usize; 4])], checks: &[(usize, usize)]) -> Vec<u8> {
        let mut img = vec![0u8; 0x4000];
        let put = |img: &mut Vec<u8>, at: usize, b: &[u8]| img[at..at + b.len()].copy_from_slice(b);
        let (mut text, mut rdata, mut data) = (TEXT, RDATA, DATA + 0x800);
        for (class, keys) in classes {
            let td = data;
            let name = format!(".?AV?$DLRuntimeClassImpl@V?$FD4Singleton@V{class}@CS@@V{class}Imp@2@@FD4@@$0A@@DLRF@@\0");
            put(&mut img, td + 0x10, name.as_bytes());
            data += (0x10 + name.len() + 15) & !15;
            let col = rdata;
            put(&mut img, col, &1u32.to_le_bytes());
            put(&mut img, col + 0x0C, &(td as u32).to_le_bytes());
            put(&mut img, col + 0x14, &(col as u32).to_le_bytes());
            let vtable = col + 0x20;
            put(&mut img, vtable - 8, &(BASE + col as u64).to_le_bytes());
            for (slot, &key) in keys.iter().enumerate() {
                // lea rax, [rip+key]; ret
                let f = text;
                put(&mut img, f, &[0x48, 0x8D, 0x05]);
                put(&mut img, f + 3, &((key as i64 - (f + 7) as i64) as i32).to_le_bytes());
                put(&mut img, f + 7, &[0xC3]);
                put(&mut img, vtable + 8 * (3 + slot), &(BASE + f as u64).to_le_bytes());
                text += 8;
            }
            rdata = vtable + 8 * 8;
        }
        for &(at, key) in checks {
            // mov rax, [rip+at]; test rax, rax; jnz +x; lea rcx, [rip+key]; call ...
            let i = text;
            put(&mut img, i, &[0x48, 0x8B, 0x05]);
            put(&mut img, i + 3, &((at as i64 - (i + 7) as i64) as i32).to_le_bytes());
            put(&mut img, i + 7, &[0x48, 0x85, 0xC0, 0x75, 0x0C, 0x48, 0x8D, 0x0D]);
            put(&mut img, i + 15, &((key as i64 - (i + 19) as i64) as i32).to_le_bytes());
            put(&mut img, i + 19, &[0xE8, 0, 0, 0, 0]);
            text += 24;
        }
        img
    }

    fn sections() -> Vec<(String, usize, usize)> {
        vec![(".text".into(), TEXT, 0x1000), (".rdata".into(), RDATA, 0x1000), (".data".into(), DATA, 0x1000)]
    }

    #[test]
    fn a_singleton_is_the_static_its_null_checks_read() {
        let (world, flags) = ([DATA + 0x10, DATA + 0x11, DATA + 0x12, DATA + 0x13], [DATA + 0x20, DATA + 0x21, DATA + 0x22, DATA + 0x23]);
        let img =
            exe(&[("WorldChrMan", world), ("CSEventFlagMan", flags)], &[(DATA + 0x100, DATA + 0x11), (DATA + 0x100, DATA + 0x12), (DATA + 0x108, DATA + 0x20)]);
        let found = find(&Image::new(&img, BASE, sections()));
        assert_eq!(
            found,
            [
                Singleton { class: "CS::CSEventFlagMan".into(), rva: DATA + 0x108, checks: 1 },
                Singleton { class: "CS::WorldChrMan".into(), rva: DATA + 0x100, checks: 2 }
            ]
        );
    }

    #[test]
    fn a_class_with_two_statics_gets_the_one_more_checks_read() {
        let keys = [DATA + 0x10, DATA + 0x11, DATA + 0x12, DATA + 0x13];
        let img = exe(&[("CSSessionManager", keys)], &[(DATA + 0x100, DATA + 0x10), (DATA + 0x100, DATA + 0x10), (DATA + 0x108, DATA + 0x10)]);
        assert_eq!(find(&Image::new(&img, BASE, sections())), [Singleton { class: "CS::CSSessionManager".into(), rva: DATA + 0x100, checks: 2 }]);
        // As many checks each: no guess.
        let img = exe(&[("CSSessionManager", keys)], &[(DATA + 0x100, DATA + 0x10), (DATA + 0x108, DATA + 0x11)]);
        assert_eq!(find(&Image::new(&img, BASE, sections())), []);
    }

    #[test]
    fn checks_of_unknown_keys_and_statics_outside_data_name_nothing() {
        let keys = [DATA + 0x10, DATA + 0x11, DATA + 0x12, DATA + 0x13];
        // A key no runtime class returns, and a "static" in .rdata.
        let img = exe(&[("WorldChrMan", keys)], &[(DATA + 0x100, DATA + 0x40), (RDATA + 0x800, DATA + 0x10)]);
        assert_eq!(find(&Image::new(&img, BASE, sections())), []);
    }

    /// An exe laid out as in memory: its image, its base, its sections (name, rva, size).
    pub(crate) type Loaded = (Vec<u8>, u64, Vec<(String, usize, usize)>);

    /// Elden Ring 2.7.0.0's exe laid out as it is in memory, from the file KINDLE_ER_EXE names (the test
    /// passes without it: no game files come with Kindle).
    pub(crate) fn eldenring() -> Option<Loaded> {
        let path = std::env::var_os("KINDLE_ER_EXE")?;
        let file = std::fs::read(path).expect("KINDLE_ER_EXE names a file");
        let u16_at = |o: usize| u16::from_le_bytes(file[o..o + 2].try_into().unwrap()) as usize;
        let u32_at = |o: usize| u32::from_le_bytes(file[o..o + 4].try_into().unwrap()) as usize;
        let pe = u32_at(0x3C);
        let opt = pe + 24;
        let base = u64::from_le_bytes(file[opt + 24..opt + 32].try_into().unwrap());
        let mut img = vec![0u8; u32_at(opt + 56)];
        let mut sections = Vec::new();
        for s in 0..u16_at(pe + 6) {
            let h = opt + u16_at(pe + 20) + 40 * s;
            let name = String::from_utf8_lossy(&file[h..h + 8]).trim_end_matches('\0').to_string();
            let (vsize, va, rsize, raw) = (u32_at(h + 8), u32_at(h + 12), u32_at(h + 16), u32_at(h + 20));
            let n = rsize.min(img.len() - va);
            img[va..va + n].copy_from_slice(&file[raw..raw + n]);
            if [".text", ".rdata", ".data"].contains(&name.as_str()) && !sections.iter().any(|(n, _, _)| *n == name) {
                sections.push((name, va, vsize.max(rsize)));
            }
        }
        Some((img, base, sections))
    }

    #[test]
    fn elden_rings_singletons_are_found() {
        let Some((img, base, sections)) = eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let found = find(&Image::new(&img, base, sections));
        let rva = |class: &str| found.iter().find(|s| s.class == class).map(|s| s.rva);
        assert_eq!(rva("CS::WorldChrMan"), Some(0x3D69FF8));
        assert_eq!(rva("CS::MapItemMan"), Some(0x3D6BAC0));
        assert_eq!(rva("CS::CSEventFlagMan"), Some(0x3D6C4B8));
        assert_eq!(rva("CS::CSSessionManager"), Some(0x3D7E540));
        assert_eq!(rva("CS::CSNetMan"), Some(0x3D5EEC0));
        assert_eq!(rva("CS::SoloParamRepository"), Some(0x3D85F58));
        assert_eq!(rva("CS::CSEmkSystem"), Some(0x3D6BC40));
        assert!(found.len() > 150, "{} singletons found", found.len());
    }
}
