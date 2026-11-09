//! Bonfires: the list (from BonfireWarpParam), whether each is lit, and warping to one
//! through the game's own warp (`Lua_Warp`, the function the bonfire menu uses).
//!
//! | | |
//! |---|---|
//! | list | BonfireWarpParam rows: `bonfireEntityId`, `eventflagId` (set once lit), `BonfireNameId` (FMG 200 base, 232 Ashes of Ariandel, 272 Ringed City), `CeremonyId` (10 for the Untended Graves pair), `IsDisableQuickwarp` |
//! | last rested | `GameMan + 0xACC` = entity id + 1000 (offset from the Grand Archives CE table; GameMan located through its AOB) |
//! | warp | `Lua_Warp(SprjLuaEventMan*, from entity, to entity, ceremony)`: four byte-identical template functions share its code, so it's picked by the name string its body references |
//! | DLC owned | `CSDlc` |
//! | rest | the flow's `OnEvent_BonfireRespawn` (+0x494AD0): the hook point `bonfire.rest_requested`, the function `bonfire.rest` |
//! | sitting | `SprjWorldTalkMan + 0x2038` (talk command 42, the sitting, sets it; 43, the standing up, clears it) |
//! | travel | talk command 41 (the travel screen): the hook point `bonfire.travel_requested` |

use std::sync::OnceLock;

use kindle_paramdef::Value;

use crate::catalogue::{self, HookPoint};
use crate::hook_sites::{self, Site};
use crate::{game, memory, params, patterns, text_override};

pub(crate) const WARP_PATTERN: &str = "48 8b c4 55 41 54 41 55 41 56 41 57 48 8d a8 08 fb ff ff 48 81 ec d0 05 00 00 \
    48 c7 44 24 20 fe ff ff ff 48 89 58 10 48 89 70 18 48 89 78 20 48 8b 05 ?? ?? ?? ?? 48 33 c4 \
    48 89 85 c0 04 00 00 4d 63 f1 4d 63 f8 4c 63 e2 48 8b d9 48 83 79 08 00 0f 84 ?? ?? ?? ?? \
    48 8b 05 ?? ?? ?? ?? 48 85";
/// `lea rdx, [rip + name]` inside each of the template functions.
const WARP_NAME_LEA: usize = 0x95;
/// `mov rax, [rip + GameMan]; mov [rax+0x28], ecx; ret` (AOB from the Grand Archives table).
const GAME_MAN_PATTERN: &str = "48 8b ?? ?? ?? ?? 04 89 48 28 c3";

const NAME_FMGS: [u32; 3] = [200, 232, 272];

static WARP: OnceLock<Option<usize>> = OnceLock::new();
static GAME_MAN: OnceLock<Option<usize>> = OnceLock::new();

#[derive(Clone)]
pub struct Bonfire {
    pub row: u64,
    pub entity_id: i32,
    pub name: Option<String>,
    pub lit: bool,
    /// The event flag set once it's lit.
    pub flag: i64,
    pub ceremony: i32,
    pub warpable: bool,
    pub dlc: u8,
}

fn int(v: Result<Value, String>) -> i64 {
    match v {
        Ok(Value::Int(i)) => i,
        _ => 0,
    }
}

/// Which DLC a bonfire belongs to, from its entity id's map (m45 Ariandel, m50 Ringed City).
fn dlc_of(entity_id: i32) -> u8 {
    match entity_id {
        5_000_000.. => 2,
        4_500_000.. => 1,
        _ => 0,
    }
}

/// The list as last made (all but whether each is lit, read again each time): the names took
/// about 11 ms, every 2 s in co-op (a hitch; verified 2026-10-11). Made again if the table's rows
/// change; a name not there yet (a DLC's text loads later) is looked for again.
static LIST: std::sync::Mutex<Option<(usize, Vec<Bonfire>)>> = std::sync::Mutex::new(None);

pub fn all() -> Result<Vec<Bonfire>, String> {
    let reg = params::registry()?;
    let t = reg.table("BonfireWarpParam")?;
    let mut cached = LIST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((rows, list)) = cached.as_mut()
        && *rows == t.row_ids.len()
    {
        for b in list.iter_mut() {
            b.lit = b.flag > 0 && game::flag_get(b.flag).unwrap_or(false);
            if b.name.is_none() {
                b.name = text_override::get_any(&NAME_FMGS, int(t.read(b.row, "BonfireNameId")) as u32);
            }
        }
        return Ok(list.clone());
    }
    let out = made(t)?;
    *cached = Some((t.row_ids.len(), out.clone()));
    Ok(out)
}

fn made(t: &params::Table) -> Result<Vec<Bonfire>, String> {
    let mut out = Vec::new();
    for &row in &t.row_ids {
        let entity_id = int(t.read(row, "bonfireEntityId")) as i32;
        if entity_id <= 0 {
            continue;
        }
        let flag = int(t.read(row, "eventflagId"));
        out.push(Bonfire {
            row,
            entity_id,
            name: text_override::get_any(&NAME_FMGS, int(t.read(row, "BonfireNameId")) as u32),
            lit: flag > 0 && game::flag_get(flag).unwrap_or(false),
            flag,
            ceremony: int(t.read(row, "CeremonyId")) as i32,
            warpable: int(t.read(row, "IsDisableQuickwarp")) == 0,
            dlc: dlc_of(entity_id),
        });
    }
    Ok(out)
}

/// The GameMan instance (None if not found or not created yet).
pub fn game_man() -> Option<usize> {
    let game_man = (*GAME_MAN.get_or_init(|| patterns::find_static("GameMan", GAME_MAN_PATTERN)))?;
    memory::read_u64(game_man).filter(|&p| p != 0).map(|p| p as usize)
}

/// GameMan + this: the loaded save slot (profile index).
const SAVE_SLOT: usize = 0xA60;

/// The save slot the loaded character is in.
pub fn save_slot() -> Result<i32, String> {
    let gm = game_man().ok_or("the save slot is unavailable on this game build")?;
    let slot = memory::read::<i32>(gm + SAVE_SLOT).ok_or("game state is unreadable")?;
    (0..100).contains(&slot).then_some(slot).ok_or_else(|| format!("unexpected save slot {slot}"))
}

/// Entity id of the bonfire the player last rested at.
pub fn last_rested() -> Result<Option<i32>, String> {
    let gm = game_man().ok_or("game state is not available yet")?;
    let raw = memory::read::<i32>(gm + 0xACC).ok_or("game state is unreadable")?;
    Ok((raw > 1000).then_some(raw - 1000))
}

fn dlc_owned(dlc: u8) -> bool {
    match dlc {
        1 | 2 => crate::ds3::dlc_installed(dlc as u32).unwrap_or(false),
        _ => true,
    }
}

/// The game's bonfire warp (`Lua_Warp`), if found on this build.
pub fn warp_fn() -> Option<usize> {
    *WARP.get_or_init(|| patterns::find_with_string("bonfire warp", WARP_PATTERN, WARP_NAME_LEA, "Lua_Warp"))
}

/// Warps the player to a bonfire, as the bonfire menu does. Unless `force`, only lit
/// bonfires that allow warping are accepted.
pub fn warp(target: &Bonfire, force: bool) -> Result<(), String> {
    game::player_chr_addr()?;
    let label = target.name.clone().unwrap_or_else(|| format!("bonfire {}", target.entity_id));
    if !dlc_owned(target.dlc) {
        return Err(format!("{label} needs a DLC that isn't installed"));
    }
    if !force && !target.lit {
        return Err(format!("{label} hasn't been lit (pass force to warp anyway)"));
    }
    if !force && !target.warpable {
        return Err(format!("{label} can't be warped to (pass force to warp anyway)"));
    }
    let f = warp_fn().ok_or("bonfire warp is unavailable on this game build")?;
    let lua_event_man = crate::ds3::LUA_EVENT_MAN.instance().filter(|&p| p != 0).ok_or("the game's event system is not available yet")?;
    let from = last_rested()?.unwrap_or(target.entity_id);
    let warp: unsafe extern "C" fn(usize, i32, i32, i32) = unsafe { std::mem::transmute(f) };
    unsafe { warp(lua_event_man, from, target.entity_id, target.ceremony.max(0)) };
    Ok(())
}

/// Resolves a bonfire by entity id (>= 1,000,000), BonfireWarpParam row id, or name
/// (case-insensitive; among bonfires sharing a name, lit and warpable ones win).
pub fn find(all: Vec<Bonfire>, key: &BonfireKey) -> Result<Bonfire, String> {
    let mut matches: Vec<Bonfire> = match key {
        BonfireKey::Id(id) if *id >= 1_000_000 => all.into_iter().filter(|b| b.entity_id as i64 == *id).collect(),
        BonfireKey::Id(id) => all.into_iter().filter(|b| b.row as i64 == *id).collect(),
        BonfireKey::Name(name) => {
            let want = name.to_lowercase();
            all.into_iter().filter(|b| b.name.as_deref().is_some_and(|n| n.to_lowercase() == want)).collect()
        }
    };
    matches.sort_by_key(|b| (!b.lit, !b.warpable, b.row));
    matches.into_iter().next().ok_or_else(|| match key {
        BonfireKey::Id(id) => format!("no bonfire with id {id} (see world.bonfires())"),
        BonfireKey::Name(n) => format!("no bonfire named {n:?} (see world.bonfires())"),
    })
}

pub enum BonfireKey {
    Id(i64),
    Name(String),
}

/// A bonfire's fire isn't its lit flag: it shows by the bonfire's kindle level in the game's
/// bonfire table (its Lua event condition, SprjLuaEventConditionBonfire, picks the object's
/// animation from it each frame: 1 unlit, 11 and up burning), which a load sets from the flag
/// and a light raises. The lighter's bonfire animation asks for that light (its Lua's
/// Util_RequestLevelUpFirst: the bonfire's entity id in a request the game takes each frame;
/// the bonfire then raises its level and sets its flag). A bonfire whose flag is set some other
/// way shows unlit until the next load (verified 2026-10-11 at Tower on the Wall, flag 13000005:
/// with the flag set the state stayed 1; with this request it went to 11, the fire and its light
/// as after a load, at once). The request is read at the start of the game's per-frame copy (the
/// pattern: four requests, each taken and reset to -1, then a byte flag the same way).
const FIRST_LIGHT: &str = "8b 05 ?? ?? ?? ?? c7 05 ?? ?? ?? ?? ff ff ff ff 89 05 ?? ?? ?? ?? 8b 05 ?? ?? ?? ?? c7 05 ?? ?? ?? ?? ff ff ff ff 89 05 ?? ?? ?? ?? 8b 05 ?? ?? ?? ?? c7 05 ?? ?? ?? ?? ff ff ff ff 89 05 ?? ?? ?? ?? 8b 05 ?? ?? ?? ?? c7 05 ?? ?? ?? ?? ff ff ff ff 89 05 ?? ?? ?? ?? 0f b6 05";

/// Asks the game to light a bonfire's fire, as the bonfire animation's own request (one a frame,
/// on the game's thread): false while an earlier request is still waiting. The entity must be one
/// of the game's bonfires. Without the request (not found) the fire shows from the next load.
pub fn request_fire(entity: i32) -> Result<bool, String> {
    static SLOT: OnceLock<Option<usize>> = OnceLock::new();
    if !crate::on_game_thread() {
        return Err("a bonfire's fire is lit from the game's thread".into());
    }
    if entity <= 0 || !all()?.iter().any(|b| b.entity_id == entity) {
        return Err(format!("{entity} isn't a bonfire"));
    }
    let Some(slot) = *SLOT.get_or_init(|| {
        let at = patterns::find_unique("bonfire light request", FIRST_LIGHT)?;
        patterns::rip_target(at + 2, at + 6)
    }) else {
        return Ok(true);
    };
    if memory::read::<i32>(slot) != Some(-1) {
        return Ok(false);
    }
    unsafe { std::ptr::write_volatile(slot as *mut i32, entity) };
    crate::info!("bonfire: its fire is lit (entity {entity})");
    Ok(true)
}

/// kindle.game: light a bonfire's fire whose lit flag was set (catalogue.rs).
pub static LIGHT_FIRE: crate::catalogue::Function = crate::catalogue::Function {
    name: "bonfire.light_fire",
    doc: "Lights a bonfire's fire (its entity id) as the game's own lighting does, for one whose lit flag was set otherwise (it would show unlit until the next load). One a frame: false while an earlier one waits.",
    args: &[("entity", crate::catalogue::Kind::Int { min: 1, max: i32::MAX as i64 })],
    call: |a| match a {
        [crate::catalogue::Value::Int(e)] => request_fire(*e as i32).map(|lit| Some(crate::catalogue::Value::Bool(lit))),
        _ => Err("bonfire.light_fire takes an entity id".into()),
    },
};

// ── resting ─────────────────────────────────────────────────────────────────────────────────
//
// Sitting at a bonfire rests at once in the game: the sitting (talk command 42, the script's
// Lua_BonfireLoopAnimBegin) raises the flow's OnEvent_BonfireRespawn (+0x494AD0) a moment later,
// which hands the bonfire to the world (+0x546D60 and +0x546D70 store it; the world then brings
// its enemies back and the player's HP and FP) and refills the Estus (+0x58DB20). The game raises
// it otherwise too, not sitting (it came as a player was killed, bonfire 0, 2026-10-13). A mod can
// hold the rest back (bonfire.rest_requested) and rest later as the game does (bonfire.rest): a
// co-op mod resting a party together.
const BONFIRE_RESPAWN: &str =
    "48 89 5c 24 08 57 48 83 ec 20 48 8b fa 8b 51 28 48 8b d9 48 8b cf e8 ?? ?? ?? ?? 8b 53 28 48 8b cf e8 ?? ?? ?? ?? 48 8b 5c 24 30 48 83 c4 20 5f e9";
/// The bonfire OnEvent_BonfireRespawn rests at: its event's +0x28 (the bonfire's entity id).
const RESPAWN_BONFIRE_AT: usize = 0x28;

/// OnEvent_BonfireRespawn (where it is) and what it calls: the two stores and the refill.
struct RestCalls {
    at: usize,
    store_a: usize,
    store_b: usize,
    refill: usize,
}

/// OnEvent_BonfireRespawn's calls, read from its code (before it's hooked: its hook writes over
/// its start), if it is as expected.
fn rest_calls() -> Option<&'static RestCalls> {
    static CALLS: OnceLock<Option<RestCalls>> = OnceLock::new();
    CALLS
        .get_or_init(|| {
            let at = BONFIRE_RESPAWN_SITE.find()?[0];
            let refill = (memory::read::<u8>(at + 0x30) == Some(0xE9)).then(|| patterns::rip_target(at + 0x31, at + 0x35)).flatten();
            let calls = (patterns::call_target(at + 0x16), patterns::call_target(at + 0x21), refill);
            let (Some(store_a), Some(store_b), Some(refill)) = calls else {
                crate::warn!("bonfire: the game's rest isn't as expected; mods can't hold or make a rest");
                return None;
            };
            Some(RestCalls { at, store_a, store_b, refill })
        })
        .as_ref()
}

/// Whether the player sits at a bonfire (SprjWorldTalkMan +0x2038).
pub fn sitting() -> Result<bool, String> {
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    let at = AT
        .get_or_init(|| memory::class_statics().into_iter().find(|(n, _)| n == "SprjWorldTalkManImpl").map(|(_, a)| a))
        .ok_or("the game's talk manager isn't found on this build")?;
    let man = memory::read_u64(at).filter(|&p| p != 0).ok_or("the game's talk manager isn't there yet")?;
    memory::read::<u8>(man as usize + 0x2038).map(|b| b != 0).ok_or_else(|| "the game's talk manager is unreadable".into())
}

/// The game's rest at a bonfire (its entity id), as OnEvent_BonfireRespawn does it: the world
/// takes it as the bonfire rested at (enemies back, HP and FP), and the Estus is refilled.
pub fn rest(entity: i32) -> Result<(), String> {
    if !crate::on_game_thread() {
        return Err("a rest is made from the game's thread".into());
    }
    game::player_chr_addr()?;
    if entity <= 0 || !all()?.iter().any(|b| b.entity_id == entity) {
        return Err(format!("{entity} isn't a bonfire"));
    }
    let c = rest_calls().ok_or("the game's rest is unavailable on this game build")?;
    let store_a: extern "C" fn(usize, i32) = unsafe { std::mem::transmute(c.store_a) };
    let store_b: extern "C" fn(usize, i32) = unsafe { std::mem::transmute(c.store_b) };
    let refill: extern "C" fn() = unsafe { std::mem::transmute(c.refill) };
    store_a(0, entity);
    store_b(0, entity);
    refill();
    crate::info!("bonfire: rested at {entity}");
    Ok(())
}

/// OnEvent_BonfireRespawn: a mod may hold the rest back (bonfire.rest_requested).
fn bonfire_respawn_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let bonfire = memory::read::<i32>(r.rcx as usize + RESPAWN_BONFIRE_AT).unwrap_or(0);
    if REST_REQUESTED.decide(&[bonfire as i64, sitting().unwrap_or(false) as i64]) == 1 {
        crate::info!("bonfire: the rest at {bonfire} is held back (a mod's answer)");
        return 0;
    }
    let f: extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize, r.rdx as usize)
}

static BONFIRE_RESPAWN_SITE: Site = Site {
    area: "bonfire",
    name: "bonfire respawn",
    pattern: BONFIRE_RESPAWN,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(bonfire_respawn_hook),
    without: "mods can't hold a rest at a bonfire back",
};

fn install_rest_hook() -> Result<(), String> {
    let c = rest_calls().ok_or("the game's rest isn't as expected")?;
    BONFIRE_RESPAWN_SITE.hook_all(&[c.at]).then_some(()).ok_or_else(|| BONFIRE_RESPAWN_SITE.without.to_string())
}

/// kindle.game: a rest at a bonfire is starting (catalogue.rs).
pub static REST_REQUESTED: HookPoint = HookPoint::hooked(
    "bonfire.rest_requested",
    "A rest at a bonfire is starting (the bonfire's entity id, 0 for none; 1 if the player sits at it, 0 if the game rests otherwise, as when the player dies): \"allow\" it, or \"hold\" it back (the player sits on, unrested, until bonfire.rest). On the game's thread, a few times a minute at most.",
    &["allow", "hold"],
    install_rest_hook,
);

/// kindle.game: the game's rest at a bonfire (catalogue.rs).
pub static REST: catalogue::Function = catalogue::Function {
    name: "bonfire.rest",
    doc: "Rests at a bonfire (its entity id) as the game does when the player sits: enemies back, HP and FP restored, the Estus refilled, the bonfire the one last rested at.",
    args: &[("entity", catalogue::Kind::Int { min: 1, max: i32::MAX as i64 })],
    call: |a| match a {
        [catalogue::Value::Int(e)] => rest(*e as i32).map(|_| None),
        _ => Err("bonfire.rest takes an entity id".into()),
    },
};

/// kindle.game: whether the player sits at a bonfire (catalogue.rs).
pub static SITTING: catalogue::Field = catalogue::Field {
    name: "bonfire.sitting",
    doc: "Whether the player sits at a bonfire (from the sitting down until it stands up).",
    kind: catalogue::Kind::Bool,
    per_character: false,
    get: |_| sitting().map(catalogue::Value::Bool),
    set: None,
};

// ── travel ──────────────────────────────────────────────────────────────────────────────────

/// Talk command 41 opens the travel screen (bonfire menus).
pub const TALK_TRAVEL: i32 = 41;
/// A bonfire's talk, after its travel command (41), waits for the travel screen's state (talk
/// function 102, the talk target's +0x40) to pass 1 (most bonfires; Firelink Shrine's doesn't ask
/// it), then, until the bonfire's menu has closed, asks each frame whether the player warped (talk
/// function 51, a byte of the game's): yes, it waits for the warp to end the talk; no, it builds
/// the bonfire's menu again. The command's opening of the travel screen (+0x7701B0) clears that
/// byte, and a warp chosen there sets it (+0x770890); refused, the byte still held the last
/// travel's 1. Kindle answered 51 with 0 once: while the menu took more than a frame to close, the
/// next frame's 51 read the 1, the talk waited for a warp that wasn't coming and the player sat at
/// the bonfire with no menu, unable to get up or open Esc (seen in co-op play 2026-10-14, 2026-10-18
/// and 2026-10-19: Travel chosen as a teammate stood up). So a refusal clears the byte as the screen's
/// opening does (the game's own answer is then 0 for as long as the talk asks), and 102 answers 2
/// until the talk's next command, so the talk goes back to the bonfire's menu as after Back on the
/// travel screen.
pub const TALK_TRAVEL_STATE: i32 = 102;
pub static TRAVEL_REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The travel command's opening of the travel screen (+0x7701B6, after its `push rdi; sub rsp`):
/// unless a travel screen is already asked for (menu request 0x24), it asks for one, keeps the
/// bonfire travelled from, and clears the "warped" byte (`mov byte [rip+X], 0`, TRAVEL_OPEN_WARPED).
const TRAVEL_OPEN: &str = "8b f9 b9 24 00 00 00 e8 ?? ?? ?? ?? 85 c0 75 ?? 48 89 5c 24 30 e8 ?? ?? ?? ?? 48 8b d8 e8 ?? ?? ?? ?? \
    44 8b c7 48 8b c8 48 8b d3 e8 ?? ?? ?? ?? 48 8b 5c 24 30 b9 24 00 00 00 8d 51 df 85 c0 7f 03 8d 51 dd e8 ?? ?? ?? ?? \
    89 3d ?? ?? ?? ?? c6 05 ?? ?? ?? ?? 00";
const TRAVEL_OPEN_WARPED: usize = 0x4F;

/// The game's "the player warped from the travel screen" byte (talk function 51 reads it).
fn warped_byte() -> Option<usize> {
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    *AT.get_or_init(|| {
        let at = patterns::find_unique("travel screen open", TRAVEL_OPEN)? + TRAVEL_OPEN_WARPED;
        patterns::rip_target(at + 2, at + 7)
    })
}

/// kindle.game: travel was chosen at a bonfire (catalogue.rs; hooks.rs's talk hooks ask it).
pub static TRAVEL_REQUESTED: HookPoint = HookPoint::hooked(
    "bonfire.travel_requested",
    "Travel was chosen in a bonfire's menu: \"allow\" the travel screen, or \"refuse\" it (the bonfire's menu comes back). On the game's thread.",
    &["allow", "refuse"],
    crate::hooks::ensure_talk_hooks,
);

/// Talk command `id` is the bonfire's travel and a mod refuses it (the talk hook's question).
pub fn travel_refused(id: i32) -> bool {
    if id != TALK_TRAVEL {
        return false;
    }
    if !TRAVEL_REQUESTED.answered() || TRAVEL_REQUESTED.decide(&[]) != 1 {
        // A travel screen that opens: its own state and warp are the game's again.
        TRAVEL_REFUSED.store(false, std::sync::atomic::Ordering::Relaxed);
        return false;
    }
    let Some(warped) = warped_byte() else {
        crate::warn!("bonfire: travel can't be refused on this game build (its \"warped\" byte isn't found); the travel screen opens");
        return false;
    };
    unsafe { std::ptr::write_volatile(warped as *mut u8, 0) };
    TRAVEL_REFUSED.store(true, std::sync::atomic::Ordering::Relaxed);
    true
}

/// The talk ran a command after a refused travel: it went on from the travel, and the travel
/// screen's state is the game's again.
pub fn talk_went_on(id: i32) {
    if id != TALK_TRAVEL {
        TRAVEL_REFUSED.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
