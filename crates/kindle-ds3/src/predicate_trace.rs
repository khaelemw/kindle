//! The predicate trace (debug.trace_predicates): which of the game's code asks "who is this
//! player" and what it's told. The game treats a summoned player differently through a few
//! predicates: whether this game is a guest in a session (Session_IsGuest +0x78C0D0), in
//! multiplayer (+0x78C0F0, talk query 52), the world's owner (+0x78C110, the cutscene skip), the
//! session's host by its world (GameMan_IsSessionHost +0x627AF0), online (+0x627B40), and the
//! per-character-type table's columns (+0x9F61A0..+0x9F6480: a ghost's form, hostile, sounds,
//! the animation events' hits, the death replay, the world owner, a message's reward). While the
//! trace is on, each call is counted by predicate, the game's caller (its return address) and,
//! for the table, the type asked about, with how many times the answer was true; the watchdog's
//! reports list them. A move made as the host and the same as a joined player, then the two lists
//! side by side, show where the game tells them apart.
//!
//! The hooks go in the first time the trace is turned on and stay (passing everything through
//! while it's off). The type table's loot column (+0x9F6420) isn't traced: co-op finds the table
//! through that function's bytes.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::hook_sites::{Kind, Site};
use crate::lock;
use crate::memory;

static ON: AtomicBool = AtomicBool::new(false);
/// (predicate, caller's offset in the game, the type asked about or -1) -> (calls, true answers).
type Counts = HashMap<(&'static str, usize, i32), (u64, u64)>;
static COUNTS: Mutex<Option<Counts>> = Mutex::new(None);

/// Calls the predicate and counts the call (the hooks below, each with its name; `typed`: rcx
/// points at a character type).
fn traced(name: &'static str, reg: *mut ilhook::x64::Registers, original: usize, typed: bool) -> usize {
    let r = unsafe { &*reg };
    let f: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(original) };
    let out = f(r.rcx as usize);
    if ON.load(Ordering::Relaxed) {
        let base = memory::game_module().map_or(0, |(b, _)| b);
        let caller = memory::read_u64(r.rsp as usize).unwrap_or(0) as usize;
        let ty = if typed { memory::read::<i32>(r.rcx as usize).unwrap_or(-1) } else { -1 };
        let mut counts = lock(&COUNTS);
        let e = counts.get_or_insert_with(HashMap::new).entry((name, caller.wrapping_sub(base), ty)).or_default();
        e.0 += 1;
        e.1 += (out & 0xFF != 0) as u64;
    }
    out
}

/// A hook counting one predicate's calls (`typed`: rcx points at a character type).
macro_rules! traced_hook {
    ($hook:ident, $name:literal, $typed:literal) => {
        fn $hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
            traced($name, reg, original, $typed)
        }
    };
}
traced_hook!(guest_hook, "Session_IsGuest", false);
traced_hook!(multiplayer_hook, "Session_InMultiplayer", false);
traced_hook!(world_owner_hook, "Session_IsWorldOwner", false);
traced_hook!(session_host_hook, "GameMan_IsSessionHost", false);
traced_hook!(online_hook, "GameMan_IsOnline", false);
traced_hook!(ghost_form_hook, "type table c0 ghost form", true);
traced_hook!(hostile_hook, "type table c1 hostile", true);
traced_hook!(sounds_hook, "type table c3 phantom sounds", true);
traced_hook!(hits_hook, "type table c5 animation hits", true);
traced_hook!(replay_hook, "type table c6 death replay", true);
traced_hook!(owner_column_hook, "type table c7 world owner", true);
traced_hook!(reward_hook, "type table c8 message reward", true);

static GUEST_SITE: Site = Site {
    area: "predicate trace",
    name: "Session_IsGuest",
    pattern: "48 83 ec 28 e8 47 be e9 ff 48 8b c8 48 83 c4 28 e9 9b 87 ef ff",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(guest_hook),
    without: "it isn't traced",
};
static MULTIPLAYER_SITE: Site = Site {
    area: "predicate trace",
    name: "Session_InMultiplayer",
    pattern: "48 83 ec 28 e8 27 be e9 ff 48 8b c8 48 83 c4 28 e9 cb 87 ef ff",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(multiplayer_hook),
    without: "it isn't traced",
};
static WORLD_OWNER_SITE: Site = Site {
    area: "predicate trace",
    name: "Session_IsWorldOwner",
    pattern: "48 83 ec 28 e8 07 be e9 ff 48 8b c8 48 83 c4 28 e9 eb 87 ef ff",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(world_owner_hook),
    without: "it isn't traced",
};
static SESSION_HOST_SITE: Site = Site {
    area: "predicate trace",
    name: "GameMan_IsSessionHost",
    pattern: "48 8b 05 ?? ?? ?? ?? 0f b6 80 1e 0b 00 00 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(session_host_hook),
    without: "it isn't traced",
};
static ONLINE_SITE: Site = Site {
    area: "predicate trace",
    name: "GameMan_IsOnline",
    pattern: "48 8b 0d ?? ?? ?? ?? 83 b9 ec 0a 00 00 01 0f 94 c0 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(online_hook),
    without: "it isn't traced",
};
static GHOST_FORM_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c0 ghost form",
    pattern: "48 63 01 83 f8 0d 77 10 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 04 88 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(ghost_form_hook),
    without: "it isn't traced",
};
static HOSTILE_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c1 hostile",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 01 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(hostile_hook),
    without: "it isn't traced",
};
static SOUNDS_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c3 phantom sounds",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 03 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(sounds_hook),
    without: "it isn't traced",
};
static HITS_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c5 animation hits",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 05 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(hits_hook),
    without: "it isn't traced",
};
static REPLAY_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c6 death replay",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 06 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(replay_hook),
    without: "it isn't traced",
};
static OWNER_COLUMN_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c7 world owner",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 07 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(owner_column_hook),
    without: "it isn't traced",
};
static REWARD_SITE: Site = Site {
    area: "predicate trace",
    name: "type table c8 message reward",
    pattern: "48 63 01 83 f8 0d 77 11 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 08 c3",
    offset: 0,
    matches: 1,
    kind: Kind::Retn(reward_hook),
    without: "it isn't traced",
};

static SITES: [&Site; 12] = [
    &GUEST_SITE,
    &MULTIPLAYER_SITE,
    &WORLD_OWNER_SITE,
    &SESSION_HOST_SITE,
    &ONLINE_SITE,
    &GHOST_FORM_SITE,
    &HOSTILE_SITE,
    &SOUNDS_SITE,
    &HITS_SITE,
    &REPLAY_SITE,
    &OWNER_COLUMN_SITE,
    &REWARD_SITE,
];

/// Turns the trace on (from now: earlier counts are dropped) or off; how many predicates are hooked.
pub fn set(on: bool) -> Result<usize, String> {
    static HOOKED: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let n = *HOOKED.get_or_init(|| SITES.iter().filter(|s| s.install().is_some()).count());
    if n == 0 {
        return Err("none of the game's predicates were found".into());
    }
    if on {
        *lock(&COUNTS) = Some(HashMap::new());
    }
    ON.store(on, Ordering::Relaxed);
    crate::info!("predicate trace {} ({n} of {} predicates)", if on { "on" } else { "off" }, SITES.len());
    Ok(n)
}

/// The counts so far, most called first: (predicate, caller offset, type or -1, calls, true answers).
pub fn counts() -> Vec<(&'static str, usize, i32, u64, u64)> {
    let mut out: Vec<_> = lock(&COUNTS).as_ref().map(|c| c.iter().map(|(&(n, at, ty), &(calls, yes))| (n, at, ty, calls, yes)).collect()).unwrap_or_default();
    out.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.cmp(b.0)).then(a.1.cmp(&b.1)));
    out
}

/// The counts as report lines (the watchdog's), while the trace is on.
pub fn report_lines() -> Vec<String> {
    if !ON.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut lines = vec!["predicate trace (predicate, caller, type: calls, true):".to_string()];
    lines.extend(
        counts()
            .into_iter()
            .map(|(n, at, ty, calls, yes)| format!("  {n} from +{at:#x}{}: {calls}, {yes}", if ty >= 0 { format!(" type {ty}") } else { String::new() })),
    );
    lines
}
