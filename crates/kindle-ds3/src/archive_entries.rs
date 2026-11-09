//! New files in the game's archives: a mod's file at a path no archive has (a new map's files,
//! say) gets an entry in the archive's index in the game's memory, so the game finds and opens
//! it as one of its own, and Kindle's overlay (files.rs) serves its bytes over the reads.
//!
//! The index is `BHD5` (its size at +0x0C) with a bucket table at +0x10 (count) / +0x14
//! (offset): each bucket is {u32 count, u32 offset} of 0x28-byte entries {u32 path hash, i32
//! padded size, i64 data offset, i64 hash offset, i64 key offset, u64 size} (archive.rs). A path's
//! bucket is its hash modulo the bucket count. The archive device's lookup (+0x1AEBC13, in the
//! protected code) reads three fields of the device: the index's start (+0xC0), the bucket count
//! (+0xC8) and the bucket table (+0xD0), and returns the entry. The file operator made from it
//! keeps the entry's data offset and padded size (what its reads read); the map loader's check
//! looks the entry up again for its hash and key offsets, and skips the hash check or the
//! decryption when one is 0 (DLEncryptedBinderLightObject). So an entry with neither is read as
//! it is.
//!
//! The offsets are 32-bit, from the index's start, and the game's copy of the index sits in its
//! own heap with no free memory within 4 GB after it (seen 2026-10-20: Data5's at
//! 0x7ff3b26f1f00), so the first new file of an archive moves its index: Kindle copies it with
//! room after it and points the archive device's fields at the copy (the old copy stays as it is
//! for a read under way). A bucket that gets a new entry gets a new entry array (its entries and
//! the new one) in that room, its count and offset changed in one 8-byte write.
//!
//! The copy is the device's own memory: when the game quits, the device's destructor (+0x1AEB9F0,
//! protected; its clean copy read) frees its index (+0xC0) through its allocator (+0x48, a
//! DLKR::DLAllocator: slot 13 frees, slot 10 allocates aligned), whose heap reads the block's header
//! before it (+0x17EB810). Memory of Kindle's there crashed the quit (+0x17EB828 reading 8 bytes
//! before the copy, 2026-10-21), so the copy is allocated from that allocator. The entry's data offset is 0 of the archive's data file, far longer than any file: the
//! game reads that many bytes there and Kindle's overlay puts the file's own in their place.
//!
//! Which archive: the game names it in the path it opens (`data5:/map/m21/...`), for a file no
//! archive has as for one they have.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::{archive, memory};

const ENTRY_SIZE: usize = 0x28;
/// Room after a moved index for new bucket arrays (each a bucket's entries and the new one, about
/// 320 bytes: the DSR world's 1400 files need under 0.5 MB).
const ROOM: usize = 2 << 20;
/// The archive device's fields for its index: its start, its bucket count (u32), its bucket table.
const DEVICE_INDEX: usize = 0xC0;
const DEVICE_BUCKETS: usize = 0xC8;
const DEVICE_TABLE: usize = 0xD0;
/// The archive device's allocator, which frees its index when the device goes.
const DEVICE_ALLOCATOR: usize = 0x48;
/// DLKR::DLAllocator's slot that allocates (this, size, alignment) -> block or null.
const ALLOCATE_ALIGNED_SLOT: usize = 10;
/// How much of a bucket table tells two apart.
const TABLE_CHECK: usize = 0x40;

/// A moved index: where it is, its size, how much of the room after it is used.
struct Moved {
    start: usize,
    size: usize,
    used: usize,
}

/// Moved indexes by archive name.
static MOVED: Mutex<Option<HashMap<String, Moved>>> = Mutex::new(None);

/// The copies of `name`'s index in the game's memory: of those whose first bytes are the index's,
/// the ones holding `hash`'s bucket as the file has it (the search also finds freed buffers that
/// start the same).
fn live_copies(game_dir: &Path, name: &str, hash: u32) -> Result<Vec<usize>, String> {
    let header = archive::index_header(game_dir, name)?;
    let starts = crate::files::index_copies(&header).ok_or_else(|| format!("{name}'s index wasn't found in the game's memory"))?;
    let (buckets, k, disk) = archive::home_bucket(game_dir, name, hash)?;
    let holds = |start: usize| -> Option<bool> {
        if memory::read::<u32>(start + 0x10)? as usize != buckets {
            return Some(false);
        }
        let bucket = start + memory::read::<u32>(start + 0x14)? as usize + 8 * k;
        let (count, offset) = (memory::read::<u32>(bucket)? as usize, memory::read::<u32>(bucket + 4)? as usize);
        Some(memory::read_bytes(start + offset, ENTRY_SIZE * count)?.starts_with(&disk))
    };
    let live: Vec<usize> = starts.into_iter().filter(|&s| holds(s) == Some(true)).collect();
    if live.is_empty() {
        return Err(format!("no copy of {name}'s index in the game's memory holds its bucket"));
    }
    Ok(live)
}

/// The archive devices reading the index copy at `start`: those whose index is it, with its
/// bucket count and a bucket table as its own.
fn devices_of(start: usize) -> Vec<usize> {
    let (Some(buckets), Some(table)) = (memory::read::<u32>(start + 0x10), memory::read::<u32>(start + 0x14)) else { return Vec::new() };
    let Some(own) = memory::read_bytes(start + table as usize, TABLE_CHECK.min(8 * buckets as usize)) else { return Vec::new() };
    memory::holders_of(start as u64)
        .into_iter()
        .filter_map(|h| h.checked_sub(DEVICE_INDEX))
        .filter(|&d| {
            memory::read::<u32>(d + DEVICE_BUCKETS) == Some(buckets)
                && memory::read::<u64>(d + DEVICE_TABLE).and_then(|t| memory::read_bytes(t as usize, own.len())).is_some_and(|t| t == own)
        })
        .collect()
}

/// `size` bytes from the game's allocator at `allocator` (its slot 10, 16-byte aligned), if it is
/// one (its vtable in the game's module) and has the room.
fn game_allocate(allocator: usize, size: usize) -> Result<usize, String> {
    let (base, len) = memory::game_module().ok_or("the game's module wasn't found")?;
    let vtable = memory::read::<u64>(allocator).ok_or("no allocator")? as usize;
    let f = memory::read::<u64>(vtable + 8 * ALLOCATE_ALIGNED_SLOT).ok_or("no allocator")? as usize;
    if !(base..base + len).contains(&vtable) || !(base..base + len).contains(&f) {
        return Err(format!("the archive device's allocator at {allocator:#x} isn't the game's"));
    }
    let allocate: extern "C" fn(usize, usize, usize) -> usize = unsafe { std::mem::transmute(f) };
    match allocate(allocator, size, 16) {
        0 => Err(format!("the game's allocator ({}) has no {size} bytes free", memory::class_name(allocator).unwrap_or_default())),
        block => Ok(block),
    }
}

/// `name`'s index moved into a block of its device's allocator (once): its start.
fn moved(game_dir: &Path, name: &str, hash: u32, map: &mut HashMap<String, Moved>) -> Result<usize, String> {
    if let Some(m) = map.get(name) {
        return Ok(m.start);
    }
    // Every device reading a copy gets the new one; the first copy a device reads is copied.
    let mut old = None;
    let mut devices = Vec::new();
    for copy in live_copies(game_dir, name, hash)? {
        let found = devices_of(copy);
        if !found.is_empty() {
            old.get_or_insert(copy);
            devices.extend(found);
        }
    }
    let old = old.ok_or_else(|| format!("no archive device reads {name}'s index"))?;
    // Each device frees its index when it goes: two sharing the copy would free it twice.
    if devices.len() > 1 {
        return Err(format!("{} archive devices read {name}'s index (one was expected)", devices.len()));
    }
    let size = memory::read::<u32>(old + 0x0C).ok_or("index cut short")? as usize;
    let table = memory::read::<u32>(old + 0x14).ok_or("index cut short")? as usize;
    let bytes = memory::read_bytes(old, size).ok_or("index cut short")?;
    let allocator = memory::read::<u64>(devices[0] + DEVICE_ALLOCATOR).ok_or("the archive device has no allocator")? as usize;
    let new = game_allocate(allocator, size + ROOM)?;
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), new as *mut u8, size) };
    for &d in &devices {
        unsafe {
            std::ptr::write_volatile((d + DEVICE_TABLE) as *mut u64, (new + table) as u64);
            std::ptr::write_volatile((d + DEVICE_INDEX) as *mut u64, new as u64);
        }
    }
    crate::files::index_moved(&archive::index_header(game_dir, name)?, old, size, new);
    crate::info!("files: {name}'s index moved from {old:#x} to {new:#x} for new files ({size} bytes and {ROOM} of room, from its device's allocator)");
    map.insert(name.to_string(), Moved { start: new, size, used: 0 });
    Ok(new)
}

/// Gives `path` (`/map/...`) an entry of `len` bytes in archive `name`'s index, unless it has
/// one. The path must be one no archive has.
pub fn add(game_dir: &Path, name: &str, path: &str, len: usize) -> Result<(), String> {
    let hash = archive::path_hash(path);
    let mut guard = MOVED.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let start = moved(game_dir, name, hash, map)?;
    let buckets = memory::read::<u32>(start + 0x10).ok_or("index cut short")? as usize;
    let bucket = start + memory::read::<u32>(start + 0x14).ok_or("index cut short")? as usize + 8 * (hash as usize % buckets);
    let (count, offset) = (memory::read::<u32>(bucket).ok_or("index cut short")? as usize, memory::read::<u32>(bucket + 4).ok_or("index cut short")? as usize);
    let mut entries = memory::read_bytes(start + offset, ENTRY_SIZE * count).ok_or("bucket cut short")?;
    if entries.as_chunks::<ENTRY_SIZE>().0.iter().any(|e| u32::from_le_bytes(e[..4].try_into().expect("4 bytes")) == hash) {
        return Ok(());
    }
    let mut e = [0u8; ENTRY_SIZE];
    e[..4].copy_from_slice(&hash.to_le_bytes());
    e[4..8].copy_from_slice(&(len as i32).to_le_bytes());
    // data offset 0, no hash, no key; size: the length, or 0 for a DCX (the game's own have 0)
    let size = if path.ends_with(".dcx") { 0 } else { len as u64 };
    e[0x20..0x28].copy_from_slice(&size.to_le_bytes());
    entries.extend_from_slice(&e);
    let m = map.get_mut(name).ok_or("index not moved")?;
    let at = (m.size + m.used).next_multiple_of(16);
    if at + entries.len() > m.size + ROOM {
        return Err("no room left after the moved index".into());
    }
    m.used = at + entries.len() - m.size;
    unsafe {
        std::ptr::copy_nonoverlapping(entries.as_ptr(), (start + at) as *mut u8, entries.len());
        std::ptr::write_volatile(bucket as *mut u64, ((at as u64) << 32) | (count as u64 + 1));
    }
    crate::debug!("files: {path} added to {name}'s index ({len} bytes, bucket of {})", count + 1);
    Ok(())
}
