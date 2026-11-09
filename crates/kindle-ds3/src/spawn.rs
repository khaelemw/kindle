//! Spawning characters (`kindle.world.spawn`) through the game's own developer character
//! creator. `CSDebugChrCreator` is still in the retail exe (its code is encrypted on disk and
//! decrypted at run time) though nothing creates one; Kindle fills the same request it
//! builds and calls the same function. DS3 1.15.2:
//!
//! - Request (0xA0 bytes; the creator's +0x110): `+0x00` position (x, y, z, 1), `+0x10`
//!   angle (radians), `+0x14` NpcThinkParam id, `+0x18` NpcParam id, `+0x20` model name
//!   (the game's wide string: 16-byte inline buffer, length, capacity, allocator; built by
//!   `+0x5BE0` and formatted `c%04d` by `+0x3F84F0`), `+0x90` the current map's data (from a
//!   static: `[S+0x10]` table, `[S+0x20]` index, entry `table+0x20 + index*0x70`, `+0x28`).
//! - Create: `+0x8D8DA0(WorldChrMan, request*) -> ChrIns*`. It adds the character to a
//!   character set of its own (`WorldChrMan +0x68`, laid out like a map block's) under a
//!   unique name (`c1100_N`), names it, and keeps a list at `WorldChrMan +0x2FF8`. The model's
//!   files load by themselves (any model works, even one not used in the area).
//! - AI: the character runs its think param's AI only if that AI's scripts are loaded, which
//!   happens for the enemies of the areas currently loaded. The creator's own AI load
//!   (`+0x448510`, through SprjWorldAiManager) reads a developer path (`dbgai:/`) that retail
//!   doesn't have, so it doesn't help: an enemy from another area stands idle unless its
//!   area's AI is loaded first (`load_ai`, below; checked in game 2026-10-04: a Black Knight
//!   at High Wall with Farron Keep's AI fights normally) or it gets a local enemy's think
//!   param (it then fights with that enemy's behaviour).
//! - Spawned characters fight, die and give souls; the game removes them when the area
//!   unloads (quitting out, warping). Checked in game 2026-10-03/04.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::{ds3, memory, patterns};

/// The creator's "create" method from the format call on: `mov r8d, [rbx+0x1B0]` (model
/// number), `lea rcx, [rbx+0x130]` (name), `lea rdx, [L"c%04d"]` (+0xE), `call format`
/// (+0x15), `mov rax, [map static]` (+0x1A), then the map entry lookup.
const CREATOR_NAME_SITE: &str = "44 8b 83 b0 01 00 00 48 8d 8b 30 01 00 00 48 8d 15 ?? ?? ?? ?? e8 ?? ?? ?? ?? \
    48 8b 05 ?? ?? ?? ?? 48 8b 7c 24 68 48 85 c0 74 ?? 48 8b 50 10 48 63 40 20";
/// The creator's step that creates the character: `lea rdx, [rsi+0x110]; call create`.
const CREATOR_CREATE_SITE: &str = "48 8d 96 10 01 00 00 e8 ?? ?? ?? ?? 48 89 86 c8 01 00 00";

const REQUEST_SIZE: usize = 0xA0;
const MODEL_AT: usize = 0x20;
const MAP_AT: usize = 0x90;

struct Game {
    create: usize,
    format: usize,
    model_format: usize,
    map_static: usize,
    string_ctor: usize,
}

static GAME: OnceLock<Option<Game>> = OnceLock::new();

fn game() -> Result<&'static Game, String> {
    GAME.get_or_init(|| {
        let name = patterns::find_unique("debug character creator", CREATOR_NAME_SITE)?;
        let create = patterns::find_unique("debug character create", CREATOR_CREATE_SITE)?;
        let label = patterns::find_unique("menu label from text", crate::front_menu::LABEL_FROM_TEXT)?;
        let string_ctor = patterns::call_target(label + crate::front_menu::LABEL_STRING_CTOR_AT)?;
        Some(Game {
            create: patterns::call_target(create + 7)?,
            format: patterns::call_target(name + 0x15)?,
            model_format: patterns::lea_target(name + 0xE)?,
            map_static: patterns::lea_target(name + 0x1A)?,
            string_ctor,
        })
    })
    .as_ref()
    .ok_or_else(|| "spawning isn't available on this game build".to_string())
}

fn read(a: usize) -> usize {
    memory::read_u64(a).map(|v| v as usize).unwrap_or(0)
}

/// The current map's data the creator puts in its request (null if there's none).
fn map_data(g: &Game) -> usize {
    let s = read(g.map_static);
    if s == 0 {
        return 0;
    }
    let table = read(s + 0x10);
    let index = memory::read::<i32>(s + 0x20).unwrap_or(-1);
    let count = memory::read::<i32>(table + 0x18).unwrap_or(0);
    if table == 0 || index < 0 || index >= count {
        return 0;
    }
    let entries = read(table + 0x20);
    if entries == 0 {
        return 0;
    }
    read(entries + index as usize * 0x70 + 0x28)
}

// ── other areas' AI ─────────────────────────────────────────────────────────────────────
//
// SprjWorldAiManager keeps one script container (`+0x43F8`), rebuilt when the area changes,
// with `aiscript:/aiCommon.luabnd` and the area's `aiscript:/<map>.luabnd` (+0x4472F0). To
// give a spawned enemy from elsewhere its own AI, Kindle loads that area's file the same way
// (`SprjFile` load, `+0x504A70(file, path, 0, 0)`, ready when `[handle +0x70] == 4`) and
// registers it in the current container (`+0x442E20(container, 0, handle, 0)`, as the
// manager does for its debug files), again whenever the container has changed. Then it
// registers the current area's own file again (`[manager +0x4440]`), so if two files define
// the same thing, the local enemies keep their own.

/// `mov rcx, [SprjWorldAiManager]` at +0x13 (the debug creator's step 1).
const AI_MANAGER_SITE: &str = "c6 81 c0 01 00 00 00 48 81 c1 b8 00 00 00 e8 ?? ?? ?? ?? 48 8b 0d ?? ?? ?? ?? 48 85 c9";
const AI_MANAGER_AT: usize = 0x13;
/// The manager loading its area's AI file: `call load` at +9.
const AI_FILE_LOAD_SITE: &str = "45 33 c9 45 33 c0 48 8b d3 e8 ?? ?? ?? ?? 48 89 87 40 44 00 00";
/// The manager registering its debug files: `call register` at +0x13.
const AI_FILE_REGISTER_SITE: &str = "45 33 c9 4c 8b 87 e0 ea 04 00 33 d2 48 8b 8f f8 43 00 00 e8";
/// The game's singleton accessors (`mov rcx, [static]`, then the class name at +0xC).
const SINGLETON_ACCESSOR: &str = "48 8b 0d ?? ?? ?? ?? 48 85 c9 75 ?? 4c 8d 0d ?? ?? ?? ??";
const SCRIPTS_AT: usize = 0x43F8;
/// The current area's AI file (the manager's own load).
const LOCAL_FILE_AT: usize = 0x4440;
const FILE_STATE_AT: usize = 0x70;
const FILE_LOADED: u8 = 4;

struct AiFiles {
    manager_static: usize,
    file_static: usize,
    load: usize,
    register: usize,
}

static AI_FILES: OnceLock<Option<AiFiles>> = OnceLock::new();
/// Area name → (file handle, the container it was registered in).
static AI_LOADED: Mutex<Option<HashMap<String, (usize, usize)>>> = Mutex::new(None);

fn ai_files() -> Result<&'static AiFiles, String> {
    AI_FILES
        .get_or_init(|| {
            let manager = patterns::find_unique("AI manager", AI_MANAGER_SITE)?;
            let load = patterns::find_unique("AI file load", AI_FILE_LOAD_SITE)?;
            let register = patterns::find_unique("AI file register", AI_FILE_REGISTER_SITE)?;
            Some(AiFiles {
                manager_static: patterns::lea_target(manager + AI_MANAGER_AT)?,
                file_static: patterns::find_singleton_static("SprjFile", SINGLETON_ACCESSOR, 0xC, "SprjFile")?,
                load: patterns::call_target(load + 9)?,
                register: patterns::call_target(register + 0x13)?,
            })
        })
        .as_ref()
        .ok_or_else(|| "loading other areas' AI isn't available on this game build".to_string())
}

/// Loads the AI scripts of an area (`m33_00_00_00`: Farron Keep) so enemies from there act
/// when spawned. Starts loading on the first call; true once loaded and in use. Call again
/// after the player changes area (the game rebuilds its scripts then).
pub fn load_ai(area: &str) -> Result<bool, String> {
    if area.is_empty() || !area.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("{area:?} isn't an area name like \"m33_00_00_00\""));
    }
    let f = ai_files()?;
    let manager = read(f.manager_static);
    let file = read(f.file_static);
    if manager == 0 || file == 0 {
        return Err("the game's AI isn't running".into());
    }
    let mut guard = AI_LOADED.lock().unwrap_or_else(|e| e.into_inner());
    let loaded = guard.get_or_insert_with(HashMap::new);
    let (handle, registered_in) = match loaded.get(area) {
        Some(&entry) => entry,
        None => {
            // Kept for the session: the file system may hold on to the path.
            let path: &'static [u16] =
                Box::leak(format!("aiscript:/{area}.luabnd").encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice());
            type LoadFn = unsafe extern "C" fn(usize, *const u16, usize, u32) -> usize;
            let start = unsafe { std::mem::transmute::<usize, LoadFn>(f.load) };
            let handle = unsafe { start(file, path.as_ptr(), 0, 0) };
            if handle == 0 {
                return Err(format!("the game couldn't start loading {area}'s AI"));
            }
            loaded.insert(area.to_string(), (handle, 0));
            (handle, 0)
        }
    };
    if memory::read::<u8>(handle + FILE_STATE_AT) != Some(FILE_LOADED) {
        return Ok(false);
    }
    let container = read(manager + SCRIPTS_AT);
    if container == 0 {
        return Ok(false);
    }
    if container != registered_in {
        type RegisterFn = unsafe extern "C" fn(usize, usize, usize, u8) -> u8;
        let register = unsafe { std::mem::transmute::<usize, RegisterFn>(f.register) };
        unsafe { register(container, 0, handle, 0) };
        // The local area's scripts last: its enemies keep their own definitions.
        let local = read(manager + LOCAL_FILE_AT);
        if local != 0 && local != handle && memory::read::<u8>(local + FILE_STATE_AT) == Some(FILE_LOADED) {
            unsafe { register(container, 0, local, 0) };
        }
        loaded.insert(area.to_string(), (handle, container));
        crate::info!("spawn: {area}'s AI loaded");
    }
    Ok(true)
}

/// The map the player is in ("m30_00_00_00"), from the play region (300001 is area 30,
/// block 0).
pub fn current_area() -> Option<String> {
    let region = crate::game_events::play_region()?;
    (region > 0).then(|| format!("m{}_{:02}_00_00", region / 10000, (region / 1000) % 10))
}

// ── spawning ────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Spawn {
    /// Model number: 1100 for c1100.
    pub model: u32,
    pub npc_param: i32,
    pub think_param: i32,
    pub pos: [f32; 3],
    /// Facing, radians.
    pub angle: f32,
    /// The map whose AI the character needs, if it isn't the current one's.
    pub area: Option<String>,
    /// Load its area's AI if needed (true), or spawn it with its AI off (false).
    pub ai: bool,
}

/// The `cNNNN` name of a model number.
pub fn model_name(model: u32) -> String {
    format!("c{model:04}")
}

/// The model of a loaded character's model name: "c1100_03" (instance 3) is c1100.
pub fn base_model(name: &str) -> &str {
    name.split('_').next().unwrap_or(name)
}

/// Spawns waiting for their area's AI, by token.
static PENDING: Mutex<Vec<(u32, Spawn)>> = Mutex::new(Vec::new());
static NEXT_TOKEN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

pub enum Started {
    /// Created now: the ChrIns address.
    Now(usize),
    /// Waiting for its area's AI to load; poll with this token.
    Pending(u32),
}

/// A spawn's ids name things the game has: its model's files (in the archives, or a mod's), its
/// NpcParam and NpcThinkParam rows. The game takes them as they are; an unknown one is a crash
/// or memory it never meant to read.
fn check_ids(s: &Spawn) -> Result<(), String> {
    let chr = format!("/chr/{}.chrbnd.dcx", model_name(s.model));
    let game_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())).ok_or("the game's folder isn't known")?;
    if !crate::files::overridden(&chr) && !crate::archive::exists(&game_dir, &chr) {
        return Err(format!("model {} isn't in the game ({chr})", model_name(s.model)));
    }
    let reg = crate::params::registry()?;
    for (table, id) in [("NpcParam", s.npc_param), ("NpcThinkParam", s.think_param)] {
        if id >= 0 && !reg.table(table)?.has_row(id as u64) {
            return Err(format!(
                "{table} has no row {id}{}",
                if table == "NpcThinkParam" { " (without think_param, the NpcParam id is used: pass one)" } else { "" }
            ));
        }
    }
    Ok(())
}

/// Whether the spawn's AI is ready (loading it if needed).
fn ai_ready(s: &Spawn) -> Result<bool, String> {
    match &s.area {
        Some(area) if s.ai && current_area().as_deref() != Some(area.as_str()) => load_ai(area),
        _ => Ok(true),
    }
}

/// Spawns now if the character's AI is ready, else starts loading it. Game thread only.
pub fn begin(s: Spawn) -> Result<Started, String> {
    if s.model > 9999 {
        return Err(format!("model {} isn't a cNNNN model number", s.model));
    }
    check_ids(&s)?;
    let g = game()?;
    ds3::main_player().ok_or("no character is loaded")?;
    if ai_ready(&s)? {
        return create(g, &s).map(Started::Now);
    }
    let token = NEXT_TOKEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((token, s));
    Ok(Started::Pending(token))
}

/// A pending spawn: Some(ChrIns) once created, None while its AI is loading.
pub fn poll(token: u32) -> Result<Option<usize>, String> {
    let s = {
        let pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
        pending.iter().find(|(t, _)| *t == token).map(|(_, s)| s.clone()).ok_or("no such pending spawn")?
    };
    let finish = || PENDING.lock().unwrap_or_else(|e| e.into_inner()).retain(|(t, _)| *t != token);
    if ds3::main_player().is_none() {
        finish();
        return Err("the character left the world before the spawn".into());
    }
    match ai_ready(&s) {
        Ok(false) => Ok(None),
        Ok(true) => {
            finish();
            create(game()?, &s).map(Some)
        }
        Err(e) => {
            finish();
            Err(e)
        }
    }
}

/// Switches a character's AI off (bit 0 of `[ChrIns +0x50] +0x182`, the flag set_ai uses).
fn ai_off(chr: usize) {
    let module = read(chr + 0x50);
    if let Some(flags) = memory::read::<u8>(module + 0x182).filter(|_| module != 0) {
        unsafe { std::ptr::write_volatile((module + 0x182) as *mut u8, flags & !1) };
    }
}

/// Creates the character; returns its ChrIns address.
fn create(g: &Game, s: &Spawn) -> Result<usize, String> {
    let world = ds3::WORLD_CHR_MAN.instance().ok_or("no world is loaded")?;
    #[repr(C, align(16))]
    struct Request([u8; REQUEST_SIZE]);
    let mut req = Request([0; REQUEST_SIZE]);
    let at = req.0.as_mut_ptr() as usize;
    let put = |off: usize, bytes: &[u8]| unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), (at + off) as *mut u8, bytes.len());
    };
    for (i, v) in [s.pos[0], s.pos[1], s.pos[2], 1.0].iter().enumerate() {
        put(i * 4, &v.to_le_bytes());
    }
    put(0x10, &s.angle.to_le_bytes());
    put(0x14, &s.think_param.to_le_bytes());
    put(0x18, &s.npc_param.to_le_bytes());
    put(MAP_AT, &map_data(g).to_le_bytes());

    type CtorFn = unsafe extern "C" fn(usize) -> usize;
    type FormatFn = unsafe extern "C" fn(usize, usize, u32) -> usize;
    type CreateFn = unsafe extern "C" fn(usize, usize) -> usize;
    let ctor = unsafe { std::mem::transmute::<usize, CtorFn>(g.string_ctor) };
    let format = unsafe { std::mem::transmute::<usize, FormatFn>(g.format) };
    let make = unsafe { std::mem::transmute::<usize, CreateFn>(g.create) };
    let chr = unsafe {
        ctor(at + MODEL_AT);
        format(at + MODEL_AT, g.model_format, s.model);
        make(world, at)
    };
    // The name fits the string's inline buffer, so there's nothing to free.
    let wanted = model_name(s.model);
    if chr == 0 {
        return Err(format!("the game didn't create {wanted}"));
    }
    if !s.ai {
        ai_off(chr);
    }
    crate::info!("spawned {wanted} (NpcParam {}, think {}) at {:?}", s.npc_param, s.think_param, s.pos);
    Ok(chr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names() {
        assert_eq!(model_name(1100), "c1100");
        assert_eq!(model_name(50), "c0050");
        assert_eq!(base_model("c1100_03"), "c1100");
        assert_eq!(base_model("c1100"), "c1100");
    }

    #[test]
    fn areas_from_regions() {
        let area = |region: i32| format!("m{}_{:02}_00_00", region / 10000, (region / 1000) % 10);
        assert_eq!(area(300001), "m30_00_00_00");
        assert_eq!(area(301002), "m30_01_00_00");
        assert_eq!(area(511000), "m51_01_00_00");
    }
}
