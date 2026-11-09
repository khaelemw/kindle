//! Replacement game files from mods: a mod's `files\<game path>` (e.g.
//! `files\chr\c1100.chrbnd.dcx`) is loaded instead of the game's own, without Mod Engine's
//! file loader.
//!
//! The game reads its data archives (Data0-5.bdt, DLC) through the encrypted-binder file
//! device, `DLEncryptedBinderLightFileDevice`; loose files go through the disk device,
//! `MicrosoftDiskFileDevice`. Both implement `open(device, path, chars, &request, allocator,
//! x) -> file operator` (vtable slot 1; seen live): `path` is the game's wide string ({data,
//! ?, length, capacity}), e.g. `data1:/chr/c1100.chrbnd.dcx` for the archive device and an
//! absolute path for the disk device, `chars` points into its characters, the request starts
//! with the allocator, and `x` is 0 for the disk device (a pointer for the archive's).
//!
//! Kindle swaps both slots (data only, like the damage hook): the disk device's to learn the
//! device (the game opens files on disk at startup, before any archive), the archive
//! device's to send a path a mod replaces to the disk device instead, with a string of
//! Kindle's holding the mod file's absolute path (laid out by hand, once per file, and kept
//! for the session: the game's string functions can't be used on its file-loading threads,
//! and the caller's string isn't touched). If the disk device can't open it, the game's own
//! file is used. Verified 2026-10-04: a menu text file at startup and an event file on
//! loading a save, both served from a mod folder.
//!
//! The device's open only creates the file operator; the file is opened later by the
//! operator's own open (slot 20, `CreateFileW` for the disk operator, with flags such as
//! 0xA9). Some loaders, e.g. the one that preloads the menus' Scaleform movies (+0x18B2980 →
//! +0x793630), set the operator's path again after the device's open (slot 2, set path: the
//! game string at operator +0x30, refused once the file is open), with the archive path. On
//! a disk operator that path can't open (error -4) and the menu doesn't build, so Kindle also
//! swaps the disk operator's set path and puts its string back for paths a mod replaces.
//! Verified 2026-10-04 by tracing the operators' calls: the pause menu movie
//! (`menu/02_000_ingametop.gfx`) failed at slot 20 because of this.
//!
//! Map files (`map/...`) are served differently. Their loader reads the 0x30-byte DCX header,
//! checks the operator's own state, and closes a disk operator again (or one whose reads
//! Kindle answers itself), then waits for good: High Wall's map file hung the loading screen,
//! even unchanged (2026-10-05). So for those the game opens and reads its own file through the
//! archive as usual, and Kindle copies the replacement over each read's bytes, at the same
//! offsets (the archive operator's read, slot 25: `read(operator, buffer, count) -> count`,
//! sequential, seen synchronous). Those reads come back as the archive stores the file: some
//! ranges are still AES-encrypted, and the loader decrypts them afterwards. So the replacement
//! is padded to the original's length and encrypted the same way (archive::as_stored) before
//! it's copied (DCX data past the replacement's compressed stream is ignored). The archive
//! reader also checks a hash of a few ranges of the stored bytes (archive::check_hash): an
//! edited file fails it, so Kindle finds the expected hash in the game's copy of the archive
//! index (writable memory; two copies were seen) and puts the replacement's in its place before
//! the file is read. A replacement longer than the original gets its sizes changed in that
//! same index entry too: the game then reads that many bytes (past the original, into what
//! follows it in the archive's data file) and every one is Kindle's; for the data file's last
//! file the read starts early enough to end inside it (what's read is never used). Verified
//! with a 2.8 MB High Wall map file in place of the 195 KB original, and a 461 KB Filianore's
//! Rest map (DLC2's last file) in place of its 35 KB (2026-10-05). Traced on a normal map load: open, header read (25), is open (19), size (13), the
//! rest in chunks (25), close (21), destroy (0). Where a read starts is the operator's own read
//! position (+0x6C, after its data offset in the archive (+0x60) and its size (+0x68)), which its
//! read, seek (23) and open keep. A split binder's data file (`.tpfbdt`, `.hkxbdt`) is read
//! through copies of its operator: the binder's data reader copies the file handle it's given
//! (+0x1793250), and a handle's copy holds a copy of the operator (slot 7: a new operator for the
//! same file at the same place, with the game's vtable), so copies get the overlay too.
//!
//! Overrides are read once, when Kindle starts (enabled mods, in name order; the first mod to
//! ship a file wins and the others are reported).
//!
//! Kindle's own edits of game files (menu_movies.rs) are served the same way: at the first
//! open, the original is read from its archive (archive.rs), edited and written to
//! `kindle\cache\<game path>`. A mod that replaces the same file wins over Kindle's edit.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::menu_movies::MenuMovie;
use crate::{archive, hooks, memory, patterns};

const OPEN_SLOT: usize = 1;
/// The file operator's set path: `(operator, path, flag) -> bool`.
const SET_PATH_SLOT: usize = 2;
const ARCHIVE_DEVICE: &str = "DLEncryptedBinderLightFileDevice@DLEBL";
const DISK_DEVICE: &str = "MicrosoftDiskFileDevice@msvc90_windows@DLIOD";
const DISK_OPERATOR: &str = "MicrosoftDiskFileOperator@msvc90_windows@DLIOD";

/// Lowercase game path ("chr/c1100.chrbnd.dcx") → (mod id, the file, as UTF-16).
static OVERRIDES: OnceLock<HashMap<String, (String, Vec<u16>)>> = OnceLock::new();
static ARCHIVE_OPEN: AtomicUsize = AtomicUsize::new(0);
static DISK_OPEN: AtomicUsize = AtomicUsize::new(0);
static DISK_SET_PATH: AtomicUsize = AtomicUsize::new(0);
/// The disk device, seen in its first `open`.
static DISK: AtomicUsize = AtomicUsize::new(0);
/// Kindle's edits of game files, by game path.
static GENERATED: OnceLock<Generated> = OnceLock::new();
/// The game strings Kindle built for mod files, by game path (never freed).
static PATHS: std::sync::Mutex<Option<HashMap<String, usize>>> = std::sync::Mutex::new(None);
/// The game's wide string: 16-byte inline buffer or pointer, length, capacity, allocator.
const STRING_SIZE: usize = 0x28;

/// The archive file operator's slots Kindle wraps for overlaid files.
const DESTROY_SLOT: usize = 0;
/// A copy of the operator: `(operator) -> operator`.
const COPY_SLOT: usize = 7;
const READ_SLOT: usize = 25;
/// The archive operator's read position in its file (u32).
const POSITION: usize = 0x6C;
/// How much of the operator's vtable the copy takes (it has fewer slots than this).
const VTABLE_COPY: usize = 64;
/// The game's archive operator vtable, and Kindle's copy of it.
static ARCHIVE_VTABLE: AtomicUsize = AtomicUsize::new(0);
static OVERLAY_VTABLE: OnceLock<usize> = OnceLock::new();
/// Operators reading an overlaid file: operator → the replacement.
static OVERLAID: std::sync::Mutex<Option<HashMap<usize, std::sync::Arc<Bytes>>>> = std::sync::Mutex::new(None);
/// Replacements read so far (None: unusable), by game path, with which version each is (a mod's
/// file, or one with mods' changes: another character can need other changes).
type Overlay = Option<std::sync::Arc<Bytes>>;
static OVERLAYS: std::sync::Mutex<Option<HashMap<String, (String, Overlay)>>> = std::sync::Mutex::new(None);

/// A replacement's bytes: made in memory (as the archive stores the file it replaces), or a mod's
/// new file mapped as it is, which the system reads as the game reads it (a texture pack's data,
/// a hundred megabytes and more, is read in parts as its textures are wanted).
enum Bytes {
    Made(Vec<u8>),
    Mapped(Mapped),
}

impl std::ops::Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Made(v) => v,
            Bytes::Mapped(m) => m,
        }
    }
}

/// A file mapped into memory, read only.
struct Mapped {
    view: usize,
    len: usize,
}

// The view is only read, and stays until the value is dropped.
unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

impl Mapped {
    fn open(path: &Path) -> Result<Mapped, String> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::{CloseHandle, HANDLE};
        use windows::Win32::System::Memory::{CreateFileMappingW, FILE_MAP_READ, MapViewOfFile, PAGE_READONLY};
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len() as usize;
        if len == 0 {
            return Ok(Mapped { view: 0, len: 0 });
        }
        let mapping =
            unsafe { CreateFileMappingW(HANDLE(file.as_raw_handle()), None, PAGE_READONLY, 0, 0, windows::core::PCWSTR::null()) }.map_err(|e| e.to_string())?;
        // The view keeps the mapping, and the mapping the file.
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
        let _ = unsafe { CloseHandle(mapping) };
        if view.Value.is_null() {
            return Err("the file couldn't be mapped".into());
        }
        Ok(Mapped { view: view.Value as usize, len })
    }
}

impl std::ops::Deref for Mapped {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        if self.len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(self.view as *const u8, self.len) } }
    }
}

impl Drop for Mapped {
    fn drop(&mut self) {
        use windows::Win32::System::Memory::{MEMORY_MAPPED_VIEW_ADDRESS, UnmapViewOfFile};
        if self.len != 0 {
            let _ = unsafe { UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS { Value: self.view as *mut _ }) };
        }
    }
}

/// What Kindle changed in the game's archive index for a replacement, to put back when the game's
/// own file (or another version) is wanted again: where the check hash sits and its old value,
/// and the entry with its old padded size, offset and size.
struct Patched {
    hash_at: usize,
    hash: [u8; 32],
    entry: usize,
    padded: i32,
    offset: u64,
    size: u64,
}

static PATCHED: std::sync::Mutex<Option<HashMap<String, Vec<Patched>>>> = std::sync::Mutex::new(None);

/// What a changed map file depends on: the changes (`version`, their fingerprint), the game build
/// and this Kindle build.
fn made_key(version: &str) -> String {
    static BUILDS: OnceLock<String> = OnceLock::new();
    let builds = BUILDS.get_or_init(|| {
        let game = memory::game_module().and_then(|(b, s)| patterns::pe_identity(b, s)).unwrap_or_default();
        let kindle = memory::own_module().and_then(|(b, s)| patterns::pe_identity(b, s)).unwrap_or_default();
        format!("game {game}; kindle {kindle}")
    });
    format!("{version}; {builds}")
}

fn key_file(made: &Path) -> PathBuf {
    let mut name = made.as_os_str().to_owned();
    name.push(".key");
    PathBuf::from(name)
}

/// A changed map file kept in kindle\cache from an earlier session, if it was made with the same
/// changes, game and Kindle (making one takes 0.6-3 s: reading, unpacking, packing).
fn kept_map(rel: &str, version: &str) -> Option<Vec<u8>> {
    let file = GENERATED.get()?.cache_dir.join(rel);
    (std::fs::read_to_string(key_file(&file)).ok()? == made_key(version)).then(|| std::fs::read(&file).ok()).flatten()
}

/// Where each archive's index copies start in the game's memory, by the index's header.
static INDEX_COPIES: std::sync::Mutex<Option<HashMap<Vec<u8>, Vec<usize>>>> = std::sync::Mutex::new(None);

/// The places of this file's check hash in its archive's index copies found before, those that
/// still hold the index and the hash (the game keeps its archive device's copy for the session).
fn known_index(check: &archive::CheckHash) -> Option<Vec<usize>> {
    let starts = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&check.header)?.clone();
    let holds = |s: usize| {
        memory::read_bytes(s, check.header.len()).is_some_and(|h| h == check.header)
            && memory::read_bytes(s + check.at, check.old.len()).is_some_and(|h| h == check.old)
    };
    let index: Vec<usize> = starts.into_iter().filter(|&s| holds(s)).map(|s| s + check.at).collect();
    (!index.is_empty()).then_some(index)
}

/// The starts of the index copies whose first 16 bytes are `header`, found by the search (waited
/// for) or, if it missed them, by a search of the game's memory now.
pub fn index_copies(header: &[u8]) -> Option<Vec<usize>> {
    wait_for_index_search();
    let holds = |s: usize| memory::read_bytes(s, header.len()).is_some_and(|h| h == header);
    let known: Vec<usize> = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(header).cloned()).unwrap_or_default();
    let known: Vec<usize> = known.into_iter().filter(|&s| holds(s)).collect();
    if !known.is_empty() {
        return Some(known);
    }
    let found: Vec<usize> = memory::find_writable(&header[..4]).into_iter().filter(|&s| holds(s)).collect();
    if found.is_empty() {
        return None;
    }
    INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(header.to_vec(), found.clone());
    Some(found)
}

/// A search for the archives' index copies is running (`find_index_copies`).
static FINDING_INDEX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// How long a file waits for that search before searching itself.
const INDEX_WAIT: std::time::Duration = std::time::Duration::from_secs(4);

/// Finds the index copies of every archive whose index Kindle can read and hasn't found yet, in
/// one search of the game's memory on a thread of its own (for each index header ("BHD5" ...,
/// its size): the copies starting with it). Searched for by the first file Kindle replaced in
/// an archive, that took 1.5 s (all the game's memory) on the load that needed it, the first
/// load into the world of every session (verified 2026-10-11: 17.4 -> 18.95 s before the
/// session's first event file). Run when the game's data is ready (the title screen) and when
/// the archive keys are all in (the DLCs' show after the title screen).
pub fn find_index_copies() {
    use std::sync::atomic::Ordering::Relaxed;
    let Some(game_dir) = GENERATED.get().map(|g| g.game_dir.clone()) else { return };
    if FINDING_INDEX.swap(true, Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new().name("kindle-index".into()).spawn(move || {
        let start = std::time::Instant::now();
        let known: Vec<Vec<u8>> = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|m| m.keys().cloned().collect()).unwrap_or_default();
        let pending: Vec<Vec<u8>> = archive::ARCHIVES.iter().filter_map(|a| archive::index_header(&game_dir, a).ok()).filter(|h| !known.contains(h)).collect();
        if !pending.is_empty() {
            let hits = memory::find_writable(b"BHD5");
            let mut found = 0;
            let mut copies = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner());
            let map = copies.get_or_insert_with(HashMap::new);
            for h in &pending {
                let starts: Vec<usize> = hits.iter().copied().filter(|&a| memory::read_bytes(a, h.len()).is_some_and(|b| b == *h)).collect();
                if !starts.is_empty() {
                    map.insert(h.clone(), starts);
                    found += 1;
                }
            }
            crate::debug!("files: archive index copies found for {found} of {} archives ({:.1}s)", pending.len(), start.elapsed().as_secs_f32());
        }
        FINDING_INDEX.store(false, Relaxed);
    });
    if spawned.is_err() {
        FINDING_INDEX.store(false, Relaxed);
    }
}

/// Waits (up to INDEX_WAIT) for a running search for the index copies.
fn wait_for_index_search() {
    let since = std::time::Instant::now();
    while FINDING_INDEX.load(std::sync::atomic::Ordering::Relaxed) && since.elapsed() < INDEX_WAIT {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn remember_index(check: &archive::CheckHash, index: &[usize]) {
    let mut copies = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner());
    copies.get_or_insert_with(HashMap::new).insert(check.header.clone(), index.iter().map(|at| at - check.at).collect());
}

/// The index copy at `old` (`size` bytes) was copied to `new`, which the game reads from now on
/// (archive_entries.rs): Kindle's changes to the index go to the new copy too, and what it changed
/// in the old one is put back in both.
pub fn index_moved(header: &[u8], old: usize, size: usize, new: usize) {
    let mut copies = INDEX_COPIES.lock().unwrap_or_else(|e| e.into_inner());
    let starts = copies.get_or_insert_with(HashMap::new).entry(header.to_vec()).or_default();
    if !starts.contains(&new) {
        starts.push(new);
    }
    drop(copies);
    let moved = |at: usize| (old..old + size).contains(&at).then(|| at - old + new);
    if let Some(patched) = PATCHED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        for patches in patched.values_mut() {
            let copied: Vec<Patched> = patches
                .iter()
                .filter_map(|p| {
                    Some(Patched { hash_at: moved(p.hash_at)?, hash: p.hash, entry: moved(p.entry)?, padded: p.padded, offset: p.offset, size: p.size })
                })
                .collect();
            patches.extend(copied);
        }
    }
}

/// Puts the game's own values back in its archive index for `rel`, if Kindle changed them.
fn unpatch(rel: &str) {
    let Some(patches) = PATCHED.lock().unwrap_or_else(|e| e.into_inner()).as_mut().and_then(|m| m.remove(rel)) else { return };
    for p in &patches {
        unsafe {
            std::ptr::copy_nonoverlapping(p.hash.as_ptr(), p.hash_at as *mut u8, p.hash.len());
            std::ptr::write_unaligned((p.entry + 4) as *mut i32, p.padded);
            std::ptr::write_unaligned((p.entry + 8) as *mut u64, p.offset);
            std::ptr::write_unaligned((p.entry + 0x20) as *mut u64, p.size);
        }
    }
    crate::debug!("files: {rel}: the game's own check hash and sizes put back");
}

/// Game paths whose replacements are copied over the game's own reads. A block's AI file
/// (`script/<block>.luabnd.dcx`) is read with the map's and hangs the loading screen as a map
/// file did when served from disk, even unchanged (m21's, 2026-10-26).
fn overlaid(rel: &str) -> bool {
    rel.starts_with("map/") || rel.starts_with("event/") || rel.ends_with(".luabnd.dcx")
}

/// The lookup key for a game path: forward slashes, lowercase, no leading slashes.
/// Whether a mod replaces the game file at `path` ("/chr/c1100.chrbnd.dcx").
pub fn overridden(path: &str) -> bool {
    OVERRIDES.get().is_some_and(|m| m.contains_key(&key(path)))
}

/// The file a mod replaces the game file at `path` with, if one does.
pub fn replacement(path: &str) -> Option<PathBuf> {
    let (_, wide) = OVERRIDES.get()?.get(&key(path))?;
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    Some(PathBuf::from(String::from_utf16_lossy(&wide[..end])))
}

fn key(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_lowercase()
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
                Some((owner, _)) => crate::warn!("files: {id} and {owner} both replace {k}; using {owner}'s"),
                None => {
                    map.insert(k, (id.clone(), path));
                }
            }
        }
    }
    map
}

fn swap(class: &str, index: usize, hook: usize, original: &AtomicUsize) -> bool {
    let Some(vt) = patterns::find_vtable(class) else { return false };
    let slot = vt + index * 8;
    let Some(o) = memory::read_u64(slot).map(|v| v as usize) else { return false };
    if o == hook {
        return true;
    }
    original.store(o, Ordering::Release);
    unsafe { hooks::write_ptr(slot, hook) }
}

struct Generated {
    game_dir: PathBuf,
    cache_dir: PathBuf,
    /// Game path → the edit, and its file once made.
    movies: HashMap<String, (MenuMovie, MadeFile)>,
}

/// A generated file's path (UTF-16) once made, None if it couldn't be.
type MadeFile = OnceLock<Option<Vec<u16>>>;

/// Finds the mods' replacement files and, if there are any (or Kindle edits some of the game's
/// `movies`), takes over the archive device's open. Call early (before the game opens what
/// mods replace).
pub fn install(mods_dir: &Path, disabled: &BTreeSet<String>, game_dir: &Path, cache_dir: &Path, movies: Vec<MenuMovie>) {
    let mut ids: Vec<String> = std::fs::read_dir(mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("files").is_dir())
        // Off in its mod.toml, for another game or switched off (the list holds mod ids, as for its scripts).
        .filter(|e| {
            crate::config::ModManifest::load(&e.path().join("mod.toml"))
                .is_ok_and(|m| m.info.enabled && m.info.for_game(crate::api::BINDINGS.game) && !disabled.contains(&m.info.id))
        })
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.') && !n.starts_with('_'))
        .collect();
    ids.sort();
    let map = collect(mods_dir, &ids);
    let mut generated = HashMap::new();
    for movie in movies {
        match map.get(movie.path) {
            Some((owner, _)) => crate::warn!("files: {owner} replaces {}, so {} is off", movie.path, movie.feature),
            None => {
                generated.insert(movie.path.to_string(), (movie, OnceLock::new()));
            }
        }
    }
    // Hooked even with no mod files: mods' map and event changes are made as areas load.
    let count = map.len();
    let _ = GENERATED.set(Generated { game_dir: game_dir.to_path_buf(), cache_dir: cache_dir.to_path_buf(), movies: generated });
    let wide: HashMap<String, (String, Vec<u16>)> =
        map.into_iter().map(|(k, (id, p))| (k, (id, p.as_os_str().to_string_lossy().encode_utf16().collect()))).collect();
    let _ = OVERRIDES.set(wide);
    // The disk hook first: archive opens are only redirected once the disk device is known.
    if !swap(DISK_DEVICE, OPEN_SLOT, disk_open as *const () as usize, &DISK_OPEN)
        || !swap(ARCHIVE_DEVICE, OPEN_SLOT, archive_open as *const () as usize, &ARCHIVE_OPEN)
    {
        crate::warn!("files: the game's file devices weren't found; mods' files won't replace the game's");
        return;
    }
    if !swap(DISK_OPERATOR, SET_PATH_SLOT, disk_set_path as *const () as usize, &DISK_SET_PATH) {
        crate::warn!("files: the disk file operator wasn't found; mods' menu files won't replace the game's");
    }
    if count > 0 {
        crate::info!("files: {count} replacement file(s) from mods");
    }
}

/// Text of one of the game's wide strings (inline buffer under 8 characters, else a pointer
/// at +0; length at +0x10, capacity at +0x18).
fn read_string(s: usize) -> Option<Vec<u16>> {
    let len = memory::read_u64(s + 0x10)? as usize;
    let cap = memory::read_u64(s + 0x18)? as usize;
    if len > 4096 || cap < len {
        return None;
    }
    let data = if cap < 8 { s } else { memory::read_u64(s)? as usize };
    let bytes = memory::read_bytes(data, len * 2)?;
    Some(bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect())
}

type OpenFn = extern "C" fn(usize, usize, usize, usize, usize, usize, usize, usize, usize) -> usize;

/// A game string holding `text`, built once per key and kept for the session: the
/// characters (NUL-terminated) in Kindle's memory, length, capacity (over the inline size, so
/// the pointer is used) and `allocator`. Built by hand: the game's string functions can't be
/// used on the file-loading threads.
fn game_string(key: &str, text: &[u16], allocator: usize) -> usize {
    let mut guard = PATHS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    *map.entry(key.to_string()).or_insert_with(|| {
        let chars: &'static [u16] = Box::leak(text.iter().copied().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
        let len = text.len() as u64;
        let fields: [u64; STRING_SIZE / 8] = [chars.as_ptr() as u64, 0, len, len.max(8), allocator as u64];
        Box::leak(Box::new(fields)).as_ptr() as usize
    })
}

/// The game path of an archive path ("data1:/chr/c1100.chrbnd.dcx" → "chr/c1100.chrbnd.dcx").
fn game_path(text: &str) -> &str {
    text.split_once(":/").map(|(_, rest)| rest).unwrap_or(text)
}

// Both hooks forward extra arguments: unused ones are harmless, and the exact count isn't
// known for every device.
#[allow(clippy::too_many_arguments)]
extern "C" fn disk_open(this: usize, a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    DISK.store(this, Ordering::Release);
    let open: OpenFn = unsafe { std::mem::transmute(DISK_OPEN.load(Ordering::Acquire)) };
    open(this, a, b, c, d, e, f, g, h)
}

#[allow(clippy::too_many_arguments)]
extern "C" fn archive_open(this: usize, path: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let original: OpenFn = unsafe { std::mem::transmute(ARCHIVE_OPEN.load(Ordering::Acquire)) };
    if let Ok(Some((rel, overlay))) = std::panic::catch_unwind(|| overlay_for(path)) {
        let op = original(this, path, b, c, d, e, f, g, h);
        if let Some(data) = overlay
            && op != 0
            && std::panic::catch_unwind(|| overlay_operator(op, data)).unwrap_or(false)
        {
            crate::debug!("files: {rel} read through the game's file");
        }
        return op;
    }
    let redirected = std::panic::catch_unwind(|| redirect(path, c, d)).ok().flatten();
    redirected.unwrap_or_else(|| original(this, path, b, c, d, e, f, g, h))
}

/// For an archive path whose replacement is overlaid: (game path, the replacement, or None if
/// it can't be used, so the game's own file is read). None for other paths.
fn overlay_for(path: usize) -> Option<(String, Overlay)> {
    let text = String::from_utf16_lossy(&read_string(path)?);
    let rel = key(game_path(&text));
    if !overlaid(&rel) {
        return None;
    }
    // An area's map or event file: mods hear of it first (map_loading.rs), so what they change
    // for this load is in place. Then a mod's replacement, else the map file with mods' changes
    // (map_edits.rs), else the event file with mods' patches (event_patches.rs).
    let map = crate::map_edits::map_of(&rel).is_some();
    if let Some(area) = crate::map_edits::map_of(&rel) {
        crate::map_loading::wait(area, "map");
    } else if let Some(area) = crate::event_patches::map_of(&rel).filter(|m| crate::map_edits::valid_map(m)) {
        crate::map_loading::wait(area, "events");
    }
    let changes = if map { crate::map_edits::version(&rel) } else { crate::event_patches::version(&rel) };
    let source = OVERRIDES.get().and_then(|o| o.get(&rel)).map(|(owner, file)| (owner.clone(), Some(file.clone())));
    let source = source.or_else(|| changes.as_ref().map(|_| ("Kindle".to_string(), None)));
    let version = match &source {
        Some((owner, Some(_))) => owner.clone(),
        Some((_, None)) if map => format!("maps {}", changes.clone().unwrap_or_default()),
        Some((_, None)) => format!("events {}", changes.clone().unwrap_or_default()),
        None => String::new(),
    };
    let mut cache = OVERLAYS.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    match cache.get(&rel) {
        Some((v, found)) if *v == version => return Some((rel, found.clone())),
        // Another version (another character's) was served: the index gets the game's values back.
        Some(_) => {
            cache.remove(&rel);
            unpatch(&rel);
        }
        None => {}
    }
    let (owner, file) = source?;
    let archive = text.split_once(":/").map(|(a, _)| a).unwrap_or("");
    let path = format!("/{rel}");
    let game_dir = GENERATED.get().map(|g| g.game_dir.clone());
    // A mod's file at a path no archive has (a new map's): the archive's index gets an entry for
    // it, read as it is (no hash, nothing encrypted), and the file is served mapped.
    if let (Some(file), Some(game_dir)) = (&file, &game_dir)
        && archive::stored_len(game_dir, archive, &path).is_err()
    {
        let loaded = Mapped::open(&PathBuf::from(String::from_utf16_lossy(file))).and_then(|mapped| {
            crate::archive_entries::add(game_dir, archive, &path, mapped.len())?;
            Ok(mapped)
        });
        let loaded = match loaded {
            Ok(mapped) => {
                crate::info!("files: {rel} from {owner} (a new file)");
                Some(std::sync::Arc::new(Bytes::Mapped(mapped)))
            }
            Err(e) => {
                crate::warn!("files: can't add {owner}'s new {rel} ({e})");
                None
            }
        };
        cache.insert(rel.clone(), (version, loaded.clone()));
        return Some((rel, loaded));
    }
    // A mod's file the archive stores as it is (no encrypted ranges: texture and collision data,
    // a hundred megabytes and more) is served mapped too, with the zeros the archive would pad it
    // with (reads past its end get zeros, and the check hash counts them).
    if let (Some(file), Some(game_dir)) = (&file, &game_dir)
        && archive::encrypted(game_dir, archive, &path) == Ok(false)
    {
        let loaded = Mapped::open(&PathBuf::from(String::from_utf16_lossy(file))).and_then(|mapped| {
            let room = archive::stored_len(game_dir, archive, &path)?;
            let padded = if mapped.len() > room { mapped.len().div_ceil(16) * 16 } else { room };
            fit_index(game_dir, archive, &path, &rel, &mapped, padded, mapped.len())?;
            Ok(mapped)
        });
        let loaded = match loaded {
            Ok(mapped) => {
                crate::info!("files: {rel} from {owner}");
                Some(std::sync::Arc::new(Bytes::Mapped(mapped)))
            }
            Err(e) => {
                crate::warn!("files: can't use {owner}'s {rel} ({e}); using the game's");
                None
            }
        };
        cache.insert(rel.clone(), (version, loaded.clone()));
        return Some((rel, loaded));
    }
    let kept = if file.is_none() && map { kept_map(&rel, &version) } else { None };
    let bytes = match (&file, &game_dir, kept) {
        (Some(file), _, _) => std::fs::read(PathBuf::from(String::from_utf16_lossy(file))).map_err(|e| e.to_string()),
        // A changed map made in an earlier session with the same changes (see kept_map).
        (None, Some(_), Some(kept)) => Ok(kept),
        (None, Some(game_dir), None) => match archive::read(game_dir, archive, &path)
            .and_then(|original| if map { crate::map_edits::edited(&rel, &original) } else { crate::event_patches::patched(&rel, &original) })
        {
            // Nothing moved: the game's own file.
            Ok(None) => {
                cache.insert(rel.clone(), (version, None));
                return Some((rel, None));
            }
            made => made.map(Option::unwrap_or_default),
        }
        .inspect(|made| {
            // A copy in kindle\cache, to look at, and for a map, to use again.
            if let Some(g) = GENERATED.get() {
                let file = g.cache_dir.join(&rel);
                let _ = file.parent().map(std::fs::create_dir_all);
                let _ = std::fs::write(&file, made);
                if map {
                    let _ = std::fs::write(key_file(&file), made_key(&version));
                }
            }
        }),
        (None, None, _) => Err("no game folder".into()),
    };
    let stored = bytes.and_then(|bytes| {
        let game_dir = game_dir.as_ref().ok_or("no game folder")?;
        let size = bytes.len();
        let stored = archive::as_stored(game_dir, archive, &path, bytes)?;
        fit_index(game_dir, archive, &path, &rel, &stored, stored.len(), size)?;
        Ok(Bytes::Made(stored))
    });
    let loaded = match stored {
        Ok(bytes) => {
            crate::info!("files: {rel} from {owner}");
            Some(std::sync::Arc::new(bytes))
        }
        Err(e) => {
            crate::warn!("files: can't use {owner}'s {rel} ({e}); using the game's");
            None
        }
    };
    cache.insert(rel.clone(), (version, loaded.clone()));
    Some((rel, loaded))
}

/// Makes the game's archive index fit a replacement of `path` whose bytes as the archive stores
/// them are `stored` followed by zeros to `padded` (`size`: its own length): the check hash
/// replaced in the index's copies, and for a longer file, the entry's sizes (and for the
/// archive's last file, its offset).
fn fit_index(game_dir: &Path, archive: &str, path: &str, rel: &str, stored: &[u8], padded: usize, size: usize) -> Result<(), String> {
    let room = archive::stored_len(game_dir, archive, path)?;
    let longer = padded > room;
    let offset = if longer { Some(archive::read_offset(game_dir, archive, path, padded)?) } else { None };
    let check = archive::check_hash(game_dir, archive, path, stored, padded)?;
    if check.is_none() && longer {
        return Err(format!("{size} bytes, longer than the game's {room}, and it has no check hash to find its index entry by"));
    }
    let Some(check) = check.filter(|c| c.old != c.new || longer) else { return Ok(()) };
    // Only copies inside a copy of the archive's index (its header where the index would start):
    // the hash also turns up in buffers the game frees and reuses while it loads, and writing
    // there corrupted memory (a later crash). The archive's index copies: searched for once per
    // archive (all the game's memory, 1.5 s), then each later file of it checks those copies.
    wait_for_index_search();
    let (index, others) = match known_index(&check) {
        Some(index) => (index, None),
        None => {
            let found = memory::find_writable(&check.old);
            let index: Vec<usize> = found
                .iter()
                .copied()
                .filter(|&at| at >= check.at && memory::read_bytes(at - check.at, check.header.len()).is_some_and(|h| h == check.header))
                .collect();
            if index.is_empty() {
                return Err(format!("the game's hash for it wasn't found in its archive index ({} other copies)", found.len()));
            }
            remember_index(&check, &index);
            let others = found.len() - index.len();
            (index, Some(others))
        }
    };
    // A longer file: its entry in each index copy gets the new sizes (padded at +4, size at
    // +0x20) and, for the archive's last file, an offset early enough for the read to end inside
    // the archive (+8), after checking it's this file's entry (its path hash, its old size).
    let entries: Vec<usize> = index.iter().map(|&at| at - check.at + check.entry_at).collect();
    if longer
        && !entries.iter().all(|&e| memory::read::<u32>(e) == Some(archive::path_hash(path)) && memory::read::<i32>(e + 4).is_some_and(|p| p as usize == room))
    {
        return Err("its entry in the game's archive index isn't where it should be".into());
    }
    // The entry's size: the file's, but 0 for a compressed file (DCX), as the game's own entries
    // have it; a longer map file given its length there crashed the game's map loader
    // (+0xE148C0, m31_02 with treasure, 2026-10-23).
    let stored_size = if path.ends_with(".dcx") { 0 } else { size as u64 };
    let mut patched = Vec::new();
    for (at, entry) in index.iter().zip(&entries) {
        patched.push(Patched {
            hash_at: *at,
            hash: check.old,
            entry: *entry,
            padded: memory::read::<i32>(entry + 4).unwrap_or(0),
            offset: memory::read::<u64>(entry + 8).unwrap_or(0),
            size: memory::read::<u64>(entry + 0x20).unwrap_or(0),
        });
        unsafe {
            std::ptr::copy_nonoverlapping(check.new.as_ptr(), *at as *mut u8, check.new.len());
            if let Some(offset) = offset {
                std::ptr::write_unaligned((entry + 4) as *mut i32, padded as i32);
                std::ptr::write_unaligned((entry + 8) as *mut u64, offset);
                std::ptr::write_unaligned((entry + 0x20) as *mut u64, stored_size);
            }
        }
    }
    PATCHED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(rel.to_string(), patched);
    match others {
        Some(n) => crate::debug!("files: {rel}: the game's check hash replaced in {} index copies ({n} others left)", index.len()),
        None => crate::debug!("files: {rel}: the game's check hash replaced in the archive's {} index copies", index.len()),
    }
    if longer {
        crate::info!("files: {rel}: {size} bytes, longer than the game's {room}; its size in the game's archive index changed to match");
    }
    Ok(())
}

/// Points the archive operator `op` at Kindle's vtable copy, which overlays `data` on its reads.
fn overlay_operator(op: usize, data: std::sync::Arc<Bytes>) -> bool {
    let Some(vt) = memory::read_u64(op).map(|v| v as usize) else { return false };
    let known = ARCHIVE_VTABLE.load(Ordering::Acquire);
    if known == 0 {
        ARCHIVE_VTABLE.store(vt, Ordering::Release);
    } else if known != vt {
        crate::warn!("files: an archive file of an unexpected kind; using the game's");
        return false;
    }
    let copy = *OVERLAY_VTABLE.get_or_init(|| {
        let mut slots: Vec<usize> = (0..VTABLE_COPY).map(|i| memory::read_u64(vt + i * 8).unwrap_or(0) as usize).collect();
        slots[DESTROY_SLOT] = overlay_destroy as *const () as usize;
        slots[COPY_SLOT] = overlay_copy as *const () as usize;
        slots[READ_SLOT] = overlay_read as *const () as usize;
        Box::leak(slots.into_boxed_slice()).as_ptr() as usize
    });
    OVERLAID.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(op, data);
    unsafe { std::ptr::write_volatile(op as *mut usize, copy) };
    true
}

/// The game's own function in slot `slot` of the archive operator.
fn archive_slot(slot: usize) -> OpenFn {
    let vt = ARCHIVE_VTABLE.load(Ordering::Acquire);
    unsafe { std::mem::transmute(memory::read_u64(vt + slot * 8).unwrap_or(0) as usize) }
}

#[allow(clippy::too_many_arguments)]
extern "C" fn overlay_read(this: usize, buffer: usize, count: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let pos = memory::read::<u32>(this + POSITION).map(|p| p as usize);
    let n = archive_slot(READ_SLOT)(this, buffer, count, c, d, e, f, g, h);
    if n == 0 || n > count || buffer == 0 {
        return n;
    }
    if let Some(pos) = pos
        && let Some(data) = OVERLAID.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&this))
    {
        let take = n.min(data.len().saturating_sub(pos));
        unsafe {
            if take > 0 {
                std::ptr::copy_nonoverlapping(data.as_ptr().add(pos), buffer as *mut u8, take);
            }
            // Past the replacement's end: the zeros the archive pads a file with.
            std::ptr::write_bytes((buffer + take) as *mut u8, 0, n - take);
        }
    }
    n
}

#[allow(clippy::too_many_arguments)]
extern "C" fn overlay_copy(this: usize, a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    let copy = archive_slot(COPY_SLOT)(this, a, b, c, d, e, f, g, h);
    let data = OVERLAID.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&this).cloned());
    if copy != 0
        && let Some(data) = data
    {
        overlay_operator(copy, data);
    }
    copy
}

#[allow(clippy::too_many_arguments)]
extern "C" fn overlay_destroy(this: usize, a: usize, b: usize, c: usize, d: usize, e: usize, f: usize, g: usize, h: usize) -> usize {
    if let Some(m) = OVERLAID.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        m.remove(&this);
    }
    archive_slot(DESTROY_SLOT)(this, a, b, c, d, e, f, g, h)
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
    let (_, rest) = text.split_once(":/")?;
    PATHS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(&key(rest)).copied()
}

/// Kindle's edit of the game file at `text` (an archive path), made at the first call.
fn generated_file<'a>(text: &str, rel: &str) -> Option<&'a Vec<u16>> {
    let generated = GENERATED.get()?;
    let (movie, file) = generated.movies.get(rel)?;
    file.get_or_init(|| {
        let archive = text.split_once(":/").map(|(a, _)| a)?;
        let made = archive::read(&generated.game_dir, archive, &format!("/{rel}")).and_then(|original| (movie.edit)(&original)).and_then(|edited| {
            let out = generated.cache_dir.join(rel.replace('/', "\\"));
            std::fs::create_dir_all(out.parent().unwrap_or(&generated.cache_dir))
                .and_then(|_| std::fs::write(&out, edited))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            Ok(out)
        });
        match made {
            Ok(out) => Some(out.as_os_str().to_string_lossy().encode_utf16().collect()),
            Err(e) => {
                crate::warn!("files: {} is off: {e}", movie.feature);
                None
            }
        }
    })
    .as_ref()
}

/// Whether Kindle's edit of this game file was made: None if the game hasn't opened it (or
/// Kindle doesn't edit it).
pub fn edit_made(rel: &str) -> Option<bool> {
    GENERATED.get()?.movies.get(rel)?.1.get().map(|made| made.is_some())
}

/// Opens a mod's file (or Kindle's edit) instead, through the disk device, if one replaces
/// this path.
fn redirect(path: usize, c: usize, d: usize) -> Option<usize> {
    let disk = DISK.load(Ordering::Acquire);
    if disk == 0 {
        return None;
    }
    let text = String::from_utf16_lossy(&read_string(path)?);
    let rel = key(game_path(&text));
    let (owner, file) = match OVERRIDES.get().and_then(|o| o.get(&rel)) {
        Some((owner, file)) => (owner.as_str(), file),
        None => ("Kindle", generated_file(&text, &rel)?),
    };
    let ours = game_string(&rel, file, memory::read_u64(c)? as usize);
    // The disk device wants the path's characters and a 0 flag.
    let chars = memory::read_u64(ours)? as usize;
    let open: OpenFn = unsafe { std::mem::transmute(DISK_OPEN.load(Ordering::Acquire)) };
    let op = open(disk, ours, chars, c, d, 0, 0, 0, 0);
    if op == 0 {
        crate::warn!("files: couldn't open {owner}'s {rel}; using the game's");
        return None;
    }
    crate::info!("files: {rel} from {owner}");
    Some(op)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_normalized() {
        assert_eq!(key("/Chr\\C1100.chrbnd.dcx"), "chr/c1100.chrbnd.dcx");
        assert_eq!(game_path("data1:/shader/gxgui.shaderbnd.dcx"), "shader/gxgui.shaderbnd.dcx");
        assert_eq!(game_path("data3:/action/eventnameid.txt"), "action/eventnameid.txt");
    }

    #[test]
    fn first_mod_wins() {
        let dir = std::env::temp_dir().join(format!("kindle-files-test-{}", std::process::id()));
        for (m, f) in [("a", "chr/c1100.chrbnd.dcx"), ("b", "chr/c1100.chrbnd.dcx"), ("b", "map/x.bin")] {
            let p = dir.join(m).join("files").join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, m).unwrap();
        }
        let map = collect(&dir, &["a".to_string(), "b".to_string()]);
        assert_eq!(map["chr/c1100.chrbnd.dcx"].0, "a");
        assert_eq!(map["map/x.bin"].0, "b");
        let _ = std::fs::remove_dir_all(dir);
    }
}
