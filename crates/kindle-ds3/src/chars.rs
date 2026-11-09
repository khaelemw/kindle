//! Characters in the world: enemies, NPCs and players.
//!
//! Enumerated through ds3.rs (`WorldChrMan` → loaded world blocks → character sets), plus
//! offsets mapped live and cross-checked with the Grand Archives CE table:
//!
//! | | |
//! |---|---|
//! | NpcParam id | `ChrIns + 0x68` (i32) — found by scanning live enemies for NpcParam row ids |
//! | position | `ChrIns + 0x1F90 → +0x68 → +0xA8 → +0x40 → +0x70` (f32 x, y, z; y is up) |
//! | live team | `ChrIns + 0x74` (u8) — only offset matching NpcParam.teamType on 24 characters |
//! | physics position | `ChrIns.chr_set_entry → +0x28 (physics module) → +0x80` (f32 x, y, z), facing at `+0x74` |
//! | debug flags | `ChrIns + 0x1A08` bit 6 no gravity; `+0x1A09` bit 7 invulnerable, bit 5 model drawn |
//! | modules | `[ChrIns + 0x1F90]`: `+0x18` data (`+0x1C0` bits: 1 no damage, 2 can't die, 4 no stamina use, 5 no FP use), `+0x20` resist (`+0x10` five i32 resistance left: poison, toxic, bleed, curse, frost; `+0x24` their maxima), `+0x28` behaviour (`+0x898` current animation name, UTF-16; `+0xA58` animation speed, f32), `+0x40` super armour (`+0x28` poise left, `+0x2C` max, f32) |
//!
//! The physics position is the one the game simulates (writing it teleports); the
//! `+0x1F90` chain is a derived copy used for reading when physics isn't available.
//!
//! Team values seen at Firelink: 1 = the player, 6 = hostile enemies, 26 = friendly NPCs,
//! 8 = invisible event helpers (huge HP, no souls, no name). Others are reported as "other".
//!
//! Characters are identified by their field-instance handle (stable while the character is
//! loaded) and, when the map defines one, by their entity id (what event scripts and the
//! a randomizer's spoiler log use).

use crate::ds3::{self, ChrIns};
use crate::{memory, params, text_override};

pub struct Character {
    pub handle: u32,
    pub entity_id: Option<i32>,
    pub npc_param_id: i32,
    pub model: String,
    pub hp: i32,
    pub max_hp: i32,
    pub pos: Option<[f32; 3]>,
    /// The main player (this game's).
    pub is_player: bool,
    /// Another player's, in a multiplayer session.
    pub other_player: bool,
    pub team: u8,
    pub addr: usize,
}

impl Character {
    /// A readable classification of the live team.
    pub fn kind(&self) -> &'static str {
        match self.team {
            _ if self.is_player || self.other_player => "player",
            1 => "player",
            6 => "enemy",
            8 => "helper",
            26 => "npc",
            _ => "other",
        }
    }
}

pub fn position(addr: usize) -> Option<[f32; 3]> {
    let mut p = addr;
    for off in [0x1F90, 0x68, 0xA8, 0x40] {
        p = memory::read_u64(p + off)? as usize;
        if p == 0 {
            return None;
        }
    }
    let v = [memory::read::<f32>(p + 0x70)?, memory::read::<f32>(p + 0x74)?, memory::read::<f32>(p + 0x78)?];
    v.iter().all(|f| f.is_finite()).then_some(v)
}

fn describe(chr: &ChrIns, entity_id: Option<i32>, is_player: bool) -> Option<Character> {
    let addr = chr.addr();
    let data = chr.data()?;
    Some(Character {
        handle: chr.field_ins_handle(),
        entity_id,
        npc_param_id: memory::read::<i32>(addr + 0x68).unwrap_or(-1),
        model: data.id(),
        hp: data.hp,
        max_hp: data.max_hp,
        pos: physics_position(addr).or_else(|| position(addr)),
        is_player,
        other_player: false,
        team: memory::read::<u8>(addr + 0x74).unwrap_or(0),
        addr,
    })
}

/// Every loaded character: the main player first, then world block characters.
pub fn all() -> Result<Vec<Character>, String> {
    let blocks = ds3::block_characters().ok_or_else(|| "the world isn't loaded".to_string())?;
    let mut out = Vec::new();
    let main = ds3::main_player().map(|p| p.addr());
    if let Some(p) = ds3::main_player() {
        out.extend(describe(p.chr(), None, true));
    }
    for (chr, entity_id) in blocks {
        if Some(chr) != main {
            out.extend(describe(unsafe { ChrIns::at(chr) }, entity_id, false));
        }
    }
    Ok(out)
}

/// The other players' characters in a multiplayer session (WorldChrMan +0x40, +0x38 how many: the
/// game's player characters, the main one among them; not in the world blocks). Read only through
/// checked reads: a player's character can be on its way out as its player leaves.
pub fn other_players() -> Vec<Character> {
    let Some(world) = ds3::WORLD_CHR_MAN.instance() else { return Vec::new() };
    let main = ds3::main_player().map(|p| p.addr());
    let n = memory::read::<i32>(world + 0x38).unwrap_or(0).clamp(0, 8) as usize;
    let list = memory::read_u64(world + 0x40).unwrap_or(0) as usize;
    (0..n)
        .filter_map(|i| {
            let addr = memory::read_u64(list + i * 0x38).filter(|&p| p != 0)? as usize;
            if Some(addr) == main {
                return None;
            }
            let modules = memory::read_u64(addr + 0x1F90).filter(|&p| p != 0)? as usize;
            let data = memory::read_u64(modules + 0x18).filter(|&p| p != 0)? as usize;
            let entry = memory::read_u64(addr + 0x18).filter(|&p| p != 0)? as usize;
            let physics = memory::read_u64(entry + 0x28).filter(|&p| p != 0)? as usize;
            let pos = [memory::read::<f32>(physics + 0x80)?, memory::read::<f32>(physics + 0x84)?, memory::read::<f32>(physics + 0x88)?];
            Some(Character {
                handle: memory::read::<u32>(addr + 0x08)?,
                entity_id: None,
                npc_param_id: -1,
                model: String::from("c0000"),
                hp: memory::read::<i32>(data + 0xD8)?,
                max_hp: memory::read::<i32>(data + 0xDC).unwrap_or(0),
                pos: pos.iter().all(|f| f.is_finite()).then_some(pos),
                is_player: false,
                other_player: true,
                team: memory::read::<u8>(addr + 0x74).unwrap_or(0),
                addr,
            })
        })
        .collect()
}

/// Every loaded character, the other players' in a session too (after the world blocks').
pub fn all_with_players() -> Result<Vec<Character>, String> {
    let mut list = all()?;
    let others = other_players();
    list.retain(|c| !others.iter().any(|o| o.addr == c.addr));
    list.extend(others);
    Ok(list)
}

/// The players' characters: this game's, then (in a session) the others'. Read without going
/// through the world's characters: a mod asking for the players every frame took 2 ms a frame
/// for the ~100 characters of an area (measured 2026-10-18).
pub fn players() -> Vec<Character> {
    let mut list: Vec<Character> = ds3::main_player().and_then(|p| describe(p.chr(), None, true)).into_iter().collect();
    list.extend(other_players());
    list
}

/// Finds a character by handle or entity id (the other players' in a session too). Players
/// first, without reading the world's characters.
pub fn find(key: i64) -> Result<Character, String> {
    if let Some(c) = players().into_iter().find(|c| c.handle as i64 == key) {
        return Ok(c);
    }
    let list = all_with_players()?;
    list.into_iter()
        .find(|c| c.handle as i64 == key || c.entity_id.map(i64::from) == Some(key))
        .ok_or_else(|| format!("no loaded character with handle or entity id {key}"))
}

/// The character's display name from NpcParam.nameId → NPC name text, if it has one.
pub fn name(npc_param_id: i32) -> Option<String> {
    if npc_param_id <= 0 {
        return None;
    }
    let reg = params::registry().ok()?;
    let table = reg.table("NpcParam").ok()?;
    let name_id = match table.read(npc_param_id as u64, "nameId").ok()? {
        kindle_paramdef::Value::Int(i) if i > 0 => i as u32,
        _ => return None,
    };
    let cats = text_override::category_ids("npc_name").ok()?;
    text_override::get_any(&cats, name_id).filter(|s| !s.trim().is_empty())
}

pub fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Sets a character's HP (clamped to 0..=max). Writing 0 kills it.
pub fn set_hp(addr: usize, value: i64) -> Result<(), String> {
    let data = unsafe { ChrIns::at(addr) }.data().ok_or("the character's data isn't available")?;
    data.hp = value.clamp(0, data.max_hp as i64) as i32;
    Ok(())
}

// ── physics position, facing, flags ────────────────────────────────────────

/// The character's physics module (ChrIns.chr_set_entry → +0x28).
fn physics(addr: usize) -> Option<usize> {
    let entry = unsafe { ChrIns::at(addr) }.chr_set_entry();
    let p = memory::read_u64(entry + 0x28)? as usize;
    (p != 0).then_some(p)
}

pub fn physics_position(addr: usize) -> Option<[f32; 3]> {
    let p = physics(addr)?;
    let v = [memory::read::<f32>(p + 0x80)?, memory::read::<f32>(p + 0x84)?, memory::read::<f32>(p + 0x88)?];
    v.iter().all(|f| f.is_finite()).then_some(v)
}

/// Moves a character by writing its physics position.
pub fn teleport(addr: usize, to: [f32; 3]) -> Result<(), String> {
    if !to.iter().all(|f| f.is_finite()) {
        return Err("teleport target must be finite numbers".into());
    }
    let p = physics(addr).ok_or("this character has no physics module")?;
    for (i, v) in to.iter().enumerate() {
        unsafe { std::ptr::write_volatile((p + 0x80 + i * 4) as *mut f32, *v) };
    }
    Ok(())
}

/// Facing angle in radians.
pub fn facing(addr: usize) -> Option<f32> {
    memory::read::<f32>(physics(addr)? + 0x74)
}

pub fn set_facing(addr: usize, radians: f32) -> Result<(), String> {
    let p = physics(addr).ok_or("this character has no physics module")?;
    unsafe { std::ptr::write_volatile((p + 0x74) as *mut f32, radians) };
    Ok(())
}

pub fn flag(at: usize, bit: u8) -> Option<bool> {
    memory::read::<u8>(at).map(|b| b & (1 << bit) != 0)
}

pub fn set_flag(at: usize, bit: u8, on: bool) -> Result<(), String> {
    let b = memory::read::<u8>(at).ok_or("character flags are unreadable")?;
    let v = if on { b | (1 << bit) } else { b & !(1 << bit) };
    unsafe { std::ptr::write_volatile(at as *mut u8, v) };
    Ok(())
}

/// The characters a mod hid (world.set_visible false), by address: not drawn on purpose (the
/// watchdog's reports).
static HIDDEN_BY_MODS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

/// A mod showed or hid a character.
pub(crate) fn mod_set_visible(addr: usize, on: bool) {
    let mut hidden = HIDDEN_BY_MODS.lock().unwrap_or_else(|e| e.into_inner());
    hidden.retain(|&a| a != addr);
    if !on {
        hidden.push(addr);
    }
}

/// Whether a mod hid this character.
pub(crate) fn hidden_by_mod(addr: usize) -> bool {
    HIDDEN_BY_MODS.lock().unwrap_or_else(|e| e.into_inner()).contains(&addr)
}

/// Characters with no entity id (event commands can't target them) whose AI a mod turned off:
/// (handle, ChrIns, mod). Held off and given back like the event-command ones (emevd AI_OFF).
static AI_OFF_BY_HANDLE: std::sync::Mutex<Vec<(u32, usize, String)>> = std::sync::Mutex::new(Vec::new());

/// Turns a character's AI on or off as the game's own setter does (1.15.2 +0x8887E0, the one
/// event command 2004[01] calls: bit 0 of `[ChrIns +0x50] +0x182`), for a character without an
/// entity id; off holds until the mod turns it on or unloads.
pub fn set_ai(c: &Character, on: bool, owner: &str) -> Result<(), String> {
    crate::game::set_ai_enabled(c.addr, on).ok_or("this character has no AI module")?;
    let mut held = AI_OFF_BY_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
    held.retain(|(handle, _, _)| *handle != c.handle);
    if !on {
        held.push((c.handle, c.addr, owner.to_string()));
    }
    Ok(())
}

/// Keeps the AI held off (the game resets it when a dormant enemy wakes) for the characters
/// still loaded; forgets the others (a reload gives them back their AI).
pub fn keep_ai_off(loaded: &[Character]) {
    let mut held = AI_OFF_BY_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
    held.retain(|(handle, addr, _)| {
        let still = loaded.iter().any(|c| c.handle == *handle && c.addr == *addr);
        if still && crate::game::ai_enabled(*addr) == Some(true) {
            crate::game::set_ai_enabled(*addr, false);
        }
        still
    });
}

/// A mod unloaded: the AI it held off comes back on (for the characters still loaded).
pub fn release_ai(owner: &str, loaded: &[Character]) {
    let mut held = AI_OFF_BY_HANDLE.lock().unwrap_or_else(|e| e.into_inner());
    held.retain(|(handle, addr, o)| {
        if o != owner {
            return true;
        }
        if loaded.iter().any(|c| c.handle == *handle && c.addr == *addr) {
            crate::game::set_ai_enabled(*addr, true);
        }
        false
    });
}

/// What event commands 2004[16] "clear AI target" and 2004[20] "AI re-plan" call for their
/// character (1.15.2 +0x4119B0 and +0x411D40, each taking the ChrIns: EventChrEntity::vf41
/// and vf45); for characters with no entity id.
const CLEAR_TARGET_PATTERN: &str =
    "40 53 48 83 ec 20 48 8b d9 48 85 c9 74 34 48 8b 01 ff 90 70 01 00 00 48 85 c0 74 26 48 8b 88 20 03 00 00 48 85 c9 74 1a 48 8b 89 b8";
const REPLAN_PATTERN: &str =
    "48 89 5c 24 08 57 48 83 ec 20 40 32 ff 48 8b d9 48 85 c9 74 29 48 8b 01 ff 90 70 01 00 00 48 85 c0 74 1b 48 8b 88 20 03 00 00 48 85";
static CLEAR_TARGET: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
static REPLAN: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();

/// Makes a character forget its target and choose what to do again, as the two event
/// commands do.
pub fn clear_aggro(chr: usize) -> Result<(), String> {
    let clear = *CLEAR_TARGET.get_or_init(|| crate::patterns::find_unique("clear AI target", CLEAR_TARGET_PATTERN));
    let replan = *REPLAN.get_or_init(|| crate::patterns::find_unique("AI re-plan", REPLAN_PATTERN));
    let (Some(clear), Some(replan)) = (clear, replan) else {
        return Err("clearing aggro is unavailable on this game build".into());
    };
    type ChrFn = unsafe extern "C" fn(usize) -> u64;
    unsafe {
        std::mem::transmute::<usize, ChrFn>(clear)(chr);
        std::mem::transmute::<usize, ChrFn>(replan)(chr);
    }
    Ok(())
}

/// Whether the game draws a character's model (ChrIns +0x1A09 bit 5: world.set_visible).
pub(crate) fn drawn(addr: usize) -> Option<bool> {
    memory::read::<u8>(addr + 0x1A09).map(|b| b & (1 << 5) != 0)
}

/// Named per-character flags: the address of the flag byte and its bit.
pub fn flag_location(addr: usize, name: &str) -> Result<(usize, u8), String> {
    Ok(match name {
        "invulnerable" => (addr + 0x1A09, 7),
        "no_gravity" => (addr + 0x1A08, 6),
        "draw_model" => (addr + 0x1A09, 5),
        "no_death" => (module(addr, 0x18).ok_or("this character has no data module")? + 0x1C0, 2),
        "no_damage" => (module(addr, 0x18).ok_or("this character has no data module")? + 0x1C0, 1),
        "no_stamina_use" => (module(addr, 0x18).ok_or("this character has no data module")? + 0x1C0, 4),
        "no_fp_use" => (module(addr, 0x18).ok_or("this character has no data module")? + 0x1C0, 5),
        // PlayerIns only (the game's "no goods consume" debug flag).
        "no_item_use" => (addr + 0x1EEA, 3),
        other => {
            return Err(format!(
                "unknown character flag {other:?} (invulnerable, no_gravity, draw_model, no_death, no_damage, no_stamina_use, no_fp_use, no_item_use)"
            ));
        }
    })
}

// ── character modules: status build-up, poise, animation state ─────────────

/// A module from the character's module container (`[ChrIns + 0x1F90] + off`).
fn module(addr: usize, off: usize) -> Option<usize> {
    let container = memory::read_u64(addr + 0x1F90)? as usize;
    if container == 0 {
        return None;
    }
    let m = memory::read_u64(container + off)? as usize;
    (m != 0).then_some(m)
}

pub const STATUSES: [&str; 5] = ["poison", "toxic", "bleed", "curse", "frost"];

/// (build-up, max) per status, in STATUSES order. Build-up is how far the bar has filled;
/// the effect triggers when it reaches max.
pub fn status(addr: usize) -> Result<Vec<(i32, i32)>, String> {
    let m = module(addr, 0x20).ok_or("this character has no resistance module")?;
    (0..5)
        .map(|i| {
            let left = memory::read::<i32>(m + 0x10 + i * 4)?;
            let max = memory::read::<i32>(m + 0x24 + i * 4)?;
            Some(((max - left).max(0), max))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "resistances are unreadable".to_string())
}

/// (current, max) poise. Most characters only have poise during certain attacks.
pub fn poise(addr: usize) -> Result<(f32, f32), String> {
    let m = module(addr, 0x40).ok_or("this character has no super armour module")?;
    match (memory::read::<f32>(m + 0x28), memory::read::<f32>(m + 0x2C)) {
        (Some(cur), Some(max)) => Ok((cur, max)),
        _ => Err("poise is unreadable".into()),
    }
}

/// The handle of the character the player has locked on to, if any, as the lock-on manager keeps
/// it (its update, +0x8651E0, picks the target): `LockTgtManImp + 0x2820` is set while locked on,
/// its candidates are a list from `+0x8` (next at `+0x80`), the one locked on has bit 0x20 at
/// `+0x90`, and its `+0x88` is the target's record, whose `+0x64` is the character's handle
/// (+0x862920 looks the character up by it when the top nibble is 1).
pub fn lock_target() -> Option<u32> {
    let man = ds3::singleton_instance("LockTgtManImp")?;
    if memory::read::<u8>(man + 0x2820)? == 0 {
        return None;
    }
    let mut entry = memory::read::<usize>(man + 0x8)?;
    for _ in 0..256 {
        if entry == 0 {
            return None;
        }
        if memory::read::<u8>(entry + 0x90)? & 0x20 != 0 {
            let record = memory::read::<usize>(entry + 0x88).filter(|&p| p != 0)?;
            let handle = memory::read::<u32>(record + 0x64)?;
            return (handle >> 28 == 1).then_some(handle);
        }
        entry = memory::read::<usize>(entry + 0x80)?;
    }
    None
}

/// Locks the player on to the character with this handle, or releases the lock (None), as the
/// game's own lock-on does: the lock-on button asks for it at `LockTgtManImp + 0x2821` (the
/// update, +0x8651E0, copies it into +0x2820 each frame; releasing writes 0), and the update
/// keeps whichever candidate holds bit 0x20 at `+0x90` by scoring it above every other while
/// locked on. Moving that bit is how the game itself holds a target. Only a candidate the game
/// can lock on to now (bit 0x10: in range and in view) can be chosen: false otherwise.
pub fn set_lock_target(handle: Option<u32>) -> Result<bool, String> {
    let man = ds3::singleton_instance("LockTgtManImp").ok_or("the lock-on manager isn't there")?;
    let request = |on: bool| unsafe { std::ptr::write_volatile((man + 0x2821) as *mut u8, on as u8) };
    let Some(handle) = handle else {
        request(false);
        return Ok(true);
    };
    let mut entries = Vec::new();
    let mut entry = memory::read::<usize>(man + 0x8).unwrap_or(0);
    while entry != 0 && entries.len() < 256 {
        entries.push(entry);
        entry = memory::read::<usize>(entry + 0x80).unwrap_or(0);
    }
    let handle_of = |e: usize| memory::read::<usize>(e + 0x88).filter(|&r| r != 0).and_then(|r| memory::read::<u32>(r + 0x64));
    let Some(&target) = entries.iter().find(|&&e| handle_of(e) == Some(handle) && memory::read::<u8>(e + 0x90).is_some_and(|b| b & 0x10 != 0)) else {
        return Ok(false);
    };
    for &e in &entries {
        let Some(bits) = memory::read::<u8>(e + 0x90) else { continue };
        let bits = if e == target { bits | 0x20 } else { bits & !0x20 };
        unsafe { std::ptr::write_volatile((e + 0x90) as *mut u8, bits) };
    }
    request(true);
    Ok(true)
}

/// Name of the animation state the character is in, e.g. "Idle".
pub fn animation_name(addr: usize) -> Option<String> {
    let s = memory::read_wstr(module(addr, 0x28)? + 0x898, 64)?;
    (!s.is_empty()).then_some(s)
}

pub fn animation_speed(addr: usize) -> Option<f32> {
    memory::read::<f32>(module(addr, 0x28)? + 0xA58)
}

pub fn set_animation_speed(addr: usize, speed: f32) -> Result<(), String> {
    if !speed.is_finite() || !(0.0..=10.0).contains(&speed) {
        return Err("animation speed must be between 0 and 10".into());
    }
    let m = module(addr, 0x28).ok_or("this character has no behaviour module")?;
    unsafe { std::ptr::write_volatile((m + 0xA58) as *mut f32, speed) };
    Ok(())
}

/// Changes the live team (see `Character::kind`): e.g. 6 hostile, 26 friendly NPC.
pub fn set_team(addr: usize, team: u8) -> Result<(), String> {
    memory::read::<u8>(addr + 0x74).ok_or("this character's team is unreadable")?;
    unsafe { std::ptr::write_volatile((addr + 0x74) as *mut u8, team) };
    Ok(())
}
