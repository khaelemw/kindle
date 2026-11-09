//! Bonfire travel within the map the player is in, without a loading screen. The game's travel
//! (`Lua_Warp`, the bonfire menu's) plays the warp (the player turns to smoke: W_BonfireWarp,
//! which also gives SpEffect 40, the smoke over the screen), sets where the player is going (the
//! map, the bonfire's player start: a c0000 part whose entity id is the bonfire's less 980, and
//! the last bonfire rested at, GameMan +0xACC = entity + 1000) and loads the world again (about 4
//! s from the menu to playing for a bonfire in the same map: 1.7 s of warp, 1 s of loading, 1.6
//! s of loading screen after; verified 2026-10-11). Where the world wouldn't change (the same
//! map, its state, its ceremony, the same) Kindle does it in place instead: the same warp, a
//! moment of black, the player at the bonfire's start (its position and facing from the map
//! file: verified to be where and how the game puts the player), the arrival (W_Event63000) and
//! the smoke taken off; the game shows the area's name as the player arrives. Not in a co-op
//! session (the players follow the host with a load).

use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use crate::hook_sites::{Kind, Site};
use crate::{chars, game, msb, ui_bridge};

/// The smoke over the screen while warping.
const WARP_SMOKE: i64 = 40;
/// The warp's start: the bonfire's player start part is its entity id less this.
const START_PART_OFFSET: i32 = 980;
/// GameMan +0xACC: the last bonfire rested at (entity id + 1000).
const LAST_BONFIRE_AT: usize = 0xACC;
/// From the warp: the screen goes black, the player is moved, the screen comes back.
const FADE_OUT_AT: Duration = Duration::from_millis(750);
const MOVE_AT: Duration = Duration::from_millis(1000);
/// Black until the area has drawn: it streams in after the player is there (inside Firelink
/// Shrine its walls and roof about 2 s the first time, 0.8 s when it was seen lately; verified
/// 2026-10-11; a load's loading screen covers the same). The screen comes back once the game
/// hasn't decompressed anything for STREAM_QUIET, at least HOLD_MIN after the move (the camera
/// swinging round to the new place) and at most HOLD_MAX.
const HOLD_MIN: Duration = Duration::from_millis(700);
const HOLD_MAX: Duration = Duration::from_millis(3000);
const STREAM_QUIET: Duration = Duration::from_millis(200);
const FADE_IN_FOR: Duration = Duration::from_millis(450);
/// The warp gives the smoke about a second in, maybe after the move: it's taken off until then.
/// After the screen starts coming back.
const SMOKE_AFTER: Duration = Duration::from_millis(500);

struct Warp {
    to: i32,
    pos: [f32; 3],
    facing: f32,
    since: Instant,
    started: bool,
    moved: bool,
    /// When the player was moved, and when the screen started coming back.
    moved_at: Option<Instant>,
    back_at: Option<Instant>,
}

static PENDING: Mutex<Option<Warp>> = Mutex::new(None);

/// The bonfire warp (`Lua_Warp`). Its pattern starts four of the game's Lua functions; bonfire.rs
/// picks this one by the name it registers, and only it is hooked (the hook audit checks all four).
pub(crate) static QUICK_WARP_SITE: Site = Site {
    area: "quick warp",
    name: "bonfire warp",
    pattern: crate::bonfire::WARP_PATTERN,
    offset: 0,
    matches: 4,
    kind: Kind::Retn(warp_hook),
    without: "travel within the map loads as before",
};

pub fn install() {
    let Some(warp) = crate::bonfire::warp_fn() else { return };
    match QUICK_WARP_SITE.hook_at(warp) {
        Ok(h) => std::mem::forget(h),
        Err(e) => crate::warn!("quick warp: couldn't hook the bonfire warp ({e}); travel loads as before"),
    }
}

type WarpFn = unsafe extern "C" fn(usize, i32, i32, i32);

fn warp_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (man, from, to, ceremony) = unsafe { ((*reg).rcx as usize, (*reg).rdx as i32, (*reg).r8 as i32, (*reg).r9 as i32) };
    crate::emevd::travel_began(map_of(to), to, "bonfire");
    match std::panic::catch_unwind(|| in_place(to, ceremony)).unwrap_or(Err("failed")) {
        Ok(w) => {
            crate::info!("quick warp: bonfire {to} is in this map: travelling there without a load");
            *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some(w);
        }
        Err(why) => {
            crate::debug!("quick warp: bonfire {to} with a load ({why})");
            unsafe { std::mem::transmute::<usize, WarpFn>(original)(man, from, to, ceremony) }
        }
    }
    0
}

/// The map a bonfire is in (packed as the game's: 0xAABB0000 for mAA_BB).
fn map_of(entity: i32) -> i32 {
    ((entity / 100_000) << 24) | (((entity / 10_000) % 10) << 16)
}

/// The travel in place, if the world wouldn't change: no co-op session, the bonfire in the
/// map the player is in, the map's state the same, and its player start in the map file.
fn in_place(to: i32, ceremony: i32) -> Result<Warp, &'static str> {
    if crate::p2p::hosting().is_some() {
        return Err("in a co-op session");
    }
    if PENDING.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
        return Err("a travel is under way");
    }
    game::player_chr_addr().map_err(|_| "no character")?;
    let here = crate::session::host_location().ok_or("the map isn't known")?;
    if map_of(to) != here.map {
        return Err("another map");
    }
    // The map's is a CeremonyParam id (area * 1000000 + its state: 40000010 is m40's 10, the
    // Untended Graves); the travel's the state.
    if ceremony.max(0) != here.ceremony.max(0) % 1_000_000 {
        return Err("another state of the map");
    }
    let name = format!("m{:02}_{:02}_00_00", (here.map >> 24) & 0xFF, (here.map >> 16) & 0xFF);
    let game_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::Path::to_path_buf)).ok_or("no game folder")?;
    let map = crate::archive::read_any(&game_dir, &format!("/map/mapstudio/{name}.msb.dcx"))
        .ok()
        .and_then(|(_, dcx)| crate::archive::undcx(&dcx))
        .and_then(|d| msb::Msb::parse(&d).ok())
        .ok_or("the map file wasn't read")?;
    let start = to - START_PART_OFFSET;
    let part = map
        .section("PARTS_PARAM_ST")
        .and_then(|s| s.entries.iter().find(|e| msb::rd_i64(e, msb::PART_ENTITY).and_then(|at| msb::rd_i32(e, at as usize)) == Some(start)))
        .ok_or("the bonfire's player start isn't in the map file")?;
    let f = |k: usize| part.get(msb::PART_POSE + 4 * k..msb::PART_POSE + 4 * k + 4).map(|b| f32::from_le_bytes(b.try_into().expect("4 bytes")));
    let (Some(x), Some(y), Some(z), Some(r)) = (f(0), f(1), f(2), f(4)) else { return Err("the player start is cut short") };
    Ok(Warp { to, pos: [x, y, z], facing: r.to_radians(), since: Instant::now(), started: false, moved: false, moved_at: None, back_at: None })
}

fn set_fade(a: f32) {
    ui_bridge::SCREEN_FADE.store(a.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
}

/// Each frame (the game's thread): the travel in place under way.
pub fn frame() {
    let mut guard = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    let Some(w) = guard.as_mut() else { return };
    let Ok(chr) = game::player_chr_addr() else {
        set_fade(0.0);
        *guard = None;
        return;
    };
    let t = w.since.elapsed();
    if !w.started {
        // Just asked for: the warp, and where the player will be back after a death.
        w.started = true;
        w.since = Instant::now();
        if let Ok(id) = crate::anim::id_of(chr, "W_BonfireWarp") {
            let _ = crate::anim::play(chr, id);
        }
        if let Some(gm) = crate::bonfire::game_man() {
            unsafe { std::ptr::write_unaligned((gm + LAST_BONFIRE_AT) as *mut i32, w.to + 1000) };
        }
        return;
    }
    if t < FADE_OUT_AT {
        return;
    }
    if t < MOVE_AT {
        set_fade((t - FADE_OUT_AT).as_secs_f32() / (MOVE_AT - FADE_OUT_AT).as_secs_f32());
        return;
    }
    if !w.moved {
        w.moved = true;
        w.moved_at = Some(Instant::now());
        set_fade(1.0);
        let _ = chars::teleport(chr, w.pos);
        let _ = chars::set_facing(chr, w.facing);
        let _ = game::player_remove_effect(WARP_SMOKE);
        if let Ok(id) = crate::anim::id_of(chr, "W_Event63000") {
            let _ = crate::anim::play(chr, id);
        }
        return;
    }
    let _ = game::player_remove_effect(WARP_SMOKE);
    let held = w.moved_at.map(|m| m.elapsed()).unwrap_or_default();
    let back_at = match w.back_at {
        Some(b) => b,
        None => {
            let streamed = crate::inflate::quiet_for().is_none_or(|q| q >= STREAM_QUIET);
            if held < HOLD_MIN || (held < HOLD_MAX && !streamed) {
                return;
            }
            crate::debug!("quick warp: the area streamed in {:.2} s after the move", held.as_secs_f32());
            *w.back_at.insert(Instant::now())
        }
    };
    let back = back_at.elapsed().as_secs_f32() / FADE_IN_FOR.as_secs_f32();
    set_fade(1.0 - back.min(1.0));
    if back_at.elapsed() >= FADE_IN_FOR + SMOKE_AFTER {
        set_fade(0.0);
        *guard = None;
    }
}
