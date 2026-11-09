//! Event flags by the block: a flag set's 1000 flags at a time (values only), and a session set's
//! tables a mod rearranges (the catalogue's `flags.use_own_table` and `flags.separate_block`).
//!
//! The game keeps a character's flags in sets (ds3::FlagSet: its own, saved; a session's, which a
//! player in someone else's world uses). A set has a table per first digit of the flag (0x18 bytes
//! a digit: the areas, how many, a third word); a table is an array of areas (0xA8 bytes: ten
//! 16-byte block descriptors, the first 8 bytes of one its bits: 32 words, highest bit first) by
//! the area index of the flag's map (EventFlag_AreaIndex +0x4C7DA0, FieldArea's order); flags
//! under 10,000 are area 0, a block per thousand (EventFlag_Block +0x4C7F00). The game gives a
//! session's set no tables for some digits (the 5s, the pickups'; the 7s, the NPCs' and shops'),
//! and never makes or frees a session set's table: a mod can point it at the character's, or have
//! Kindle make it a copy whose chosen blocks are buffers of their own (seeded from the
//! character's), the rest the character's own bits. A block's values are copied, never the game's
//! pointers. Before a block is made separate the table's layout is checked against the game's own
//! flag reads (a mismatch refuses it).

use std::sync::{Mutex, OnceLock};

use crate::catalogue::{self, Kind, Value};
use crate::ds3::FlagSet;
use crate::lock;
use crate::{memory, patterns};

const DIGIT_BYTES: usize = 0x18;
const AREA_BYTES: usize = 0xA8;
const DESCRIPTOR_BYTES: usize = 0x10;
pub const WORDS: usize = 32;

/// EventFlag_AreaIndex (+0x4C7DA0: the flag manager, the map's area and block, 1): the area's
/// index in a table, or -1.
const AREA_INDEX: &str = "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 8b fa 48 8b e9 41 8b d0 8b cf 41 0f b6 f1 41 8b d8 e8";

fn flag_man() -> Result<usize, String> {
    crate::ds3::EVENT_FLAG_MAN.instance().ok_or_else(|| "event flags are not available yet".into())
}

/// A block's first flag, checked.
fn block_first(first: i64) -> Result<u32, String> {
    let f = crate::ds3::check_flag(first)?;
    if f % 1000 != 0 {
        return Err(format!("{first} isn't a block's first flag (a multiple of 1000)"));
    }
    Ok(f)
}

/// The table slot of a set for a flag's first digit.
fn slot(man: usize, set: FlagSet, first: u32) -> usize {
    man + set.at().0 + (first / 10_000_000) as usize % 10 * DIGIT_BYTES
}

/// A table's (begin, end), from its slot.
fn table(slot: usize) -> Option<(usize, usize)> {
    let (begin, areas) = (memory::read_u64(slot)? as usize, memory::read_u64(slot + 8)? as usize);
    (begin != 0 && (1..4096).contains(&areas)).then(|| (begin, begin + areas * AREA_BYTES))
}

/// Where a block's descriptor is in its table (bytes from its start).
fn descriptor_offset(man: usize, first: u32) -> Result<usize, String> {
    if first < 10_000 {
        return Ok((first / 1000) as usize * DESCRIPTOR_BYTES);
    }
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    let at = AT.get_or_init(|| patterns::find_unique("event flag area", AREA_INDEX)).ok_or("the game's flag area lookup is unavailable on this game build")?;
    let f: extern "C" fn(usize, u32, u32, u8) -> i32 = unsafe { std::mem::transmute(at) };
    let index = f(man, first / 100_000 % 100, first / 10_000 % 10, 1);
    if index < 0 {
        return Err(format!("flag {first}'s map has no flags"));
    }
    Ok(index as usize * AREA_BYTES + (first / 1000 % 10) as usize * DESCRIPTOR_BYTES)
}

fn words(at: usize) -> Option<[u32; WORDS]> {
    let b = memory::read_bytes(at, WORDS * 4)?;
    let mut w = [0u32; WORDS];
    for (i, c) in b.as_chunks::<4>().0.iter().enumerate() {
        w[i] = u32::from_le_bytes(*c);
    }
    Some(w)
}

fn write_words(at: usize, w: &[u32; WORDS]) {
    for (i, v) in w.iter().enumerate() {
        unsafe { std::ptr::write_volatile((at + i * 4) as *mut u32, *v) };
    }
}

/// Where a set's block's bits are (None: the set has no such block now).
fn bits_at(set: FlagSet, first: u32) -> Result<Option<usize>, String> {
    let man = flag_man()?;
    let Some((begin, end)) = table(slot(man, set, first)) else { return Ok(None) };
    let off = descriptor_offset(man, first)?;
    if begin + off + DESCRIPTOR_BYTES > end {
        return Ok(None);
    }
    Ok(memory::read_u64(begin + off).filter(|&b| b != 0).map(|b| b as usize))
}

/// A set's block of 1000 flags from `first` (32 words, highest bit first), or None.
pub fn block(set: FlagSet, first: i64) -> Result<Option<[u32; WORDS]>, String> {
    let first = block_first(first)?;
    Ok(bits_at(set, first)?.and_then(words))
}

/// Writes a set's block (on the game's thread).
pub fn set_block(set: FlagSet, first: i64, w: &[u32; WORDS]) -> Result<(), String> {
    if !crate::on_game_thread() {
        return Err("flag blocks are written on the game's thread only".into());
    }
    let first = block_first(first)?;
    let at = bits_at(set, first)?.ok_or_else(|| format!("the {set:?} set has no block for {first} now"))?;
    if words(at).as_ref() != Some(w) {
        write_words(at, w);
    }
    Ok(())
}

// ── a session set's tables ──────────────────────────────────────────────────────────────────

/// The session set's table for a digit points at the character's own (where it has none): its
/// flags of that digit are the character's in a session as at home.
fn use_own_table(digit: usize) -> Result<bool, String> {
    if !crate::on_game_thread() {
        return Err("from the game's thread only".into());
    }
    let man = flag_man()?;
    let first = digit as u32 * 10_000_000;
    let (own, session) = (slot(man, FlagSet::Own, first), slot(man, FlagSet::Session, first));
    if memory::read_u64(session) != Some(0) {
        return Ok(false);
    }
    if !memory::read_u64(own).is_some_and(|p| p != 0) {
        return Err(format!("the character has no table for the {digit}s now"));
    }
    for i in 0..3 {
        if let Some(v) = memory::read_u64(own + i * 8) {
            unsafe { std::ptr::write_volatile((session + i * 8) as *mut u64, v) };
        }
    }
    crate::info!("flags: the session set's {digit}s are the character's own");
    Ok(true)
}

/// A digit's session table made Kindle's copy of the character's, with blocks of its own.
struct Split {
    digit: usize,
    /// The character's table (begin), which it was made from.
    character: usize,
    /// Kindle's copy (begin), as long as its descriptors.
    table: usize,
    len: usize,
    /// The blocks made separate (first flag).
    blocks: Vec<u32>,
}

static SPLITS: Mutex<Vec<Split>> = Mutex::new(Vec::new());
/// Digits whose layout was checked against the game's reads: (digit, the character's table).
static CHECKED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());

/// Whether a table read with these offsets is what the game reads (the set in use pointing at
/// it) for this block: Some(true) yes, Some(false) no, None nothing set there to compare.
fn layout_matches(man: usize, begin: usize, end: usize, first: u32) -> Option<bool> {
    let Ok(off) = descriptor_offset(man, first) else { return Some(false) };
    if begin + off + DESCRIPTOR_BYTES > end {
        return Some(false);
    }
    let bits = memory::read_u64(begin + off).filter(|&b| b != 0)?;
    let w = words(bits as usize)?;
    if w.iter().all(|&x| x == 0) {
        return None;
    }
    for n in 0..1000u32 {
        let mine = w[n as usize / 32] & (1 << (31 - n % 32)) != 0;
        if crate::ds3::get_flag(first + n).ok() != Some(mine) {
            return Some(false);
        }
    }
    Some(true)
}

/// Makes a block of the session set separate from the character's (see the module's notes): 2 made
/// now, 1 already, 0 not yet (the layout can't be checked yet: nothing set in it).
fn separate_block(first: i64) -> Result<i64, String> {
    if !crate::on_game_thread() {
        return Err("from the game's thread only".into());
    }
    let first = block_first(first)?;
    let digit = (first / 10_000_000) as usize % 10;
    let man = flag_man()?;
    if crate::ds3::flag_set_in_use()? != FlagSet::Session {
        return Err("the session's flag set isn't in use".into());
    }
    if first < 10_000 {
        return separate_low_block(man, first);
    }
    let session_slot = slot(man, FlagSet::Session, first);
    let (character, end) = table(slot(man, FlagSet::Own, first)).ok_or_else(|| format!("the character has no table for the {digit}s now"))?;
    let session = memory::read_u64(session_slot).unwrap_or(0) as usize;
    let mut splits = lock(&SPLITS);
    // Made for another character or another session set: the game's own now.
    splits.retain(|s| s.digit != digit || (s.character == character && s.table == session));
    if let Some(s) = splits.iter().find(|s| s.digit == digit)
        && s.blocks.contains(&first)
    {
        return Ok(1);
    }
    if !splits.iter().any(|s| s.digit == digit) {
        if session != character {
            return Err(format!("the session set's {digit}s aren't the character's (flags.use_own_table first)"));
        }
        let mut checked = lock(&CHECKED);
        if !checked.contains(&(digit, character)) {
            match layout_matches(man, character, end, first) {
                Some(true) => checked.push((digit, character)),
                Some(false) => return Err(format!("the {digit}s flags aren't laid out as Kindle reads them")),
                None => return Ok(0),
            }
        }
        let bytes = memory::read_bytes(character, end - character).ok_or("the character's table is unreadable")?;
        let copy: Vec<u64> = bytes.as_chunks::<8>().0.iter().map(|c| u64::from_le_bytes(*c)).collect();
        let len = copy.len() * 8;
        let table = Box::leak(copy.into_boxed_slice()).as_ptr() as usize;
        let third = memory::read_u64(slot(man, FlagSet::Own, first) + 16).unwrap_or(0);
        for (i, v) in [table as u64, (len / AREA_BYTES) as u64, third].into_iter().enumerate() {
            unsafe { std::ptr::write_volatile((session_slot + i * 8) as *mut u64, v) };
        }
        splits.push(Split { digit, character, table, len, blocks: Vec::new() });
    }
    let s = splits.iter_mut().find(|s| s.digit == digit).expect("made above");
    let off = descriptor_offset(man, first)?;
    if off + DESCRIPTOR_BYTES > s.len {
        return Err(format!("flag {first} is outside the {digit}s table"));
    }
    let own = memory::read_u64(s.table + off).unwrap_or(0) as usize;
    let seed = if own != 0 { words(own).unwrap_or([0; WORDS]) } else { [0; WORDS] };
    let ours = Box::leak(Box::new(seed)).as_mut_ptr() as usize;
    unsafe { std::ptr::write_volatile((s.table + off) as *mut u64, ours as u64) };
    s.blocks.push(first);
    Ok(2)
}

/// The low flags (under 10,000) have a table of their own in each set, but the game has the
/// session set's 6000s, 7000s and 8000s be the character's own bits (their descriptors point at
/// the own set's: seen on two games, host and player, 2026-10-17), so a value written in the
/// session's (a host's world flags) was written in the character's own world too. Such a block
/// gets a copy of its own, seeded from the character's; a block the session set has apart already
/// is left. The game makes each set's blocks once, from its own memory, as it starts, and frees
/// none of them one by one (SprjEventFlagManImp's constructor +0x4C3680 -> +0x4C6AD0), so the
/// descriptor pointing at Kindle's copy is safe; if the game points it back at the character's,
/// the next call makes it apart again.
fn separate_low_block(man: usize, first: u32) -> Result<i64, String> {
    let own = bits_at(FlagSet::Own, first)?.ok_or_else(|| format!("the character has no block for {first} now"))?;
    let (begin, end) = table(slot(man, FlagSet::Session, first)).ok_or("the session set has no table for the low flags now")?;
    let off = descriptor_offset(man, first)?;
    if begin + off + DESCRIPTOR_BYTES > end {
        return Err(format!("flag {first} is outside the session set's table"));
    }
    if memory::read_u64(begin + off).is_none_or(|b| b as usize != own) {
        return Ok(1);
    }
    let seed = words(own).ok_or("the character's block is unreadable")?;
    let ours = Box::leak(Box::new(seed)).as_mut_ptr() as usize;
    unsafe { std::ptr::write_volatile((begin + off) as *mut u64, ours as u64) };
    crate::info!("flags: the session set's {first}s are apart from the character's own");
    Ok(2)
}

/// kindle.game: flags.use_own_table (catalogue.rs).
pub static USE_OWN_TABLE: catalogue::Function = catalogue::Function {
    name: "flags.use_own_table",
    doc: "In a session, the session's flag set has no table for some first digits (the 5s, the pickups'; the 7s, the NPCs' and shops'): points it at the character's own table for this digit, so those flags are the character's there as at home. True if it did now, false if the session set has a table of its own.",
    args: &[("digit", Kind::Int { min: 0, max: 9 })],
    call: |a| match a {
        [Value::Int(d)] => use_own_table(*d as usize).map(|b| Some(Value::Bool(b))),
        _ => Err("flags.use_own_table takes a digit".into()),
    },
};

/// kindle.game: flags.separate_block (catalogue.rs).
pub static SEPARATE_BLOCK: catalogue::Function = catalogue::Function {
    name: "flags.separate_block",
    doc: "With the session's flag set in use and its table for the block's digit the character's (flags.use_own_table): gives the session set its own copy of the block of 1000 flags from `first`, seeded from the character's, while the digit's other blocks stay the character's own (kindle.flags.block and set_block read and write either). 2 made now, 1 already, 0 not yet (Kindle checks the table's layout against the game's own reads first, and needs a flag set in the block for it). A block of the low flags (under 10,000) the game has the session set share with the character's (the 6000s, 7000s, 8000s) gets a copy of its own too; one already apart gives 1.",
    args: &[("first", Kind::Int { min: 0, max: 99_999_000 })],
    call: |a| match a {
        [Value::Int(f)] => separate_block(*f).map(|r| Some(Value::Int(r))),
        _ => Err("flags.separate_block takes a block's first flag".into()),
    },
};

/// kindle.game: the game's network-connected flag sets (catalogue.rs; emevd.rs reports them).
pub static NETWORK_SET: catalogue::Event = catalogue::Event::hooked(
    "flags.network_set",
    "The game's event scripts set network-connected flags on this game (2003[62] and [63]: a door opened, an object broken, a shortcut): the first and last flag and the state. Mods' own commands (kindle.events.run) aren't reported.",
    &["first", "last", "on"],
    crate::emevd::watch_network_flags,
);

/// kindle.game: the game's plain flag sets (catalogue.rs; emevd.rs reports them).
pub static PLAIN_SET: catalogue::Event = catalogue::Event::hooked(
    "flags.plain_set",
    "The game's event scripts set flags the plain way on this game (2003[2], [22], [9], [17]: set, batch set, invert, random set; what they may change, first to last; whether it's the random set: each game rolls its own; and whether the event keeps the game's network sync on: an event that turns it off (2000[02]) runs for this game alone; and the event's id). Mods' own commands (kindle.events.run) aren't reported.",
    &["first", "last", "random", "synced", "event"],
    crate::emevd::watch_network_flags,
);

// ── every change the game makes ─────────────────────────────────────────────────────────────

/// The game's flag setter (+0x4C5E20: the flag manager, the flag, the state, a fourth byte it
/// gives the block lookup; its entry is a jump into Arxan's code, which comes back to the body).
/// Every flag the game sets goes through it: its event scripts' commands, and its own code with no
/// command (a summon sign's summon flag, a character's registration, an item's); only the game's
/// network-connected sets (EventFlag_Set +0x4C6170, 2003[62]/[63]) then send the flag to the
/// session's other games. The hook is at the body's start (`cmp byte [rcx+0x228], 0`, the
/// manager's "flags ready"), the arguments still in rcx, edx, r8b and r9b.
const SETTER_BODY: &str = "80 b9 28 02 00 00 00 45 0f b6 f9 45 0f b6 e0 8b ea 48 8b f9 0f 84 ?? ?? ?? ?? 45 0f b6 c1 4c 89 74 24 50 e8";
/// From the hook's place to the body's call of EventFlag_Block (+0x4C7F00: the manager, the flag,
/// the fourth byte; the flag's block descriptor, its first 8 bytes the bits, or 0).
const BLOCK_CALL: usize = 0x23;
static BLOCK_FN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

thread_local! {
    /// Kindle's (and so mods') own sets on this thread, which aren't reported: a mod knows what it
    /// set.
    static KINDLE_SETTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The game setting a flag another game of the session sent (EventFlag_SetFromNetwork).
    static RECEIVING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// EventFlag_SetFromNetwork (+0x4C61A0: the flag manager, the flag, the state): sets a flag
/// another game sent (packet 0x1A, read each frame by +0x77B740) into the session's set through
/// the setter. Its changes are told apart from this game's own: a player's game sets the host's
/// there as well as its own world's changes.
const SET_FROM_NETWORK: &str = "48 89 6c 24 10 48 89 74 24 18 57 48 83 ec 20 41 0f b6 e8 8b f2 48 8b f9 e8 ?? ?? ?? ?? 84 c0 74";

fn set_from_network_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            RECEIVING.with(|k| k.set(self.0));
        }
    }
    let _restore = Restore(RECEIVING.with(|k| k.replace(true)));
    let f: extern "C" fn(usize, usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize, r.rdx as usize, r.r8 as usize)
}

static SET_FROM_NETWORK_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "flags",
    name: "event flag set from the network",
    pattern: SET_FROM_NETWORK,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(set_from_network_hook),
    without: "flags.changed can't tell the flags other games sent from this game's own",
};

/// Runs `f`, the flags it sets not reported as the game's changes (flags.changed).
pub fn kindle_sets<R>(f: impl FnOnce() -> R) -> R {
    // Put back as it was however `f` ends (a panic caught further out included).
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            KINDLE_SETTING.with(|k| k.set(self.0));
        }
    }
    let _restore = Restore(KINDLE_SETTING.with(|k| k.replace(true)));
    f()
}

fn setter_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &*reg };
    let (man, flag, on, fourth) = (r.rcx as usize, r.rdx as u32 as i32, r.r8 as u8 != 0, r.r9 as u8);
    if flag < 0 || KINDLE_SETTING.with(|k| k.get()) || memory::read::<u8>(man + 0x228).is_none_or(|ready| ready == 0) {
        return;
    }
    let block_fn = BLOCK_FN.load(std::sync::atomic::Ordering::Relaxed);
    if block_fn == 0 {
        return;
    }
    let block: extern "C" fn(usize, i32, u8) -> usize = unsafe { std::mem::transmute(block_fn) };
    let descriptor = block(man, flag, fourth);
    let Some(bits) = (descriptor != 0).then(|| memory::read_u64(descriptor)).flatten().filter(|&b| b != 0) else { return };
    let offset = (flag % 1000) as usize;
    let Some(word) = memory::read::<u32>(bits as usize + offset / 32 * 4) else { return };
    if (word >> (31 - offset % 32) & 1 != 0) == on {
        return;
    }
    let session = match memory::read::<u32>(man + 0x220) {
        Some(0) => false,
        Some(1) => true,
        _ => return,
    };
    let received = RECEIVING.with(|k| k.get());
    catalogue::emit(&CHANGED, vec![Value::Int(flag.into()), Value::Bool(on), Value::Bool(session), Value::Bool(received)]);
}

static SETTER_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "flags",
    name: "event flag setter",
    pattern: SETTER_BODY,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::JmpBack(setter_hook),
    without: "mods aren't told of the flags the game changes (flags.changed)",
};

fn watch_changes() -> Result<(), String> {
    let at = SETTER_SITE.find().and_then(|m| m.first().copied()).ok_or(SETTER_SITE.without)?;
    let call = at + BLOCK_CALL;
    let rel = memory::read::<i32>(call + 1).filter(|_| memory::read::<u8>(call) == Some(0xE8)).ok_or("the flag setter's block lookup isn't where it was")?;
    BLOCK_FN.store((call as i64 + 5 + rel as i64) as usize, std::sync::atomic::Ordering::Relaxed);
    // Before the setter's: a received flag is never reported as this game's own.
    SET_FROM_NETWORK_SITE.install().ok_or(SET_FROM_NETWORK_SITE.without)?;
    std::mem::forget(SETTER_SITE.hook_at(at)?); // for the life of the process
    Ok(())
}

/// kindle.game: every flag the game changes (catalogue.rs).
pub static CHANGED: catalogue::Event = catalogue::Event::hooked(
    "flags.changed",
    "A flag changed on this game, whatever part of the game set it: its event scripts, and its own code with no event command (a summon sign's summon flag, a character's, an item's), which flags.plain_set and flags.network_set don't report. The flag, its new state, whether it changed in the session's set (a player's in someone else's world; on any game, the flags the session's other games send go there) rather than the character's own, and whether another game of the session sent it (received; else this game's own change). Only real changes (a set to the state it had isn't one); mods' own sets (kindle.flags.set, kindle.events.run) aren't reported.",
    &["flag", "on", "session", "received"],
    watch_changes,
);
