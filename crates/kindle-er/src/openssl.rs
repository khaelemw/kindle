//! The game's OpenSSL (1.0, linked into its exe) adds its archive cipher again on every archive file it
//! opens: the cipher setup (`+0x242d1b0`) calls `EVP_add_cipher`, which is `OBJ_NAME_add` ("AES-128-ECB").
//! `OBJ_NAME_add` takes no lock: it puts a new entry in the names' hash table and frees the one it
//! replaced, so two file-loading threads adding at once free the same entry twice. The game's small-block
//! heap is broken then, and something else crashes on it seconds later (`+0xe1e662` / `+0xe1e90b`, reading
//! 2 or -1, from whatever allocated next).
//!
//! The game streams few enough files that it rarely happens to it, but a map whose files keep streaming
//! (Firelink's pieces as one 93 MB model in m46: 400 opens a second) crashed 13 runs of 16. Found
//! 2026-11-01 by a watcher on the heap's free lists: the bad free block was a whole OpenSSL name entry,
//! AES-128-ECB's. Kindle runs `OBJ_NAME_add` under a lock: 3 runs of 3 clean, the lock holding 17-22
//! calls a run that came in while another thread was in it.

use std::sync::Mutex;

use kindle_core::hook_sites::{Kind, Site};

/// `OBJ_NAME_add(name, type, data)`, its entry (`+0x24452c0` in 2.7.0.0): the stack probe, then the
/// names' table checked (made on the first add).
const OBJ_NAME_ADD: &str = "48 89 6C 24 10 48 89 74 24 18 57 B8 20 00 00 00 E8 ?? ?? ?? ?? 48 2B E0 48 83 3D ?? ?? ?? ?? 00 \
    49 8B F0 8B FA 48 8B E9 75 19 E8 ?? ?? ?? ?? 85 C0 75 10";

static OBJ_NAME_ADD_SITE: Site = Site {
    area: "stability",
    name: "OpenSSL OBJ_NAME_add",
    pattern: OBJ_NAME_ADD,
    offset: 0,
    matches: 1,
    kind: Kind::Retn(obj_name_add_hook),
    without: "the game's heap can break when its files stream fast (a big map's)",
};

static LOCK: Mutex<()> = Mutex::new(());

type ObjNameAddFn = unsafe extern "system" fn(name: u64, kind: u32, data: u64) -> i32;

/// The game's `OBJ_NAME_add` one thread at a time.
fn obj_name_add_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let add: ObjNameAddFn = unsafe { std::mem::transmute(original) };
    let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { add(r.rcx, r.rdx as u32, r.r8) as u32 as usize }
}

/// Hooks `OBJ_NAME_add` (from Kindle's start, before the game opens its archives).
pub fn install() {
    if OBJ_NAME_ADD_SITE.install().is_some() {
        kindle_core::info!("stability: the game's OpenSSL name table is added to one thread at a time");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `OBJ_NAME_add` found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE).
    #[test]
    fn obj_name_add_is_found() {
        let Some((img, _, sections)) = crate::singletons::tests::eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let (rva, size) = sections.iter().find(|(n, _, _)| n == ".text").map(|&(_, r, s)| (r, s)).unwrap();
        let text = &img[rva..rva + size];
        let p: Vec<Option<u8>> = OBJ_NAME_ADD.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect();
        let found: Vec<usize> =
            (0..text.len() - p.len()).filter(|&i| p.iter().zip(&text[i..]).all(|(p, b)| p.is_none_or(|v| v == *b))).map(|i| rva + i).collect();
        assert_eq!(found, [0x24452C0]);
    }
}
