//! Elden Ring's param tables for Kindle's registry (kindle-core's params.rs, re-exported here): the
//! paramdefs embedded in the DLL (build.rs) and the tables the game has loaded, from its
//! SoloParamRepository.
//!
//! The repository has 0xC2 slots of 0x48 bytes from +0x80, one per param (the exe's own table of
//! names gives each its slot): an i32 count at +0x80, then up to 8 `CS::ParamResCap*` from +0x88. The
//! game reads a param through the slot's first cap. A cap's name is an MSVC wide string at +0x18;
//! its +0x80 holds an object whose +0x80 is the table: the param file as on disk, with its size as a
//! u32 16 bytes before it.

pub use kindle_core::params::*;

use kindle_core::memory;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/paramdefs.rs"));
}

/// Elden Ring's paramdefs (file name, XML), for the bindings: none in a build without them.
pub use embedded::PARAMDEFS;

/// The repository's slots: where the first is, how many, their size, the caps each can hold.
const SLOTS_AT: usize = 0x80;
const SLOTS: usize = 0xC2;
const SLOT_SIZE: usize = 0x48;
const CAPS_PER_SLOT: usize = 8;

/// A loaded param: its slot's cap and where the table hangs from it.
pub struct Loaded {
    pub name: String,
    /// The repository's slot (the param's index in the game's table of names).
    pub slot: usize,
    /// How many caps the slot holds (the game reads the first).
    pub caps_in_slot: usize,
    /// The `CS::ParamResCap`.
    pub cap: usize,
    /// The object at the cap's +0x80: its +0x80 holds the table.
    pub holder: usize,
    pub table: usize,
    pub table_size: usize,
}

/// The params loaded in the repository at `repo`, in slot order: each slot's first cap.
pub fn loaded_in(repo: usize) -> Vec<Loaded> {
    let mut out = Vec::new();
    for slot in 0..SLOTS {
        let at = repo + SLOTS_AT + slot * SLOT_SIZE;
        let Some(count) = memory::read::<i32>(at) else { continue };
        if count <= 0 || count as usize > CAPS_PER_SLOT {
            continue;
        }
        let Some(cap) = memory::read_u64(at + 8).map(|c| c as usize).filter(|&c| c != 0) else { continue };
        let Some(holder) = memory::read_u64(cap + 0x80).map(|h| h as usize).filter(|&h| h != 0) else { continue };
        let Some(table) = memory::read_u64(holder + 0x80).map(|t| t as usize).filter(|&t| t != 0) else { continue };
        let (Some(name), Some(size)) = (memory::msvc_wstring(cap + 0x18), memory::read::<u32>(table - 0x10)) else { continue };
        out.push(Loaded { name, slot, caps_in_slot: count as usize, cap, holder, table, table_size: size as usize });
    }
    out
}

/// A repository a test made, read instead of the game's.
#[cfg(test)]
pub(crate) static TEST_REPO: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The game's SoloParamRepository (None before the game makes it).
pub(crate) fn repository() -> Option<usize> {
    #[cfg(test)]
    if let r @ 1.. = TEST_REPO.load(std::sync::atomic::Ordering::Relaxed) {
        return Some(r);
    }
    crate::er::instance("CS::SoloParamRepository")
}

/// The repository's params now (None before the game makes it or loads them), for the bindings.
pub fn caps() -> Option<Vec<ParamCap>> {
    let loaded = loaded_in(repository()?);
    if loaded.is_empty() {
        return None;
    }
    Some(loaded.into_iter().map(|l| ParamCap { name: l.name, table: l.table, table_size: l.table_size }).collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::Ordering::Relaxed;

    use super::*;

    /// One test at a time on what tests serve Kindle as the game's (a repository, a flag store): those that
    /// serve one and those that expect none.
    pub(crate) static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A slot's cap, holder, table (as the game holds it) and name's characters.
    type Held = (Vec<u64>, Vec<u64>, Vec<u128>, Vec<u16>);

    /// A repository laid out as the game's, served to Kindle (TEST_REPO) until dropped: a slot per
    /// table, each with one cap, its holder and the table as the game holds it.
    pub(crate) struct Repo {
        slots: Vec<u64>,
        _held: Vec<Held>,
    }

    impl Repo {
        /// Serves `tables` (name, .param file), one per slot in order.
        pub(crate) fn serve(tables: &[(String, Vec<u8>)]) -> Self {
            assert!(tables.len() <= SLOTS, "{} tables for {SLOTS} slots", tables.len());
            let mut slots = vec![0u64; (SLOTS_AT + SLOTS * SLOT_SIZE) / 8];
            let mut held = Vec::new();
            for (slot, (name, file)) in tables.iter().enumerate() {
                let block = crate::param_rows::tests::in_memory(file);
                let mut holder = vec![0u64; 0x88 / 8];
                holder[0x80 / 8] = block.as_ptr() as u64 + 16;
                let mut heap = Vec::new();
                let mut cap = vec![0u64; 0x88 / 8];
                cap[0x18 / 8..0x38 / 8].copy_from_slice(&wstring(name, &mut heap));
                cap[0x80 / 8] = holder.as_ptr() as u64;
                let at = (SLOTS_AT + slot * SLOT_SIZE) / 8;
                slots[at] = 1;
                slots[at + 1] = cap.as_ptr() as u64;
                held.push((cap, holder, block, heap));
            }
            let mut repo = Self { slots, _held: held };
            TEST_REPO.store(repo.slots.as_mut_ptr() as usize, Relaxed);
            invalidate();
            repo
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            TEST_REPO.store(0, Relaxed);
            invalidate();
        }
    }

    /// The game's register and unregister on a served repository: the cap's slot holds it (a count of
    /// 1) or not (0); 0 for a cap that isn't in it, or already in (out).
    pub(crate) extern "C" fn register(repo: usize, cap: usize) -> u8 {
        set_count(repo, cap, 0, 1)
    }

    pub(crate) extern "C" fn unregister(repo: usize, cap: usize) -> u8 {
        set_count(repo, cap, 1, 0)
    }

    fn set_count(repo: usize, cap: usize, from: i32, to: i32) -> u8 {
        let slot = (0..SLOTS).map(|s| repo + SLOTS_AT + s * SLOT_SIZE).find(|&at| memory::read_u64(at + 8) == Some(cap as u64));
        match slot {
            Some(at) if memory::read::<i32>(at) == Some(from) => {
                unsafe { (at as *mut i32).write(to) };
                1
            }
            _ => 0,
        }
    }

    /// An MSVC wide string's 0x20 bytes ({buffer or pointer, length +0x10, capacity +0x18}), its
    /// characters at `heap` when they don't fit the buffer.
    fn wstring(s: &str, heap: &mut Vec<u16>) -> [u64; 4] {
        let units: Vec<u16> = s.encode_utf16().collect();
        if units.len() < 8 {
            let mut buf = [0u64; 4];
            let raw = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u16, 8) };
            raw[..units.len()].copy_from_slice(&units);
            buf[2] = units.len() as u64;
            buf[3] = 7;
            return buf;
        }
        *heap = units.clone();
        heap.push(0);
        [heap.as_ptr() as u64, 0, units.len() as u64, units.len() as u64]
    }

    #[test]
    fn each_slots_first_cap_names_a_table() {
        // A repository: slot 0 with two caps, slot 3 with one, slot 5 with a count past 8.
        let mut repo = vec![0u64; (SLOTS_AT + SLOTS * SLOT_SIZE) / 8];
        let mut heaps = vec![Vec::new(), Vec::new(), Vec::new()];
        let tables: Vec<Vec<u64>> = (0..3).map(|i| vec![0x40 + i, 0, 0, 0]).collect();
        let holders: Vec<Vec<u64>> = tables
            .iter()
            .map(|t| {
                let mut h = vec![0u64; 0x88 / 8];
                h[0x80 / 8] = t.as_ptr() as u64 + 16;
                h
            })
            .collect();
        let names = ["EquipParamWeapon", "Magic", "EquipParamGoods"];
        let caps: Vec<Vec<u64>> = names
            .iter()
            .zip(&holders)
            .zip(&mut heaps)
            .map(|((n, h), heap)| {
                let mut c = vec![0u64; 0x88 / 8];
                c[0x18 / 8..0x38 / 8].copy_from_slice(&wstring(n, heap));
                c[0x80 / 8] = h.as_ptr() as u64;
                c
            })
            .collect();
        let slot = |s: usize| (SLOTS_AT + s * SLOT_SIZE) / 8;
        repo[slot(0)] = 2;
        repo[slot(0) + 1] = caps[0].as_ptr() as u64;
        repo[slot(0) + 2] = caps[1].as_ptr() as u64;
        repo[slot(3)] = 1;
        repo[slot(3) + 1] = caps[2].as_ptr() as u64;
        repo[slot(5)] = 9;
        repo[slot(5) + 1] = caps[1].as_ptr() as u64;
        let loaded = loaded_in(repo.as_ptr() as usize);
        let got: Vec<(&str, usize, usize, usize)> = loaded.iter().map(|l| (l.name.as_str(), l.slot, l.caps_in_slot, l.table_size)).collect();
        assert_eq!(got, [("EquipParamWeapon", 0, 2, 0x40), ("EquipParamGoods", 3, 1, 0x42)]);
        assert_eq!(loaded[1].cap, caps[2].as_ptr() as usize);
        assert_eq!(loaded[1].holder, holders[2].as_ptr() as usize);
        assert_eq!(loaded[1].table, tables[2].as_ptr() as usize + 16);
        // A short name lives in the string's own buffer.
        let mut heap = Vec::new();
        let short = wstring("Magic", &mut heap);
        assert_eq!(memory::msvc_wstring(short.as_ptr() as usize).as_deref(), Some("Magic"));
    }

    /// Kindle's paramdef reader on Elden Ring's paramdefs (the folder KINDLE_ER_PARAMDEFS names) and
    /// tables (KINDLE_ER_PARAMS): each def's row size against its table's, and fields read by name (the
    /// test passes without them: no game files or Elden Ring paramdefs come with Kindle yet).
    #[test]
    fn elden_rings_paramdefs_read_its_tables() {
        let (Some(defs_dir), Some(params_dir)) = (std::env::var_os("KINDLE_ER_PARAMDEFS"), std::env::var_os("KINDLE_ER_PARAMS")) else {
            eprintln!("KINDLE_ER_PARAMDEFS or KINDLE_ER_PARAMS isn't set: Elden Ring's paramdefs not checked");
            return;
        };
        let mut defs = std::collections::HashMap::new();
        for e in std::fs::read_dir(defs_dir).unwrap() {
            let xml = std::fs::read_to_string(e.unwrap().path()).unwrap();
            let def = kindle_paramdef::ParamDef::parse(&xml).unwrap();
            defs.insert(def.param_type.clone(), def);
        }
        let (mut fit, mut tables) = (0, std::collections::HashMap::new());
        for e in std::fs::read_dir(params_dir).unwrap() {
            let path = e.unwrap().path();
            let file = std::fs::read(&path).unwrap();
            let n = u16::from_le_bytes([file[0x0A], file[0x0B]]) as usize;
            let type_at = u64::from_le_bytes(file[0x10..0x18].try_into().unwrap()) as usize;
            let ty = String::from_utf8_lossy(&file[type_at..type_at + file[type_at..].iter().position(|&b| b == 0).unwrap()]).to_string();
            let row = |i: usize| u64::from_le_bytes(file[0x48 + i * 0x18..0x50 + i * 0x18].try_into().unwrap()) as usize;
            if n >= 2 && defs.get(&ty).is_some_and(|d| d.size == row(1) - row(0)) {
                fit += 1;
            }
            tables.insert(path.file_stem().unwrap().to_string_lossy().to_string(), (file, ty));
        }
        assert!(fit >= 170, "{fit} tables fit their paramdef");
        let value = |table: &str, id: u32, field: &str| {
            let (file, ty) = &tables[table];
            let def = &defs[ty];
            let n = u16::from_le_bytes([file[0x0A], file[0x0B]]) as usize;
            let i = (0..n).find(|&i| u32::from_le_bytes(file[0x40 + i * 0x18..0x44 + i * 0x18].try_into().unwrap()) == id).unwrap();
            let at = u64::from_le_bytes(file[0x48 + i * 0x18..0x50 + i * 0x18].try_into().unwrap()) as usize;
            def.read(def.field(field).unwrap(), &file[at..at + def.size])
        };
        use kindle_paramdef::Value::{Float, Int};
        // The Dagger, a Flask of Crimson Tears, the souls of a Golden Rune [1]'s effect.
        assert_eq!(value("EquipParamWeapon", 1000000, "weight"), Float(1.5));
        assert_eq!(value("EquipParamWeapon", 1000000, "originEquipWep"), Int(1000000));
        assert_eq!(value("EquipParamWeapon", 1000000, "attackBasePhysics"), Int(74));
        assert_eq!(value("EquipParamGoods", 1000, "maxNum"), Int(1));
        assert_eq!(value("SpEffectParam", 3269, "soul"), Int(200));
    }
}
