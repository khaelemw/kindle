//! The game's online session, carried by Kindle's own connection (p2p): create (host), join
//! (the summon to the host's place), state, the session's code, leave; the host's place for a
//! player joining it, through its loads; and, as hook points of the catalogue, the game's own
//! ends of a hosted session (its leave and its close), which a mod can keep open. kindle.net
//! (natives/net.rs) gives mods host, join (off the game's thread), leave and code.
//!
//! The game's session manager (`SprjSessionManagerImp`) keeps a state at +0x16C: 0 none,
//! 1 creating, 2 create failed, 3 hosting, 4 joining, 5 join failed, 6 joined (7 is set by the
//! leave paths). Its create (`create(manager, timeout 60.0, players 4, flag)`, the arguments the
//! game's own caller passes) makes a lobby through the manager's Steam session
//! (`SteamSessionLight` at +0x18, the lobby id at +0x3F8) and goes on to hosting; Kindle makes
//! that lobby its own (p2p). Verified 2026-10-06 with Steam's lobby: from
//! Kindle, in single player with the launch setting on Play Offline, state 1, then 3 within a
//! second.
//!
//! Joining uses the manager's session info at +0x58 (the same object holds a host's own info):
//! a map of properties, the host's name, and at +0x58 a pointer to the lobby id's 8 bytes (+0x60
//! their count). The game normally gets it from a summon sign through FromSoft's server; its
//! join (`join(manager)`) needs only that lobby id set. So Kindle connects to the host, writes
//! the host's lobby id into that buffer and calls join. A game that has never hosted or used a
//! sign has no buffer yet; Kindle makes one as the game's own info constructor does (+0x1963590):
//! 8 bytes from the game's network allocator (its vtable slot 10: size, alignment 16), and
//! +0x68 the 16-bit value the constructor stores there. Whether the join needs more than the id
//! is still to be seen in a session with two players (not tested yet).
//!
//! Summoning (experimental, 2026-10-06): a guest gets into the host's world the way the game's
//! JoinSessionJob does it (+0xC84DA0 → +0x8507E0): `+0x851180(_, start*)` puts a 0x25-byte start
//! block into GameMan's multiplay start fields (+0xA68 map, +0xA70 position, +0xA80 rotation,
//! +0xA90, +0xA94, +0xC54, +0xC58 type, +0xC59), then FrpgSessionManagerImp's start
//! (`+0x78C600(manager, info*)`) joins with the session info (SprjSessionManager's JoinWith) and
//! sets up the multiplay. The block: i32 map (packed like the current map's block id,
//! 0x28000000 for m40_00), f32 x, y, z, f32 facing, i32 the host's ceremony for that map, i32 -1,
//! i32 -1, i32 type, u8 0 (JoinSessionJob's defaults, -1/0, for the rest). The ceremony (+0xA90)
//! is the state the map loads in for a joining player (the map load, +0x8E9A57, takes it while the
//! session is joining, a player's own world state otherwise): -1 left the player's own, so a
//! player whose world had the Cemetery of Ash as Untended Graves (m40's ceremony 10) got the
//! host's Cemetery that way, a black sky, no tower and Untended Graves' enemies (verified
//! 2026-10-09 with two games on one PC). The third -1 (+0xC54) is the summon's kind
//! as the game's JoinSession step (+0x470920) reads it: -1 a white sign ("JoinSession_White"),
//! -2 a black one, and so on. The type's values aren't decoded (the game checks 11 and 12);
//! the join takes one to try.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::catalogue::{Event, HookPoint, Value};
use crate::hook_sites::{Kind, Site};
use crate::lock;
use crate::{memory, patterns};

const CREATE: &str = "48 8b c4 44 88 48 20 55 41 56 41 57 48 8d 68 a1 48 81 ec 90 00 00 00 48 c7 45 d7 fe ff ff ff \
    48 89 58 08 48 89 70 10 48 89 78 18 0f 29 70 d8";
const JOIN: &str = "48 8b c4 57 48 83 ec 70 48 c7 40 a8 fe ff ff ff 48 89 58 08 48 89 70 10 48 8b d9 \
    83 b9 6c 01 00 00 06 0f 84";
const MANAGER: &str = "SprjSessionManagerImp";
const STATE_AT: usize = 0x16C;
const INFO_AT: usize = 0x58;
const INFO_ID_AT: usize = 0x58;
const INFO_ID_LEN_AT: usize = 0x60;
const INFO_TAG_AT: usize = 0x68;
/// In the game's info constructor, where it makes the lobby id buffer: the call getting the
/// network allocator, the allocation, and the two calls giving the value stored at +0x68.
const INFO_ID_FILL: &str = "e8 ?? ?? ?? ?? 4c 8b 08 41 b8 10 00 00 00 48 8b 9c 24 98 00 00 00 48 8b d3 48 8b c8 41 ff 51 50 \
    49 89 47 58 48 85 c0 75 17 44 8d 40 5e 48 8d 15 ?? ?? ?? ?? 48 8d 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 49 8b 4f 58 \
    48 85 c9 74 0f 49 89 5f 60 4c 8b c3 48 8b d7 e8 ?? ?? ?? ?? e8 ?? ?? ?? ?? 48 8b c8 e8 ?? ?? ?? ?? 66 41 89 47 68";
const ALLOCATOR_CALL_AT: usize = 0;
const TAG_SOURCE_CALL_AT: usize = 88;
const TAG_CALL_AT: usize = 96;
const ALLOC_SLOT: usize = 10;
const ALLOC_ALIGN: usize = 0x10;
const LOBBY_ID_SIZE: usize = 8;
/// Puts a multiplay start block into GameMan (+0x851180).
const SET_START: &str = "40 53 48 83 ec 70 48 c7 44 24 30 fe ff ff ff 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 44 24 60 48 8b da 8b 4a 1c e8";
/// FrpgSessionManagerImp's join with a session info, multiplay setup included (+0x78C600).
const START_JOIN: &str = "48 89 5c 24 08 57 48 83 ec 20 48 8b d9 48 8b 0d ?? ?? ?? ?? 48 8b fa 48 85 c9 75 26 4c 8d 0d ?? ?? ?? ?? \
    4c 8d 05 ?? ?? ?? ?? 48 8d 0d ?? ?? ?? ?? ba b1 00 00 00 e8 ?? ?? ?? ?? 48 8b 0d ?? ?? ?? ?? 48 8b d7 e8 ?? ?? ?? ?? \
    0f b6 f8 84 c0 74 0c 48 8b 8b 78 01 00 00";
pub(crate) const FRPG_MANAGER: &str = "FrpgSessionManagerImp";
const FIELD_AREA: &str = "FieldArea";
/// The multiplay type a join asks for when none is given (a guess: not decoded yet).
pub const DEFAULT_TYPE: u32 = 1;
const START_SIZE: usize = 0x28;
/// What the game's own caller passes to create.
const CREATE_TIMEOUT: f32 = 60.0;
const PLAYERS: u32 = 4;

type CreateFn = unsafe extern "C" fn(usize, f32, u32, u8) -> u8;
type JoinFn = unsafe extern "C" fn(usize) -> u8;

struct Game {
    create: usize,
    join: usize,
}

fn game() -> Option<&'static Game> {
    static FOUND: OnceLock<Option<Game>> = OnceLock::new();
    FOUND.get_or_init(|| Some(Game { create: patterns::find_unique("session create", CREATE)?, join: patterns::find_unique("session join", JOIN)? })).as_ref()
}

/// The game's functions for a session info's lobby id buffer: (allocator, tag source, tag).
fn info_fill() -> Option<(usize, usize, usize)> {
    static FOUND: OnceLock<Option<(usize, usize, usize)>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        let at = patterns::find_unique("session info lobby id", INFO_ID_FILL)?;
        Some((patterns::call_target(at + ALLOCATOR_CALL_AT)?, patterns::call_target(at + TAG_SOURCE_CALL_AT)?, patterns::call_target(at + TAG_CALL_AT)?))
    })
}

/// The game's online identity: a static (read at +0x783DB2 in the player record check +0x783D80,
/// pattern at +0x0E) holding P; X = [P], Y = [X+0x60]; the player's online id is the dword at
/// Y+0x518 (what +0xC7E090(P) returns). FromSoftware's server sets it; offline it is 0 on every
/// game, and a game rejects another's player record (message 0x08) whose id equals its own
/// (verified with a trace: `call +0xC7E090; cmp [rbx+4], eax; je`), so offline no remote player
/// record is ever taken and the game never builds the other players' characters.
const ONLINE_ID_SITE: &str = "48 8b 48 10 48 85 c9 74 05 3b 51 10 74 e3 48 8b 0d ?? ?? ?? ?? e8";
const ONLINE_ID_STATIC_AT: usize = 0x0E;
const ONLINE_ID_AT: usize = 0x518;

/// Gives this game a unique online id when it has none (offline): the low 32 bits of its Steam
/// id (made non-zero), so the other players' games accept its player record.
fn ensure_online_id() {
    static STATIC: OnceLock<Option<usize>> = OnceLock::new();
    let Some(st) = *STATIC.get_or_init(|| patterns::lea_target(patterns::find_unique("online id", ONLINE_ID_SITE)? + ONLINE_ID_STATIC_AT)) else {
        return;
    };
    let y = memory::read_u64(st)
        .filter(|&p| p != 0)
        .and_then(|p| memory::read_u64(p as usize))
        .filter(|&x| x != 0)
        .and_then(|x| memory::read_u64(x as usize + 0x60))
        .filter(|&y| y != 0);
    let Some(y) = y else { return crate::warn!("session: the game's online identity isn't there; other players may not appear") };
    let at = y as usize + ONLINE_ID_AT;
    if memory::read::<u32>(at) == Some(0) {
        let id = crate::p2p::my_id().map_or(1, |s| (s as u32).max(1));
        unsafe { std::ptr::write_volatile(at as *mut u32, id) };
        crate::info!("session: online id {id:#x} (none offline)");
    }
}

/// The game's "leave the session" (SprjSessionManager, rcx: state +0x16C to 7 and the
/// low-level leave started with a reason; two copies, +0xDF8730 and +0xDF7A00, differing only in
/// that). The game asks for it when the last guest leaves (its member-left handler +0x78D220),
/// when a summoned player hasn't arrived in time (a timer in +0x78F4B0) and more. A host that
/// ends its session goes to the title screen 5 s later (verified 2026-10-07 on two games on one
/// PC: the session state +0xDC goes 5 to 0, then the game's protected network check sets
/// GameMan+0x8, the "end this world" menu request 0xFA follows and the world unloads). Answered
/// "keep", a host's session stays (the call reports "not leaving", as the game's own does when a
/// leave is already under way), as Seamless Co-op does: players can join again. Kindle's own
/// leave (leave()) always goes through.
const SESSION_LEAVE: &str = "48 89 5c 24 10 57 48 83 ec 20 8b 81 6c 01 00 00 40 32 ff 48 8b d9 83 f8 05 77 17 b9 25 00 00 00 0f a3 c1 73 0d 32 c0";
/// The session's close to new players (+0x78C7F0, FrpgSessionManagerImp): it marks the hosted
/// session closed (SprjSessionManagerImp +0x160 = 1, +0xDF87D0) and kicks the players still
/// arriving; a player who joins a closed session is kicked as it arrives (reason 0xff000019, in
/// the host's member-joined handler +0xDF9E30). The multiplayer flow's "BlockClear2" (+0x48EE70,
/// the area's boss defeated) calls it: vanilla co-op ends there. Answered "keep", a host's session
/// stays open, so its players come back after a boss dies (verified 2026-10-07 at Vordt with two
/// games on one PC: before, every rejoin was kicked in half a second).
const SESSION_CLOSE: &str = "48 89 5c 24 10 56 48 83 ec 20 48 8b f1 48 8b 0d ?? ?? ?? ?? 48 85 c9 75 ?? 4c 8d 0d ?? ?? ?? ?? 4c 8d 05 ?? ?? ?? ?? 48 8d 0d ?? ?? ?? ?? ba b1 00 00 00 e8 ?? ?? ?? ?? 48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 48 8b 86 a8 00 00 00";

static SESSION_LEAVE_SITE: Site = Site {
    area: "session",
    name: "session leave",
    pattern: SESSION_LEAVE,
    offset: 0,
    matches: 2,
    kind: Kind::Retn(session_leave_hook),
    without: "a host's session ends with its last guest",
};

static SESSION_CLOSE_SITE: Site = Site {
    area: "session",
    name: "session close",
    pattern: SESSION_CLOSE,
    offset: 0,
    matches: 1,
    kind: Kind::Retn(session_close_hook),
    without: "players can't come back after a boss dies",
};

/// The game's session leave functions (both copies), found before either is hooked (the hook
/// writes over the pattern).
fn session_leave_fns() -> Option<&'static [usize]> {
    static FOUND: OnceLock<Option<Vec<usize>>> = OnceLock::new();
    FOUND.get_or_init(|| SESSION_LEAVE_SITE.find()).as_deref()
}

/// Kindle's own leave is under way: the hook lets it through.
static KINDLE_LEAVING: AtomicBool = AtomicBool::new(false);

fn session_leave_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    if !KINDLE_LEAVING.load(Ordering::Relaxed) && LEAVE_REQUESTED.switch() == 1 && crate::p2p::hosting() == Some(true) {
        note_kept_leave(crate::multiplayer_flow::game_callers(r.rsp as usize, 10));
        return 0;
    }
    let leave: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(original) };
    leave(r.rcx as usize)
}

/// A kept leave in the log: the game's calls that led to it (its immediate caller is inside
/// FrpgSession_LeaveSession, which says nothing of why). The game asks again every frame while its
/// reason lasts (60 a second in one co-op session): a chain is logged when it first
/// comes, and then how many times it came, every LEAVE_LOG_EVERY.
const LEAVE_LOG_EVERY: std::time::Duration = std::time::Duration::from_secs(10);

fn note_kept_leave(callers: String) {
    static LAST: Mutex<Option<(String, std::time::Instant, u32)>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::Instant::now();
    match last.as_mut() {
        Some((chain, since, count)) if *chain == callers => {
            *count += 1;
            if now.duration_since(*since) >= LEAVE_LOG_EVERY {
                crate::info!("session: the game asked {count} more time(s) to end the hosted session (from {chain}); kept");
                (*since, *count) = (now, 0);
            }
        }
        _ => {
            crate::info!("session: the game would end the hosted session (from {callers}); kept (a mod's answer)");
            *last = Some((callers, now, 0));
        }
    }
}

fn session_close_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    if CLOSE_REQUESTED.switch() == 1 && crate::p2p::hosting() == Some(true) {
        crate::info!("session: the game would close the session to new players (a boss died); kept open (a mod's answer)");
        return 0;
    }
    let f: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize)
}

fn install_leave_hook() -> Result<(), String> {
    let found = session_leave_fns().ok_or(SESSION_LEAVE_SITE.without)?;
    for &at in found {
        let hook = SESSION_LEAVE_SITE.hook_at(at)?;
        std::mem::forget(hook);
    }
    Ok(())
}

/// kindle.game: session.leave_requested (catalogue.rs).
pub static LEAVE_REQUESTED: HookPoint = HookPoint::hooked(
    "session.leave_requested",
    "The game ends the session it hosts (its last guest gone, a summon not arrived in time...), and the host goes to the title screen 5 s later: \"game\", or \"keep\" (the session stays: players can join again). Only a host's; Kindle's own leave (kindle.net.leave) goes through. A switch.",
    &["game", "keep"],
    install_leave_hook,
);

/// kindle.game: session.close_requested (catalogue.rs).
pub static CLOSE_REQUESTED: HookPoint = HookPoint::hooked(
    "session.close_requested",
    "The game closes the session it hosts to new players (a boss died: vanilla co-op ends there): \"game\", or \"keep\" (it stays open: players can come back). A switch.",
    &["game", "keep"],
    || SESSION_CLOSE_SITE.install().map(|_| ()).ok_or_else(|| SESSION_CLOSE_SITE.without.to_string()),
);

/// The summon functions: (set start, start join).
fn summon_fns() -> Option<(usize, usize)> {
    static FOUND: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    *FOUND.get_or_init(|| Some((patterns::find_unique("multiplay start", SET_START)?, patterns::find_unique("multiplay join", START_JOIN)?)))
}

/// Where this player is, for a guest to be summoned to: the current map's block id and state
/// (FieldArea: +0x10 table, +0x20 index; entries of 0x70 bytes at table +0x20, the id at +8, the
/// ceremony at +0x64, a CeremonyParam id as Set Map Ceremony writes it) and the character's
/// position and facing.
pub fn host_location() -> Option<crate::p2p::Location> {
    let field = crate::ds3::singleton_instance(FIELD_AREA)?;
    let table = memory::read_u64(field + 0x10).filter(|&t| t != 0)? as usize;
    let index = memory::read::<i32>(field + 0x20)?;
    let count = memory::read::<i32>(table + 0x18)?;
    if index < 0 || index >= count {
        return None;
    }
    let entries = memory::read_u64(table + 0x20).filter(|&e| e != 0)? as usize;
    let entry = entries + index as usize * 0x70;
    let map = memory::read::<i32>(entry + 8)?;
    let ceremony = memory::read::<i32>(entry + 0x64).unwrap_or(-1);
    let pos = crate::lua_values::player_pos()?;
    let angle = crate::chars::facing(crate::game::player_chr_addr().ok()?)?;
    Some(crate::p2p::Location { map, pos, angle, ceremony })
}

/// The info's lobby id buffer, made as the game makes it if the info has none.
fn lobby_id_buffer(info: usize) -> Result<usize, String> {
    if let Some(buffer) = memory::read_u64(info + INFO_ID_AT).filter(|&p| p != 0) {
        if memory::read_u64(info + INFO_ID_LEN_AT) != Some(LOBBY_ID_SIZE as u64) || memory::read_u64(buffer as usize).is_none() {
            return Err("the game's session info isn't ready".into());
        }
        return Ok(buffer as usize);
    }
    let (allocator, tag_source, tag) = info_fill().ok_or("the game's session info functions weren't found")?;
    let allocator: extern "C" fn() -> usize = unsafe { std::mem::transmute(allocator) };
    let a = allocator();
    let alloc =
        memory::read_u64(a).and_then(|vt| memory::read_u64(vt as usize + 8 * ALLOC_SLOT)).filter(|&f| f != 0).ok_or("the game's allocator isn't there")?;
    let alloc: extern "C" fn(usize, usize, usize) -> usize = unsafe { std::mem::transmute(alloc as usize) };
    let buffer = alloc(a, LOBBY_ID_SIZE, ALLOC_ALIGN);
    if buffer == 0 {
        return Err("the game couldn't make room for the session".into());
    }
    let tag_source: extern "C" fn() -> usize = unsafe { std::mem::transmute(tag_source) };
    let tag: extern "C" fn(usize) -> u16 = unsafe { std::mem::transmute(tag) };
    let value = tag(tag_source());
    unsafe {
        std::ptr::write_volatile(buffer as *mut u64, 0);
        std::ptr::write_volatile((info + INFO_ID_LEN_AT) as *mut u64, LOBBY_ID_SIZE as u64);
        std::ptr::write_volatile((info + INFO_TAG_AT) as *mut u16, value);
        std::ptr::write_volatile((info + INFO_ID_AT) as *mut u64, buffer as u64);
    }
    Ok(buffer)
}

pub(crate) fn manager() -> Result<usize, String> {
    crate::ds3::singleton_instance(MANAGER).ok_or_else(|| "the game's session manager isn't there yet".to_string())
}

/// The session state, by name.
pub fn state() -> Result<&'static str, String> {
    let m = manager()?;
    let s = memory::read::<u32>(m + STATE_AT).ok_or("the session state is unreadable")?;
    Ok(match s {
        0 => "none",
        1 => "creating",
        2 => "create failed",
        3 => "hosting",
        4 => "joining",
        5 => "join failed",
        6 => "joined",
        7 => "leaving",
        _ => "unknown",
    })
}

/// Hosts a session (the game's create). Needs a character in the world and no session.
pub fn host() -> Result<(), String> {
    if !crate::game::in_world() {
        return Err("host from the world, with a character loaded".into());
    }
    if !matches!(state()?, "none" | "create failed" | "join failed") {
        return Err(format!("there's a session already ({})", state()?));
    }
    let g = game().ok_or("the game's session functions weren't found")?;
    let create: CreateFn = unsafe { std::mem::transmute(g.create) };
    ensure_online_id();
    // The lobby the game makes is Kindle's (p2p).
    crate::p2p::arm_host();
    if unsafe { create(manager()?, CREATE_TIMEOUT, PLAYERS, 0) } == 0 {
        return Err("the game couldn't start a session".into());
    }
    Ok(())
}

/// This session's code: a host's (once it's up), or the one this player joined with.
pub fn code() -> Option<String> {
    crate::p2p::code()
}

/// Whether a join may start now (from the world, with no session).
fn can_join() -> Result<(), String> {
    if !crate::game::in_world() {
        return Err("join from the world, with a character loaded".into());
    }
    if !matches!(state()?, "none" | "create failed" | "join failed") {
        return Err(format!("there's a session already ({})", state()?));
    }
    game().ok_or("the game's session functions weren't found")?;
    Ok(())
}

/// The game's join of the session Kindle connected to (lobby, the host's location).
fn join_connected(kind: u32, lobby: u64, host: Option<crate::p2p::Location>) -> Result<(), String> {
    let g = game().ok_or("the game's session functions weren't found")?;
    let m = manager()?;
    let buffer = lobby_id_buffer(m + INFO_AT)?;
    unsafe { std::ptr::write_volatile(buffer as *mut u64, lobby) };
    // With the host's location: the game's summon (start block, then its full join).
    if let (Some(at), Some((set_start, start_join)), Some(frpg)) = (host, summon_fns(), crate::ds3::singleton_instance(FRPG_MANAGER)) {
        #[repr(C, align(16))]
        struct Start([u8; START_SIZE]);
        let mut start = Start([0; START_SIZE]);
        let b = &mut start.0;
        b[0..4].copy_from_slice(&at.map.to_le_bytes());
        for (i, f) in [at.pos[0], at.pos[1], at.pos[2], at.angle].iter().enumerate() {
            b[4 + 4 * i..8 + 4 * i].copy_from_slice(&f.to_le_bytes());
        }
        b[0x14..0x18].copy_from_slice(&at.ceremony.to_le_bytes());
        for at in [0x18, 0x1C] {
            b[at..at + 4].copy_from_slice(&(-1i32).to_le_bytes());
        }
        b[0x20..0x24].copy_from_slice(&kind.to_le_bytes());
        let set_start: extern "C" fn(usize, *const u8) = unsafe { std::mem::transmute(set_start) };
        let start_join: extern "C" fn(usize, usize) -> u8 = unsafe { std::mem::transmute(start_join) };
        set_start(0, start.0.as_ptr());
        let joined = start_join(frpg, m + INFO_AT);
        crate::info!("session: summon to {:#x} (ceremony {}) at {:?} (type {kind}): join {}", at.map, at.ceremony, at.pos, joined != 0);
        if joined == 0 {
            crate::p2p::leave();
            return Err("the game couldn't join that session".into());
        }
        return Ok(());
    }
    let join: JoinFn = unsafe { std::mem::transmute(g.join) };
    if unsafe { join(m) } == 0 {
        crate::p2p::leave();
        return Err("the game couldn't join that session".into());
    }
    Ok(())
}

// ── a join off the game's thread ────────────────────────────────────────────────────────────

/// How a join ended, for its caller.
#[derive(Debug, Clone, PartialEq)]
pub enum JoinOutcome {
    Joined,
    /// Nothing answers at the host's address (no host there any more).
    Gone(String),
    /// The host turned this player away for now (why, from the host: "the host is on a loading
    /// screen", a DLC it doesn't have...).
    Refused(String),
    /// Anything else (no character in the world, a session already, the game's join failed).
    Failed(String),
}

/// A join under way (join_later): who asked (a mod's id and its callback's handle), and Kindle's
/// connection's result once its thread has it (lobby, the host's location).
struct PendingJoin {
    owner: String,
    handle: u64,
    connected: Option<Result<(u64, Option<crate::p2p::Location>), crate::p2p::ConnectError>>,
}

static PENDING: Mutex<Option<PendingJoin>> = Mutex::new(None);

/// Starts a join without blocking the game: Kindle connects to the host on a thread of its own,
/// and the game's join follows on the game's thread (finished_joins, each frame). One at a time.
pub fn join_later(code: &str, owner: &str, handle: u64) -> Result<(), String> {
    let (address, key) = crate::p2p::parse(code).ok_or("that isn't a session code")?;
    can_join()?;
    let mut pending = lock(&PENDING);
    if pending.is_some() {
        return Err("a join is under way already".into());
    }
    ensure_online_id();
    *pending = Some(PendingJoin { owner: owner.to_string(), handle, connected: None });
    drop(pending);
    let spawned = std::thread::Builder::new().name("kindle-session-join".into()).spawn(move || {
        let connected = crate::p2p::connect_detailed(address, key);
        if let Some(p) = lock(&PENDING).as_mut() {
            p.connected = Some(connected);
        }
    });
    if spawned.is_err() {
        *lock(&PENDING) = None;
        return Err("couldn't start the join".into());
    }
    Ok(())
}

/// Each frame (the game's thread): a join whose connection is done is made in the game; who
/// asked, and how it ended.
pub fn finished_joins() -> Vec<(String, u64, JoinOutcome)> {
    let mut pending = lock(&PENDING);
    if pending.as_ref().is_none_or(|p| p.connected.is_none()) {
        return Vec::new();
    }
    let p = pending.take().expect("checked above");
    drop(pending);
    let outcome = match p.connected.expect("checked above") {
        Ok((lobby, host)) => match can_join().and_then(|_| join_connected(DEFAULT_TYPE, lobby, host)) {
            Ok(()) => JoinOutcome::Joined,
            Err(e) => {
                crate::p2p::leave();
                JoinOutcome::Failed(e)
            }
        },
        Err(crate::p2p::ConnectError::Refused(why)) => JoinOutcome::Refused(why),
        Err(crate::p2p::ConnectError::Other(e)) if crate::p2p::host_gone(&e) => JoinOutcome::Gone(e),
        Err(crate::p2p::ConnectError::Other(e)) => JoinOutcome::Failed(e),
    };
    vec![(p.owner, p.handle, outcome)]
}

// ── leaving ──────────────────────────────────────────────────────────────────────────────────

/// Ends this game's session: Kindle's connection (the others see it leave) and the game's (a
/// player's game goes home; a host's session ends). Kindle's own leave goes past a mod's
/// session.leave_requested. `by_choice`: the player means to leave (not a follow's leave for now):
/// Kindle's party doesn't join it to that world again.
pub fn leave(by_choice: bool) -> Result<(), String> {
    let had_session = crate::p2p::hosting().is_some();
    let _ = by_choice;
    crate::p2p::leave();
    if had_session {
        ended(true);
    }
    if !matches!(state(), Ok("none" | "create failed" | "join failed"))
        && let Some(&at) = session_leave_fns().and_then(|f| f.first())
        && let Ok(m) = manager()
    {
        KINDLE_LEAVING.store(true, Ordering::Relaxed);
        let leave: extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(at) };
        leave(m);
        KINDLE_LEAVING.store(false, Ordering::Relaxed);
    }
    Ok(())
}

/// kindle.game: session.left (catalogue.rs).
pub static LEFT: Event = Event::new(
    "session.left",
    "Kindle's session ended: by_this_game true when this game left it (kindle.net.leave, or the Co-op tab's Leave), false when the link to the host closed (the host left, or went away).",
    &["by_this_game"],
);

/// The session's state, for the watchdog's reports.
pub(crate) fn summary() -> Vec<String> {
    vec![
        format!("session: {:?} (hosting {:?}), me {:?}, members {:?}", state(), crate::p2p::hosting(), crate::p2p::me(), crate::p2p::members()),
        format!("host location (this game's world): {:?}", host_location()),
    ]
}

/// Kindle's session ended (this game's leave, or p2p: the link closed): mods hear it at the
/// next frame.
pub(crate) fn ended(by_this_game: bool) {
    crate::catalogue::emit(&LEFT, vec![Value::Bool(by_this_game)]);
}

// ── where a joining player arrives ───────────────────────────────────────────────────────────

/// The host's load, as its game goes through it: the old world still up, then taken down (no
/// map), then the new one built with this game's character in it.
static HOST_LOAD: AtomicU8 = AtomicU8::new(0);
const LOAD_OLD_WORLD: u8 = 1;
const LOAD_TORN_DOWN: u8 = 2;
const LOAD_ARRIVED: u8 = 3;
static HOST_WAS_IN_WORLD: AtomicBool = AtomicBool::new(false);
/// The host: where a player joining now arrives (see arrival()), kept by the game's thread.
static ARRIVAL: Mutex<Option<crate::p2p::Location>> = Mutex::new(None);

/// The host: where a player that joins now is summoned to. In a world, where the host is. On a
/// loading screen (a travel, a death's reload, any load), the place its game has put its
/// character in the new world: a load builds the new map and places the character at its bonfire
/// about 3.5 s before the loading screen ends (High Wall and Cemetery of Ash, 2026-10-16), so a
/// player's load runs alongside the host's instead of after it. Before that, none: the player
/// asks again.
pub fn arrival() -> Option<crate::p2p::Location> {
    *lock(&ARRIVAL)
}

/// Each frame (the game's thread): where the host's load is, and where a player arrives.
pub(crate) fn frame() {
    if crate::p2p::hosting() != Some(true) {
        HOST_LOAD.store(0, Ordering::Relaxed);
        HOST_WAS_IN_WORLD.store(false, Ordering::Relaxed);
        *lock(&ARRIVAL) = None;
        return;
    }
    let in_world = crate::game::in_world();
    if HOST_WAS_IN_WORLD.swap(in_world, Ordering::Relaxed) && !in_world {
        HOST_LOAD.store(LOAD_OLD_WORLD, Ordering::Relaxed);
    }
    let here = host_location();
    let phase = if in_world { 0 } else { HOST_LOAD.load(Ordering::Relaxed) };
    let phase = match phase {
        LOAD_OLD_WORLD if here.is_none() => LOAD_TORN_DOWN,
        LOAD_TORN_DOWN if here.is_some() => {
            crate::info!("session: the host's character is in its new world; players joining now load in alongside it");
            LOAD_ARRIVED
        }
        p => p,
    };
    HOST_LOAD.store(phase, Ordering::Relaxed);
    *lock(&ARRIVAL) = if in_world || phase == LOAD_ARRIVED { here } else { None };
}
