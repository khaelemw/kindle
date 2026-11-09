//! Elden Ring's event flags (kindle.flags): read from the game's flag store, set through the game's own
//! setter.
//!
//! CSEventFlagMan's object is the store (`CS::CSFD4VirtualMemoryFlag`). Its flags are in blocks of
//! `+0x1C` flags (1000), each `+0x20` bytes; a flag's bit is bit 7 - n % 8 of its block's byte n / 8, n
//! its place in the block (the highest bit first). A std::map at +0x38 (its head node) gives each block
//! number (a node's key, +0x20) its storage: a kind at +0x28, 1 for an index into the store's blocks
//! (at +0x28, the index at +0x30 times the block's size), 2 for a pointer at +0x30. A flag in no block
//! reads as off and the game's set leaves it be. The game points a block at other storage to read
//! another set of its flags (a session's, as Dark Souls III's two sets) [u].
//!
//! Kindle reads a flag as the game's read does (its code, found once in the exe, reads those offsets),
//! from any thread. It sets one through the setter the event scripts' SetEventFlag calls (the flag by
//! pointer, the state, a label for the game's logs, whether to queue the change for the game's
//! per-frame sync of flags; a first set of a flag also goes to the game's play log when that is on),
//! with the queue off: the queue is the game's frame's, and Kindle's frame isn't the game's yet [u].

use std::sync::OnceLock;

use kindle_core::{memory, patterns};

/// The store's block size (flags), a block's size (bytes), the blocks' storage, the blocks' map.
const BLOCK_FLAGS: usize = 0x1C;
const BLOCK_BYTES: usize = 0x20;
const BLOCKS: usize = 0x28;
const MAP: usize = 0x38;
/// A map node's left child, parent, right child, its "is the head" byte, its key, its storage.
const LEFT: usize = 0x00;
const PARENT: usize = 0x08;
const RIGHT: usize = 0x10;
const IS_HEAD: usize = 0x19;
const KEY: usize = 0x20;
const KIND: usize = 0x28;
const STORAGE: usize = 0x30;
/// Deeper than any balanced map of blocks gets: a walk past it is reading something else.
const MAX_DEPTH: usize = 64;

/// The game's read of a flag (store, flag): checked to read the store's block size at +0x1C and its
/// map at +0x38, and a node's head byte at +0x19 and key at +0x20.
const GET: &str = "44 8B 41 1C 44 8B DA 33 D2 41 8B C3 41 F7 F0 4C 8B D1 45 33 C9 44 0F AF C0 45 2B D8 4C 8B 41 38 49 8B D0 49 8B 48 08 44 38 49 19 75 ?? 0F 1F 00 39 41 20 73 ?? 48 8B 49 10";
/// The game's set (store, &flag, state, label, queue, unused), the one SetEventFlag calls.
const SET: &str = "48 89 5C 24 08 48 89 74 24 18 57 48 83 EC 30 48 8B DA 41 0F B6 F8 8B 12 48 8B F1 85 D2 0F 84 ?? ?? ?? ?? 45 84 C0 74 ?? E8 ?? ?? ?? ?? 85 C0 0F 95 C0 84 C0 75 ?? 48 8B 0D ?? ?? ?? ?? 48 85 C9";

/// What the game's set takes as its label: a kind (1, a name) and the name.
#[repr(C)]
struct Label {
    kind: u8,
    name: *const u8,
}

// A static string's address.
unsafe impl Sync for Label {}

static KINDLE: Label = Label { kind: 1, name: c"Kindle".as_ptr() as *const u8 };

type SetFn = unsafe extern "C" fn(usize, *const u32, u8, *const Label, u8, u8);

/// A store a test made, read instead of the game's.
#[cfg(test)]
pub(crate) static TEST_STORE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The game's flag store (None before the game makes it).
fn store() -> Result<usize, String> {
    #[cfg(test)]
    if let r @ 1.. = TEST_STORE.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(r);
    }
    crate::er::instance("CS::CSEventFlagMan").ok_or_else(|| "event flags are not available yet".into())
}

/// The game's set, found once (its read is only checked: Kindle reads flags itself).
fn setter() -> Result<SetFn, String> {
    #[cfg(test)]
    if TEST_STORE.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        return Ok(tests::game_set as SetFn);
    }
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    let at = FOUND
        .get_or_init(|| patterns::find_unique("flag read", GET).and(patterns::find_unique("flag set", SET)))
        .ok_or("the game's flag set wasn't found in this build")?;
    // Found by its code in the game's .text: the function the game's event scripts set flags with.
    Ok(unsafe { std::mem::transmute::<usize, SetFn>(at) })
}

/// A flag id as the game takes it: 32 bits.
pub fn check(id: i64) -> Result<u32, String> {
    u32::try_from(id).map_err(|_| format!("event flag {id} is out of range"))
}

/// Where the store keeps block `block`'s flags (None for a block it doesn't have), as the game finds it:
/// the map's first node whose key isn't below the block's, if that is the block's.
fn block_storage(store: usize, block: u32) -> Option<usize> {
    let head = memory::read_u64(store + MAP)? as usize;
    let (mut found, mut node) = (head, memory::read_u64(head + PARENT)? as usize);
    for _ in 0..MAX_DEPTH {
        if memory::read::<u8>(node + IS_HEAD)? != 0 {
            if found == head || block < memory::read::<u32>(found + KEY)? {
                return None;
            }
            let at = match memory::read::<i32>(found + KIND)? {
                // (The game multiplies in 32 bits.)
                1 => {
                    (memory::read::<u32>(found + STORAGE)?.wrapping_mul(memory::read::<u32>(store + BLOCK_BYTES)?)) as usize
                        + memory::read_u64(store + BLOCKS)? as usize
                }
                2 => memory::read_u64(found + STORAGE)? as usize,
                _ => return None,
            };
            return (at != 0).then_some(at);
        }
        if memory::read::<u32>(node + KEY)? < block {
            node = memory::read_u64(node + RIGHT)? as usize;
        } else {
            found = node;
            node = memory::read_u64(node + LEFT)? as usize;
        }
    }
    None
}

/// The store's block size, in flags and in bytes.
fn block_size(store: usize) -> Option<(u32, usize)> {
    let flags = memory::read::<u32>(store + BLOCK_FLAGS).filter(|&n| n != 0)?;
    Some((flags, memory::read::<u32>(store + BLOCK_BYTES)? as usize))
}

/// Flag `id`'s byte and bit in the store (None for a flag in no block).
fn flag_at(store: usize, id: u32) -> Option<(usize, u8)> {
    let (per_block, _) = block_size(store)?;
    let n = id % per_block;
    Some((block_storage(store, id / per_block)? + (n / 8) as usize, 1 << (7 - n % 8)))
}

/// Flag `id`'s state (off for a flag in no block, as the game reads it).
pub fn get(id: u32) -> Result<bool, String> {
    let store = store()?;
    let Some((byte, bit)) = flag_at(store, id) else { return Ok(false) };
    Ok(memory::read::<u8>(byte).ok_or_else(|| format!("event flag {id} is unreadable"))? & bit != 0)
}

/// `get` for kindle-core's flag watches.
pub fn get_i64(id: i64) -> Result<bool, String> {
    get(check(id)?)
}

/// Sets flag `id` through the game's setter; true if it now has that state (false for a flag in no
/// block, which the game's set leaves be). The game ignores flag 0.
pub fn set(id: u32, state: bool) -> Result<bool, String> {
    let (store, set) = (store()?, setter()?);
    if id == 0 || flag_at(store, id).is_none() {
        return Ok(false);
    }
    // The game's set of a flag that is off looks it up in CSEventFlagUsageParamManager, asserting
    // (the game stops) if there is none yet.
    #[cfg(not(test))]
    if crate::er::instance("CS::CSEventFlagUsageParamManager").is_none() {
        return Err("event flags can't be set yet (the game's flag usage params aren't loaded)".into());
    }
    unsafe { set(store, &id, state as u8, &KINDLE, 0, 0) };
    Ok(get(id)? == state)
}

/// The block of flags holding `id`: its first flag and its flags as words, the highest bit first (the
/// block's first flag is bit 31 of the first word), or None when the store has no such block.
pub fn block(id: u32) -> Result<Option<(u32, Vec<u32>)>, String> {
    let store = store()?;
    let (per_block, bytes) = block_size(store).ok_or("the event flags' block size is unreadable")?;
    let Some(at) = block_storage(store, id / per_block) else { return Ok(None) };
    let mut raw = memory::read_bytes(at, bytes).ok_or_else(|| format!("event flag {id}'s block is unreadable"))?;
    raw.resize(bytes.next_multiple_of(4), 0);
    Ok(Some((id - id % per_block, raw.as_chunks::<4>().0.iter().map(|w| u32::from_be_bytes(*w)).collect())))
}

/// The natives for kindle.flags.
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::{FlagWatch, ctx, rt_err};
    n.set("flag_get", lua.create_function(|_, id: i64| get(check(id).map_err(rt_err)?).map_err(rt_err))?)?;
    n.set("flag_set", lua.create_function(|_, (id, state): (i64, bool)| set(check(id).map_err(rt_err)?, state).map_err(rt_err))?)?;
    n.set(
        "flag_block",
        lua.create_function(|_, id: i64| {
            Ok(match block(check(id).map_err(rt_err)?).map_err(rt_err)? {
                Some((first, words)) => (Some(words), Some(first)),
                None => (None, None),
            })
        })?,
    )?;
    n.set(
        "flag_watch",
        lua.create_function(|lua, (flag, on_set, f): (i64, bool, mlua::Function)| {
            check(flag).map_err(rt_err)?;
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.flags.push(FlagWatch { handle, flag, on_set, last: None, func: f });
            Ok(handle)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::Ordering::Relaxed;

    use super::*;

    /// The game's set on a test's store: the flag's bit, as the game's own set writes it.
    pub(super) extern "C" fn game_set(store: usize, id: *const u32, state: u8, _: *const Label, _: u8, _: u8) {
        let id = unsafe { *id };
        if id == 0 {
            return;
        }
        if let Some((byte, bit)) = flag_at(store, id) {
            unsafe { *(byte as *mut u8) = if state != 0 { *(byte as *const u8) | bit } else { *(byte as *const u8) & !bit } };
        }
    }

    /// A store laid out as the game's, served to Kindle (TEST_STORE) until dropped: blocks of 1000 flags
    /// (125 bytes); block 20 the store's second block, 30 its first, 10 one of its own, 40 one of a
    /// kind the game doesn't read; a map of them with 20 at its root.
    pub(crate) struct Store {
        pub(crate) store: Vec<u64>,
        pub(crate) blocks: Vec<u8>,
        pub(crate) own: Vec<u8>,
        _nodes: Vec<Vec<u64>>,
    }

    impl Store {
        pub(crate) fn serve() -> Self {
            let mut blocks = vec![0u8; 250];
            let mut own = vec![0u8; 125];
            let mut store = vec![0u64; 0x40 / 8];
            let node = |key: u32, kind: u64, storage: u64| {
                let mut n = vec![0u64; 0x38 / 8];
                n[KEY / 8] = key as u64;
                n[KIND / 8] = kind;
                n[STORAGE / 8] = storage;
                n
            };
            let mut head = vec![0u64; 0x38 / 8];
            head[0x18 / 8] = 1 << 8; // is the head
            let mut nodes = vec![node(20, 1, 1), node(10, 2, own.as_mut_ptr() as u64), node(30, 1, 0), node(40, 0, 0)];
            let at = |v: &Vec<u64>| v.as_ptr() as u64;
            let (root, left, right, far) = (at(&nodes[0]), at(&nodes[1]), at(&nodes[2]), at(&nodes[3]));
            let h = at(&head);
            for (n, (l, p, r)) in nodes.iter_mut().zip([(left, h, right), (h, root, h), (h, root, far), (h, right, h)]) {
                n[LEFT / 8] = l;
                n[PARENT / 8] = p;
                n[RIGHT / 8] = r;
            }
            head[LEFT / 8] = left;
            head[PARENT / 8] = root;
            head[RIGHT / 8] = far;
            store[BLOCK_FLAGS / 8] = 1000 << 32;
            store[BLOCK_BYTES / 8] = 125;
            store[BLOCKS / 8] = blocks.as_mut_ptr() as u64;
            store[MAP / 8] = h;
            nodes.push(head);
            let mut s = Self { store, blocks, own, _nodes: nodes };
            TEST_STORE.store(s.store.as_mut_ptr() as usize, Relaxed);
            s
        }

        pub(crate) fn at(&self) -> usize {
            self.store.as_ptr() as usize
        }
    }

    impl Drop for Store {
        fn drop(&mut self) {
            TEST_STORE.store(0, Relaxed);
        }
    }

    #[test]
    fn flags_are_found_in_their_blocks_as_the_game_finds_them() {
        let _one = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut s = Store::serve();
        let store = s.at();
        // Each block where its node says; blocks the map hasn't, or of a kind the game doesn't read, none.
        assert_eq!(block_storage(store, 20), Some(s.blocks.as_ptr() as usize + 125));
        assert_eq!(block_storage(store, 30), Some(s.blocks.as_ptr() as usize));
        assert_eq!(block_storage(store, 10), Some(s.own.as_ptr() as usize));
        for missing in [0, 9, 11, 25, 31, 40, 41, 1000] {
            assert_eq!(block_storage(store, missing), None, "block {missing}");
        }
        // A flag's bit, the highest first.
        s.blocks[125] = 0b1000_0001;
        s.blocks[125 + 124] = 0b0001_0000;
        s.own[2] = 0b0100_0000;
        let on: Vec<u32> = [20_000, 20_007, 20_992, 20_995, 10_017, 10_018, 30_000, 99_999].into_iter().filter(|&id| get(id).unwrap()).collect();
        assert_eq!(on, [20_000, 20_007, 20_995, 10_017]);
        // The game's set and Kindle's read agree; a flag in no block takes no state.
        assert_eq!(set(30_123, true), Ok(true));
        assert_eq!(s.blocks[15], 0b0001_0000);
        assert_eq!(set(30_123, false), Ok(true));
        assert_eq!(s.blocks[15], 0);
        assert_eq!(set(25_000, true), Ok(false));
        assert_eq!(get(25_000), Ok(false));
        // A block as words: flag 20000 is the first word's bit 31.
        let (first, words) = block(20_500).unwrap().unwrap();
        assert_eq!((first, words.len(), words[0], words[31]), (20_000, 32, 0x8100_0000, 0x1000_0000));
        assert_eq!(block(25_000), Ok(None));
        assert!(check(-1).is_err() && check(1 << 32).is_err() && check(1_034_500_000).is_ok());
    }

    /// The game's read and set of a flag, each found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE).
    #[test]
    fn the_games_flag_functions_are_found() {
        let Some((img, _, sections)) = crate::singletons::tests::eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let (rva, size) = sections.iter().find(|(n, _, _)| n == ".text").map(|&(_, r, s)| (r, s)).unwrap();
        let text = &img[rva..rva + size];
        let find = |pattern: &str| -> Vec<usize> {
            let p: Vec<Option<u8>> = pattern.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect();
            (0..text.len() - p.len()).filter(|&i| p.iter().zip(&text[i..]).all(|(p, b)| p.is_none_or(|v| v == *b))).map(|i| rva + i).collect()
        };
        assert_eq!(find(GET), [0x5FA250]);
        assert_eq!(find(SET), [0x5D2F60]);
    }
}
