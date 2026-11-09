//! Replacement game files from mods: a mod's `files\<game path>` (e.g. `files\menu\hi\00_solo.tpfbhd`)
//! is loaded instead of the game's own, as Kindle for Dark Souls III serves them (kindle-ds3's
//! files.rs, whose redirect this is; its map-file overlay isn't ported).
//!
//! Elden Ring reads its archives (Data0-3.bdt, DLC) through the same encrypted-binder file device,
//! `DLEncryptedBinderLightFileDevice`, and loose files through the disk device,
//! `MicrosoftDiskFileDevice`. Both open with `open(device, path, ?, request, allocator, flag) -> file
//! operator` (vtable slot 1): the archive's hashes the path after its root (`data0:`) and makes an
//! operator for its entry (`FUN_142429730`), the disk's checks the path (`FUN_141fc1d40`, which also
//! turns `/` into `\` in it) and, with a 0 flag, makes a `MicrosoftDiskFileOperator` for it. Neither
//! reads the third argument. Their path is the game's wide string, laid out differently from Dark
//! Souls III's: the allocator at +0, the characters at +8 (inline under 8 characters, else a pointer
//! there), the length at +0x18 and the capacity at +0x20.
//!
//! Kindle swaps both slots (data only): the disk device's to learn the device, the archive device's to
//! send a path a mod replaces to the disk device instead, with a string of Kindle's holding the mod
//! file's absolute path (laid out by hand, once per file, and kept for the session: the game's string
//! functions can't be used on its file-loading threads). If the disk device can't open it, the game's
//! own file is used. The disk operator's set path (slot 2, `(operator, path, flag) -> bool`) is swapped
//! too, as for Dark Souls III's menus: a loader that sets the archive path again on the operator gets
//! Kindle's string back. So is the archive object's setup (`DLEncryptedBinderLightObject`, slot 1,
//! `FUN_14242b2c0`), which reads an entry's ranges for the loader before the device opens the file:
//! Kindle sees the device there first (`first_sight`).
//!
//! Overrides are read once, when Kindle starts (enabled mods, in name order; the first mod to ship a
//! file wins and the others are reported).
//!
//! A map's event file that mods change (kindle_core::event_patches) is served the same way, from
//! `kindle\cache\event`: the game's file read out of its archive (archive.rs, with the device and
//! mount found here), unpacked, changed and packed again. Its entry is made plain when a mod first
//! changes the map, so from then on Kindle serves the file each time the map loads, changed or not
//! (the game's own, decrypted, once mods' changes are gone); if Kindle can't make it, the entry gets
//! its ranges back and the game reads its own.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use kindle_core::{lock, memory, patterns};

const OPEN_SLOT: usize = 1;
const SET_PATH_SLOT: usize = 2;
/// The archive object's setup: `(object, path, device) -> bool`, the path a NUL-ended wide string.
const SETUP_SLOT: usize = 1;
const ARCHIVE_DEVICE: &str = "DLEncryptedBinderLightFileDevice@DLEBL";
const ARCHIVE_OBJECT: &str = "DLEncryptedBinderLightObject@DLEBL";
const DISK_DEVICE: &str = "MicrosoftDiskFileDevice@msvc90_windows@DLIOD";
const DISK_OPERATOR: &str = "MicrosoftDiskFileOperator@msvc90_windows@DLIOD";

/// The game's wide string: its allocator, the characters (or a pointer to them), length, capacity.
const ALLOCATOR: usize = 0x00;
const CHARS: usize = 0x08;
const LENGTH: usize = 0x18;
const CAPACITY: usize = 0x20;
/// Characters held inline (a longer string's are behind the pointer).
const INLINE: usize = 8;
/// Kindle's strings are this big (the game's is 0x28 and maybe a flag after it; zeros there).
const STRING_SIZE: usize = 0x30;
/// Longest path Kindle reads from a game string.
const MOST_CHARS: usize = 4096;

/// Lowercase game path ("menu/hi/00_solo.tpfbhd") → (mod id, the file, as UTF-16).
static OVERRIDES: OnceLock<HashMap<String, (String, Vec<u16>)>> = OnceLock::new();
static ARCHIVE_OPEN: AtomicUsize = AtomicUsize::new(0);
static OBJECT_SETUP: AtomicUsize = AtomicUsize::new(0);
static DISK_OPEN: AtomicUsize = AtomicUsize::new(0);
static DISK_SET_PATH: AtomicUsize = AtomicUsize::new(0);
/// The disk device, seen in its first `open`.
static DISK: AtomicUsize = AtomicUsize::new(0);
/// The game strings Kindle built for the files it serves, by game path: the text and the string
/// (never freed; a path served from another file gets a new one).
static PATHS: Mutex<Option<HashMap<String, GameString>>> = Mutex::new(None);
type GameString = (Vec<u16>, usize);
/// Whether the first archive path was logged (how the game names its files, for mod makers).
static LOGGED_FIRST: AtomicBool = AtomicBool::new(false);
/// The game's folder (its archives) and Kindle's cache (the event files it makes).
static GAME_DIR: OnceLock<PathBuf> = OnceLock::new();
static CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();

type OpenFn = extern "C" fn(usize, usize, usize, usize, usize, usize, usize, usize, usize) -> usize;

/// The lookup key for a game path: forward slashes, lowercase, no leading slashes.
fn key(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_lowercase()
}

/// The game path of an archive path ("data0:/menu/hi/00_solo.tpfbhd" → "/menu/hi/00_solo.tpfbhd").
fn game_path(text: &str) -> &str {
    text.split_once(':').map(|(_, rest)| rest).unwrap_or(text)
}

/// Whether a mod replaces the game file at `path` ("menu/hi/00_solo.tpfbhd").
pub fn overridden(path: &str) -> bool {
    OVERRIDES.get().is_some_and(|m| m.contains_key(&key(path)))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push((key(&rel.to_string_lossy()), p));
        }
    }
}

/// Every replacement file of the given mods (folder names, in priority order).
pub fn collect(mods_dir: &Path, mods: &[String]) -> HashMap<String, (String, PathBuf)> {
    let mut map: HashMap<String, (String, PathBuf)> = HashMap::new();
    for id in mods {
        let root = mods_dir.join(id).join("files");
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        files.sort();
        for (k, path) in files {
            match map.get(&k) {
                Some((owner, _)) => kindle_core::warn!("files: {id} and {owner} both replace {k}; using {owner}'s"),
                None => {
                    map.insert(k, (id.clone(), path));
                }
            }
        }
    }
    map
}

/// Writes a vtable slot: one aligned 8-byte write (other threads may be calling through it).
fn write_slot(slot: usize, value: usize) -> bool {
    use windows::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
    let mut old = PAGE_PROTECTION_FLAGS(0);
    let p = slot as *const std::ffi::c_void;
    if unsafe { VirtualProtect(p, 8, PAGE_READWRITE, &mut old) }.is_err() {
        return false;
    }
    unsafe { std::ptr::write_volatile(slot as *mut usize, value) };
    let mut ignored = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(p, 8, old, &mut ignored) };
    true
}

fn swap(class: &str, index: usize, hook: usize, original: &AtomicUsize) -> bool {
    let Some(vt) = patterns::find_vtable(class) else { return false };
    let slot = vt + index * 8;
    let Some(o) = memory::read_u64(slot).map(|v| v as usize) else { return false };
    if o == hook {
        return true;
    }
    original.store(o, Ordering::Release);
    write_slot(slot, hook)
}

/// Finds the enabled mods' replacement files and takes over the file devices' opens (also for the
/// event files mods change: `game_dir` has the archives, `cache_dir` gets the files Kindle makes).
/// Call early (before the game opens what mods replace).
pub fn install(mods_dir: &Path, disabled: &BTreeSet<String>, game_dir: &Path, cache_dir: &Path) {
    let _ = GAME_DIR.set(game_dir.to_path_buf());
    let _ = CACHE_DIR.set(cache_dir.to_path_buf());
    let mut ids: Vec<String> = std::fs::read_dir(mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("files").is_dir())
        .filter(|e| {
            kindle_core::config::ModManifest::load(&e.path().join("mod.toml"))
                .is_ok_and(|m| m.info.enabled && m.info.for_game(crate::BINDINGS.game) && !disabled.contains(&m.info.id))
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !n.starts_with('_'))
        .collect();
    ids.sort();
    let map = collect(mods_dir, &ids);
    let count = map.len();
    let wide: HashMap<String, (String, Vec<u16>)> =
        map.into_iter().map(|(k, (id, p))| (k, (id, p.as_os_str().to_string_lossy().encode_utf16().collect()))).collect();
    let _ = OVERRIDES.set(wide);
    // The disk hook first: archive opens are only redirected once the disk device is known.
    if !swap(DISK_DEVICE, OPEN_SLOT, disk_open as *const () as usize, &DISK_OPEN)
        || !swap(ARCHIVE_DEVICE, OPEN_SLOT, archive_open as *const () as usize, &ARCHIVE_OPEN)
    {
        kindle_core::warn!("files: the game's file devices weren't found; mods' files and event changes won't be used");
        return;
    }
    if !swap(DISK_OPERATOR, SET_PATH_SLOT, disk_set_path as *const () as usize, &DISK_SET_PATH) {
        kindle_core::warn!("files: the disk file operator wasn't found; some of mods' files may not load");
    }
    if !swap(ARCHIVE_OBJECT, SETUP_SLOT, object_setup as *const () as usize, &OBJECT_SETUP) {
        kindle_core::warn!("files: the archive object wasn't found; a file the game loads at its start may be read wrong");
    }
    kindle_core::info!("files: {count} replacement file(s) from mods");
}

/// Text of one of the game's wide strings.
fn read_string(s: usize) -> Option<Vec<u16>> {
    let len = memory::read_u64(s + LENGTH)? as usize;
    let cap = memory::read_u64(s + CAPACITY)? as usize;
    if len > MOST_CHARS || cap < len {
        return None;
    }
    let data = if cap < INLINE { s + CHARS } else { memory::read_u64(s + CHARS)? as usize };
    let bytes = memory::read_bytes(data, len * 2)?;
    Some(bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect())
}

/// A game string holding `text`, built once per key and text and kept for the session: `allocator`,
/// the characters (NUL-terminated, in Kindle's memory, so writable: the disk device's check edits
/// them), length and capacity (over the inline size, so the pointer is used).
fn game_string(key: &str, text: &[u16], allocator: usize) -> usize {
    let mut guard = PATHS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some((had, s)) = map.get(key)
        && had == text
    {
        return *s;
    }
    let s = {
        let chars: &'static mut [u16] = Box::leak(text.iter().copied().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
        let len = text.len() as u64;
        let mut fields = [0u64; STRING_SIZE / 8];
        fields[ALLOCATOR / 8] = allocator as u64;
        fields[CHARS / 8] = chars.as_mut_ptr() as u64;
        fields[LENGTH / 8] = len;
        fields[CAPACITY / 8] = len.max(INLINE as u64);
        Box::leak(Box::new(fields)).as_ptr() as usize
    };
    map.insert(key.to_string(), (text.to_vec(), s));
    s
}

// The hooks forward extra arguments: unused ones are harmless, and the exact count isn't known for
// every caller.
#[allow(clippy::too_many_arguments)]
extern "C" fn disk_open(this: usize, a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    DISK.store(this, Ordering::Release);
    let open: OpenFn = unsafe { std::mem::transmute(DISK_OPEN.load(Ordering::Acquire)) };
    open(this, a, b, c, d, e, f, g, h)
}

#[allow(clippy::too_many_arguments)]
extern "C" fn archive_open(this: usize, path: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let original: OpenFn = unsafe { std::mem::transmute(ARCHIVE_OPEN.load(Ordering::Acquire)) };
    let redirected = std::panic::catch_unwind(|| {
        let text = String::from_utf16_lossy(&read_string(path)?);
        first_sight(this, &text);
        redirect(&text, path, b, c, d)
    });
    let redirected = redirected.ok().flatten();
    redirected.unwrap_or_else(|| original(this, path, b, c, d, e, f, g, h))
}

/// The archive object's setup reads an entry's ranges for the loader, and can come before its
/// device's first open (the flag allocation list, read with the first param file at the start, was
/// read wrong and the game's data never loaded, 2026-10-30): the device is seen here first.
#[allow(clippy::too_many_arguments)]
extern "C" fn object_setup(this: usize, path: usize, device: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let original: OpenFn = unsafe { std::mem::transmute(OBJECT_SETUP.load(Ordering::Acquire)) };
    if device != 0 && !DEVICES.iter().any(|d| d.load(Ordering::Acquire) == device) {
        let _ = std::panic::catch_unwind(|| {
            if let Some(text) = read_wide(path) {
                first_sight(device, &String::from_utf16_lossy(&text));
            }
        });
    }
    original(this, path, device, c, d, e, f, g, h)
}

/// A NUL-ended wide string's text.
fn read_wide(p: usize) -> Option<Vec<u16>> {
    let mut out = Vec::new();
    while out.len() < MOST_CHARS {
        match memory::read::<u16>(p + out.len() * 2)? {
            0 => return Some(out),
            c => out.push(c),
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
extern "C" fn disk_set_path(this: usize, path: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let original: OpenFn = unsafe { std::mem::transmute(DISK_SET_PATH.load(Ordering::Acquire)) };
    let path = std::panic::catch_unwind(|| mod_string(path)).ok().flatten().unwrap_or(path);
    original(this, path, b, c, d, e, f, g, h)
}

/// Kindle's string for an archive path that `redirect` sent to the disk device.
fn mod_string(path: usize) -> Option<usize> {
    let text = String::from_utf16_lossy(&read_string(path)?);
    if !text.contains(":/") {
        return None;
    }
    PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&key(game_path(&text))).map(|(_, s)| *s)
}

/// The archive device's index (its header file, decrypted in memory): the base its offsets count from,
/// the bucket count and the buckets (`{i32 count, u32 first entry}` each), as its open reads them.
const INDEX_BASE: usize = 0xB0;
const INDEX_BUCKETS: usize = 0xB8;
const INDEX_BUCKET_TABLE: usize = 0xC0;
/// An index entry: `{u64 path hash, i32 padded size, i32 size, i64 offset, i64 hash ranges, i64 AES
/// ranges}`, the ranges' offsets from the base (0: none).
const ENTRY_SIZE: usize = 0x28;
const ENTRY_HASH_RANGES: usize = 0x18;
const ENTRY_AES_RANGES: usize = 0x20;
/// Most entries one bucket holds (the game's have a handful).
const MOST_IN_BUCKET: usize = 4096;

/// The archive's hash of a path: from after its root (`data0:`), letters lowercased and `\` as `/`.
fn path_hash(text: &str) -> Option<u64> {
    let (_, rest) = text.split_once(':')?;
    Some(rest.encode_utf16().fold(0u64, |h, c| {
        let c = match c {
            0x41..=0x5A => c + 0x20,
            0x5C => 0x2F,
            c => c,
        };
        h.wrapping_mul(0x85).wrapping_add(c as u64)
    }))
}

/// The archive device's index entry for `text` (an archive path), as its open finds it.
fn index_entry(device: usize, text: &str) -> Option<usize> {
    let hash = path_hash(text)?;
    let base = memory::read_u64(device + INDEX_BASE)? as usize;
    let buckets = memory::read::<u32>(device + INDEX_BUCKETS)? as u64;
    let table = memory::read_u64(device + INDEX_BUCKET_TABLE)? as usize;
    if base == 0 || buckets == 0 || table == 0 {
        return None;
    }
    let bucket = table + (hash % buckets) as usize * 8;
    let [count, first] = memory::read::<[u32; 2]>(bucket)?;
    (0..(count as usize).min(MOST_IN_BUCKET)).map(|i| base + first as usize + i * ENTRY_SIZE).find(|&e| memory::read_u64(e) == Some(hash))
}

/// Makes the game read an entry's file as it is: what the archive stores is partly encrypted and
/// checked by hashes of a few ranges, and the loader decrypts and checks what it read by the entry,
/// whichever device read it (a decrypted file served from disk was read wrong and opened again, and
/// the title screen never came, 2026-10-30), so a mod's file served for it can have neither. The
/// entry is the game's for the session; its own file isn't read again (Kindle serves the path).
fn plain_entry(entry: usize) -> bool {
    write_slot(entry + ENTRY_HASH_RANGES, 0) && write_slot(entry + ENTRY_AES_RANGES, 0)
}

/// The archive devices seen so far (one per archive: Data0-3, the DLC's).
static DEVICES: [AtomicUsize; 16] = [const { AtomicUsize::new(0) }; 16];
/// Each device seen and its mount, the root of the paths it opens ("data0", its archive's name).
static MOUNTS: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());
/// The entries made plain, by game path, with the ranges they had.
static PLAIN: Mutex<Option<HashMap<String, Plain>>> = Mutex::new(None);
/// Event files mods change that no archive seen so far has.
static WANTED: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

#[derive(Clone, Copy)]
struct Plain {
    entry: usize,
    hashes: u64,
    aes: u64,
    /// An event file Kindle serves (else a mod's file).
    events: bool,
}

/// Makes `rel`'s entry plain, if `device`'s archive has it: whether it is (or was already).
fn make_plain(device: usize, rel: &str, events: bool) -> bool {
    let mut plain = lock(&PLAIN);
    let plain = plain.get_or_insert_with(HashMap::new);
    if plain.contains_key(rel) {
        return true;
    }
    let Some(entry) = index_entry(device, &format!(":/{rel}")) else { return false };
    let (Some(hashes), Some(aes)) = (memory::read_u64(entry + ENTRY_HASH_RANGES), memory::read_u64(entry + ENTRY_AES_RANGES)) else { return false };
    let was = Plain { entry, hashes, aes, events };
    if !plain_entry(entry) {
        restore(&was);
        return false;
    }
    plain.insert(rel.to_string(), was);
    true
}

/// Gives an entry made plain its ranges back.
fn restore(was: &Plain) {
    write_slot(was.entry + ENTRY_HASH_RANGES, was.hashes as usize);
    write_slot(was.entry + ENTRY_AES_RANGES, was.aes as usize);
}

/// The game reads its own event file at `rel` again (Kindle couldn't serve it).
fn stop_serving(rel: &str) {
    let mut plain = lock(&PLAIN);
    if let Some(was) = plain.as_mut().and_then(|p| p.remove(rel)) {
        restore(&was);
    }
}

/// The first time an archive device opens a file (`text`, whose root is the device's mount), the
/// entries of every path a mod replaces are made plain (see plain_entry): the loader reads an entry
/// before it opens the file (changed at the open, the first read of the file still failed; the game's
/// second try read it), and the devices open other files first. So are those of event files mods
/// changed before.
fn first_sight(device: usize, text: &str) {
    if DEVICES.iter().any(|d| d.load(Ordering::Acquire) == device) {
        return;
    }
    if !DEVICES.iter().any(|d| d.compare_exchange(0, device, Ordering::AcqRel, Ordering::Acquire).is_ok()) {
        return;
    }
    if !LOGGED_FIRST.swap(true, Ordering::Relaxed) {
        kindle_core::info!("files: the game's first archive file: {text}");
    }
    if let Some((mount, _)) = text.split_once(':') {
        lock(&MOUNTS).push((device, mount.to_lowercase()));
    }
    for (rel, (owner, _)) in OVERRIDES.get().into_iter().flatten() {
        if index_entry(device, &format!(":/{rel}")).is_none() {
            // Another archive's file, or one no archive has.
            continue;
        }
        if make_plain(device, rel, false) {
            kindle_core::debug!("files: the archive's entry for {rel} reads {owner}'s file as it is");
        } else {
            kindle_core::warn!("files: {rel}'s archive entry couldn't be changed; the game may read {owner}'s file wrong");
        }
    }
    let wanted: Vec<String> = lock(&WANTED).iter().cloned().collect();
    for rel in wanted {
        if make_plain(device, &rel, true) {
            lock(&WANTED).remove(&rel);
        }
    }
}

/// A mod changes `map`'s events (kindle_core::event_patches::on_change): Kindle serves its event file
/// from now on (its next load has the changes). A mod's own file for it wins over the changes.
pub fn events_changed(map: &str) {
    let rel = format!("event/{map}.emevd.dcx");
    if overridden(&rel) {
        kindle_core::warn!("files: a mod replaces {rel}, so mods' changes to its events aren't used");
        return;
    }
    lock(&WANTED).insert(rel.clone());
    let devices: Vec<usize> = lock(&MOUNTS).iter().map(|(d, _)| *d).collect();
    if devices.into_iter().any(|d| make_plain(d, &rel, true)) {
        lock(&WANTED).remove(&rel);
    }
}

/// The game's own file at `rel` ("event/m60_42_36_00.emevd.dcx") as its archive stores it,
/// decrypted (still packed).
pub fn read_original(rel: &str) -> Result<Vec<u8>, String> {
    let rel = key(rel);
    let game_dir = GAME_DIR.get().ok_or("the game's folder isn't known")?;
    let mounts = lock(&MOUNTS).clone();
    if mounts.is_empty() {
        return Err("the game hasn't opened its archives yet".into());
    }
    let was = lock(&PLAIN).as_ref().and_then(|p| p.get(&rel).copied());
    let unreadable = || "the archive's index is unreadable".to_string();
    for (device, mount) in mounts {
        let Some(entry) = index_entry(device, &format!(":/{rel}")) else { continue };
        let aes = match was {
            Some(was) if was.entry == entry => was.aes,
            _ => memory::read_u64(entry + ENTRY_AES_RANGES).ok_or_else(unreadable)?,
        };
        let base = memory::read_u64(device + INDEX_BASE).ok_or_else(unreadable)? as usize;
        let stored = crate::archive::stored(base, entry, aes)?;
        let bdt = crate::archive::bdt_of(game_dir, &mount).ok_or_else(|| format!("the game's folder has no {mount}.bdt"))?;
        return crate::archive::read(&bdt, &stored);
    }
    Err(format!("no archive of the game's has {rel}"))
}

/// The event file Kindle serves for `rel`: the game's with mods' changes, or as it is without any,
/// made once in kindle\cache\event (named by the game's file and the changes).
fn generated(rel: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    let cache = CACHE_DIR.get().ok_or("Kindle's cache folder isn't known")?;
    let map = kindle_core::event_patches::map_of(rel).ok_or("not an event file")?;
    let original = read_original(rel)?;
    let version = kindle_core::event_patches::version(rel);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    original.hash(&mut h);
    let file = cache.join("event").join(format!("{map}.{:016x}.{}.emevd.dcx", h.finish(), version.as_deref().unwrap_or("game")));
    if file.is_file() {
        return Ok(file);
    }
    let made = match version {
        None => original,
        Some(_) => {
            let raw = crate::archive::undcx(&original)?;
            match kindle_core::event_patches::patched(rel, &raw)? {
                Some(made) => crate::archive::dcx(&original, &made)?,
                // The changes were dropped meanwhile.
                None => original,
            }
        }
    };
    let dir = file.parent().ok_or("no cache folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // Written whole before it's named, so a cut-off file is never served.
    let part = file.with_extension(format!("{}.part", std::process::id()));
    std::fs::write(&part, &made).map_err(|e| format!("{}: {e}", part.display()))?;
    std::fs::rename(&part, &file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

/// Opens a mod's file instead, through the disk device, if one replaces this path, or Kindle's for
/// an event file it serves: the device's own arguments passed on (the request and allocator), its
/// flag 0 (a file).
fn redirect(text: &str, path: usize, b: usize, request: usize, allocator: usize) -> Option<usize> {
    let rel = key(game_path(text));
    let events = OVERRIDES.get().is_none_or(|o| !o.contains_key(&rel));
    let (owner, file) = if events {
        if !lock(&PLAIN).as_ref()?.get(&rel)?.events {
            return None;
        }
        match generated(&rel) {
            Ok(file) => ("Kindle", file.as_os_str().to_string_lossy().encode_utf16().collect::<Vec<u16>>()),
            Err(e) => {
                kindle_core::warn!("files: couldn't make {rel} ({e}); the game reads its own");
                stop_serving(&rel);
                return None;
            }
        }
    } else {
        let (owner, file) = OVERRIDES.get()?.get(&rel)?;
        (owner.as_str(), file.clone())
    };
    let disk = DISK.load(Ordering::Acquire);
    if disk == 0 {
        kindle_core::warn!("files: {rel} was opened before the disk device was; using the game's");
        if events {
            stop_serving(&rel);
        }
        return None;
    }
    let ours = game_string(&rel, &file, memory::read_u64(path + ALLOCATOR)? as usize);
    let open: OpenFn = unsafe { std::mem::transmute(DISK_OPEN.load(Ordering::Acquire)) };
    let op = open(disk, ours, b, request, allocator, 0, 0, 0, 0);
    if op == 0 {
        kindle_core::warn!("files: couldn't open {owner}'s {rel}; using the game's");
        if events {
            stop_serving(&rel);
        }
        return None;
    }
    if events && kindle_core::event_patches::version(&rel).is_some() {
        kindle_core::info!("files: {rel} with mods' changes to its events");
    } else if events {
        kindle_core::debug!("files: {rel} as the game's (no changes to its events now)");
    } else {
        kindle_core::info!("files: {rel} from {owner}");
    }
    Some(op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        assert_eq!(key("/Menu\\Hi\\00_Solo.tpfbhd"), "menu/hi/00_solo.tpfbhd");
        assert_eq!(key(game_path("data0:/menu/hi/00_solo.tpfbhd")), "menu/hi/00_solo.tpfbhd");
        assert_eq!(key(game_path("data3:/msg/engus/item_dlc02.msgbnd.dcx")), "msg/engus/item_dlc02.msgbnd.dcx");
    }

    #[test]
    fn strings_are_laid_out_as_the_games() {
        let text: Vec<u16> = "C:\\mods\\m\\files\\regulation.bin".encode_utf16().collect();
        let s = game_string("test/strings", &text, 0x1234);
        assert_eq!(memory::read_u64(s + ALLOCATOR), Some(0x1234));
        assert_eq!(read_string(s).as_deref(), Some(&text[..]));
        // Built once per path and text; another text gets another string.
        assert_eq!(game_string("test/strings", &text, 0), s);
        let other = game_string("test/strings", &text[..3], 0);
        assert_ne!(other, s);
        assert_eq!(read_string(other).as_deref(), Some(&text[..3]));
        // A short string's characters are inline.
        let mut short = [0u64; STRING_SIZE / 8];
        let chars: Vec<u16> = "ab".encode_utf16().collect();
        short[CHARS / 8] = u32::from(chars[0]) as u64 | (u32::from(chars[1]) as u64) << 16;
        short[LENGTH / 8] = 2;
        short[CAPACITY / 8] = 7;
        assert_eq!(read_string(short.as_ptr() as usize).as_deref(), Some(&chars[..]));
    }

    #[test]
    fn paths_hash_as_the_archives_index() {
        // The game's archive index holds /menu/hi/00_solo.tpfbhd under this hash.
        assert_eq!(path_hash("data0:/menu/hi/00_solo.tpfbhd"), Some(4262951649920129589));
        assert_eq!(path_hash("data0:\\MENU\\Hi\\00_solo.tpfbhd"), path_hash("data0:/menu/hi/00_solo.tpfbhd"));
        assert_eq!(path_hash("no root"), None);
    }

    #[test]
    fn entries_are_found_as_the_device_finds_them() {
        let hash = path_hash("data0:/regulation.bin").unwrap();
        // One bucket of two entries 16 bytes past the base, the one wanted second.
        let mut base = [0u64; 16];
        base[2] = 7;
        base[2 + ENTRY_SIZE / 8] = hash;
        let table: [u32; 2] = [2, 16];
        let mut device = [0u64; 0x100 / 8];
        device[INDEX_BASE / 8] = base.as_ptr() as u64;
        device[INDEX_BUCKETS / 8] = 1;
        device[INDEX_BUCKET_TABLE / 8] = table.as_ptr() as u64;
        assert_eq!(index_entry(device.as_ptr() as usize, "data0:/regulation.bin"), Some(base.as_ptr() as usize + 16 + ENTRY_SIZE));
        assert_eq!(index_entry(device.as_ptr() as usize, "data0:/other.bin"), None);
    }

    /// The classes' vtables in Elden Ring 2.7.0.0's exe (KINDLE_ER_EXE; Kindle finds them by their RTTI
    /// names), and the slots Kindle swaps: the archive device's open (which hashes the path after its
    /// root), the disk device's and the disk operator's set path.
    #[test]
    fn the_games_file_devices_are_found() {
        let Some((img, base, _)) = crate::singletons::tests::eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let u64_at = |rva: usize| u64::from_le_bytes(img[rva..rva + 8].try_into().unwrap()) as usize;
        let u32_at = |rva: usize| u32::from_le_bytes(img[rva..rva + 4].try_into().unwrap()) as usize;
        for (class, vtable, slot, function) in [
            (ARCHIVE_DEVICE, 0x3203820, OPEN_SLOT, 0x2429730),
            (DISK_DEVICE, 0x30D2740, OPEN_SLOT, 0x1F91830),
            (DISK_OPERATOR, 0x30DA088, SET_PATH_SLOT, 0x1FC2B70),
            (ARCHIVE_OBJECT, 0x32041C0, SETUP_SLOT, 0x242B2C0),
        ] {
            // The complete object locator before the vtable names the class.
            let locator = u64_at(vtable - 8) - base as usize;
            let descriptor = u32_at(locator + 12);
            let name = format!(".?AV{class}@@");
            assert_eq!(&img[descriptor + 0x10..descriptor + 0x10 + name.len()], name.as_bytes(), "{class}");
            assert_eq!(u64_at(vtable + slot * 8) - base as usize, function, "{class}");
        }
    }

    #[test]
    fn first_mod_wins() {
        let dir = std::env::temp_dir().join(format!("kindle-er-files-test-{}", std::process::id()));
        for (m, f) in [("a", "menu/hi/00_solo.tpfbhd"), ("b", "menu/hi/00_solo.tpfbhd"), ("b", "regulation.bin")] {
            let p = dir.join(m).join("files").join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, m).unwrap();
        }
        let map = collect(&dir, &["a".to_string(), "b".to_string()]);
        assert_eq!(map["menu/hi/00_solo.tpfbhd"].0, "a");
        assert_eq!(map["regulation.bin"].0, "b");
        let _ = std::fs::remove_dir_all(dir);
    }
}
