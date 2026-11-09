//! Safe, read-only access to arbitrary game memory, for reverse engineering from the console
//! (`debug.*`) and for Kindle's own data walks (e.g. the text repository).
//!
//! Every read goes through `ReadProcessMemory` on our own process, so a bad address returns
//! `None` instead of crashing the game.

use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::GetCurrentProcess;

/// The game exe's import slots for `dll`'s functions imported by name (name -> slot address),
/// from its in-memory import directory (PE32+: data directory 1 at the optional header +112+8).
pub fn game_imports(dll: &str) -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    let Some((base, _)) = game_module() else { return out };
    let rd32 = |a: usize| read::<u32>(a).unwrap_or(0) as usize;
    let cstr = |a: usize, max: usize| read_bytes(a, max).map(|b| String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(max)]).to_string());
    let nt = base + rd32(base + 0x3C);
    let imports = rd32(nt + 24 + 112 + 8);
    if imports == 0 {
        return out;
    }
    let mut d = base + imports;
    loop {
        let (names, name, iat) = (rd32(d), rd32(d + 12), rd32(d + 16));
        if name == 0 {
            break;
        }
        if cstr(base + name, 64).is_some_and(|n| n.eq_ignore_ascii_case(dll)) {
            let list = if names != 0 { names } else { iat };
            for i in 0.. {
                let entry = read_u64(base + list + 8 * i).unwrap_or(0);
                if entry == 0 {
                    break;
                }
                if entry >> 63 == 0
                    && let Some(n) = cstr(base + entry as usize + 2, 64)
                {
                    out.insert(n, base + iat + 8 * i);
                }
            }
        }
        d += 20;
    }
    out
}

/// Copies `buf.len()` bytes from `addr`. Returns false if any byte is unreadable.
pub fn read_into(addr: usize, buf: &mut [u8]) -> bool {
    if addr == 0 || buf.is_empty() {
        return buf.is_empty();
    }
    let mut read = 0usize;
    let ok = unsafe { ReadProcessMemory(GetCurrentProcess(), addr as *const _, buf.as_mut_ptr() as *mut _, buf.len(), Some(&mut read)) };
    ok.is_ok() && read == buf.len()
}

pub fn read_bytes(addr: usize, len: usize) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; len];
    read_into(addr, &mut buf).then_some(buf)
}

pub fn read<T: Copy + Default>(addr: usize) -> Option<T> {
    let mut v = T::default();
    let buf = unsafe { std::slice::from_raw_parts_mut(&mut v as *mut T as *mut u8, std::mem::size_of::<T>()) };
    read_into(addr, buf).then_some(v)
}

pub fn read_u64(addr: usize) -> Option<u64> {
    read::<u64>(addr)
}

/// Reads a null-terminated UTF-16 string of at most `max` characters.
pub fn read_wstr(addr: usize, max: usize) -> Option<String> {
    let mut units = Vec::new();
    for i in 0..max {
        let u: u16 = read(addr + i * 2)?;
        if u == 0 {
            break;
        }
        units.push(u);
    }
    Some(String::from_utf16_lossy(&units))
}

/// An MSVC wide string ({buffer or pointer, length +0x10, capacity +0x18}).
pub fn msvc_wstring(at: usize) -> Option<String> {
    let len = read_u64(at + 0x10)? as usize;
    let cap = read_u64(at + 0x18)? as usize;
    if len > 4096 {
        return None;
    }
    // Short strings (capacity < 8 wide chars) live in the buffer itself.
    let data = if cap >= 8 { read_u64(at)? as usize } else { at };
    let bytes = read_bytes(data, len * 2)?;
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    Some(String::from_utf16_lossy(&units))
}

/// Kindle's own module (base, size).
pub fn own_module() -> Option<(usize, usize)> {
    use windows::Win32::System::LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleHandleExW};
    unsafe {
        let mut module = windows::Win32::Foundation::HMODULE::default();
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            windows::core::PCWSTR(own_module as *const () as *const u16),
            &mut module,
        )
        .ok()?;
        let mut info = MODULEINFO::default();
        GetModuleInformation(GetCurrentProcess(), module, &mut info, std::mem::size_of::<MODULEINFO>() as u32).ok()?;
        Some((info.lpBaseOfDll as usize, info.SizeOfImage as usize))
    }
}

/// The game exe's file version (e.g. "1.15.2.0" for Dark Souls III 1.15.2, "2.7.0.0" for Elden Ring 1.17), from its version resource.
pub fn exe_version() -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW};
    use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
    use windows::core::{PCWSTR, w};
    let mut path = [0u16; 1024];
    let len = unsafe { GetModuleFileNameW(None, &mut path) } as usize;
    if len == 0 {
        return None;
    }
    path[len] = 0;
    let file = PCWSTR(path.as_ptr());
    let size = unsafe { GetFileVersionInfoSizeW(file, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(file, None, size, data.as_mut_ptr() as *mut c_void) }.ok()?;
    let mut info: *mut c_void = std::ptr::null_mut();
    let mut info_len = 0u32;
    if !unsafe { VerQueryValueW(data.as_ptr() as *const c_void, w!("\\"), &mut info, &mut info_len) }.as_bool() || info.is_null() {
        return None;
    }
    let v = unsafe { &*(info as *const VS_FIXEDFILEINFO) };
    Some(format!("{}.{}.{}.{}", v.dwFileVersionMS >> 16, v.dwFileVersionMS & 0xFFFF, v.dwFileVersionLS >> 16, v.dwFileVersionLS & 0xFFFF))
}

/// (base, size) of the game's exe in memory.
pub fn game_module() -> Option<(usize, usize)> {
    unsafe {
        let exe = GetModuleHandleA(None).ok()?;
        let mut info = MODULEINFO::default();
        GetModuleInformation(GetCurrentProcess(), exe, &mut info, std::mem::size_of::<MODULEINFO>() as u32).ok()?;
        Some((info.lpBaseOfDll as usize, info.SizeOfImage as usize))
    }
}

/// The game exe's sections from its in-memory PE headers: (name, rva, size), in order. The
/// exe has two `.text` sections; the first is the game's code.
pub fn sections() -> Vec<(String, usize, usize)> {
    let mut out = Vec::new();
    let Some((base, _)) = game_module() else { return out };
    let Some(e_lfanew) = read::<u32>(base + 0x3C) else { return out };
    let nt = base + e_lfanew as usize;
    let count = read::<u16>(nt + 6).unwrap_or(0) as usize;
    let opt_size = read::<u16>(nt + 20).unwrap_or(0) as usize;
    for i in 0..count {
        let h = nt + 24 + opt_size + i * 40;
        let (Some(name), Some(size), Some(rva)) = (read_bytes(h, 8), read::<u32>(h + 8), read::<u32>(h + 12)) else {
            continue;
        };
        let end = name.iter().position(|&b| b == 0).unwrap_or(8);
        out.push((String::from_utf8_lossy(&name[..end]).into_owned(), rva as usize, size as usize));
    }
    out
}

/// The first section with this name: (rva, size).
pub fn section(name: &str) -> Option<(usize, usize)> {
    sections().into_iter().find(|(n, _, _)| n == name).map(|(_, r, s)| (r, s))
}

/// The class of the object at `addr`, from the game's run-time type information: its vtable
/// (in .rdata) points one slot back at a complete object locator {u32 signature 1, ...,
/// +0x0C type descriptor rva, +0x14 its own rva}, and the type descriptor holds the
/// decorated name (".?AVWorldChrMan@NS_SPRJ@@" -> "WorldChrMan").
pub fn class_name(addr: usize) -> Option<String> {
    let (base, _) = game_module()?;
    let (rdata, rdata_size) = section(".rdata")?;
    class_name_in(addr, base, base + rdata..base + rdata + rdata_size)
}

fn class_name_in(addr: usize, base: usize, rdata: std::ops::Range<usize>) -> Option<String> {
    let in_rdata = |a: usize| rdata.contains(&a);
    let vtable = read_u64(addr)? as usize;
    if !in_rdata(vtable) {
        return None;
    }
    let col = read_u64(vtable - 8)? as usize;
    if !in_rdata(col) || read::<u32>(col)? != 1 || read::<u32>(col + 0x14)? as usize != col - base {
        return None;
    }
    let name = read_bytes(base + read::<u32>(col + 0x0C)? as usize + 0x10, 128)?;
    let name = &name[..name.iter().position(|&b| b == 0)?];
    let name = std::str::from_utf8(name).ok()?.strip_prefix(".?AV")?;
    Some(name.split('@').next()?.to_string())
}

/// Every static in the game's .data that points to a live object with a class name (the FD4
/// singletons among them): class name -> address of the first such static. Scans the whole
/// section, so it's for the debug module, not per frame.
pub fn class_statics() -> Vec<(String, usize)> {
    let Some((base, size)) = game_module() else { return Vec::new() };
    let Some((rva, len)) = section(".data") else { return Vec::new() };
    let Some(data) = read_bytes(base + rva, len) else { return Vec::new() };
    let Some((rdata, rdata_size)) = section(".rdata") else { return Vec::new() };
    let rdata = base + rdata..base + rdata + rdata_size;
    let mut seen = std::collections::HashMap::new();
    for (i, q) in data.as_chunks::<8>().0.iter().enumerate() {
        let p = u64::from_le_bytes(*q) as usize;
        // Objects live outside the exe image, 8-aligned, in user space.
        if p < 0x10000 || !p.is_multiple_of(8) || (p >= base && p < base + size) || p > 0x7FFF_FFFF_FFFF {
            continue;
        }
        if let Some(name) = class_name_in(p, base, rdata.clone()) {
            seen.entry(name).or_insert(base + rva + i * 8);
        }
    }
    let mut out: Vec<_> = seen.into_iter().collect();
    out.sort();
    out
}

/// The current instance of the class `name` held by a static `class_statics` finds (None if no
/// static holds one yet). Each game's runtime asks its own table of statics first.
pub fn class_instance(name: &str) -> Option<usize> {
    let addr = static_of(name)?;
    let p = read_u64(addr).map(|p| p as usize).filter(|&p| p != 0)?;
    if class_name(p).as_deref() != Some(name) {
        // Not (or no longer) that class's: looked for again.
        if let Some(m) = STATICS.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            m.remove(name);
        }
        return None;
    }
    Some(p)
}

/// Where each class's static is (from `class_statics`): a scan of the game's .data takes about
/// 9 ms, and co-op asks for the session manager many times a frame (verified 2026-10-11: a joined
/// player's game at 15 fps, 57 ms a frame in Kindle's co-op code, nearly all of it these scans).
/// A static stays where it is; the instance is read from it each time.
static STATICS: std::sync::Mutex<Option<std::collections::HashMap<String, usize>>> = std::sync::Mutex::new(None);
/// When `class_statics` last ran for a class it didn't have (one that doesn't exist yet: the game
/// makes its singletons as it starts), and how long until it's tried again.
static STATICS_SCANNED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const STATICS_RESCAN: std::time::Duration = std::time::Duration::from_secs(1);

fn static_of(name: &str) -> Option<usize> {
    if let Some(&at) = STATICS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(name)) {
        return Some(at);
    }
    {
        let mut scanned = STATICS_SCANNED.lock().unwrap_or_else(|e| e.into_inner());
        if scanned.is_some_and(|t| t.elapsed() < STATICS_RESCAN) {
            return None;
        }
        *scanned = Some(std::time::Instant::now());
    }
    let found = class_statics();
    let mut statics = STATICS.lock().unwrap_or_else(|e| e.into_inner());
    let map = statics.get_or_insert_with(std::collections::HashMap::new);
    for (n, at) in found {
        map.entry(n).or_insert(at);
    }
    map.get(name).copied()
}

/// (base, size) of each committed, private, read-write region.
fn writable_regions() -> Vec<(usize, usize)> {
    use windows::Win32::System::Memory::{MEM_COMMIT, MEM_PRIVATE, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_READWRITE, VirtualQuery};
    let mut out = Vec::new();
    let mut addr = 0usize;
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    while unsafe { VirtualQuery(Some(addr as *const _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) } != 0 {
        let base = mbi.BaseAddress as usize;
        let size = mbi.RegionSize;
        if mbi.State == MEM_COMMIT && mbi.Type == MEM_PRIVATE && mbi.Protect == PAGE_READWRITE && (mbi.Protect & PAGE_GUARD).0 == 0 {
            out.push((base, size));
        }
        addr = match base.checked_add(size) {
            Some(a) if a < 0x7FFF_FFFF_0000 => a,
            _ => break,
        };
    }
    out
}

/// Runs `find(piece, address)` over the process's private writable memory, cut into pieces of
/// at most 4 MB (8-aligned) that reach `overlap` bytes into the next piece of the same region,
/// on every core: the game's heap is several gigabytes. Each piece is copied out first
/// (`read_into`): the game frees memory while it loads, and a page gone between listing the
/// regions and reading them must fail the read, not crash (seen: a map load crashed in here).
/// The copies are small and their memory is asked for without aborting: a scan late in a long
/// session (the game at ~7 GB) must not end the game if memory is short; that piece is skipped.
fn scan_writable<F: Fn(&[u8], usize) -> Vec<usize> + Sync>(overlap: usize, find: F) -> Vec<usize> {
    const PIECE: usize = 4 << 20;
    let pieces: Vec<(usize, usize)> = writable_regions()
        .into_iter()
        .flat_map(|(base, size)| (0..size).step_by(PIECE).map(move |off| (base + off, (size - off).min(PIECE + overlap))))
        .collect();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 16);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let found = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                let mut mine = Vec::new();
                let mut buf = Vec::new();
                while let Some(&(base, len)) = pieces.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                    if len > buf.len() && buf.try_reserve_exact(len - buf.len()).is_err() {
                        continue;
                    }
                    buf.resize(len, 0);
                    if read_into(base, &mut buf) {
                        mine.extend(find(&buf, base));
                    }
                }
                found.lock().unwrap_or_else(|e| e.into_inner()).extend(mine);
            });
        }
    });
    let mut out = found.into_inner().unwrap_or_else(|e| e.into_inner());
    out.sort_unstable();
    out.dedup();
    out
}

/// Every 8-aligned address in the process's private writable memory holding `value`.
pub fn holders_of(value: u64) -> Vec<usize> {
    scan_writable(0, |bytes, base| {
        let words = bytes.as_chunks::<8>().0;
        words.iter().enumerate().filter(|(_, w)| u64::from_le_bytes(**w) == value).map(|(i, _)| base + i * 8).collect()
    })
}

/// Every address in the process's private writable memory where `pattern` starts.
pub fn find_writable(pattern: &[u8]) -> Vec<usize> {
    let Some(&first) = pattern.first() else { return Vec::new() };
    scan_writable(pattern.len() - 1, |bytes, base| {
        let mut hits = Vec::new();
        let mut at = 0;
        while let Some(i) = bytes[at..].iter().position(|&b| b == first) {
            let start = at + i;
            if bytes[start..].starts_with(pattern) {
                hits.push(base + start);
            }
            at = start + 1;
        }
        hits
    })
}
