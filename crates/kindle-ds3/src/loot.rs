//! Loot on a guest's game, as hook points of the catalogue: where the game keeps a game that is a
//! guest in a session (a co-op player, to it a phantom) from its own drops, treasure and awards. A
//! mod (co-op) answers them so loot is every player's own.
//!
//! | | |
//! |---|---|
//! | drops and treasure | the guest checks before an enemy's drop roll (+0x7C0D30) and the area's treasure (+0x7C13D0): `loot.guest_loot` |
//! | item numbers | the map item makers' numbering (+0x7C54A0, +0x7C50C0): `loot.guest_item_ids` |
//! | treasure pickup | +0x460C20: `loot.treasure_flag` |
//! | item messages | session message 0x2C, read by +0x7C3C00: `loot.item_messages` |
//! | event awards | the award's checks (+0x7C10F0): `loot.guest_awards` |
//! | host's enemy HP | WorldChrMan +0x8DB570's guest branch: `enemy.host_hp_death` |

use std::sync::{Mutex, OnceLock};

use crate::catalogue::HookPoint;
use crate::hook_sites::{self, Patch, Site};
use crate::lock;
use crate::{memory, patterns};

// ── a guest's drops and treasure ────────────────────────────────────────────────────────────

/// A game that's a guest in a session (asked of the session manager, +0x4763F38) neither rolls an
/// enemy's drop as it dies (+0x7C0D30) nor loads its area's map treasure (+0x7C13D0): a co-op
/// player saw no item on the host's corpses and treasure (verified 2026-10-09 with two games on one
/// PC: the drop was on the host's corpse, the player's MapItemMan held no item at all). Each check
/// is followed by a branch past its "no" (je over `xor al, al; jmp`); answered "own", it's made
/// unconditional, so a guest's game rolls its own drops and loads its own treasure, as by its own
/// pickup flags. Then each asks whether the player's character type takes loot (the per-type
/// table's column 2, chr_type.set).
const ENEMY_DROPS: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 74 04 32 c0 eb ?? bb 01 00 00 00";
const MAP_TREASURE: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 8d 4b 01 84 c0 74 04 32 c0 eb";

static ENEMY_DROPS_PATCH: Patch =
    Patch { area: "loot", name: "enemy drops", pattern: ENEMY_DROPS, offset: 14, matches: 1, from: &[0x74], to: &[0xEB], without: "a guest gets no drops" };

static MAP_TREASURE_PATCH: Patch = Patch {
    area: "loot",
    name: "map treasure",
    pattern: MAP_TREASURE,
    offset: 17,
    matches: 1,
    from: &[0x74],
    to: &[0xEB],
    without: "a guest gets no treasure",
};

/// kindle.game: loot.guest_loot (catalogue.rs).
pub static GUEST_LOOT: HookPoint = HookPoint::hooked(
    "loot.guest_loot",
    "A game that is a guest in a session rolls no drops for the enemies that die and loads no treasure in its areas: \"game\", or \"own\" (it rolls and loads its own, by its own pickup flags). Answered \"own\", the game's code is changed until it closes (it changes only a guest's game).",
    &["game", "own"],
    || {
        let drops = ENEMY_DROPS_PATCH.apply();
        let treasure = MAP_TREASURE_PATCH.apply();
        (drops && treasure).then_some(()).ok_or_else(|| ENEMY_DROPS_PATCH.without.to_string())
    },
);

// ── a guest's own item numbers ──────────────────────────────────────────────────────────────

/// As the game makes a map item (+0x7C54A0 treasure, +0x7C50C0 an enemy's drop, +0x7C52C0 an item
/// left on the ground) it gives it its number (the id's upper bits, from a counter of MapItemMan's:
/// +0xC, +0x10, +0x8) only if it isn't a guest in a session (the host's numbers come with the host's
/// items). A guest's own treasure and drops (loot.guest_loot) so all had the same id and a pickup,
/// which looks the item up by id, found another and did nothing (verified 2026-10-09). Answered
/// "own", the branch past the numbering (jne) goes in the two makers of a guest's own items, and
/// as a guest's game numbers its first own item after a load (a hook at the counter's read) its
/// counters move up into a range of their own (OWN_ITEM_BASE on): both games load the same
/// treasure in the same order, so a guest's ids were the host's, and the host's message naming one
/// of its items (take it off) named one of the guest's too. An item left on the ground is the
/// host's (one a guest leaves goes to the host, which adds it and sends it to all with its number):
/// a guest's game keeps that number, as the game has it.
const ITEM_NUMBERING: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 75 ?? 8b ?? ?? 8d ?? 01 89 ?? ?? 48 8b ?? e8";
const OWN_ITEM_COUNTERS: [usize; 2] = [0xC, 0x10];
const OWN_ITEM_BASE: i32 = 1 << 24;
const MAP_ITEM_MAN_NAME: &str = "MapItemMan";
/// The makers of a guest's own items among the numbering's matches: those whose counter (the
/// `mov reg, [base+disp8]` read at +16) is one of OWN_ITEM_COUNTERS.
const COUNTER_READ_AT: usize = 16;

fn own_item_makers(found: &[usize]) -> Vec<usize> {
    found
        .iter()
        .copied()
        .filter(|&m| memory::read::<u8>(m + COUNTER_READ_AT) == Some(0x8B))
        .filter(|&m| memory::read::<u8>(m + COUNTER_READ_AT + 2).is_some_and(|d| OWN_ITEM_COUNTERS.contains(&(d as usize))))
        .collect()
}

fn own_item_ids_hook(_reg: *mut ilhook::x64::Registers) {
    if GUEST_ITEM_IDS.switch() != 1 {
        return;
    }
    let Some(items) = crate::ds3::singleton_instance(MAP_ITEM_MAN_NAME) else { return };
    for at in OWN_ITEM_COUNTERS.map(|off| items + off) {
        if let Some(n) = memory::read::<i32>(at).filter(|&n| (0..OWN_ITEM_BASE).contains(&n)) {
            unsafe { std::ptr::write_volatile(at as *mut i32, n + OWN_ITEM_BASE) };
        }
    }
}

static ITEM_NUMBERING_PATCH: Patch = Patch {
    area: "loot",
    name: "map item numbering",
    pattern: ITEM_NUMBERING,
    offset: 14,
    matches: 3,
    from: &[0x75],
    to: &[0x90, 0x90],
    without: "a guest can't pick up its treasure",
};

static OWN_ITEM_IDS_SITE: Site = Site {
    area: "loot",
    name: "a guest's own item numbers",
    pattern: ITEM_NUMBERING,
    offset: 16,
    matches: 3,
    kind: hook_sites::Kind::JmpBack(own_item_ids_hook),
    without: "a guest can't pick up items left on the ground once someone took them",
};

fn install_item_ids() -> Result<(), String> {
    // (One search for both: each changes bytes the pattern matches.)
    let found = ITEM_NUMBERING_PATCH.find().ok_or(ITEM_NUMBERING_PATCH.without)?;
    let own = own_item_makers(&found);
    if own.len() != OWN_ITEM_COUNTERS.len() {
        return Err(format!("{} of the game's item makers are a guest's own (expected {})", own.len(), OWN_ITEM_COUNTERS.len()));
    }
    (ITEM_NUMBERING_PATCH.apply_at(&own) && OWN_ITEM_IDS_SITE.hook_all(&own)).then_some(()).ok_or_else(|| ITEM_NUMBERING_PATCH.without.to_string())
}

/// kindle.game: loot.guest_item_ids (catalogue.rs).
pub static GUEST_ITEM_IDS: HookPoint = HookPoint::hooked(
    "loot.guest_item_ids",
    "A guest's own map items (its treasure and drops, loot.guest_loot) get no numbers of their own: \"game\", or \"own\" (numbered as the host's are, in a range the host's never reach). Answered \"own\", the game's numbering is changed until it closes (only a guest's own items); the range follows the answer.",
    &["game", "own"],
    install_item_ids,
);

// ── a guest's treasure pickup ───────────────────────────────────────────────────────────────

/// A treasure picked up (+0x460C20, the item's id in r8): a host's game sets the treasure's pickup
/// flag, a guest's doesn't (the host does, for its own world), so a guest's own treasure was back
/// at its next load (verified 2026-10-09: picked up, its flag still unset). Answered "set", this
/// game sets it, from the item's record in MapItemMan (its treasure list at +0x28; a node's id at
/// +0x20, its flag at +0x28), when it's a pickup flag (5xxxxxxx).
const TREASURE_PICKUP: &str = "48 89 5c 24 08 57 48 83 ec 20 48 8b 0d ?? ?? ?? ?? 48 8b fa 41 8b d0 41 8b d9 e8";
const PICKUP_FLAG_DIGIT: u32 = 5;
/// The static holding MapItemMan (read by the treasure pickup at +0xD of TREASURE_PICKUP; read
/// before the hook: its instruction is among those the hook moves).
static MAP_ITEM_MAN: OnceLock<usize> = OnceLock::new();

fn treasure_pickup_hook(reg: *mut ilhook::x64::Registers) {
    if TREASURE_FLAG.switch() != 1 {
        return;
    }
    let id = unsafe { (*reg).r8 } as u32;
    let Some(items) = MAP_ITEM_MAN.get().copied().filter(|&a| a != 0).and_then(memory::read_u64).filter(|&p| p != 0) else { return };
    let mut node = memory::read_u64(items as usize + 0x28).unwrap_or(0) as usize;
    for _ in 0..4096 {
        if node == 0 {
            return;
        }
        if memory::read::<u32>(node + 0x20) == Some(id) {
            if let Some(flag) = memory::read::<u32>(node + 0x28).filter(|&f| f / 10_000_000 % 10 == PICKUP_FLAG_DIGIT)
                && let Err(e) = crate::ds3::set_flag(flag, true)
            {
                crate::warn!("loot: couldn't keep a treasure picked up ({e})");
            }
            return;
        }
        node = memory::read_u64(node).unwrap_or(0) as usize;
    }
}

static TREASURE_PICKUP_SITE: Site = Site {
    area: "loot",
    name: "treasure pickup",
    pattern: TREASURE_PICKUP,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(treasure_pickup_hook),
    without: "a guest's treasure comes back",
};

fn install_treasure_pickup() -> Result<(), String> {
    let found = TREASURE_PICKUP_SITE.find().ok_or(TREASURE_PICKUP_SITE.without)?;
    let at = found[0];
    let _ = MAP_ITEM_MAN.set(patterns::rip_target(at + 0xD, at + 0x11).unwrap_or(0));
    TREASURE_PICKUP_SITE.hook_all(&found).then_some(()).ok_or_else(|| TREASURE_PICKUP_SITE.without.to_string())
}

/// kindle.game: loot.treasure_flag (catalogue.rs).
pub static TREASURE_FLAG: HookPoint = HookPoint::hooked(
    "loot.treasure_flag",
    "A guest picks up treasure, and its game doesn't set the treasure's pickup flag (the host's does, for its own world): \"game\", or \"set\" (this game sets it, so the treasure stays taken). A switch.",
    &["game", "set"],
    install_treasure_pickup,
);

// ── the map item messages ───────────────────────────────────────────────────────────────────

/// The map item messages between the games (session message type 0x2C, read each frame by
/// +0x7C3C00; a header of kind 1 and a subtype, then an int): as a guest's world loads it asks the
/// host for its items (0), the host answers "clear yours" (1, -1: +0x7C5990, +0x7C5A80, +0x7C58A0
/// take them all off), and when the host picks one up it tells the guests to take it off too (2).
/// With its own treasure a guest then had it for a second only (verified 2026-10-09: 33 items as it
/// arrived, none a second later). Answered "keep_own", 1 is ignored; 2 is taken as the game does
/// (a guest's own items' ids are its own range, loot.guest_item_ids). The hook (at +0x25) is where
/// the loop looks at the header ([rsp+0x40]), whose kind it zeroes (matches no case).
const ITEM_MESSAGES: &str = "41 b8 2c 00 00 00 48 8b d7 48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 8b d0 85 c0 0f 84 ?? ?? ?? ?? 66 0f 1f 44 00 00 66 83 7c 24 40 01";

fn item_message_hook(reg: *mut ilhook::x64::Registers) {
    let header = unsafe { &*reg }.rsp as usize + 0x40;
    if ITEM_MESSAGES_ANSWER.switch() != 1 || memory::read::<u16>(header) != Some(1) {
        return;
    }
    if memory::read::<u16>(header + 2) == Some(1) {
        unsafe { std::ptr::write_volatile(header as *mut u16, 0) };
    }
}

static ITEM_MESSAGES_SITE: Site = Site {
    area: "loot",
    name: "map item messages",
    pattern: ITEM_MESSAGES,
    offset: 0x25,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(item_message_hook),
    without: "guests lose their map treasure to the host's",
};

/// kindle.game: loot.item_messages (catalogue.rs).
pub static ITEM_MESSAGES_ANSWER: HookPoint = HookPoint::hooked(
    "loot.item_messages",
    "The host's game tells a guest's to take all its map items off (as the guest's world loads): \"game\", or \"keep_own\" (ignored: a guest's own treasure and drops stay; items the host picks up still go). A switch.",
    &["game", "keep_own"],
    || ITEM_MESSAGES_SITE.install_or_why(),
);

// ── event awards ────────────────────────────────────────────────────────────────────────────

/// Event awards that check (r8b 1: 2003[04], the items events hand out, NPCs' items) turn a guest
/// away (+0x7C10F0: in a session, a check it fails skips it). Answered "allow", the branch to there
/// (je at +0x17 of EVENT_AWARD_GATE) goes, and the character type check after decides (the
/// per-type table's column 2). 2003[36] is the award with those checks off.
const EVENT_AWARD_GATE: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 74 09 e8 ?? ?? ?? ?? 84 c0 74 1e bb 01 00 00 00";

static EVENT_AWARD_GATE_PATCH: Patch = Patch {
    area: "loot",
    name: "event award gate",
    pattern: EVENT_AWARD_GATE,
    offset: 0x17,
    matches: 1,
    from: &[0x74],
    to: &[0x90, 0x90],
    without: "a guest gets no event awards",
};

/// kindle.game: loot.guest_awards (catalogue.rs).
pub static GUEST_AWARDS: HookPoint = HookPoint::hooked(
    "loot.guest_awards",
    "An event's award that checks the session (2003[04], items events hand out) turns a guest away: \"game\", or \"allow\" (the character type's loot column decides, as at home). Answered \"allow\", the game's code is changed until it closes.",
    &["game", "allow"],
    || EVENT_AWARD_GATE_PATCH.apply().then_some(()).ok_or_else(|| EVENT_AWARD_GATE_PATCH.without.to_string()),
);

// ── the host's enemy HP ─────────────────────────────────────────────────────────────────────

/// An enemy's HP is the host's: each frame a joined game sets its copies to the HP the host's game
/// has for them (WorldChrMan +0x8DB570, the branch for a game joined and not the host: the
/// session's table at WorldChrMan +0x630, 0xF987 for "none yet"), and a copy that had HP and hasn't
/// now dies there (+0x888FC0 with 0), and its death pays this game's player its souls (+0x8991B0),
/// as the game shares a kill's souls with phantoms. An enemy already dead in the host's world is
/// alive on a joined game for its first moments after a load, until its own event kills it without
/// souls (2004[04], "should receive souls" 0: ChrIns +0x1A0A bit 6): the host's 0 came first now
/// and then, and the player was paid for it at loads (Iudex Gundyr's 4000 at the Cemetery of Ash,
/// verified 2026-10-16). Answered "silent", a copy whose first value from the host since this
/// game's load is already 0 dies as the event would kill it, with that bit: no souls. One the
/// host's table had alive (or none yet) while this game had it pays as before.
const HOST_HP_APPLY: &str = "8b 50 78 66 3b f2 74 ?? 49 8b 8d 30 06 00 00 e8 ?? ?? ?? ?? 48 8b f0 48 85 c0 74 ?? \
    48 8b 8f 90 1f 00 00 48 8b 69 18 48 8b cd e8 ?? ?? ?? ?? b9 87 f9 00 00";
/// The hook: rdi the enemy's copy, rsi the host's entry for it (u16 HP).
const HOST_HP_APPLY_AT: isize = 0x1C;
const HP_NONE: u16 = 0xF987;
/// ChrIns: the byte whose bit 6 is "no souls for this death" (the event's forced death sets it).
const DEATH_FLAGS_AT: usize = 0x1A0A;
const NO_SOULS: u8 = 0x40;
/// The copies the host's table had alive (or none yet) since this game's load, and that load.
static SEEN_ALIVE: Mutex<Option<(std::time::Instant, std::collections::HashSet<usize>)>> = Mutex::new(None);

fn host_hp_hook(reg: *mut ilhook::x64::Registers) {
    if HOST_HP_DEATH.switch() != 1 {
        return;
    }
    let r = unsafe { &*reg };
    let (chr, entry) = (r.rdi as usize, r.rsi as usize);
    let Some(hp) = memory::read::<u16>(entry) else { return };
    let Some(load) = crate::game::in_world_since() else { return };
    let mut guard = lock(&SEEN_ALIVE);
    // A load makes new copies: their record starts again.
    if guard.as_ref().is_none_or(|(l, _)| *l != load) {
        *guard = Some((load, Default::default()));
    }
    let seen = &mut guard.as_mut().expect("made above").1;
    if hp == HP_NONE || hp != 0 {
        seen.insert(chr);
    } else if !seen.contains(&chr)
        && let Some(flags) = memory::read::<u8>(chr + DEATH_FLAGS_AT)
        && flags & NO_SOULS == 0
    {
        unsafe { std::ptr::write_volatile((chr + DEATH_FLAGS_AT) as *mut u8, flags | NO_SOULS) };
        seen.insert(chr);
        crate::debug!("loot: an enemy dead in the host's world before this game had it ({chr:#x}) dies without souls");
    }
}

static HOST_HP_SITE: Site = Site {
    area: "loot",
    name: "host's enemy HP",
    pattern: HOST_HP_APPLY,
    offset: HOST_HP_APPLY_AT,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(host_hp_hook),
    without: "a guest is paid the souls of an enemy the host killed before, at loads",
};

/// kindle.game: enemy.host_hp_death (catalogue.rs).
pub static HOST_HP_DEATH: HookPoint = HookPoint::hooked(
    "enemy.host_hp_death",
    "On a joined game an enemy's copy dies when the host's game has its HP at 0, and pays this game's player its souls: \"game\", or \"silent\" (one already dead in the host's world before this game had it dies without souls, as its own event would kill it). Hot: a switch.",
    &["game", "silent"],
    || HOST_HP_SITE.install_or_why(),
);
