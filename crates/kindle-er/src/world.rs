//! Elden Ring's characters (kindle.world): listed, or found by entity id with the game's own lookup, read where the
//! player is read (player.rs's layout: every character is a ChrIns), their effects applied and ended
//! through the game's own functions.
//!
//! The lookup (`FUN_1405eefd0(&entity, 0, out)`) is the one the event commands use to find their
//! character: 10000 (and 20000) give WorldChrMan's main player, any other entity id its character from
//! WorldChrMan's sets, null when that character isn't loaded (its map isn't). A character's active
//! effects hang off `[chr + 0x178]`: the first at +8, each with its SpEffectParam row at +8 and the
//! next at +0x30. An effect is applied by `FUN_1403e8dc0(chr, id, 0)` (what Set SpEffect's command
//! calls for one character) and one instance of it ended by `FUN_1404f7680(effects, id)` (Clear
//! SpEffect's), which says whether it found one. A row SpEffectParam hasn't is refused before the call.
//!
//! Every loaded character is listed from WorldChrMan's character sets, which the game keeps by set index
//! (`[WorldChrMan + 0x1DED8 + 8n]`, n to 0xC4, as its find by handle reads them): the main player's,
//! Torrent's and the summons', and the open world's (an OpenFieldChrSet, some 500 at a time). A set has
//! its capacity at +0x10 and its entries at +0x18, 16 bytes each, the character first (the walk the
//! game's find by name does). A character's handle is at +8 (its index and set, then its map), the map
//! block it's in at +0x38, its NpcParam row at +0x60, its NpcThinkParam row at +0x64, its team at +0x6C
//! and its entity id at +0x1E8 (checked against the lookup on each list); its name, the map part's
//! ("c1000_9000[42 36 0]", its model first), is its data module's game string at +0x220. A character
//! without an entity id is named by its handle.
//!
//! Characters are looked up on the game's thread each time (Kindle keeps no pointer to one: the game
//! frees a character as its map unloads).

use std::sync::OnceLock;

use kindle_core::{memory, patterns};

use crate::task;

/// The lookup, the effect's apply and the end of one of its instances (each found once).
const LOOKUP: &str = "48 89 5C 24 08 48 89 74 24 10 48 89 7C 24 18 41 56 48 83 EC 20 48 8B 3D ?? ?? ?? ?? 33 DB 49 8B F0 4C 8B F1 48 85 FF 0F 84 ?? ?? ?? ??";
const APPLY: &str = "48 8B C4 48 89 58 08 48 89 70 10 57 48 81 EC C0 00 00 00 0F 28 05 ?? ?? ?? ?? 48 8B F1 0F 28 0D ?? ?? ?? ?? 48 8D 48 88 0F 29 40 A8";
const REMOVE: &str = "48 83 EC 28 8B C2 48 8B 51 08 48 85 D2 74 ?? 90 39 42 08 74 ?? 48 8B 52 30";

/// A character's effects, the first of them, an effect's row and the next.
const EFFECTS: usize = 0x178;
const FIRST: usize = 0x08;
const EFFECT_ID: usize = 0x08;
const NEXT: usize = 0x30;
/// Most effects Kindle walks (a character has a few dozen).
const MOST_EFFECTS: usize = 512;

/// The game's find by handle reading WorldChrMan's sets by index, and its find by name walking a set's
/// entries to the name in a character's data module (each found once).
const SET_TABLE: &str = "41 81 F8 C4 00 00 00 77 ?? 4A 8B 9C C3 D8 DE 01 00 48 85 DB 74 ?? 48 8B CF E8";
const SET_WALK: &str = "48 63 69 10 33 FF 4D 8B F8 4C 8B E2 4C 8B F1 48 85 ED 0F 8E ?? ?? ?? ?? 48 89 5C 24 58 48 89 74 24 60 8B F7 49 8B 46 18 48 8B 1C 06 48 85 DB 0F 84 ?? ?? ?? ?? 48 8B 83 90 01 00 00 48 8D 54 24 50 48 8B 08 E8 ?? ?? ?? ?? 48 8B C8 48 8D 54 24 68 E8 ?? ?? ?? ?? 48 8B C8 41 8B 04 24 39 01 75 ?? 48 8B 83 90 01 00 00 48 8B 08 E8 ?? ?? ?? ?? 48 85 C0 48 8B 83 90 01 00 00 74 ?? 48 8B 08 E8 ?? ?? ?? ?? EB ?? 48 8B 00 48 05 28 02 00 00 48 83 78 18 08 72 ?? 48 8B 00";

/// WorldChrMan's sets by index and how many; a set's capacity, entries and entry size; most entries Kindle
/// walks in one set.
const SETS: usize = 0x1DED8;
const SET_COUNT: usize = 0xC5;
const SET_CAPACITY: usize = 0x10;
const SET_ENTRIES: usize = 0x18;
const SET_ENTRY: usize = 0x10;
const MOST_IN_SET: usize = 4096;

/// A character's handle, map block (else its handle's), NpcParam and NpcThinkParam rows, team and entity id; its data
/// module's name (a game string: characters at +8, inline below 8, length +0x18, capacity +0x20).
const HANDLE: usize = 0x08;
const BLOCK: usize = 0x38;
const NPC: usize = 0x60;
const THINK: usize = 0x64;
const TEAM: usize = 0x6C;
const ENTITY: usize = 0x1E8;
const NAME: usize = 0x220;
const MOST_NAME: usize = 256;

/// Elden Ring's invisible event characters (the maps' c0100 and c1000 dummies).
const HELPERS: [&str; 2] = ["c0100", "c1000"];

/// A character's kind by its team (the numbers the event commands' team names stand for: enemy 6, strong
/// enemy 7, ally 8, hostile ally 9, charmed 15, the friendly, hostile and co-op NPCs' 26 to 28,
/// indiscriminate 29); another player's character is a PlayerIns of a players' team (human 1, host 19,
/// co-op 20).
fn kind_of(team: u8, class: Option<&str>) -> &'static str {
    match team {
        26..=28 => "npc",
        6 | 7 | 9 | 29 => "enemy",
        8 | 15 => "ally",
        1 | 19 | 20 if class == Some("PlayerIns") => "player",
        _ => "other",
    }
}

/// The player, as the game's lookup and event scripts name it.
pub const PLAYER: u32 = 10000;

/// A character's modules, its data module (the first: HP at +0x138, its maximum after it) and physics
/// module (+0x68: the position at +0x70), as player.rs reads the player's.
const MODULES: usize = 0x190;
const HP: usize = 0x138;
const PHYSICS: usize = 0x68;
const POSITION: usize = 0x70;

type Lookup = unsafe extern "C" fn(*const i32, u64, *mut u32) -> usize;
type Apply = unsafe extern "C" fn(usize, i32, u8);
type Remove = unsafe extern "C" fn(usize, i32) -> u8;

struct Functions {
    lookup: usize,
    apply: usize,
    remove: usize,
}

fn functions() -> Result<&'static Functions, String> {
    #[cfg(test)]
    if let Some(f) = tests::FAKE.with(|f| f.get()) {
        return Ok(f);
    }
    static FOUND: OnceLock<Option<Functions>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            Some(Functions {
                lookup: patterns::find_unique("character lookup", LOOKUP)?,
                apply: patterns::find_unique("effect apply", APPLY)?,
                remove: patterns::find_unique("effect end", REMOVE)?,
            })
        })
        .as_ref()
        .ok_or_else(|| "Kindle doesn't know where this build looks up its characters".into())
}

/// The character with this entity id, if it's loaded (the player is 10000).
pub fn character(entity: u32) -> Result<Option<usize>, String> {
    task::on_games_thread("characters are looked up")?;
    let f = functions()?;
    if !crate::player::is_loaded()? {
        return Ok(None);
    }
    let id = entity as i32;
    let lookup: Lookup = unsafe { std::mem::transmute(f.lookup) };
    Ok(Some(unsafe { lookup(&id, 0, std::ptr::null_mut()) }).filter(|&c| c != 0))
}

/// Whether a key names a character by entity id (1 to 2^32 - 1) rather than by handle.
fn is_entity(key: i64) -> bool {
    (1..=u32::MAX as i64).contains(&key)
}

/// The character a key names: an entity id, or a handle `characters` gave.
pub fn find(key: i64) -> Result<Option<usize>, String> {
    if is_entity(key) {
        return character(key as u32);
    }
    Ok(all()?.into_iter().find(|&c| memory::read_u64(c + HANDLE) == Some(key as u64)))
}

fn loaded(key: i64) -> Result<usize, String> {
    let what = if is_entity(key) { "entity id" } else { "handle" };
    find(key)?.ok_or_else(|| format!("no character with {what} {key} is loaded"))
}

/// WorldChrMan's sets and their layout, checked once against the game's code.
fn sets_checked() -> Result<(), String> {
    #[cfg(test)]
    if tests::FAKE.with(|f| f.get()).is_some() {
        return Ok(());
    }
    static CHECKED: OnceLock<bool> = OnceLock::new();
    CHECKED
        .get_or_init(|| patterns::find_unique("character sets", SET_TABLE).is_some() && patterns::find_unique("character set walk", SET_WALK).is_some())
        .then_some(())
        .ok_or_else(|| "Kindle doesn't know where this build keeps its characters".into())
}

fn world_chr_man() -> Option<usize> {
    #[cfg(test)]
    if tests::FAKE.with(|f| f.get()).is_some() {
        return Some(tests::WORLD.with(|w| w.get())).filter(|&w| w != 0);
    }
    crate::er::instance("CS::WorldChrMan")
}

/// Every loaded character, set by set.
fn all() -> Result<Vec<usize>, String> {
    task::on_games_thread("characters are listed")?;
    sets_checked()?;
    if !crate::player::is_loaded()? {
        return Ok(Vec::new());
    }
    let Some(world) = world_chr_man() else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for n in 0..SET_COUNT {
        let Some(set) = memory::read_u64(world + SETS + 8 * n).filter(|&s| s != 0).map(|s| s as usize) else { continue };
        let capacity = memory::read::<i32>(set + SET_CAPACITY).unwrap_or(0).clamp(0, MOST_IN_SET as i32) as usize;
        let Some(entries) = memory::read_u64(set + SET_ENTRIES).filter(|&e| e != 0).map(|e| e as usize) else { continue };
        out.extend((0..capacity).filter_map(|i| memory::read_u64(entries + i * SET_ENTRY)).filter(|&c| c != 0).map(|c| c as usize));
    }
    Ok(out)
}

/// A character as `characters` lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub handle: i64,
    pub entity: Option<u32>,
    pub npc: Option<i32>,
    pub think: Option<i32>,
    pub name: Option<String>,
    pub model: Option<String>,
    pub kind: &'static str,
    pub team: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub map: u32,
    pub position: Option<(f32, f32, f32)>,
    pub is_player: bool,
}

/// A character's map part name, its data module's game string.
fn part_name(chr: usize) -> Option<String> {
    let at = module(chr, 0).ok()? + NAME;
    let len = memory::read_u64(at + 0x18)? as usize;
    if len > MOST_NAME {
        return None;
    }
    let chars = if memory::read_u64(at + 0x20)? < 8 { at + 8 } else { memory::read_u64(at + 8)? as usize };
    let units: Vec<u16> = memory::read_bytes(chars, len * 2)?.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
    Some(String::from_utf16_lossy(&units))
}

/// Whether NpcParam has this row.
fn npc_row(id: i32) -> bool {
    #[cfg(test)]
    return id > 0;
    #[cfg(not(test))]
    kindle_core::params::registry().and_then(|r| r.table("NpcParam").map(|t| t.has_row(id as u64))).unwrap_or(false)
}

/// An NpcParam row's name (its nameId's NpcName text), looked up once per list.
fn npc_name(npc: i32, names: &mut std::collections::HashMap<i32, Option<String>>) -> Option<String> {
    #[cfg(test)]
    return names.get(&npc).cloned().flatten();
    #[cfg(not(test))]
    names
        .entry(npc)
        .or_insert_with(|| {
            let row = kindle_core::params::registry().ok()?.table("NpcParam").ok()?.read(npc as u64, "nameId").ok()?;
            let kindle_paramdef::Value::Int(id) = row else { return None };
            let fmgs = crate::text::category("npc_name").ok()?;
            crate::text::get(&fmgs, u32::try_from(id).ok()?).ok().flatten().filter(|t| !t.is_empty())
        })
        .clone()
}

/// Every loaded character, as the game's sets hold them.
pub fn characters() -> Result<Vec<Listed>, String> {
    let chrs = all()?;
    let player = crate::player::main_player()?;
    let lookup: Lookup = unsafe { std::mem::transmute(functions()?.lookup) };
    let mut names = std::collections::HashMap::new();
    let mut entity_checked = false;
    let mut out = Vec::new();
    for chr in chrs {
        let read_i32 = |off: usize| memory::read::<i32>(chr + off).unwrap_or(0);
        let is_player = Some(chr) == player;
        let mut entity = Some(read_i32(ENTITY) as u32).filter(|&e| e != 0 && e != u32::MAX);
        if let Some(e) = entity.filter(|_| !entity_checked && !is_player) {
            // The entity id's place, checked against the game's own lookup.
            if unsafe { lookup(&(e as i32), 0, std::ptr::null_mut()) } != chr {
                return Err("Kindle doesn't know where this build keeps a character's entity id".into());
            }
            entity_checked = true;
        }
        if is_player {
            entity = Some(PLAYER);
        }
        let npc = Some(read_i32(NPC)).filter(|&n| n > 0 && npc_row(n));
        let class = memory::class_name(chr);
        let model = part_name(chr).map(|n| n.split('_').next().unwrap_or_default().to_string()).filter(|m| m.starts_with('c'));
        let team = memory::read::<u8>(chr + TEAM).unwrap_or(0);
        let kind = if is_player {
            "player"
        } else if model.as_deref().is_some_and(|m| HELPERS.contains(&m)) {
            "helper"
        } else {
            kind_of(team, class.as_deref())
        };
        let [hp, max_hp] = module(chr, 0).ok().and_then(|d| memory::read::<[i32; 2]>(d + HP)).unwrap_or_default();
        out.push(Listed {
            handle: memory::read_u64(chr + HANDLE).unwrap_or(0) as i64,
            entity,
            npc,
            think: Some(read_i32(THINK)).filter(|&t| t > 0),
            name: npc.and_then(|n| npc_name(n, &mut names)),
            model,
            kind,
            team,
            hp,
            max_hp,
            map: map_of(chr).unwrap_or(u32::MAX),
            position: module(chr, PHYSICS).ok().and_then(|p| memory::read::<[f32; 3]>(p + POSITION)).map(|[x, y, z]| (x, y, z)),
            is_player,
        });
    }
    Ok(out)
}

/// The size of an open-world map block of this id's scale (its last number: 0 small, 1 medium, 2 large).
fn block_size(map: u32) -> f32 {
    256.0 * (1u32 << (map & 0xFF).min(2)) as f32
}

/// A position in map block `from` in block `to`'s coordinates: the same block, or two of the open world's
/// (area 60 or 61, each block a square of its grid with its origin at the centre, x east, z north: a
/// small block's run -128 to 128); None for blocks apart (a legacy dungeon's and the open world's).
pub fn relative(from: u32, to: u32, (x, y, z): (f32, f32, f32)) -> Option<(f32, f32, f32)> {
    if from == to {
        return Some((x, y, z));
    }
    let area = |m: u32| m >> 24;
    if area(from) != area(to) || !(60..=61).contains(&area(from)) {
        return None;
    }
    let centre = |m: u32| ((((m >> 16) & 0xFF) as f32 + 0.5) * block_size(m), (((m >> 8) & 0xFF) as f32 + 0.5) * block_size(m));
    let ((fx, fz), (tx, tz)) = (centre(from), centre(to));
    Some((x + fx - tx, y, z + fz - tz))
}

fn module(chr: usize, slot: usize) -> Result<usize, String> {
    let modules = memory::read_u64(chr + MODULES).filter(|&m| m != 0).ok_or("the character has no modules yet")?;
    memory::read_u64(modules as usize + slot).filter(|&m| m != 0).map(|m| m as usize).ok_or_else(|| "the character isn't made yet".into())
}

/// A character's HP and its maximum.
pub fn hp(key: i64) -> Result<(i32, i32), String> {
    let data = module(loaded(key)?, 0)?;
    let [hp, max] = memory::read::<[i32; 2]>(data + HP).ok_or("the character is unreadable")?;
    Ok((hp, max))
}

/// Sets a character's HP, within 0 and its maximum (the player's from 1: player.set_hp).
pub fn set_hp(key: i64, value: i64) -> Result<(), String> {
    let data = module(loaded(key)?, 0)?;
    let max = memory::read::<i32>(data + HP + 4).ok_or("the character is unreadable")?;
    let least: i64 = if key == PLAYER as i64 { 1 } else { 0 };
    unsafe { std::ptr::write_volatile((data + HP) as *mut i32, value.clamp(least, (max as i64).max(least)) as i32) };
    Ok(())
}

/// A character's position: x, y (up), z, in the coordinates of the map it's in.
pub fn position(key: i64) -> Result<(f32, f32, f32), String> {
    let [x, y, z, _] = memory::read::<[f32; 4]>(module(loaded(key)?, PHYSICS)? + POSITION).ok_or("the character is unreadable")?;
    Ok((x, y, z))
}

/// The SpEffectParam rows of a character's active effects, as the game lists them.
pub fn effects(key: i64) -> Result<Vec<i32>, String> {
    let list = memory::read_u64(loaded(key)? + EFFECTS).filter(|&l| l != 0).ok_or("the character has no effects yet")? as usize;
    let mut out = Vec::new();
    let mut e = memory::read_u64(list + FIRST).unwrap_or(0) as usize;
    while e != 0 && out.len() < MOST_EFFECTS {
        out.push(memory::read::<i32>(e + EFFECT_ID).ok_or("the character's effects are unreadable")?);
        e = memory::read_u64(e + NEXT).unwrap_or(0) as usize;
    }
    Ok(out)
}

fn check_effect(id: i64) -> Result<i32, String> {
    let row = i32::try_from(id).map_err(|_| format!("SpEffect {id} is out of range"))?;
    #[cfg(not(test))]
    if !kindle_core::params::registry()?.table("SpEffectParam")?.has_row(row as u64) {
        return Err(format!("SpEffectParam has no row {row}"));
    }
    Ok(row)
}

/// Applies an effect (an SpEffectParam row) to a character, as Set SpEffect does.
pub fn apply_effect(key: i64, id: i64) -> Result<(), String> {
    let row = check_effect(id)?;
    let chr = loaded(key)?;
    let apply: Apply = unsafe { std::mem::transmute(functions()?.apply) };
    unsafe { apply(chr, row, 0) };
    Ok(())
}

/// Ends every instance of an effect on a character, as Clear SpEffect does: how many there were.
pub fn remove_effect(key: i64, id: i64) -> Result<usize, String> {
    let row = i32::try_from(id).map_err(|_| format!("SpEffect {id} is out of range"))?;
    let list = memory::read_u64(loaded(key)? + EFFECTS).filter(|&l| l != 0).ok_or("the character has no effects yet")? as usize;
    let remove: Remove = unsafe { std::mem::transmute(functions()?.remove) };
    let mut n = 0;
    while n < MOST_EFFECTS && unsafe { remove(list, row) } != 0 {
        n += 1;
    }
    Ok(n)
}

/// A listed character as Lua sees it, with its distance from `from` (the player's map block and position)
/// when the two blocks' coordinates meet.
pub(crate) fn listed_table(lua: &mlua::Lua, c: &Listed, from: Option<(u32, (f32, f32, f32))>) -> mlua::Result<mlua::Table> {
    let t = lua.create_table()?;
    t.set("handle", c.handle)?;
    t.set("entity", c.entity)?;
    t.set("npc_id", c.npc)?;
    t.set("think_id", c.think)?;
    t.set("name", c.name.clone())?;
    t.set("model", c.model.clone())?;
    t.set("kind", c.kind)?;
    t.set("team", c.team)?;
    t.set("hp", c.hp)?;
    t.set("max_hp", c.max_hp)?;
    t.set("alive", c.hp > 0)?;
    t.set("map", c.map)?;
    t.set("is_player", c.is_player)?;
    if let Some((x, y, z)) = c.position {
        t.set("x", x)?;
        t.set("y", y)?;
        t.set("z", z)?;
        if let Some((map, (px, py, pz))) = from
            && let Some((x, y, z)) = relative(c.map, map, (x, y, z))
        {
            t.set("distance", ((x - px).powi(2) + (y - py).powi(2) + (z - pz).powi(2)).sqrt())?;
        }
    }
    Ok(t)
}

/// The map block the player is in.
pub fn player_map() -> Option<u32> {
    let chr = crate::player::main_player().ok()??;
    map_of(chr)
}

/// A character's map block: its own, else its handle's.
fn map_of(chr: usize) -> Option<u32> {
    [BLOCK, HANDLE + 4].into_iter().filter_map(|off| memory::read::<u32>(chr + off)).find(|&m| m != u32::MAX)
}

/// The player's map block and position among the listed.
pub(crate) fn from_player(all: &[Listed]) -> Option<(u32, (f32, f32, f32))> {
    all.iter().find(|c| c.is_player).and_then(|p| Some((p.map, p.position?)))
}

/// The natives for kindle.world (and the player's effects, which are the player's character's).
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::rt_err;
    n.set(
        "world_characters",
        lua.create_function(|lua, (radius, alive_only, with_player, with_helpers, kind): (Option<f32>, bool, bool, bool, Option<String>)| {
            let all = characters().map_err(rt_err)?;
            let from = from_player(&all);
            let mut out = Vec::new();
            for c in all.iter().filter(|c| {
                (!alive_only || c.hp > 0)
                    && (with_player || !c.is_player)
                    && (with_helpers || c.kind != "helper" || kind.as_deref() == Some("helper"))
                    && kind.as_deref().is_none_or(|k| k == c.kind)
            }) {
                let t = listed_table(lua, c, from)?;
                let distance: Option<f32> = t.get("distance")?;
                if radius.is_none_or(|r| distance.is_some_and(|d| d <= r)) {
                    out.push((distance.unwrap_or(f32::INFINITY), t));
                }
            }
            out.sort_by(|a, b| a.0.total_cmp(&b.0));
            lua.create_sequence_from(out.into_iter().map(|(_, t)| t))
        })?,
    )?;
    n.set(
        "world_character",
        lua.create_function(|lua, key: i64| {
            let Some(chr) = find(key).map_err(rt_err)? else { return Ok(None) };
            let handle = memory::read_u64(chr + HANDLE).unwrap_or(0) as i64;
            let all = characters().map_err(rt_err)?;
            all.iter().find(|c| c.handle == handle).map(|c| listed_table(lua, c, from_player(&all))).transpose()
        })?,
    )?;
    n.set(
        "world_relative",
        lua.create_function(|_, (from, to, x, y, z): (u32, u32, f32, f32, f32)| {
            Ok(relative(from, to, (x, y, z)).map_or((None, None, None), |(x, y, z)| (Some(x), Some(y), Some(z))))
        })?,
    )?;
    n.set("world_set_hp", lua.create_function(|_, (key, hp): (i64, i64)| set_hp(key, hp).map_err(rt_err))?)?;
    n.set("world_position", lua.create_function(|_, key: i64| position(key).map_err(rt_err))?)?;
    n.set("world_effects", lua.create_function(|_, key: i64| effects(key).map_err(rt_err))?)?;
    n.set("world_apply_effect", lua.create_function(|_, (key, id): (i64, i64)| apply_effect(key, id).map_err(rt_err))?)?;
    n.set("world_remove_effect", lua.create_function(|_, (key, id): (i64, i64)| remove_effect(key, id).map_err(rt_err))?)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    thread_local! {
        pub(crate) static FAKE: Cell<Option<&'static Functions>> = const { Cell::new(None) };
        /// The characters the fake lookup knows: entity id -> character.
        static CHARACTERS: RefCell<Vec<(i32, usize)>> = const { RefCell::new(Vec::new()) };
        /// The effects the fake apply was asked for: (character, row).
        pub(crate) static APPLIED: RefCell<Vec<(usize, i32)>> = const { RefCell::new(Vec::new()) };
        /// A WorldChrMan a test laid out.
        pub(crate) static WORLD: Cell<usize> = const { Cell::new(0) };
    }

    unsafe extern "C" fn lookup(entity: *const i32, _: u64, _: *mut u32) -> usize {
        let id = unsafe { *entity };
        CHARACTERS.with(|c| c.borrow().iter().find(|(e, _)| *e == id).map_or(0, |&(_, chr)| chr))
    }

    unsafe extern "C" fn apply(chr: usize, id: i32, _: u8) {
        APPLIED.with(|a| a.borrow_mut().push((chr, id)));
    }

    /// The game's end of one instance: unlinks the first entry with this row.
    unsafe extern "C" fn remove(list: usize, id: i32) -> u8 {
        unsafe {
            let mut link = (list + FIRST) as *mut usize;
            while *link != 0 {
                if *((*link + EFFECT_ID) as *const i32) == id {
                    *link = *((*link + NEXT) as *const usize);
                    return 1;
                }
                link = (*link + NEXT) as *mut usize;
            }
        }
        0
    }

    static FUNCTIONS: OnceLock<Functions> = OnceLock::new();

    /// A character laid out as the game's: its modules (data first, physics at +0x68) and effects.
    pub(crate) struct Character {
        pub(crate) chr: Vec<u64>,
        _modules: Vec<u64>,
        _data: Vec<i32>,
        _physics: Vec<f32>,
        _effects: Vec<u64>,
        _entries: Vec<Vec<u64>>,
        _name: Vec<u16>,
    }

    impl Character {
        pub(crate) fn new(hp: i32, max: i32, at: [f32; 3], effects: &[i32]) -> Self {
            let mut data = vec![0i32; 0x280 / 4];
            data[HP / 4] = hp;
            data[HP / 4 + 1] = max;
            let mut physics = vec![0f32; 0x100 / 4];
            physics[POSITION / 4..POSITION / 4 + 3].copy_from_slice(&at);
            let mut modules = vec![0u64; 0x100 / 8];
            modules[0] = data.as_ptr() as u64;
            modules[PHYSICS / 8] = physics.as_ptr() as u64;
            let mut entries: Vec<Vec<u64>> = effects
                .iter()
                .map(|&id| {
                    let mut e = vec![0u64; 0x40 / 8];
                    e[EFFECT_ID / 8] = id as u32 as u64;
                    e
                })
                .collect();
            for i in (0..entries.len().saturating_sub(1)).rev() {
                let next = entries[i + 1].as_ptr() as u64;
                entries[i][NEXT / 8] = next;
            }
            let mut list = vec![0u64; 4];
            list[FIRST / 8] = entries.first().map_or(0, |e| e.as_ptr() as u64);
            let mut chr = vec![0u64; 0x400 / 8];
            chr[MODULES / 8] = modules.as_ptr() as u64;
            chr[EFFECTS / 8] = list.as_ptr() as u64;
            Character { chr, _modules: modules, _data: data, _physics: physics, _effects: list, _entries: entries, _name: Vec::new() }
        }

        /// Its handle, entity id, NpcParam row, team, map block and map part name, where the game keeps them.
        pub(crate) fn with(mut self, handle: u64, entity: u32, npc: i32, team: u8, block: u32, name: &str) -> Self {
            let chr = self.chr.as_mut_ptr() as usize;
            self._name = name.encode_utf16().chain([0]).collect();
            let string = self._data.as_mut_ptr() as usize + NAME;
            unsafe {
                ((chr + HANDLE) as *mut u64).write(handle);
                ((chr + ENTITY) as *mut u32).write(entity);
                ((chr + NPC) as *mut i32).write(npc);
                ((chr + TEAM) as *mut u8).write(team);
                ((chr + BLOCK) as *mut u32).write(block);
                ((string + 8) as *mut u64).write(self._name.as_ptr() as u64);
                ((string + 0x18) as *mut u64).write(name.encode_utf16().count() as u64);
                ((string + 0x20) as *mut u64).write(15);
            }
            self
        }

        pub(crate) fn at(&self) -> usize {
            self.chr.as_ptr() as usize
        }
    }

    /// Serves characters to the fake lookup, by entity id.
    pub(crate) fn serve(characters: &[(u32, &Character)]) {
        let functions = FUNCTIONS.get_or_init(|| Functions {
            lookup: lookup as *const () as usize,
            apply: apply as *const () as usize,
            remove: remove as *const () as usize,
        });
        FAKE.with(|f| f.set(Some(functions)));
        CHARACTERS.with(|c| *c.borrow_mut() = characters.iter().map(|&(e, c)| (e as i32, c.at())).collect());
        APPLIED.with(|a| a.borrow_mut().clear());
    }

    #[test]
    fn characters_are_found_by_entity_id_and_read_as_the_game_lays_them_out() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _player = crate::player::tests::Player::serve(0);
        let knight = Character::new(800, 1200, [1.0, 2.0, 3.0], &[4330, 100, 4330]);
        serve(&[(1042360800, &knight)]);
        assert_eq!(character(1042360800), Ok(Some(knight.at())));
        assert_eq!(character(1042360801), Ok(None));
        assert!(hp(1042360801).unwrap_err().contains("1042360801"));
        assert_eq!(hp(1042360800), Ok((800, 1200)));
        assert_eq!(position(1042360800), Ok((1.0, 2.0, 3.0)));
        set_hp(1042360800, 5000).unwrap();
        assert_eq!(hp(1042360800), Ok((1200, 1200)));
        set_hp(1042360800, -5).unwrap();
        assert_eq!(hp(1042360800), Ok((0, 1200)));
        assert_eq!(effects(1042360800), Ok(vec![4330, 100, 4330]));
        apply_effect(1042360800, 3000).unwrap();
        assert_eq!(APPLIED.with(|a| a.borrow().clone()), [(knight.at(), 3000)]);
        assert!(apply_effect(1042360800, 1 << 40).is_err());
        assert_eq!(remove_effect(1042360800, 4330), Ok(2));
        assert_eq!(remove_effect(1042360800, 4330), Ok(0));
        assert_eq!(effects(1042360800), Ok(vec![100]));
    }

    /// A WorldChrMan laid out as the game's: sets by index, each a capacity and 16-byte entries.
    struct World {
        world: Vec<u64>,
        _sets: Vec<Vec<u64>>,
        _entries: Vec<Vec<u64>>,
    }

    impl World {
        fn serve(sets: &[(usize, &[usize])]) -> Self {
            let mut world = vec![0u64; (SETS + 8 * SET_COUNT) / 8];
            let mut all_sets = Vec::new();
            let mut all_entries = Vec::new();
            for &(n, chrs) in sets {
                let entries: Vec<u64> = chrs.iter().flat_map(|&c| [c as u64, 0]).collect();
                let mut set = vec![0u64; 4];
                set[SET_CAPACITY / 8] = chrs.len() as u64;
                set[SET_ENTRIES / 8] = entries.as_ptr() as u64;
                world[SETS / 8 + n] = set.as_ptr() as u64;
                all_sets.push(set);
                all_entries.push(entries);
            }
            WORLD.with(|w| w.set(world.as_ptr() as usize));
            World { world, _sets: all_sets, _entries: all_entries }
        }
    }

    impl Drop for World {
        fn drop(&mut self) {
            WORLD.with(|w| w.set(0));
            self.world.clear();
        }
    }

    #[test]
    fn every_set_is_listed_and_a_character_found_by_handle() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let player = crate::player::tests::Player::serve(0);
        let elleh = 0x3C2A2400;
        let knight = Character::new(800, 1200, [1.0, 2.0, 3.0], &[]).with(0x3C2A2400_17300005, 1042360800, 30100010, 6, elleh, "c3100_9000[42 36 0]");
        let dummy = Character::new(1, 1, [0.0; 3], &[]).with(0x3C2A2400_17300006, 0, 0, 0, elleh + 0x10000, "c1000_9000");
        let fallen = Character::new(0, 900, [0.0; 3], &[]).with(0x3C2A2400_17300007, 1042360801, 30100010, 6, 0x0A000000, "c3100_9001");
        serve(&[(1042360800, &knight), (1042360801, &fallen)]);
        let player_at = player.chr.as_ptr() as usize;
        let _world = World::serve(&[(0x6F, &[player_at]), (0x73, &[knight.at(), 0, dummy.at(), fallen.at()])]);
        let listed = characters().unwrap();
        assert_eq!(listed.len(), 4);
        assert_eq!((listed[0].kind, listed[0].entity, listed[0].is_player), ("player", Some(PLAYER), true));
        let k = &listed[1];
        assert_eq!((k.handle, k.entity, k.npc, k.team, k.map), (0x3C2A2400_17300005, Some(1042360800), Some(30100010), 6, elleh));
        assert_eq!((k.model.as_deref(), k.kind, k.hp, k.max_hp, k.position), (Some("c3100"), "enemy", 800, 1200, Some((1.0, 2.0, 3.0))));
        assert_eq!((listed[2].kind, listed[2].entity, listed[2].model.as_deref()), ("helper", None, Some("c1000")));
        // A character without an entity id is found by its handle.
        assert_eq!(find(0x3C2A2400_17300006), Ok(Some(dummy.at())));
        assert_eq!(hp(0x3C2A2400_17300006), Ok((1, 1)));
        assert!(hp(0x3C2A2400_17300009).unwrap_err().contains("handle"));
        // An entity id the game's lookup doesn't give back means its place has moved.
        serve(&[(1042360800, &dummy)]);
        assert!(characters().unwrap_err().contains("entity id"));
    }

    #[test]
    fn positions_meet_across_the_open_worlds_blocks_only() {
        let (m60_42_36, m60_43_36, m60_10_09_02, stormveil) = (0x3C2A2400, 0x3C2B2400, 0x3C0A0902, 0x0A000000);
        assert_eq!(relative(m60_42_36, m60_42_36, (1.0, 2.0, 3.0)), Some((1.0, 2.0, 3.0)));
        assert_eq!(relative(m60_42_36, m60_43_36, (10.0, 2.0, 3.0)), Some((-246.0, 2.0, 3.0)));
        assert_eq!(relative(m60_43_36, m60_42_36, (10.0, 2.0, 3.0)), Some((266.0, 2.0, 3.0)));
        // A large block (1024 m) of 4 x 4 small ones: its centre is the corner of its third small one.
        assert_eq!(relative(m60_10_09_02, 0x3C2A2600, (0.0, 0.0, 0.0)), Some((-128.0, 0.0, -128.0)));
        assert_eq!(relative(stormveil, m60_42_36, (1.0, 2.0, 3.0)), None);
    }

    /// The lookup, the apply and the end found once each in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE).
    #[test]
    fn the_games_character_functions_are_found() {
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
        assert_eq!(find(LOOKUP), [0x5EEFD0]);
        assert_eq!(find(APPLY), [0x3E8DC0]);
        assert_eq!(find(REMOVE), [0x4F7680]);
        assert_eq!(find(SET_TABLE), [0x508C23]);
        assert_eq!(find(SET_WALK), [0x4952CD]);
        // The lookup reads WorldChrMan's static first.
        let disp = i32::from_le_bytes(img[0x5EEFD0 + 24..0x5EEFD0 + 28].try_into().unwrap());
        assert_eq!((0x5EEFD0usize + 28).wrapping_add_signed(disp as isize), 0x3D69FF8);
    }
}
