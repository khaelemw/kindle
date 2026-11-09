//! Where the game treats a summoned player (a phantom) differently from the host, as hook points
//! and functions of the catalogue: a mod (co-op) answers them so a joined player plays as the host
//! does. Each is the game's own decision at the place it's made; the mod sets a switch (they run
//! on the game's thread, some every frame), and the game is hooked there once a mod first answers.
//!
//! | | |
//! |---|---|
//! | per-type table | 14 types x 20 bytes (the loot table's, +0x9F6420): `chr_type.get` / `chr_type.set` |
//! | event type test | 4[03] IF Character Type, built by +0x79C4F0: `events.chr_type_test` |
//! | event multiplayer state test | 3[06] built by +0x7A5580, 1003[05]/[06]/[105] checked by +0x7A5490: `events.multiplayer_state_test` |
//! | event own-world test | 3[26] built by +0x523590, 1003[12]/[13] and the skip checked by +0x523430: `events.own_world_test` |
//! | NPCs made as players | +0x89EA10's guest check: `npc.guest_look` |
//! | cutscene skip | SprjRemoMan::vf21 (+0xCDA640)'s world owner test: `cutscene.skip_owner` |
//! | ember's look | the ember slot's (+0x5B1680) world owner test: `chr.ember_look` |
//! | Estus allotment | World_LoadPlayerData (+0x8F7420)'s guest check: `load.estus_allotment` |
//! | flask level | FrpgNetBonfireDb's read and write (+0x546790, +0x546BA0): `bonfire.level_copy` |
//! | bonfires' session list | FrpgNetBonfireDb's session list, kept by the pre-load save (+0x475B50): `bonfire.session_list` |
//! | talk check | a talk's per-frame check of the player (+0xF032B0): `talk.player_check` |
//! | talk queries | 52 (in a session) and 60 (an NPC's ready effect), hooks.rs's talk hooks: `talk.in_session`, `talk.npc_ready` |

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::catalogue::{self, HookPoint, Kind, Value};
use crate::hook_sites::{self, Patch, Site};
use crate::{memory, patterns};

// ── the per-type table ──────────────────────────────────────────────────────────────────────

/// The per-type table (14 character types x 20 bytes; the loot table's, read by +0x9F6420): how
/// the world treats a character of each type (0 the host, 1 a white phantom, 2 a dark one...).
/// Its columns (read 2026-10-14 from each column's readers): 0 a ghost's form (SpEffectParam's
/// Live/Ghost targets, +0xA05030; the behaviour scripts' env query 1002; magic and equipment
/// conditions); 1 a hostile phantom; 2 takes loot (an enemy's drop, an event's award, treasure);
/// 3 sounds as a phantom (+0x9F63F0, animation and cutscene sound events); 4 (read nowhere that
/// tells it apart); 5 its animations act on the world (+0x9CDAA0/+0x9CDB60/+0x9CE1C0: the
/// animation events' hits and trigger shapes); 6 the player's death replay recording (PlayerIns
/// +0x8C9D10); 7 the world's owner (the session's join and leave, the host's event manager, enemy
/// detection); 8 a rated blood message's reward (+0x539880).
const LOOT_BY_TYPE: &str = "48 63 01 83 f8 0d 77 ?? 48 8d 0c 80 48 8d 05 ?? ?? ?? ?? 0f b6 44 88 02 c3";
const TYPES: i64 = 14;
const ROW_BYTES: usize = 20;
const COLUMNS: i64 = 9;

fn type_table() -> Result<usize, String> {
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    AT.get_or_init(|| patterns::find_unique("loot by chr type", LOOT_BY_TYPE).and_then(|at| patterns::rip_target(at + 15, at + 19)))
        .ok_or_else(|| "the game's per-type table is unavailable on this game build".into())
}

fn cell(a: &[Value]) -> Result<usize, String> {
    match a {
        [Value::Int(t), Value::Int(c), ..] => Ok(type_table()? + *t as usize * ROW_BYTES + *c as usize),
        _ => Err("a type and a column".into()),
    }
}

/// kindle.game: chr_type.get (catalogue.rs).
pub static CHR_TYPE_GET: catalogue::Function = catalogue::Function {
    name: "chr_type.get",
    doc: "A column of the game's per-type table (how the world treats a character type: 0 the host, 1 a white phantom, 2 a dark one, ...): 0 a ghost's form, 1 a hostile phantom, 2 takes loot, 3 sounds as a phantom, 4 (unused), 5 its animations act on the world, 6 the death replay, 7 the world's owner, 8 a rated message's reward.",
    args: &[("type", Kind::Int { min: 0, max: TYPES - 1 }), ("column", Kind::Int { min: 0, max: COLUMNS - 1 })],
    call: |a| memory::read::<u8>(cell(a)?).map(|v| Some(Value::Bool(v != 0))).ok_or_else(|| "the per-type table is unreadable".into()),
};

/// kindle.game: chr_type.set (catalogue.rs).
pub static CHR_TYPE_SET: catalogue::Function = catalogue::Function {
    name: "chr_type.set",
    doc: "Sets a column of the game's per-type table (see chr_type.get) for a type, until the game closes.",
    args: &[("type", Kind::Int { min: 0, max: TYPES - 1 }), ("column", Kind::Int { min: 0, max: COLUMNS - 1 }), ("on", Kind::Bool)],
    call: |a| {
        let Value::Bool(on) = a[2] else { return Err("chr_type.set: on is a boolean".into()) };
        let at = cell(a)?;
        if unsafe { crate::hooks::write_bytes(at, &[on as u8]) } { Ok(None) } else { Err("the per-type table couldn't be written".into()) }
    },
};

// ── the event scripts' character type test ──────────────────────────────────────────────────

/// The event scripts' character type test (4[03] IF Character Type, built by +0x79C4F0 as an
/// EcTestChrType: +0x20 the entity, +0x24 the type): every one in the game's events is of the local
/// player (10000), and they tell the host (0) from a white phantom (1). 16 events act for the host
/// only (one-sided doors, contraptions, Painted World and Lothric events, enemies' reactions to the
/// player in an area); 4 for a white phantom only (the phantom's half of something the host does
/// through "in its own world"); 104 take any (2026-10-14, from all events). Answered "as_phantom",
/// a test of "the host" built now tests a white phantom: on a co-op player's game in a host's world
/// (the events are built as the world loads) the host's events run for it too. The hook is after
/// the stores (rax: the new test).
const CHR_TYPE_CONDITION: &str = "40 57 48 83 ec 50 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 60 48 89 74 24 68 0f 29 74 24 40 0f 28 f3 41 8b d8 0f b6 fa 8b f1 4c 8b 05 ?? ?? ?? ?? 4c 89 44 24 28 ba 08 00 00 00 8d 4a 30 e8";
const LOCAL_PLAYER: i32 = 10000;
const HOST_TYPE: u8 = 0;
const WHITE_PHANTOM: u8 = 1;

fn chr_type_condition_hook(reg: *mut ilhook::x64::Registers) {
    let test = unsafe { (*reg).rax } as usize;
    if test == 0 || CHR_TYPE_TEST.switch() != 1 || memory::read::<i32>(test + 0x20) != Some(LOCAL_PLAYER) || memory::read::<u8>(test + 0x24) != Some(HOST_TYPE)
    {
        return;
    }
    unsafe { std::ptr::write_volatile((test + 0x24) as *mut u8, WHITE_PHANTOM) };
}

static CHR_TYPE_CONDITION_SITE: Site = Site {
    area: "multiplayer",
    name: "character type condition",
    pattern: CHR_TYPE_CONDITION,
    offset: 0x82,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(chr_type_condition_hook),
    without: "mods can't make the events' host tests take a phantom",
};

/// kindle.game: events.chr_type_test (catalogue.rs).
pub static CHR_TYPE_TEST: HookPoint = HookPoint::hooked(
    "events.chr_type_test",
    "An event script's test of the player's character type is built (as a map's events load) and asks for the host (type 0): \"game\", or \"as_phantom\" (it tests a white phantom, type 1, instead: a co-op player in a host's world runs the host's events). Answer before the world loads; a switch.",
    &["game", "as_phantom"],
    || CHR_TYPE_CONDITION_SITE.install_or_why(),
);

// ── the event scripts' own-world test ───────────────────────────────────────────────────────

/// The event scripts' "in its own world" test, two ways. As a condition (3[26] IF Player Is Not In
/// Own World): +0x523590 builds an EcTestIsMyWorld from the wanted answer (the command's byte 1:
/// 1 "not in its own world"), called from the condition bank (+0x7AF920) with the event's instance
/// in r8 and the wanted answer in ecx. As a jump (1003[12] GOTO, [13] END, and a SKIP the game's
/// events don't use), the control bank (+0x7B06A0) calls the check +0x523430 (the instance, the
/// wanted answer; whether the player is "not in its own world" as wanted), the instance in r14,
/// the command's bytes in rbx, and tests al after. Both are protected code (their tests are in
/// Arxan's), so the hooks are at their callers. 480 events of the game's take one of these (the
/// effect-reach sweep, local notes 2026-10-19), 302 of them to change only the world (characters,
/// objects, flags): on a joined player's game they never ran, and the host's world changed on the
/// host's screen alone.
const OWN_WORLD_CHECK_CALLS: &str = "48 8b d8 48 85 c0 0f 84 ?? ?? ?? ?? 0f b6 53 01 49 8b ce e8 ?? ?? ?? ?? 84 c0";
/// From a match: the call, and the `test al, al` after it (the hook).
const OWN_WORLD_CHECK_CALL_AT: usize = 0x13;
const OWN_WORLD_CHECK_TEST_AT: usize = 0x18;
/// The control bank's three calls of the check (the same code shape calls something else once).
const OWN_WORLD_CHECK_CALL_COUNT: usize = 3;
/// The condition's one build among the condition bank's same-shaped builds: the call whose target
/// makes an EcTestIsMyWorld (its vtable's `lea` within the builder's first bytes; the builder's
/// own bytes are shared by a dozen other builders).
const OWN_WORLD_TEST_CLASS: &str = "EcTestIsMyWorld@NS_SPRJ";
const BUILDER_SCAN: usize = 0x80;
const OWN_WORLD_TEST_BUILDS: &str = "48 8b d8 48 85 c0 0f 84 ?? ?? ?? ?? 0f b6 4b 01 e8";
const OWN_WORLD_TEST_BUILD_CALL_AT: usize = 0x10;
/// An event instance's key, as Initialize Event makes it (+0x457B90): the event's id, its slot
/// (an i16), then the two added.
const INSTANCE_EVENT_ID: usize = 0x28;
const INSTANCE_SLOT: usize = 0x2C;

/// Whether the function at `f` loads `vtable` with a `lea reg, [rip+x]` in its first bytes.
fn loads_vtable(f: usize, vtable: usize) -> bool {
    let Some(code) = memory::read_bytes(f, BUILDER_SCAN) else { return false };
    (0..BUILDER_SCAN - 7)
        .any(|i| matches!(code[i], 0x48 | 0x4C) && code[i + 1] == 0x8D && code[i + 2] & 0xC7 == 0x05 && patterns::lea_target(f + i) == Some(vtable))
}

fn instance_event(instance: usize) -> Option<(i64, i64)> {
    Some((memory::read::<i32>(instance + INSTANCE_EVENT_ID)?.into(), memory::read::<i16>(instance + INSTANCE_SLOT)?.into()))
}

/// After a jump's check: on a game not in its own world, an event the mod answers "own_world"
/// for gets the answer the host's game gets.
fn own_world_check_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &mut *reg };
    let (instance, command) = (r.r14 as usize, r.rbx as usize);
    let Some(wanted_not_own) = memory::read::<u8>(command + 1).map(|b| b != 0) else { return };
    let held = r.rax as u8 != 0;
    // The check holds when "not in its own world" is as wanted: this game's own state from it.
    let not_own_world = held == wanted_not_own;
    if !not_own_world {
        return;
    }
    let Some((event, slot)) = instance_event(instance) else { return };
    if OWN_WORLD_TEST.decide(&[event, slot]) != 1 {
        return;
    }
    r.rax = (r.rax & !0xFF) | u64::from(!wanted_not_own);
}

/// The condition's builder (its one caller's event instance still in r8): the event the mod answers
/// "own_world" for gets the opposite wanted answer, so on a game not in its own world it holds as
/// it does on the host's. (The mod answers so only on such a game; the condition lives as long as
/// the event, rebuilt at a load.) The builder's entry, not its call: a jmp-back on the call ran
/// over the next case's start, a target of the condition bank's jump table.
fn own_world_test_builder_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let mut wanted = r.rcx as u8;
    if let Some((event, slot)) = instance_event(r.r8 as usize)
        && OWN_WORLD_TEST.decide(&[event, slot]) == 1
    {
        wanted ^= 1;
    }
    let build: extern "C" fn(u8) -> usize = unsafe { std::mem::transmute(original) };
    build(wanted)
}

static OWN_WORLD_CHECK_SITE: Site = Site {
    area: "multiplayer",
    name: "own-world check",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: hook_sites::Kind::JmpBack(own_world_check_hook),
    without: "mods can't let a joined player's events run as the host's past an own-world check",
};

static OWN_WORLD_TEST_BUILD_SITE: Site = Site {
    area: "multiplayer",
    name: "own-world condition builder",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: hook_sites::Kind::Retn(own_world_test_builder_hook),
    without: "mods can't let a joined player's events wait on the host's own-world condition",
};

fn install_own_world_test() -> Result<(), String> {
    // The check: the call target most of the control bank's matches share, called three times.
    let matches = patterns::find_all(OWN_WORLD_CHECK_CALLS);
    let targets: Vec<Option<usize>> = matches.iter().map(|&m| patterns::call_target(m + OWN_WORLD_CHECK_CALL_AT)).collect();
    let check = targets.iter().flatten().copied().find(|t| targets.iter().filter(|&&o| o == Some(*t)).count() == OWN_WORLD_CHECK_CALL_COUNT);
    let check = check.ok_or(OWN_WORLD_CHECK_SITE.without)?;
    let sites: Vec<usize> = matches.iter().zip(&targets).filter(|(_, t)| **t == Some(check)).map(|(&m, _)| m + OWN_WORLD_CHECK_TEST_AT).collect();
    let vtable = patterns::find_vtable(OWN_WORLD_TEST_CLASS).ok_or(OWN_WORLD_TEST_BUILD_SITE.without)?;
    let build = patterns::find_where("own-world condition build", OWN_WORLD_TEST_BUILDS, |m| {
        patterns::call_target(m + OWN_WORLD_TEST_BUILD_CALL_AT).is_some_and(|f| loads_vtable(f, vtable))
    })
    .ok_or(OWN_WORLD_TEST_BUILD_SITE.without)?;
    if !OWN_WORLD_CHECK_SITE.hook_all(&sites) {
        return Err(OWN_WORLD_CHECK_SITE.without.into());
    }
    let builder = patterns::call_target(build + OWN_WORLD_TEST_BUILD_CALL_AT).ok_or(OWN_WORLD_TEST_BUILD_SITE.without)?;
    OWN_WORLD_TEST_BUILD_SITE.hook_at(builder).map(std::mem::forget)
}

/// kindle.game: events.own_world_test (catalogue.rs).
pub static OWN_WORLD_TEST: HookPoint = HookPoint::hooked(
    "events.own_world_test",
    "An event script's \"in its own world\" test on a game that isn't in its own world (a player joined to someone's world): the event's id and slot. \"game\" keeps the game's answer (not in its own world); \"own_world\" answers as the host's game does, so the event runs past it as it does there. Asked as a condition is built (the map's events load) and each time a jump on it is made; quick answers only.",
    &["game", "own_world"],
    install_own_world_test,
);

// ── the event scripts' multiplayer state test ───────────────────────────────────────────────

/// The event scripts' multiplayer state test (EcTestMultiplayState; its evaluation +0x7A5760 is
/// protected): 0 no session or the world's owner, 1 a guest, 2 other characters registered, 3 broad
/// multiplayer, 4 alone, 5 a guest with others (FACTS). Single player holds 0 and 4 only. As a
/// condition (3[06]) +0x7A5580 builds one from the wanted state; as a jump or skip (1003[05], [06],
/// [105]) +0x7A5490 (the event's instance, the wanted state) builds one on its stack, evaluates it
/// and answers whether it holds. In a session they skipped boss intro cutscenes, raised the
/// multiplayer fog walls, sent players home at a dead boss's fog and kept a guest's game from
/// starting its own progress events (common 0).
const MP_STATE_TEST_CLASS: &str = "EcTestMultiplayState@NS_SPRJ";
/// The check's and the builder's first bytes (each shared with other functions; the one that makes
/// an EcTestMultiplayState is kept).
const MP_STATE_CHECK: &str = "4c 8b dc 57 48 83 ec 60 49 c7 43 b8 fe ff ff ff 49 89 5b 08 49 89 6b 10 49 89 73 18";
const MP_STATE_BUILDER: &str = "40 53 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 0f b6 d9 4c 8b 05";
const SINGLE_PLAYER_STATES: [u8; 2] = [0, 4];
/// A condition lives as long as its event, so it's built to answer as single player's from then on:
/// on the world's owner's game (alone, or hosting later), "the world's owner" (0) holds and "a
/// guest" (1) never does; on a guest's game "others registered" (2) holds and "alone" (4) never
/// does while it's in the session (its events are rebuilt as it leaves).
const OWNER: u8 = 0;
const GUEST: u8 = 1;
const OTHERS_REGISTERED: u8 = 2;
const ALONE: u8 = 4;
static MP_STATE_CHECK_FN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
thread_local! {
    /// The builder asking the game's own check whether this game owns its world.
    static ASKING_OWNER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn single_player_holds(wanted: u8) -> bool {
    SINGLE_PLAYER_STATES.contains(&wanted)
}

/// The jump's check: single player's answer for an event the mod answers "single_player" for.
fn mp_state_check_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (instance, wanted) = (r.rcx as usize, r.rdx as u8);
    if !ASKING_OWNER.with(|a| a.get())
        && let Some((event, slot)) = instance_event(instance)
        && MP_STATE_TEST.decide(&[event, slot, wanted.into()]) == 1
    {
        return single_player_holds(wanted).into();
    }
    let check: extern "C" fn(usize, u8, usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    check(r.rcx as usize, wanted, r.r8 as usize, r.r9 as usize)
}

/// The condition's builder (its one caller's event instance in r8): for such an event, a state
/// that holds exactly when single player's would, by whether this game owns its world (the game's
/// own check of state 0, on the same event).
fn mp_state_builder_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let mut wanted = r.rcx as u8;
    let instance = r.r8 as usize;
    if let Some((event, slot)) = instance_event(instance)
        && MP_STATE_TEST.decide(&[event, slot, wanted.into()]) == 1
    {
        let check = MP_STATE_CHECK_FN.load(std::sync::atomic::Ordering::Relaxed);
        if check != 0 {
            let check: extern "C" fn(usize, u8, usize, usize) -> usize = unsafe { std::mem::transmute(check) };
            ASKING_OWNER.with(|a| a.set(true));
            let owner = check(instance, OWNER, 0, 0) as u8 != 0;
            ASKING_OWNER.with(|a| a.set(false));
            wanted = match (owner, single_player_holds(wanted)) {
                (true, true) => OWNER,
                (true, false) => GUEST,
                (false, true) => OTHERS_REGISTERED,
                (false, false) => ALONE,
            };
        }
    }
    let build: extern "C" fn(u8) -> usize = unsafe { std::mem::transmute(original) };
    build(wanted)
}

static MP_STATE_CHECK_SITE: Site = Site {
    area: "multiplayer",
    name: "multiplayer state check",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: hook_sites::Kind::Retn(mp_state_check_hook),
    without: "mods can't make a session's events jump on the multiplayer state as single player's do",
};

static MP_STATE_BUILDER_SITE: Site = Site {
    area: "multiplayer",
    name: "multiplayer state condition builder",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: hook_sites::Kind::Retn(mp_state_builder_hook),
    without: "mods can't make a session's events wait on the multiplayer state as single player's do",
};

fn install_mp_state_test() -> Result<(), String> {
    let vtable = patterns::find_vtable(MP_STATE_TEST_CLASS).ok_or(MP_STATE_CHECK_SITE.without)?;
    let check = patterns::find_where("multiplayer state check", MP_STATE_CHECK, |f| loads_vtable(f, vtable)).ok_or(MP_STATE_CHECK_SITE.without)?;
    let builder =
        patterns::find_where("multiplayer state condition builder", MP_STATE_BUILDER, |f| loads_vtable(f, vtable)).ok_or(MP_STATE_BUILDER_SITE.without)?;
    std::mem::forget(MP_STATE_CHECK_SITE.hook_at(check)?);
    MP_STATE_CHECK_FN.store(check, std::sync::atomic::Ordering::Relaxed);
    MP_STATE_BUILDER_SITE.hook_at(builder).map(std::mem::forget)
}

/// kindle.game: events.multiplayer_state_test (catalogue.rs).
pub static MP_STATE_TEST: HookPoint = HookPoint::hooked(
    "events.multiplayer_state_test",
    "An event script's multiplayer state test (0 no session or the world's owner, 1 a guest, 2 others registered, 3 multiplayer, 4 alone, 5 a guest with others): the event's id, slot and the state it wants. \"game\" keeps the game's answer; \"single_player\" answers as on a game alone (0 and 4 hold), so a session's events run as single player's. Asked as a condition is built (the map's events load) and each time a jump on it is made; quick answers only.",
    &["game", "single_player"],
    install_mp_state_test,
);

// ── NPCs made as players ────────────────────────────────────────────────────────────────────

/// The NPCs made as players (+0x89EA10: a PlayerIns built from its CharaInitParam, the Fire Keeper
/// among them): on a game that is a guest in a session (Session_IsGuest, through +0x8DA1E0, its
/// only caller), an NPC whose NpcParam has a wandering ghost's look (isChangeWanderGhost, or
/// WanderGhostPhantomId 0 or more: the Fire Keeper's 140000 and 140010 and 86 more) is made as kind
/// 4, a wandering ghost, drawn with that look (a co-op player saw the Fire Keeper as a phantom,
/// 2026-10-15). The hook is on the test of the check's answer.
const NPC_GUEST_CHECK: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 74 ?? 48 8b 85 a0 00 00 00 48 8b 88 b8 00 00 00 8b 51 0c";

fn npc_guest_check_hook(reg: *mut ilhook::x64::Registers) {
    if NPC_GUEST_LOOK.switch() == 1 {
        unsafe { (*reg).rax &= !0xFF };
    }
}

static NPC_GUEST_CHECK_SITE: Site = Site {
    area: "multiplayer",
    name: "NPC guest check",
    pattern: NPC_GUEST_CHECK,
    offset: 0xC,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(npc_guest_check_hook),
    without: "mods can't keep a guest's NPCs from looking like wandering ghosts",
};

/// kindle.game: npc.guest_look (catalogue.rs).
pub static NPC_GUEST_LOOK: HookPoint = HookPoint::hooked(
    "npc.guest_look",
    "An NPC made as a player (the Fire Keeper, ...) is made on a game that is a guest in a session, which draws it as a wandering ghost: \"game\", or \"as_host\" (made as on the host's game). A switch.",
    &["game", "as_host"],
    || NPC_GUEST_CHECK_SITE.install_or_why(),
);

// ── the cutscene skip ───────────────────────────────────────────────────────────────────────

/// A cutscene's skip: out of a session it ends the cutscene at once (+0xCD8C30); in a session it's a
/// request that the cutscene manager's step (SprjRemoMan::vf21, +0xCDA640) acts on only on the world
/// owner's game (Session_IsWorldOwner, +0x78C110: the per-type table's column 7), which ends it
/// there and tells every other game to end it too (session message 0x39). The hook is on the test
/// of its answer (bpl), 0x37 into the match.
const CUTSCENE_SKIP_OWNER: &str = "48 85 db 74 ?? 48 8b cb e8 ?? ?? ?? ?? 48 8b 1d ?? ?? ?? ?? 0f b6 e8 48 85 db 74 ?? 48 8b cb e8 ?? ?? ?? ?? 48 8b 1d ?? ?? ?? ?? 44 0f b6 f0 eb ?? 40 32 ed 45 32 f6 40 84 ed";

fn cutscene_skip_owner_hook(reg: *mut ilhook::x64::Registers) {
    if CUTSCENE_SKIP.switch() == 1 {
        let r = unsafe { &mut *reg };
        r.rbp = (r.rbp & !0xFF) | 1;
    }
}

static CUTSCENE_SKIP_OWNER_SITE: Site = Site {
    area: "multiplayer",
    name: "cutscene skip owner",
    pattern: CUTSCENE_SKIP_OWNER,
    offset: 0x37,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(cutscene_skip_owner_hook),
    without: "mods can't let a guest skip cutscenes",
};

/// kindle.game: cutscene.skip_owner (catalogue.rs).
pub static CUTSCENE_SKIP: HookPoint = HookPoint::hooked(
    "cutscene.skip_owner",
    "In a session only the world owner's skip of a cutscene ends it (on every game): \"game\", or \"owner\" (this game's skip counts as the owner's). A switch.",
    &["game", "owner"],
    || CUTSCENE_SKIP_OWNER_SITE.install_or_why(),
);

// ── the ember's look ────────────────────────────────────────────────────────────────────────

/// An embered character's look: its ember slot (ChrLoadOfCinderSpEffectSlot, run once as the
/// character is made or embers, +0x5B1680) asks the per-type table's column 7 (the world's owner,
/// +0x9F6370) whether the character's type gets it. One that does gets CSStayParam row 1's
/// effects (SpEffect 9139: the ember cracks) and PlayerGameData +0x8E8 bit 6, which the HUD's
/// hexagon shows as fire; else the bit is cleared and nothing applied (but a dark spirit of some
/// invade types). So a joined player (a white phantom) embered as the host is, with the ember's HP
/// and 490 on, had the plain hexagon and no cracks on any game (seen on two games on one PC,
/// 2026-10-18). The hook is on the test of the column's answer (`test al, al`), 0x14 into the
/// match (r12: the character).
const EMBER_LOOK_OWNER: &str = "48 8d 55 77 49 8b cc e8 ?? ?? ?? ?? 48 8b c8 e8 ?? ?? ?? ?? 84 c0 0f 84";

fn ember_look_owner_hook(reg: *mut ilhook::x64::Registers) {
    if EMBER_LOOK.switch() != 1 {
        return;
    }
    let r = unsafe { &mut *reg };
    // A player's character (of the player's class), not another character the slot runs for.
    let main = crate::ds3::main_player().map(|p| p.addr()).unwrap_or(0);
    let class = |chr: usize| crate::memory::read_u64(chr).filter(|&v| v != 0);
    if main != 0 && class(r.r12 as usize).is_some() && class(r.r12 as usize) == class(main) {
        r.rax = (r.rax & !0xFF) | 1;
    }
}

static EMBER_LOOK_OWNER_SITE: Site = Site {
    area: "multiplayer",
    name: "ember look owner",
    pattern: EMBER_LOOK_OWNER,
    offset: 0x14,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(ember_look_owner_hook),
    without: "mods can't give an embered phantom the ember's look",
};

/// kindle.game: chr.ember_look (catalogue.rs).
pub static EMBER_LOOK: HookPoint = HookPoint::hooked(
    "chr.ember_look",
    "An embered character gets the ember's look (the cracks, SpEffect 9139, and the HUD's fiery hexagon) only when its type is the world's owner's: \"game\", or \"any\" (every embered player's character, a white phantom's too). Decided once as a character is made or embers: answer before the characters it's for are made. A switch.",
    &["game", "any"],
    || EMBER_LOOK_OWNER_SITE.install_or_why(),
);

// ── the Estus allotment ─────────────────────────────────────────────────────────────────────

/// The game's load into a world (World_LoadPlayerData, +0x8F7420) sets the flasks' allotment by
/// asking whether this game is a guest in a session (Session_IsGuest, at +0x8F7551): not, the rate
/// and offset of home (1 and 0, through +0x5A8C20); a guest, a white phantom's from CSStayParam row
/// 1 (+0x54: half the flasks the character owns; for invade type 6 the Estus's and the Ashen
/// Estus's each). The phantom's rate cut the charges at each join (3 of 3 at home, 1 in the host's
/// world), capped every refill there, and made Andre's "Reinforce Estus Flask" take flasks away.
/// The hook is on the test of the answer (`test al, al`).
const LOAD_GUEST_CHECK: &str = "48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 0f 84 ?? ?? ?? ?? 83 ff 0d 0f 84 ?? ?? ?? ?? 48 8b 0d";

fn load_guest_check_hook(reg: *mut ilhook::x64::Registers) {
    if ESTUS_ALLOTMENT.switch() == 1 {
        unsafe { (*reg).rax &= !0xFF };
    }
}

static LOAD_GUEST_CHECK_SITE: Site = Site {
    area: "multiplayer",
    name: "load guest check",
    pattern: LOAD_GUEST_CHECK,
    offset: 0xC,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(load_guest_check_hook),
    without: "mods can't give a guest home's Estus allotment",
};

/// kindle.game: load.estus_allotment (catalogue.rs).
pub static ESTUS_ALLOTMENT: HookPoint = HookPoint::hooked(
    "load.estus_allotment",
    "A load into a world sets the Estus flasks' allotment, a phantom's (half the flasks the character owns) for a guest in a session: \"game\", or \"as_host\" (home's, all of them). A switch.",
    &["game", "as_host"],
    || LOAD_GUEST_CHECK_SITE.install_or_why(),
);

// ── the flask level ─────────────────────────────────────────────────────────────────────────

/// Burning an Undead Bone Shard at Firelink Shrine's bonfire (its talk, t400000: function 110
/// reads the level, command 109 writes it plus one) goes through the bonfire database's level
/// (FrpgNetBonfireDb, saved with the character), which the game keeps twice: the character's own
/// (+0x4561248) and, in someone else's world, the host's (+0x4561249, from the host's packet 0x11
/// at the join). Its read (+0x546790) and write (+0x546BA0) take the host's copy whenever the game
/// isn't the session's host, so a joined player burning a shard got the host's level plus one. In
/// the base game a phantom never sits at a bonfire. Answered "own", both use the character's own
/// copy (the read's conditional move becomes a move, the write's branch to the host's copy goes).
const BONFIRE_LEVEL_READ: &str = "48 83 ec 28 e8 ?? ?? ?? ?? 0f b6 15 ?? ?? ?? ?? 0f b6 0d ?? ?? ?? ?? 84 c0 0f 45 d1 0f b6 c2";
const BONFIRE_LEVEL_WRITE: &str = "88 4c 24 08 53 48 83 ec 20 c6 44 24 40 63 c6 44 24 38 01 80 f9 01 73 07";

static BONFIRE_LEVEL_READ_PATCH: Patch = Patch {
    area: "multiplayer",
    name: "bonfire level read",
    pattern: BONFIRE_LEVEL_READ,
    offset: 0x19,
    matches: 1,
    from: &[0x0F, 0x45, 0xD1],
    to: &[0x8B, 0xD1, 0x90],
    without: "a guest burns Undead Bone Shards at the host's level",
};

static BONFIRE_LEVEL_WRITE_PATCH: Patch = Patch {
    area: "multiplayer",
    name: "bonfire level write",
    pattern: BONFIRE_LEVEL_WRITE,
    offset: 0x3A,
    matches: 1,
    from: &[0x74, 0x0C],
    to: &[0x90, 0x90],
    without: "a guest burns Undead Bone Shards at the host's level",
};

/// kindle.game: bonfire.level_copy (catalogue.rs).
pub static BONFIRE_LEVEL: HookPoint = HookPoint::hooked(
    "bonfire.level_copy",
    "The Estus Flask's level (Undead Bone Shards) a game reads and writes at Firelink's bonfire: the host's copy on a guest's game (\"host_copy\", the game's), or always the character's own (\"own\"). Answered \"own\", the game's code is changed until it closes.",
    &["host_copy", "own"],
    || {
        let read = BONFIRE_LEVEL_READ_PATCH.apply();
        let write = BONFIRE_LEVEL_WRITE_PATCH.apply();
        (read && write).then_some(()).ok_or_else(|| BONFIRE_LEVEL_READ_PATCH.without.to_string())
    },
);

// ── the bonfires' session list ──────────────────────────────────────────────────────────────

/// The bonfire database (FrpgNetBonfireDb) keeps two lists of the world's bonfires, each entry
/// with the bonfire's fire (+0x12, 0 unlit): the character's own (+0x20, used while GameMan's "my
/// world" byte +0xB1E is set) and the session's (+0x38, used otherwise). A bonfire's talk asks for
/// it (talk function 38, through +0x546C00): unlit, sitting lights it (BonfireInitialize, the
/// lighting animation) instead of resting. Before a load (+0x475B50) the game keeps the own list in
/// GameMan's buffer +0xC30; the load's step (+0x546EF0, from +0x8F2B40) builds the list again from
/// +0xC30, or, not in its own world, the session's from +0xC20, the host's list as its join data
/// (packet 0x11), and frees the buffer. But leaving a world (+0x8F36F0) sets "my world" and only
/// the new world's setup (+0x8F45C0, after that step) clears it for a game in someone else's world,
/// so the step always builds the own list: the host's join data was never used, and the session's
/// list held only what the map's events registered (unlit) and the bonfires the player lit since
/// the load: its first sit at each lit bonfire after every load lit it again rather than rested
/// (verified 2026-11-09 on two games: the player's +0xC20 held the host's 112 bonfires all along,
/// its session list at Road of Sacrifices had every bonfire unlit but one it had sat at; a trace
/// of the step took the own branch). Answered "keep", once a game is in someone else's world after
/// a load the step runs again (the session's list from +0xC20), and before its next load the game
/// keeps the session's list in +0xC20 as it keeps its own in +0xC30.
const PRE_LOAD_SAVE: &str = "48 83 ec 28 48 8b 0d ?? ?? ?? ?? 48 89 5c 24 30 48 89 7c 24 20 48 85 c9 0f 84 ?? ?? ?? ?? e8 ?? ?? ?? ?? 48 63 d8 85 c0 7e";
/// The bonfire's fire read (+0x546C00): `mov rax, [rip+FrpgNetBonfireDb's owner]` at 4.
const BONFIRE_FIRE_READ: &str = "48 83 ec 28 48 8b 05 ?? ?? ?? ?? 8b d1 48 8b 88 b0 0b 00 00 e8 ?? ?? ?? ?? 48 85 c0 74 09 0f be 40 12";
/// The level read up to the bytes the level patch changes: `movzx edx, [rip+the session's level]`
/// at 9.
const BONFIRE_LEVEL_SESSION: &str = "48 83 ec 28 e8 ?? ?? ?? ?? 0f b6 15 ?? ?? ?? ?? 0f b6 0d ?? ?? ?? ?? 84 c0";
/// The load's step building the lists (+0x546EF0): (the database).
const BONFIRE_LIST_LOAD: &str = "48 89 4c 24 08 41 57 48 83 ec 70 e8 ?? ?? ?? ?? 0f b6 c8 44 0f b6 f8 88 84 24 88 00 00 00 e8 ?? ?? ?? ?? 84 c0 0f 84";
/// GameMan's buffer setter (+0x62BD90): (GameMan, size, data, own) copies the data in.
const GAME_MAN_DB_BUFFER: &str =
    "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 49 8b f0 48 63 da 48 8b f9 45 84 c9 74 2e 41 0f b6 d1 e8 ?? ?? ?? ?? 89 9f 30 0c 00 00";
const DB_AT: usize = 0xBB0;
const SESSION_LIST_AT: usize = 0x38;
const SESSION_BUFFER_SIZE_AT: usize = 0xC20;
const SESSION_HOST_AT: usize = 0xB1E;
/// The load's checks of a buffer: a level of 1 to 99; entries kept to a sane count.
const MAX_LEVEL: u8 = 99;
const MAX_ENTRIES: usize = 1000;

/// What the session's list needs: the database's owner, the session's level, the buffer setter.
struct SessionList {
    db_owner: usize,
    level: usize,
    set_buffer: usize,
    load: usize,
}

static SESSION_LIST: OnceLock<Option<SessionList>> = OnceLock::new();

fn session_list() -> Option<&'static SessionList> {
    SESSION_LIST
        .get_or_init(|| {
            let read = patterns::find_unique("bonfire fire read", BONFIRE_FIRE_READ)?;
            let level = patterns::find_unique("bonfire session level", BONFIRE_LEVEL_SESSION)?;
            Some(SessionList {
                db_owner: patterns::rip_target(read + 7, read + 11)?,
                level: patterns::rip_target(level + 12, level + 16)?,
                set_buffer: patterns::find_unique("GameMan bonfire buffer", GAME_MAN_DB_BUFFER)?,
                load: patterns::find_unique("bonfire list load", BONFIRE_LIST_LOAD)?,
            })
        })
        .as_ref()
}

/// The session's list as the load reads a buffer: the count and level, then per bonfire its id
/// and two flags, its two counts and its fire.
fn session_list_buffer(list: &SessionList, db: usize) -> Option<Vec<u8>> {
    let level = memory::read::<u8>(list.level)?;
    if level == 0 || level > MAX_LEVEL {
        return None;
    }
    let head = memory::read_u64(db + SESSION_LIST_AT)? as usize;
    let mut entries = Vec::new();
    let mut node = memory::read_u64(head)? as usize;
    while node != head && node != 0 && entries.len() < MAX_ENTRIES {
        let item = memory::read_u64(node + 0x10)? as usize;
        if item != 0 {
            let id = memory::read::<u32>(item + 8)?;
            let flags = (memory::read::<u8>(item + 0x10)? & 1) | ((memory::read::<u8>(item + 0x11)? & 1) << 1);
            let counts = memory::read::<u32>(item + 0xC)?;
            let fire = memory::read::<u8>(item + 0x12)?;
            entries.push(((id as u64) << 32 | flags as u64, counts as u64 | (fire as u64) << 32));
        }
        node = memory::read_u64(node)? as usize;
    }
    if entries.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(4 + entries.len() * 16);
    out.extend_from_slice(&(entries.len() as u32 | (level as u32) << 16).to_le_bytes());
    for (a, b) in entries {
        out.extend_from_slice(&a.to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
    }
    Some(out)
}

/// The bonfire database, once the game has one.
fn bonfire_db(list: &SessionList) -> Option<usize> {
    let owner = memory::read_u64(list.db_owner).filter(|&o| o != 0)?;
    memory::read_u64(owner as usize + DB_AT).filter(|&d| d != 0).map(|d| d as usize)
}

/// Before a load: the game keeps its own list; a game in someone else's world keeps the
/// session's too, unless the buffer still holds the host's join data (not built yet).
fn pre_load_save_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let save: extern "C" fn(usize, usize, usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    let out = save(r.rcx as usize, r.rdx as usize, r.r8 as usize, r.r9 as usize);
    if BONFIRE_SESSION_LIST.switch() == 1
        && let Some(gm) = crate::bonfire::game_man()
        && memory::read::<u8>(gm + SESSION_HOST_AT) == Some(0)
        && memory::read::<i32>(gm + SESSION_BUFFER_SIZE_AT) == Some(0)
        && let Some(list) = session_list()
        && let Some(db) = bonfire_db(list)
        && let Some(buffer) = session_list_buffer(list, db)
    {
        let set: extern "C" fn(usize, i32, *const u8, u8) = unsafe { std::mem::transmute(list.set_buffer) };
        set(gm, buffer.len() as i32, buffer.as_ptr(), 0);
        crate::debug!("multiplayer: the session's bonfires kept for the load ({} entries)", (buffer.len() - 4) / 16);
    }
    out
}

/// Whether the session's list was built since the last load.
static SESSION_LIST_BUILT: AtomicBool = AtomicBool::new(false);

/// Each frame: once the game is in someone else's world after a load, the load's step again,
/// which now builds the session's list from the host's (+0xC20, freed after).
pub fn frame() {
    if BONFIRE_SESSION_LIST.switch() != 1 {
        return;
    }
    if !crate::game::player_loaded() {
        SESSION_LIST_BUILT.store(false, Ordering::Relaxed);
        return;
    }
    if SESSION_LIST_BUILT.load(Ordering::Relaxed) {
        return;
    }
    let Some(gm) = crate::bonfire::game_man() else { return };
    if memory::read::<u8>(gm + SESSION_HOST_AT) != Some(0) {
        return;
    }
    SESSION_LIST_BUILT.store(true, Ordering::Relaxed);
    let size = memory::read::<i32>(gm + SESSION_BUFFER_SIZE_AT).unwrap_or(0);
    if size <= 0 {
        return;
    }
    let Some(list) = session_list() else { return };
    let Some(db) = bonfire_db(list) else { return };
    let load: extern "C" fn(usize) = unsafe { std::mem::transmute(list.load) };
    load(db);
    crate::info!("multiplayer: the session's bonfires built from the host's ({} bonfires)", (size - 4) / 16);
}

static PRE_LOAD_SAVE_SITE: Site = Site {
    area: "multiplayer",
    name: "pre-load world save",
    pattern: PRE_LOAD_SAVE,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(pre_load_save_hook),
    without: "a joined player's bonfires are unlit after each load",
};

/// kindle.game: bonfire.session_list (catalogue.rs).
pub static BONFIRE_SESSION_LIST: HookPoint = HookPoint::hooked(
    "bonfire.session_list",
    "A game in someone else's world uses the session's list of the bonfires' fires, which its loads never build from the host's (each bonfire unlit: sitting lights it instead of resting): \"game\", or \"keep\" (built from the host's after each load, and kept over loads as the game's own list is). A switch.",
    &["game", "keep"],
    || {
        session_list().ok_or("the bonfire database wasn't found")?;
        PRE_LOAD_SAVE_SITE.install_or_why()
    },
);

// ── talking ─────────────────────────────────────────────────────────────────────────────────

/// A talk's per-frame check of the player (+0xF032B0): a player whose kind (+0x8894B0) is anything
/// but the host's (a phantom) gets the talk's "can't talk" byte (+0x25, talk query 19) set, and its
/// bonfire and NPC scripts close. Matched at the check, 0x4B into the function.
const TALK_PLAYER_CHECK: &str = "48 8d 54 24 30 e8 ?? ?? ?? ?? f7 44 24 30 f7 ff ff ff 74 04 c6 47 25 01";
const CANT_TALK_AT: usize = 0x25;

fn talk_player_check_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let talk = unsafe { &*reg }.rcx as usize;
    let before = memory::read::<u8>(talk + CANT_TALK_AT);
    let f: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(original) };
    let r = f(talk);
    if before == Some(0) && TALK_CHECK.switch() == 1 {
        unsafe { std::ptr::write_volatile((talk + CANT_TALK_AT) as *mut u8, 0) };
    }
    r
}

static TALK_PLAYER_CHECK_SITE: Site = Site {
    area: "multiplayer",
    name: "talk player check",
    pattern: TALK_PLAYER_CHECK,
    offset: -75,
    matches: 1,
    kind: hook_sites::Kind::Retn(talk_player_check_hook),
    without: "mods can't let a phantom use bonfires and NPCs",
};

/// kindle.game: talk.player_check (catalogue.rs).
pub static TALK_CHECK: HookPoint = HookPoint::hooked(
    "talk.player_check",
    "A talk (a bonfire's, an NPC's) checks the player each frame, and closes for a phantom: \"game\", or \"as_host\" (it stays open as for the host). Every frame: a switch.",
    &["game", "as_host"],
    || TALK_PLAYER_CHECK_SITE.install_or_why(),
);

/// Talk query 52 tells a talk script whether the game is in a multiplayer session; a bonfire turns
/// itself off while it answers 1 (its script checks it first), and some NPCs hold back options.
pub const TALK_IN_SESSION: i32 = 52;

/// kindle.game: talk.in_session (catalogue.rs; hooks.rs's talk hooks answer it).
pub static IN_SESSION: HookPoint = HookPoint::hooked(
    "talk.in_session",
    "A talk script asks whether the game is in a multiplayer session (talk query 52): a bonfire turns itself off while it is, and some NPCs hold back options. \"game\", or \"no\". A switch.",
    &["game", "no"],
    crate::hooks::ensure_talk_hooks,
);

/// Talk query 60: whether the NPC talked to has an SpEffect. A talk that has the NPC gesture waits
/// for its "ready" one (150) first: the Fire Keeper's Level Up, Yoel's drawing out true strength.
/// An NPC has it whenever it's idle, from its own behaviour, which only the world owner's game
/// runs: on a guest's game it never has it, so the talk went no further (verified 2026-10-10 on
/// two games: the host's Fire Keeper had 150 idle and 152 talking, the player's neither).
pub const TALK_NPC_EFFECT: i32 = 60;
pub const NPC_READY_EFFECT: i32 = 150;

/// kindle.game: talk.npc_ready (catalogue.rs; hooks.rs's talk hooks answer it).
pub static NPC_READY: HookPoint = HookPoint::hooked(
    "talk.npc_ready",
    "A talk asks whether the NPC has its \"ready\" effect (SpEffect 150, talk query 60), which only the world owner's game gives its NPCs (a guest's talk waits for it forever): \"game\", or \"always\". A switch.",
    &["game", "always"],
    crate::hooks::ensure_talk_hooks,
);
