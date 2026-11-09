//! Finding game functions by byte pattern in the game's in-memory `.text` section.
//!
//! Patterns are hex bytes with `??` for wildcards (relative call/jump/data operands that
//! differ between builds). A pattern must resolve to exactly one place; anything else is
//! treated as "not found" rather than guessed.
//!
//! A scan of the code takes 30-40 ms and a start does dozens, so what each pattern found is
//! kept in `cache\patterns.json` (for this exe: its build time stamp and size) and used on
//! the next start once the bytes there still match (the game decodes some code as it runs, so
//! a place that doesn't match yet is scanned for as before). Patterns that found nothing aren't
//! kept.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::memory;

fn parse(p: &str) -> Vec<Option<u8>> {
    p.split_whitespace().map(|t| if t == "??" { None } else { u8::from_str_radix(t, 16).ok() }).collect()
}

/// (rva, size) of the game exe's first `.text` section (the game's code).
fn text_section(_base: usize) -> Option<(usize, usize)> {
    memory::section(".text")
}

/// Where the cache lives (set at start).
static CACHE_PATH: OnceLock<PathBuf> = OnceLock::new();
/// This exe's identity, and pattern -> where it matched (offsets from the exe).
type Cache = (String, HashMap<String, Vec<usize>>);
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Keeps scan results in this file from now on.
pub fn set_cache(path: PathBuf) {
    let _ = CACHE_PATH.set(path);
}

/// A module's build time stamp and image size (its PE header), as text.
pub fn pe_identity(base: usize, size: usize) -> Option<String> {
    exe_identity(base, size)
}

/// The exe's build time stamp and image size (its PE header), as text.
fn exe_identity(base: usize, size: usize) -> Option<String> {
    let pe = memory::read::<u32>(base + 0x3C)? as usize;
    Some(format!("{:08x}-{size:x}", memory::read::<u32>(base + pe + 8)?))
}

/// The cached offsets for `pattern`, read from the file on first use.
fn cached(identity: &str, pattern: &str) -> Option<Vec<usize>> {
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if c.is_none() {
        let read = CACHE_PATH
            .get()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .filter(|v| v["exe"].as_str() == Some(identity))
            .and_then(|v| serde_json::from_value::<HashMap<String, Vec<usize>>>(v["patterns"].clone()).ok());
        *c = Some((identity.to_string(), read.unwrap_or_default()));
    }
    c.as_ref().and_then(|(_, m)| m.get(pattern).cloned())
}

fn remember(pattern: &str, offsets: Vec<usize>) {
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let Some((identity, map)) = c.as_mut() else { return };
    map.insert(pattern.to_string(), offsets);
    if let Some(path) = CACHE_PATH.get() {
        let text = serde_json::json!({ "exe": identity, "patterns": map }).to_string();
        let _ = path.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(path, text);
    }
}

fn matches_at(pattern: &[Option<u8>], at: usize) -> bool {
    memory::read_bytes(at, pattern.len()).is_some_and(|b| pattern.iter().zip(&b).all(|(p, b)| p.is_none_or(|v| v == *b)))
}

/// Every address where `pattern` matches in the game's code.
pub fn find_all(pattern: &str) -> Vec<usize> {
    let Some((base, image)) = memory::game_module() else { return Vec::new() };
    let identity = exe_identity(base, image);
    let parsed = parse(pattern);
    if let Some(id) = &identity
        && let Some(offsets) = cached(id, pattern)
        && !offsets.is_empty()
        && offsets.iter().all(|&o| matches_at(&parsed, base + o))
    {
        return offsets.into_iter().map(|o| base + o).collect();
    }
    let found = scan(base, &parsed);
    if identity.is_some() && !found.is_empty() {
        remember(pattern, found.iter().map(|&a| a - base).collect());
    }
    found
}

/// Every address where `pattern` matches in the game's code, by scanning it.
fn scan(base: usize, pattern: &[Option<u8>]) -> Vec<usize> {
    let Some((rva, size)) = text_section(base) else { return Vec::new() };
    let Some(first) = pattern.first().copied().flatten() else { return Vec::new() };
    // .text is mapped and readable for the life of the process.
    let text = unsafe { std::slice::from_raw_parts((base + rva) as *const u8, size) };
    let mut found = Vec::new();
    let mut i = 0;
    while i + pattern.len() <= text.len() {
        match text[i..].iter().position(|&b| b == first) {
            None => break,
            Some(off) => i += off,
        }
        if i + pattern.len() > text.len() {
            break;
        }
        if pattern.iter().zip(&text[i..]).all(|(p, b)| p.is_none_or(|v| v == *b)) {
            found.push(base + rva + i);
        }
        i += 1;
    }
    found
}

fn report(name: &str, found: Option<usize>, matches: usize) -> Option<usize> {
    let base = memory::game_module().map(|(b, _)| b).unwrap_or(0);
    match found {
        Some(a) => crate::info!("{name}: found at {}+{:#x}", crate::bindings::exe(), a - base),
        None if matches > 1 => crate::warn!("{name}: pattern matched {matches} times; refusing to guess"),
        None => crate::warn!("{name}: not found in this game build"),
    }
    found
}

/// Address of the single match of `pattern` in the game's code, or None if it matches
/// zero or several times.
pub fn find_unique(name: &str, pattern: &str) -> Option<usize> {
    let all = find_all(pattern);
    report(name, (all.len() == 1).then(|| all[0]), all.len())
}

/// Absolute target of the RIP-relative operand whose 4-byte displacement is at `disp_at`
/// and whose instruction ends at `next_ip`.
pub fn rip_target(disp_at: usize, next_ip: usize) -> Option<usize> {
    let disp = memory::read::<i32>(disp_at)?;
    Some(next_ip.wrapping_add_signed(disp as isize))
}

/// Target of the `call rel32` (5 bytes) at `at`, if that is what's there.
pub fn call_target(at: usize) -> Option<usize> {
    (memory::read::<u8>(at)? == 0xE8).then_some(())?;
    rip_target(at + 1, at + 5)
}

/// Address of a 7-byte `lea reg, [rip+disp]` / `mov reg, [rip+disp]` operand at `at`.
pub fn lea_target(at: usize) -> Option<usize> {
    rip_target(at + 3, at + 7)
}

/// The single match of `pattern` for which `keep` holds: for code repeated with different
/// operands (one menu entry among several built the same way).
pub fn find_where(name: &str, pattern: &str, keep: impl Fn(usize) -> bool) -> Option<usize> {
    let hits: Vec<usize> = find_all(pattern).into_iter().filter(|&a| keep(a)).collect();
    report(name, (hits.len() == 1).then(|| hits[0]), hits.len())
}

/// For template-generated functions that are byte-identical apart from the data they use:
/// the match whose `lea reg, [rip+disp]` at `lea_offset` (7 bytes) points at the C string
/// `expected`.
pub fn find_with_string(name: &str, pattern: &str, lea_offset: usize, expected: &str) -> Option<usize> {
    let all = find_all(pattern);
    let hits: Vec<usize> = all
        .iter()
        .copied()
        .filter(|&a| {
            rip_target(a + lea_offset + 3, a + lea_offset + 7)
                .and_then(|s| memory::read_bytes(s, expected.len() + 1))
                .is_some_and(|b| b[..expected.len()] == *expected.as_bytes() && b[expected.len()] == 0)
        })
        .collect();
    report(name, (hits.len() == 1).then(|| hits[0]), hits.len().max(all.len()))
}

/// The static the game's singleton accessors for `class` read: every match of `pattern`
/// starts with `mov reg, [rip+disp]` (the static) and has `lea reg, [class name]` at
/// `lea_offset` (for its error message). The accessor is repeated wherever it's inlined, so
/// there are many matches; they must all name the same static.
pub fn find_singleton_static(name: &str, pattern: &str, lea_offset: usize, class: &str) -> Option<usize> {
    let statics: Vec<usize> = find_all(pattern)
        .into_iter()
        .filter(|&a| {
            rip_target(a + lea_offset + 3, a + lea_offset + 7)
                .and_then(|s| memory::read_bytes(s, class.len() + 1))
                .is_some_and(|b| b[..class.len()] == *class.as_bytes() && b[class.len()] == 0)
        })
        .filter_map(|a| rip_target(a + 3, a + 7))
        .collect();
    let first = statics.first().copied();
    let agree = first.is_some() && statics.iter().all(|&s| Some(s) == first);
    report(&format!("{name} (static)"), agree.then_some(first).flatten(), if agree { 1 } else { statics.len() })
}

/// The global a uniquely matched `mov reg, [rip+disp]` (7 bytes, at the start of the
/// pattern) reads: the address of the static pointer.
pub fn find_static(name: &str, pattern: &str) -> Option<usize> {
    let all = find_all(pattern);
    let target = (all.len() == 1).then(|| all[0]).and_then(|a| rip_target(a + 3, a + 7));
    report(&format!("{name} (static)"), target, all.len())
}

fn image() -> Option<(usize, &'static [u8])> {
    let (base, size) = memory::game_module()?;
    // Only sections known to be readable (.rdata/.data) are scanned below.
    Some((base, unsafe { std::slice::from_raw_parts(base as *const u8, size) }))
}

/// Readable sections of the game image: (rva, size) of .rdata and .data.
fn data_sections(_base: usize) -> Vec<(usize, usize)> {
    memory::sections().into_iter().filter(|(n, _, _)| n.starts_with(".rdata") || n.starts_with(".data")).map(|(_, r, s)| (r, s)).collect()
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = hay[i..].windows(needle.len()).position(|w| w == needle) {
        out.push(i + p);
        i += p + 1;
    }
    out
}

/// The vtable of a class, found through its RTTI (type descriptor `.?AV<name>@@` -> complete
/// object locator -> the vtable that points at it). `name` is the decorated class name, e.g.
/// `SprjEzStateTalkEvent@NS_SPRJ`. None unless exactly one vtable matches. Kept in the cache
/// (see the module docs), checked through the RTTI again when used from it.
pub fn find_vtable(name: &str) -> Option<usize> {
    let (base, img) = image()?;
    let identity = exe_identity(base, img.len());
    let key = format!("vtable:{name}");
    if let Some(id) = &identity
        && let Some(&[vt]) = cached(id, &key).as_deref()
        && vtable_is(base, base + vt, name)
    {
        crate::info!("{name}: vtable at {}+{vt:#x}", crate::bindings::exe());
        return Some(base + vt);
    }
    let sections = data_sections(base);
    let in_data = |rva: usize| sections.iter().any(|&(r, s)| rva >= r && rva < r + s);
    let slice = |r: usize, s: usize| &img[r.min(img.len())..(r + s).min(img.len())];
    let td_name = format!(".?AV{name}@@\0");
    let u32_at = |rva: usize| img.get(rva..rva + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let mut found = Vec::new();
    for &(r, s) in &sections {
        for at in find_bytes(slice(r, s), td_name.as_bytes()) {
            let Some(td_rva) = (r + at).checked_sub(0x10) else { continue };
            for &(r2, s2) in &sections {
                for col_at in find_bytes(slice(r2, s2), &(td_rva as u32).to_le_bytes()) {
                    let Some(col) = (r2 + col_at).checked_sub(12) else { continue };
                    if u32_at(col) != Some(1) || u32_at(col + 4) != Some(0) || u32_at(col + 20) != Some(col as u32) {
                        continue;
                    }
                    for &(r3, s3) in &sections {
                        for p in find_bytes(slice(r3, s3), &((base + col) as u64).to_le_bytes()) {
                            let vt = r3 + p + 8;
                            if in_data(vt) {
                                found.push(base + vt);
                            }
                        }
                    }
                }
            }
        }
    }
    found.sort_unstable();
    found.dedup();
    match found.as_slice() {
        [one] => {
            crate::info!("{name}: vtable at {}+{:#x}", crate::bindings::exe(), one - base);
            if identity.is_some() {
                remember(&key, vec![one - base]);
            }
            Some(*one)
        }
        [] => {
            crate::warn!("{name}: vtable not found");
            None
        }
        many => {
            crate::warn!("{name}: {} vtables match; refusing to guess", many.len());
            None
        }
    }
}

/// Whether `vt` is the vtable of class `name`: the slot before it points at a complete object
/// locator (signature 1, offset 0) whose type descriptor is `.?AV<name>@@`.
fn vtable_is(base: usize, vt: usize, name: &str) -> bool {
    let Some(col) = memory::read_u64(vt - 8).map(|c| c as usize) else { return false };
    let u32_at = |a: usize| memory::read::<u32>(a);
    if col < base || u32_at(col) != Some(1) || u32_at(col + 4) != Some(0) {
        return false;
    }
    let Some(td) = u32_at(col + 12) else { return false };
    let want = format!(".?AV{name}@@\0");
    memory::read_bytes(base + td as usize + 0x10, want.len()).is_some_and(|b| b == want.as_bytes())
}
