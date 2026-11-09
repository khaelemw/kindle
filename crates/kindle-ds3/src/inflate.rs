//! The game's decompression on a faster inflate. The game reads its compressed files (DCX
//! "DFLT": zlib streams; models, textures, maps) through zlib 1.2.3 (2005), linked into the exe:
//! `inflateInit2_` (`inflateInit_` jumps to it), `inflate` and `inflateEnd`, the only inflate
//! functions the rest of the game calls (verified in the decompile, 2026-10-11). Loading an area
//! is mostly one thread decompressing (zlib's inflate_fast its busiest function; the streaming
//! goes on for seconds after the loading screen). zlib-rs implements the same functions on the
//! same z_stream (laid out alike since 1.2.3) about 1.5 times as fast on the game's own files
//! (469 MB/s against 312 for classic zlib, 171 MB of models, textures and messages, fed in 64 KB
//! pieces). Kindle replaces the three: a stream started after that is zlib-rs's from init to end
//! (its state noted); one the game started before goes on with the game's own zlib.

use std::collections::HashSet;
use std::sync::Mutex;

use crate::patterns;

const INFLATE: &str = "89 54 24 10 48 89 4c 24 08 55 41 54 41 55 48 8b ec 48 81 ec 80 00 00 00 4c 8b e1 48 85 c9 0f 84 ?? ?? ?? ?? 4c 8b 69 28 4d 85 ed";
/// (The exe has a second zlib, another library's: the one next to `inflate` is the game's.)
const INFLATE_END: &str =
    "40 53 48 83 ec 20 48 8b d9 48 85 c9 74 ?? 48 8b 41 28 48 85 c0 74 ?? 4c 8b 41 38 4d 85 c0 74 ?? 48 8b 50 38 48 85 d2 74 ?? 48 8b 49 40 41 ff d0";
const INFLATE_INIT2: &str =
    "48 89 5c 24 10 57 48 83 ec 20 8b fa 48 8b d9 4d 85 c0 0f 84 ?? ?? ?? ?? 0f b6 05 ?? ?? ?? ?? 41 38 00 0f 85 ?? ?? ?? ?? 41 83 f9 58 0f 85";
/// How far from `inflate` the game's other zlib functions are (they're about 0x1C70 apart).
const NEAR: usize = 0x4000;
/// The game's z_stream: 0x58 bytes (1.2.3 on 64-bit Windows), as zlib-rs's.
const STREAM_SIZE: usize = 0x58;
const _: () = assert!(std::mem::size_of::<libz_rs_sys::z_stream>() == STREAM_SIZE);
/// Z_STREAM_ERROR.
const STREAM_ERROR: i32 = -2;
/// z_stream +0x28: the state.
const STATE_AT: usize = 0x28;

/// When the game last decompressed something (ms since the process's first call here; 0 never):
/// how the quick warp knows the area it moved to has streamed in.
static LAST_INFLATE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static EPOCH: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn now_ms() -> u64 {
    EPOCH.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

/// How long since the game last decompressed (None if it never has since Kindle's hooks).
pub fn quiet_for() -> Option<std::time::Duration> {
    let last = LAST_INFLATE_MS.load(std::sync::atomic::Ordering::Relaxed);
    (last != 0).then(|| std::time::Duration::from_millis(now_ms().saturating_sub(last)))
}

/// The states of the streams zlib-rs runs.
static OURS: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

fn ours() -> std::sync::MutexGuard<'static, Option<HashSet<usize>>> {
    OURS.lock().unwrap_or_else(|e| e.into_inner())
}

fn state_of(strm: usize) -> usize {
    if strm == 0 { 0 } else { crate::memory::read_u64(strm + STATE_AT).unwrap_or(0) as usize }
}

static INFLATE_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "inflate",
    name: "zlib inflate",
    pattern: INFLATE,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(inflate_hook),
    without: "streams started from now on still run on the game's zlib",
};
static INFLATE_END_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "inflate",
    name: "zlib inflateEnd",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(end_hook),
    without: "streams started from now on still run on the game's zlib",
};
static INFLATE_INIT_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "inflate",
    name: "zlib inflateInit2_",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(init_hook),
    without: "streams started from now on still run on the game's zlib",
};

/// (inflateEnd and inflateInit2_ match more than once: the copies near inflate are the game's.)
pub fn install() {
    let Some(inflate) = INFLATE_SITE.find().map(|f| f[0]) else { return };
    let near = |a: usize| a.abs_diff(inflate) < NEAR;
    let (Some(end), Some(init)) = (patterns::find_where("zlib inflateEnd", INFLATE_END, near), patterns::find_where("zlib inflateInit2_", INFLATE_INIT2, near))
    else {
        return crate::warn!("inflate: the game's zlib functions weren't all found; it decompresses as before");
    };
    // Init first: until it's in, every stream is the game's zlib's, which inflate and end pass on.
    for (site, at) in [(&INFLATE_SITE, inflate), (&INFLATE_END_SITE, end), (&INFLATE_INIT_SITE, init)] {
        match site.hook_at(at) {
            Ok(h) => std::mem::forget(h),
            Err(_) => return,
        }
    }
    crate::info!("inflate: the game's decompression runs on zlib-rs");
}

type Init2 = unsafe extern "C" fn(usize, i32, *const u8, i32) -> i32;
type Inflate = unsafe extern "C" fn(usize, i32) -> i32;
type End = unsafe extern "C" fn(usize) -> i32;

fn init_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (strm, window_bits, version, size) = unsafe { ((*reg).rcx as usize, (*reg).rdx as i32, (*reg).r8 as *const u8, (*reg).r9 as i32) };
    if strm == 0 || size as usize != STREAM_SIZE {
        return unsafe { std::mem::transmute::<usize, Init2>(original)(strm, window_bits, version, size) } as u32 as usize;
    }
    let r = unsafe { libz_rs_sys::inflateInit2_(strm as *mut libz_rs_sys::z_stream, window_bits, libz_rs_sys::zlibVersion(), STREAM_SIZE as i32) };
    if r == 0 {
        ours().get_or_insert_with(HashSet::new).insert(state_of(strm));
    }
    r as u32 as usize
}

fn inflate_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (strm, flush) = unsafe { ((*reg).rcx as usize, (*reg).rdx as i32) };
    LAST_INFLATE_MS.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    let state = state_of(strm);
    if state != 0 && ours().as_ref().is_some_and(|s| s.contains(&state)) {
        return unsafe { libz_rs_sys::inflate(strm as *mut libz_rs_sys::z_stream, flush) } as u32 as usize;
    }
    unsafe { std::mem::transmute::<usize, Inflate>(original)(strm, flush) as u32 as usize }
}

fn end_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let strm = unsafe { (*reg).rcx as usize };
    let state = state_of(strm);
    if state != 0 && ours().as_mut().is_some_and(|s| s.remove(&state)) {
        return unsafe { libz_rs_sys::inflateEnd(strm as *mut libz_rs_sys::z_stream) } as u32 as usize;
    }
    if strm == 0 {
        return STREAM_ERROR as u32 as usize;
    }
    unsafe { std::mem::transmute::<usize, End>(original)(strm) as u32 as usize }
}
