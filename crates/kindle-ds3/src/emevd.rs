//! Event script (EMEVD) commands, run on demand.
//!
//! The game's map event scripts are lists of commands: "bank" (category: 2003 events,
//! 2004 characters, 2006 effects, 2007 messages, 2010 sound, ...) plus an index and packed
//! arguments. The event system runs each one through a dispatcher (1.15.2 `+0x7AA140`,
//! `(system, dt, SprjEmkEventIns*) -> bool`, the only caller of the bank handlers) that
//! switches on the bank and calls that bank's handler; the handlers read the command header
//! from `ins+0xB0` (`{u32 bank, u32 index, u64 args length, u64 args offset, i64 layer}`) and
//! its arguments from `ins+0xB8` when that's set (otherwise they resolve them from the
//! event's file). Entity ids go through `+0x4C2AB0(ins+0x50, id)`, which only swaps the
//! "this event's entity" placeholder for `[ins+0x58]`.
//!
//! The game only calls the dispatcher when an event moves on to its next command, which can
//! be seconds apart. So Kindle calls it itself, from its frame task: with the event system
//! object the game passes (`[[static]+0x30]`, the static read at the dispatcher's call site)
//! and an event instance of Kindle's own, zeroed except for the header and arguments. The
//! game's own handler runs the command, exactly as a script would.
//!
//! A hook on the dispatcher watches the game's own commands: the trace (debug.trace_events), the
//! flag sets the catalogue reports, boss bars (the boss_bar event). The banner function
//! the DisplayBanner command calls (`+0x765F30(type, 0)`) is also called directly, for
//! `ui.game_banner`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::catalogue::Value;
use crate::{memory, patterns};
pub use kindle_core::emevd::pack;

const DISPATCH_PATTERN: &str = "48 83 ec 28 49 8b 80 b0 00 00 00 b2 01 44 8b 08 41 81 f9 e8 03 00 00 0f 87";
/// At the dispatcher's call site: `mov rdi, [rip+static]; mov rdi, [rdi+0x30]; test rdi, rdi`.
const SYSTEM_PATTERN: &str = "48 8b 3d ?? ?? ?? ?? 48 8b 7f 30 48 85 ff 74 ?? 8b 93 a0 00 00 00 48 8d 8b 2b 01 00 00 e8";
const BANNER_PATTERN: &str = "40 57 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 40 0f b6 fa 8b d9 8d 41 ff 83 f8 15 0f 87";
/// Banner types the game accepts.
pub const BANNER_TYPES: std::ops::RangeInclusive<i32> = 1..=22;

const INS_HEADER: usize = 0xB0;
const INS_ARGS: usize = 0xB8;
/// Whether the event keeps the game's network sync on (2000[02] Set Network Sync State writes it;
/// SprjEventIns's constructor, +0x457D60, starts it at 1): its conditions held on the session's
/// other games too, or this game's own.
const INS_SYNCED: usize = 0x70;
/// The event's id (its key, as Initialize Event makes it: +0x28 the id, +0x2C the slot).
const INS_EVENT_ID: usize = 0x28;
/// Size of Kindle's event instance (the game's is smaller; the rest is zeroed room).
const INS_SIZE: usize = 0x400;

pub struct Command {
    pub id: u64,
    /// The mod that ran it.
    pub owner: String,
    pub bank: i32,
    pub index: i32,
    pub args: Vec<u8>,
}

/// "Set character AI state" (entity, enabled).
const SET_AI: (i32, i32) = (2004, 1);

/// Characters (entity ids) whose AI a mod turned off, with that mod. Map event scripts turn
/// AI back on as the player moves around (ambushes); the dispatcher hook drops those
/// commands for these characters until the mod turns the AI on again or unloads.
static AI_OFF: Mutex<Option<std::collections::HashMap<i32, String>>> = Mutex::new(None);

/// Character states mods can change, which Kindle puts back when the mod unloads: (bank,
/// index) and the game's default value.
const RESTORABLE: [((i32, i32), u8); 4] = [
    (SET_AI, 1),
    ((2004, 5), 1),  // enabled
    ((2004, 12), 0), // immortal
    ((2004, 15), 0), // invincible
];

/// One character state a mod changed: (save slot, entity id, bank, index).
type Change = (i32, i32, i32, i32);

/// Kindle's own restores (their owner).
const RESTORER: &str = "kindle";

struct Entry {
    change: Change,
    /// The running mod holding the change; None once it unloaded.
    owner: Option<String>,
}

/// Every change a mod made to a character's state that hasn't been put back yet. Once its
/// mod unloads, it goes back to the game's default when the character is loaded and the
/// world is running.
static CHANGES: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn update_changes(f: impl FnOnce(&mut Vec<Entry>)) {
    f(&mut CHANGES.lock().unwrap_or_else(|e| e.into_inner()));
}

/// Records a command that changes a character's state (from Kindle's own runs, before they
/// reach the game).
fn note_change(cmd: &Command) {
    let Some(&(_, default)) = RESTORABLE.iter().find(|(c, _)| *c == (cmd.bank, cmd.index)) else { return };
    if cmd.args.len() < 5 {
        return;
    }
    let entity = i32::from_le_bytes([cmd.args[0], cmd.args[1], cmd.args[2], cmd.args[3]]);
    let on = cmd.args[4];
    let slot = crate::bonfire::save_slot().unwrap_or(-1);
    let same = |k: &Change| k.1 == entity && (k.2, k.3) == (cmd.bank, cmd.index) && (k.0 == slot || k.0 == -1 || slot == -1);
    update_changes(|entries| {
        if on == default {
            // Kindle's own AI restores stay until the character's AI reads as on
            // (owed_restores checks); everything else is put back by this command.
            if !(cmd.owner == RESTORER && (cmd.bank, cmd.index) == SET_AI) {
                entries.retain(|e| !same(&e.change));
            }
        } else if let Some(e) = entries.iter_mut().find(|e| same(&e.change)) {
            e.owner = Some(cmd.owner.clone());
        } else {
            entries.push(Entry { change: (slot, entity, cmd.bank, cmd.index), owner: Some(cmd.owner.clone()) });
        }
    });

    if (cmd.bank, cmd.index) == SET_AI {
        let mut map = AI_OFF.lock().unwrap_or_else(|e| e.into_inner());
        let map = map.get_or_insert_with(Default::default);
        if on == 0 {
            map.insert(entity, cmd.owner.clone());
            if let Err(e) = ensure_hook() {
                crate::warn!("events: AI may come back on: {e}");
            }
        } else {
            map.remove(&entity);
        }
    }
}

/// Commands putting back character states no running mod holds, for the loaded characters
/// (entity id -> ChrIns address) in the current save. Call only while the world is running:
/// map scripts set characters up when a loading screen ends. AI is read back first: a
/// character whose AI is on again is done.
pub fn owed_restores(loaded: &std::collections::HashMap<i32, usize>) -> Vec<Command> {
    let Ok(slot) = crate::bonfire::save_slot() else { return Vec::new() };
    let mut out = Vec::new();
    update_changes(|entries| {
        entries.retain(|e| {
            let (s, entity, bank, index) = e.change;
            let Some(&chr) = loaded.get(&entity) else { return true };
            if e.owner.is_some() || (s != slot && s != -1) {
                return true;
            }
            if (bank, index) == SET_AI && crate::game::ai_enabled(chr) == Some(true) {
                return false;
            }
            if let Some(&(_, default)) = RESTORABLE.iter().find(|(c, _)| *c == (bank, index)) {
                let mut args = entity.to_le_bytes().to_vec();
                args.extend([default, 0, 0, 0]);
                out.push(Command { id: 0, owner: RESTORER.into(), bank, index, args });
            }
            true
        });
    });
    out
}

/// Characters whose AI mods hold off: (entity id, mod).
pub fn ai_held_off() -> Vec<(i32, String)> {
    AI_OFF.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|m| m.iter().map(|(e, o)| (*e, o.clone())).collect()).unwrap_or_default()
}

/// A mod unloaded: the character states it changed go back to the game's defaults as each
/// character is loaded (owed_restores), and its AI locks are released.
pub fn release_characters(owner: &str) {
    if let Some(map) = AI_OFF.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        map.retain(|_, o| o != owner);
    }
    update_changes(|entries| {
        for e in entries.iter_mut().filter(|e| e.owner.as_deref() == Some(owner)) {
            e.owner = None;
        }
    });
}

/// Whether the command an event instance is about to run turns on AI a mod turned off.
fn blocked(ins: usize) -> bool {
    let map = AI_OFF.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = map.as_ref().filter(|m| !m.is_empty()) else { return false };
    let Some(header) = memory::read_u64(ins + INS_HEADER).map(|h| h as usize).filter(|&h| h != 0) else { return false };
    if (memory::read::<i32>(header), memory::read::<i32>(header + 4)) != (Some(SET_AI.0), Some(SET_AI.1)) {
        return false;
    }
    let Some(args) = current_args(ins, header).filter(|a| a.len() >= 5) else { return false };
    let entity = i32::from_le_bytes([args[0], args[1], args[2], args[3]]);
    args[4] != 0 && map.contains_key(&entity)
}

static QUEUE: Mutex<Vec<Command>> = Mutex::new(Vec::new());
/// (command id, whether the game's handler reported it done; None if it couldn't run).
static RESULTS: Mutex<Vec<(u64, Option<bool>)>> = Mutex::new(Vec::new());
/// Log the commands the game's events run (discovery aid).
pub static TRACE: AtomicBool = AtomicBool::new(false);

/// (dispatcher, static holding the event system's owner), found before any hook changes
/// the dispatcher's first bytes.
fn addresses() -> Option<(usize, usize)> {
    static A: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    *A.get_or_init(|| Some((patterns::find_unique("event command dispatcher", DISPATCH_PATTERN)?, patterns::find_static("event system", SYSTEM_PATTERN)?)))
}

static DISPATCH_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "events",
    name: "event command dispatcher",
    pattern: DISPATCH_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(dispatch_hook),
    without: "Kindle can't run or watch event commands",
};

fn ensure_hook() -> Result<(), String> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| {
        // (addresses() found it before: the hook changes the bytes its pattern matches.)
        let (f, _) = addresses().ok_or("the event command dispatcher wasn't found on this game build")?;
        std::mem::forget(DISPATCH_SITE.hook_at(f)?); // for the life of the process
        crate::info!("events: trace hook installed");
        Ok(())
    })
    .clone()
}

/// The game's "set network-connected event flag" commands (2003[62]: flag, state; 2003[63]: first,
/// last, state) and its plain sets are reported to mods (the catalogue's flags.network_set and
/// flags.plain_set, flag_blocks.rs): a co-op player's network sets don't reach the host by
/// themselves. Not while mods' own commands run.
static NETWORK_FLAGS: AtomicBool = AtomicBool::new(false);
static RUNNING_MODS: AtomicBool = AtomicBool::new(false);
const SET_NETWORK_FLAG: (i32, i32) = (2003, 62);
const SET_NETWORK_FLAGS: (i32, i32) = (2003, 63);
/// The plain sets (the game's own only): set (flag, state), batch set (first, last, state), invert
/// (flag), random set in a range (first, last, state). Reported as the flags they may change.
const SET_FLAG: (i32, i32) = (2003, 2);
const SET_FLAGS: (i32, i32) = (2003, 22);
const INVERT_FLAG: (i32, i32) = (2003, 9);
const SET_RANDOM_FLAG: (i32, i32) = (2003, 17);

/// Starts reporting network-connected flags (flags.network_set; installs the hook if needed).
pub fn watch_network_flags() -> Result<(), String> {
    ensure_hook()?;
    NETWORK_FLAGS.store(true, Ordering::Relaxed);
    Ok(())
}

fn network_flag(ins: usize) {
    let Some(header) = memory::read_u64(ins + INS_HEADER).map(|h| h as usize).filter(|&h| h != 0) else { return };
    let (Some(bank), Some(index)) = (memory::read::<i32>(header), memory::read::<i32>(header + 4)) else { return };
    let plain = [SET_FLAG, SET_FLAGS, INVERT_FLAG, SET_RANDOM_FLAG].contains(&(bank, index));
    if (bank, index) != SET_NETWORK_FLAG && (bank, index) != SET_NETWORK_FLAGS && !plain {
        return;
    }
    let Some(args) = current_args(ins, header) else { return };
    let int = |at: usize| args.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().expect("4 bytes")));
    if plain {
        let (first, last) = match (bank, index) {
            SET_FLAG | INVERT_FLAG => (int(0), int(0)),
            _ => (int(0), int(4)),
        };
        if let (Some(first), Some(last)) = (first, last) {
            let random = Value::Bool((bank, index) == SET_RANDOM_FLAG);
            let synced = Value::Bool(memory::read::<u8>(ins + INS_SYNCED).is_some_and(|s| s != 0));
            let event = Value::Int(memory::read::<i32>(ins + INS_EVENT_ID).unwrap_or(-1).into());
            crate::catalogue::emit(&crate::flag_blocks::PLAIN_SET, vec![Value::Int(first.into()), Value::Int(last.into()), random, synced, event]);
        }
        return;
    }
    let (first, last, state) = if (bank, index) == SET_NETWORK_FLAG { (int(0), int(0), args.get(4)) } else { (int(0), int(4), args.get(8)) };
    if let (Some(first), Some(last), Some(&state)) = (first, last, state) {
        crate::catalogue::emit(&crate::flag_blocks::NETWORK_SET, vec![Value::Int(first.into()), Value::Int(last.into()), Value::Bool(state != 0)]);
    }
}

/// The game's scripts showing or hiding a boss's health bar: 2003[11] Display Boss Health Bar and
/// 2003[80] its decorated form (state s8, entity s32, slot s16, name id s32: NpcName text), for
/// the boss_bar event. A fight's start and end as the game sees it.
static BOSS_BARS: AtomicBool = AtomicBool::new(false);
const BOSS_BAR: (i32, i32) = (2003, 11);
const BOSS_BAR_DECORATED: (i32, i32) = (2003, 80);

/// A boss bar shown or hidden: (shown, entity id, name text id).
static BOSS_BAR_QUEUE: Mutex<Vec<(bool, i32, i32)>> = Mutex::new(Vec::new());

/// Starts reporting boss bars (installs the hook if needed).
pub fn watch_boss_bars() -> Result<(), String> {
    ensure_hook()?;
    BOSS_BARS.store(true, Ordering::Relaxed);
    Ok(())
}

/// The boss bars the game's scripts showed or hid since the last call.
pub fn take_boss_bars() -> Vec<(bool, i32, i32)> {
    std::mem::take(&mut *BOSS_BAR_QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn boss_bar(ins: usize) {
    let Some(header) = memory::read_u64(ins + INS_HEADER).map(|h| h as usize).filter(|&h| h != 0) else { return };
    let (Some(bank), Some(index)) = (memory::read::<i32>(header), memory::read::<i32>(header + 4)) else { return };
    if (bank, index) != BOSS_BAR && (bank, index) != BOSS_BAR_DECORATED {
        return;
    }
    let Some(args) = current_args(ins, header) else { return };
    let int = |at: usize| args.get(at..at + 4).map(|b| i32::from_le_bytes(b.try_into().expect("4 bytes")));
    // (The boss events name their boss's entity id, not the event's own entity placeholder.)
    if let (Some(&state), Some(entity), Some(name)) = (args.first(), int(4), int(12)) {
        BOSS_BAR_QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push((state != 0, entity, name));
    }
}

/// The game sending the player somewhere: a bonfire's travel (quick_warp's hook on the bonfire
/// warp, every one) and the event scripts' 2003[14] Warp Player (area u8, block u8, start
/// entity s32; watched once a mod listens), for the travel event: (map packed as 0xAABB0000,
/// entity, how).
static TRAVELS: AtomicBool = AtomicBool::new(false);
const WARP_PLAYER: (i32, i32) = (2003, 14);
static TRAVEL_QUEUE: Mutex<Vec<(i32, i32, &'static str)>> = Mutex::new(Vec::new());

/// Starts reporting the scripts' warps (installs the hook if needed).
pub fn watch_travel() -> Result<(), String> {
    ensure_hook()?;
    TRAVELS.store(true, Ordering::Relaxed);
    Ok(())
}

/// A travel the game began.
pub fn travel_began(map: i32, entity: i32, how: &'static str) {
    let mut queue = TRAVEL_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
    if queue.len() < 16 {
        queue.push((map, entity, how));
    }
}

/// The travels the game began since the last call.
pub fn take_travels() -> Vec<(i32, i32, &'static str)> {
    std::mem::take(&mut *TRAVEL_QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

fn script_warp(ins: usize) {
    let Some(header) = memory::read_u64(ins + INS_HEADER).map(|h| h as usize).filter(|&h| h != 0) else { return };
    if (memory::read::<i32>(header), memory::read::<i32>(header + 4)) != (Some(WARP_PLAYER.0), Some(WARP_PLAYER.1)) {
        return;
    }
    let Some(args) = current_args(ins, header) else { return };
    if let (Some(&area), Some(&block), Some(entity)) = (args.first(), args.get(1), args.get(4..8)) {
        let entity = i32::from_le_bytes(entity.try_into().expect("4 bytes"));
        travel_began((i32::from(area) << 24) | (i32::from(block) << 16), entity, "script");
    }
}

/// Turns the command trace on or off (installs the hook if needed).
pub fn set_trace(on: bool) -> Result<(), String> {
    ensure_hook()?;
    TRACE.store(on, Ordering::Relaxed);
    Ok(())
}

/// The arguments of the command an event instance is running: the substituted copy at
/// `ins+0xB8` when the event has parameters, else the event file's argument data
/// (`file + [file+0x78] + offset`, file = `[[ins+0xA8]+8]`; what the game's resolver reads).
fn current_args(ins: usize, header: usize) -> Option<Vec<u8>> {
    let len = (memory::read_u64(header + 8)? as usize).min(64);
    let mut p = memory::read_u64(ins + INS_ARGS)? as usize;
    if p == 0 {
        let file = memory::read_u64(memory::read_u64(ins + 0xA8)? as usize + 8)? as usize;
        let base = memory::read_u64(file + 0x78)?;
        if file == 0 || base == u64::MAX {
            return None;
        }
        p = file + base as usize + memory::read_u64(header + 0x10)? as usize;
    }
    memory::read_bytes(p, len)
}

/// One trace line per distinct command (bank, index and arguments).
fn trace(ins: usize) {
    use std::collections::HashSet;
    static SEEN: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let Some(header) = memory::read_u64(ins + INS_HEADER).map(|h| h as usize).filter(|&h| h != 0) else { return };
    let (Some(bank), Some(index)) = (memory::read::<i32>(header), memory::read::<i32>(header + 4)) else { return };
    let args = current_args(ins, header).unwrap_or_default();
    let words: Vec<String> = args
        .chunks(4)
        .map(|c| {
            let mut w = [0u8; 4];
            w[..c.len()].copy_from_slice(c);
            i32::from_le_bytes(w).to_string()
        })
        .collect();
    let line = format!("event command {bank}[{index:02}]({})", words.join(", "));
    if SEEN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(line.clone()) {
        crate::info!("{line}");
    }
}

/// Queues a command; it runs on Kindle's next frame.
pub fn run(cmd: Command) -> Result<(), String> {
    addresses().ok_or("the event command dispatcher wasn't found on this game build")?;
    QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(cmd);
    Ok(())
}

/// "Award item lot" (lot).
pub const AWARD_ITEM_LOT: (i32, i32) = (2003, 4);

/// Takes the queued commands, to run with [`run_commands`].
pub fn take_queued() -> Vec<Command> {
    std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Runs commands (from the frame task) and returns every result so far.
pub fn run_commands(queued: Vec<Command>) -> Vec<(u64, Option<bool>)> {
    let mut out = std::mem::take(&mut *RESULTS.lock().unwrap_or_else(|e| e.into_inner()));
    RUNNING_MODS.store(true, Ordering::Relaxed);
    for cmd in queued {
        // (The flags it sets aren't the game's changes: flags.changed leaves them out.)
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::flag_blocks::kindle_sets(|| unsafe { execute(&cmd) })))
            .unwrap_or(Err("the command crashed in Kindle".into()));
        match done {
            Ok(d) => out.push((cmd.id, Some(d))),
            Err(e) => {
                crate::warn!("events.run {}[{}]: {e}", cmd.bank, cmd.index);
                out.push((cmd.id, None));
            }
        }
    }
    RUNNING_MODS.store(false, Ordering::Relaxed);
    out
}

type Dispatch = unsafe extern "C" fn(usize, f32, usize) -> u8;

fn dispatch_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (system, ins, dt) = unsafe { ((*reg).rcx as usize, (*reg).r8 as usize, f32::from_bits((*reg).xmm1 as u32)) };
    if TRACE.load(Ordering::Relaxed) && ins != 0 {
        let _ = std::panic::catch_unwind(|| trace(ins));
    }
    if NETWORK_FLAGS.load(Ordering::Relaxed) && ins != 0 && !RUNNING_MODS.load(Ordering::Relaxed) {
        let _ = std::panic::catch_unwind(|| network_flag(ins));
    }
    if BOSS_BARS.load(Ordering::Relaxed) && ins != 0 && !RUNNING_MODS.load(Ordering::Relaxed) {
        let _ = std::panic::catch_unwind(|| boss_bar(ins));
    }
    if TRAVELS.load(Ordering::Relaxed) && ins != 0 {
        let _ = std::panic::catch_unwind(|| script_warp(ins));
    }
    // The game turning on AI a mod turned off: report it done without doing it.
    if ins != 0 && std::panic::catch_unwind(|| blocked(ins)).unwrap_or(false) {
        return 1;
    }
    let f: Dispatch = unsafe { std::mem::transmute(original) };
    unsafe { f(system, dt, ins) as usize }
}

/// Runs one command through the game's dispatcher on Kindle's own event instance.
unsafe fn execute(cmd: &Command) -> Result<bool, String> {
    note_change(cmd);
    let (dispatch, owner_static) = addresses().ok_or("the event command dispatcher wasn't found")?;
    let owner = memory::read_u64(owner_static).filter(|&p| p != 0).ok_or("the event system isn't running yet")? as usize;
    let system = memory::read_u64(owner + 0x30).filter(|&p| p != 0).ok_or("the event system isn't running yet")? as usize;
    #[repr(C)]
    struct Header {
        bank: i32,
        index: i32,
        args_len: u64,
        args_offset: u64,
        layer: i64,
    }
    let header = Header { bank: cmd.bank, index: cmd.index, args_len: cmd.args.len() as u64, args_offset: 0, layer: -1 };
    // Handlers read past the declared arguments in a few places: give them zeroed room.
    let mut args = cmd.args.clone();
    args.resize(args.len().max(64), 0);
    let mut ins = vec![0u64; INS_SIZE / 8];
    let base = ins.as_mut_ptr() as usize;
    unsafe {
        std::ptr::write((base + INS_HEADER) as *mut u64, &header as *const Header as u64);
        std::ptr::write((base + INS_ARGS) as *mut u64, args.as_ptr() as u64);
        // Through the entry point: with the trace on, the hook logs Kindle's commands too.
        let f: Dispatch = std::mem::transmute(dispatch);
        Ok(f(system, 1.0 / 60.0, base) != 0)
    }
}

/// Shows one of the game's own banners (the DisplayBanner command's function). Call from the
/// game thread.
pub fn banner(kind: i32) -> Result<(), String> {
    if !BANNER_TYPES.contains(&kind) {
        return Err(format!("banner type must be {}-{}", BANNER_TYPES.start(), BANNER_TYPES.end()));
    }
    let f = banner_fn().ok_or("the game's banner function wasn't found on this game build")?;
    let f: unsafe extern "C" fn(i32, u8) = unsafe { std::mem::transmute(f) };
    // A mod's own banner isn't reported back as one the game showed.
    OWN_BANNER.with(|b| b.set(true));
    unsafe { f(kind, 0) };
    OWN_BANNER.with(|b| b.set(false));
    Ok(())
}

/// The banners, by type, as they read in game (English). Types 3 and 8-11 show nothing.
pub const BANNER_NAMES: &[(&str, i32)] = &[
    ("heir_of_fire_destroyed", 1),
    ("you_died", 2),
    ("souls_retrieved", 4),
    ("host_of_embers_destroyed", 5),
    ("dark_spirit_destroyed", 7),
    ("begin_match", 12),
    ("bonfire_lit", 13),
    ("draw", 14),
    ("you_win", 15),
    ("you_lose", 16),
    ("ember_restored", 17),
    ("duty_fulfilled", 19),
    ("hollowing_reversed", 21),
    ("lord_of_cinder_fallen", 22),
];

/// A banner type's name (types sharing a banner give the first name).
pub fn banner_name(kind: i32) -> Option<&'static str> {
    let kind = match kind {
        6 => 2,
        18 => 17,
        20 => 19,
        k => k,
    };
    BANNER_NAMES.iter().find(|(_, k)| *k == kind).map(|(n, _)| *n)
}

fn banner_fn() -> Option<usize> {
    static F: OnceLock<Option<usize>> = OnceLock::new();
    *F.get_or_init(|| patterns::find_unique("banner", BANNER_PATTERN))
}

thread_local! {
    static OWN_BANNER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Banners the game showed since the last call (for the banner / boss_defeated events).
static SHOWN: Mutex<Vec<i32>> = Mutex::new(Vec::new());

static BANNER_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "events",
    name: "banner",
    pattern: BANNER_PATTERN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(banner_hook),
    without: "mods don't see the game's banners",
};

/// Watches the game's banners (once; the hook passes everything through).
pub fn watch_banners() -> Result<(), String> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| {
        let f = banner_fn().ok_or("the game's banner function wasn't found on this game build")?;
        std::mem::forget(BANNER_SITE.hook_at(f)?);
        crate::info!("events: banner hook installed");
        Ok(())
    })
    .clone()
}

/// kindle.game: one of the game's banners is about to show (catalogue.rs). The game shows some for
/// whichever player's character does a thing, not only this game's: its player update shows
/// "EMBER RESTORED" (17) for any player's character that takes the Ember's embering effect while
/// unembered (the state 0x115 test of PlayerIns's update, +0x8C7C90, protected code, with no test
/// that the character is this game's player, which its next test, 0x147's, has). In a game alone
/// only its player can; a mod letting other players use an Ember (their effect reaches this game
/// with their character) has it show here too. The hook is on the banner function itself, so a
/// mod asks this game's own state (an embering player has the effect) to tell.
pub static BANNER_SHOWING: crate::catalogue::HookPoint = crate::catalogue::HookPoint::hooked(
    "ui.banner",
    "One of the game's banners is about to show (its type; ui.game_banner's names): \"show\" it, or \"hide\" it. The game's own (its event scripts', its player update's), not a mod's. On the game's thread.",
    &["show", "hide"],
    watch_banners,
);

/// Banners of the game's hidden until a time (by type): a mod playing the bonfire lighting's
/// animation hides the "BONFIRE LIT" it shows.
static HIDE_UNTIL: Mutex<Vec<(i32, std::time::Instant)>> = Mutex::new(Vec::new());

/// Hides one of the game's banners (its type) for a while (installs the banner hook if needed).
pub fn hide_banner_for(kind: i32, d: std::time::Duration) -> Result<(), String> {
    if !BANNER_TYPES.contains(&kind) {
        return Err(format!("banner type must be {}-{}", BANNER_TYPES.start(), BANNER_TYPES.end()));
    }
    watch_banners()?;
    let mut hide = HIDE_UNTIL.lock().unwrap_or_else(|e| e.into_inner());
    hide.retain(|&(k, t)| k != kind && t > std::time::Instant::now());
    hide.push((kind, std::time::Instant::now() + d));
    Ok(())
}

fn banner_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let (kind, extra) = unsafe { ((*reg).rcx as i32, (*reg).rdx as u8) };
    if HIDE_UNTIL.lock().unwrap_or_else(|e| e.into_inner()).iter().any(|&(k, t)| k == kind && std::time::Instant::now() < t) {
        return 0;
    }
    if !OWN_BANNER.with(|b| b.get()) && BANNER_TYPES.contains(&kind) {
        if BANNER_SHOWING.answered() && BANNER_SHOWING.decide(&[kind as i64]) == 1 {
            crate::debug!("banner {kind} hidden (a mod's answer)");
            return 0;
        }
        SHOWN.lock().unwrap_or_else(|e| e.into_inner()).push(kind);
    }
    let f: unsafe extern "C" fn(i32, u8) = unsafe { std::mem::transmute(original) };
    unsafe { f(kind, extra) };
    0
}

/// Banner types the game showed since the last call.
pub fn take_banners() -> Vec<i32> {
    std::mem::take(&mut *SHOWN.lock().unwrap_or_else(|e| e.into_inner()))
}
