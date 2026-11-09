//! The game's multiplayer flow (its flow script's handlers and the calls they make): the ways it
//! ends a phantom's visit and sends a player home, as hook points of the catalogue, each hooked
//! on the game when a mod first answers it. A mod (co-op) answers them so its players stay.
//!
//! | | |
//! |---|---|
//! | boss clear fade-out | BlockClear_LeaveAction2 (+0x48F7C0): `flow.boss_clear_fade` |
//! | boss clear way home | BlockClear2_3 (+0x48F5B0): `flow.boss_clear_home`, the event `flow.boss_clear_stayed` |
//! | boss death "leaving" | the boss death's phantom branch (+0x48EFCB): `flow.boss_death_leaving` |
//! | boss clear session leave | BlockClear2_3Leave's method (+0x48F630): `flow.boss_leave_session` |
//! | a host's last guest gone | OnLeave_Limit (+0x495CA0): `flow.leave_limit` |
//! | the save-and-reload | +0x4D1BB0 (WarpNextStageKick's): `world.reload_requested` |
//! | network notices | +0x474B70: `ui.network_notice` |
//!
//! The flow's way home ends a phantom's visit by asking for a world reload (+0x4D1BB0: the
//! position saved, GameMan's reload request), and then, in the same handler, sets what that load
//! gives the player: GameDataMan +0x9C the chr type to give its data (+0x485690; the load,
//! +0x8F7420, gives it and sets -1) and +0x80 "back home" (+0x485050; the load then refills HP, FP
//! and stamina, clears effects and sets 0). A refused reload must take those off too, or the next
//! load (in the host's world) made the player a phantom and then gave it its home type (0, team 1:
//! the host's HUD called it "Summoner") and refilled it (verified 2026-10-08 with two games on one
//! PC: after a follow the player was type 0 in the host's world, set from +0x8F7495).

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::catalogue::{Event, HookPoint};
use crate::hook_sites::{self, Patch, Site};
use crate::lock;
use crate::{memory, patterns};

/// The game's code addresses on the stack from `rsp` (return addresses and others: the calls that
/// led here, outermost last), as offsets in the game, for the log.
pub(crate) fn game_callers(rsp: usize, max: usize) -> String {
    let Some((base, size)) = memory::game_module() else { return String::new() };
    let mut out = Vec::new();
    for i in 0..0x100 {
        if out.len() >= max {
            break;
        }
        if let Some(v) = memory::read_u64(rsp + i * 8).map(|v| v as usize)
            && v > base + 0x1000
            && v < base + size
        {
            out.push(format!("+{:#x}", v - base));
        }
    }
    out.join(" <- ")
}

// ── a boss's death ──────────────────────────────────────────────────────────────────────────
//
// A boss's death in the multiplayer flow: BlockClear2 (the banner, the players' rewards), then
// BlockClear2_2, which schedules BlockClear_LeaveAction2 (+0x48F7C0: every game fades the summoned
// players out, ChrFadeOut +0x47DE80 for its own, +0x47DF30 for the others') and, 8 s later,
// BlockClear2_3 (+0x48F5B0): the boss clear's menu state off (+0x486100, menu request 0xAF = 0),
// and for a white phantom (live chr type 1, +0x483240) the way home: GameDataMan +0x80 = 1, GameMan
// +0xAC4 = 1, its return point (+0x4859B0), WarpNextStageKick. The world reload it asks for makes
// the game end the world and leave the session (the menu request 0xFA of the end-world check).

const BLOCK_CLEAR_LEAVE: &str = "48 89 5c 24 08 57 48 83 ec 30 41 80 78 08 01 49 8b f8 48 8b da 0f 85 ?? ?? ?? ?? 48 8b 0d ?? ?? ?? ?? 48 85 c9 75 ?? 4c 8d 0d";

fn block_clear_leave_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    if BOSS_CLEAR_FADE.switch() == 1 {
        crate::info!("flow: the boss died; nobody fades out (a mod's answer)");
        return 0;
    }
    let f: extern "C" fn(usize, usize, usize) = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize, r.rdx as usize, r.r8 as usize);
    0
}

static BOSS_CLEAR_LEAVE_SITE: Site = Site {
    area: "flow",
    name: "boss clear leave",
    pattern: BLOCK_CLEAR_LEAVE,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(block_clear_leave_hook),
    without: "summoned players fade out when a boss dies",
};

/// kindle.game: flow.boss_clear_fade (catalogue.rs).
pub static BOSS_CLEAR_FADE: HookPoint = HookPoint::hooked(
    "flow.boss_clear_fade",
    "A boss died in a session: every game fades the summoned players out (BlockClear_LeaveAction2): \"game\", or \"skip\" (nobody fades). A switch.",
    &["game", "skip"],
    || BOSS_CLEAR_LEAVE_SITE.install_or_why(),
);

const BLOCK_CLEAR: &str =
    "40 53 48 83 ec 20 48 8b da 33 d2 48 8b cb e8 ?? ?? ?? ?? 48 8b cb e8 ?? ?? ?? ?? 3c 01 74 ?? 48 8b cb e8 ?? ?? ?? ?? 3c 01 75 ?? 83 63 54 fc";
/// BlockClear2_3's call of the boss clear's menu state (+0x486100(_, on)).
const BOSS_MENU_CALL_AT: usize = 0x0E;
static BOSS_MENU_AT: OnceLock<usize> = OnceLock::new();
/// The multiplayer flow's (SprjLuaEventProxy, BlockClear2_3's rdx) "action buttons off" byte,
/// which the flow copies to the action-button manager each frame (SprjActionButtonManImp +0x42, no
/// prompts at all), and its state bits, the low two "leaving". The boss clear sets both on a
/// summoned player (+0x485EE0) for its way home; BlockClear2_3 clears the bits for a player who
/// stays (the host).
const FLOW_BUTTONS_OFF_AT: usize = 0x53;
const FLOW_STATE_AT: usize = 0x54;
const FLOW_LEAVING: u32 = 3;

fn block_clear_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    if BOSS_CLEAR_HOME.switch() == 1
        && let Some(&menu) = BOSS_MENU_AT.get().filter(|&&m| m != 0)
    {
        crate::info!("flow: the boss died; the game would send this player home; it stays (a mod's answer)");
        let off: extern "C" fn(usize, u8) = unsafe { std::mem::transmute(menu) };
        off(r.rcx as usize, 0);
        // What the way home would have given back: the action buttons, and no longer leaving.
        let flow = r.rdx as usize;
        if let Some(state) = memory::read::<u32>(flow + FLOW_STATE_AT) {
            unsafe {
                std::ptr::write_volatile((flow + FLOW_STATE_AT) as *mut u32, state & !FLOW_LEAVING);
                std::ptr::write_volatile((flow + FLOW_BUTTONS_OFF_AT) as *mut u8, 0);
            }
        }
        crate::catalogue::emit(&BOSS_CLEAR_STAYED, Vec::new());
        return 0;
    }
    let f: extern "C" fn(usize, usize, usize) = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize, r.rdx as usize, r.r8 as usize);
    0
}

static BOSS_CLEAR_SITE: Site = Site {
    area: "flow",
    name: "boss clear",
    pattern: BLOCK_CLEAR,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(block_clear_hook),
    without: "summoned players go home when a boss dies",
};

fn install_boss_clear() -> Result<(), String> {
    let found = BOSS_CLEAR_SITE.find().ok_or(BOSS_CLEAR_SITE.without)?;
    let menu = patterns::call_target(found[0] + BOSS_MENU_CALL_AT).ok_or("the boss clear isn't as expected")?;
    let _ = BOSS_MENU_AT.set(menu);
    BOSS_CLEAR_SITE.hook_all(&found).then_some(()).ok_or_else(|| BOSS_CLEAR_SITE.without.to_string())
}

/// kindle.game: flow.boss_clear_home (catalogue.rs).
pub static BOSS_CLEAR_HOME: HookPoint = HookPoint::hooked(
    "flow.boss_clear_home",
    "8 s after a boss's death (BlockClear2_3) the game turns the boss clear's menu state off and sends a white phantom home: \"game\", or \"stay\" (only the menu state goes off, and the player gets its action buttons back, no longer leaving; the event flow.boss_clear_stayed follows). A switch.",
    &["game", "stay"],
    install_boss_clear,
);

/// kindle.game: flow.boss_clear_stayed (catalogue.rs).
pub static BOSS_CLEAR_STAYED: Event = Event::new(
    "flow.boss_clear_stayed",
    "A boss's clear left this player where it is (flow.boss_clear_home \"stay\"): a phantom who helped would have gone home embered (the game's revive as it loads home, +0x485440).",
    &[],
);

/// A boss's death on a white phantom's game (the flow's handler at +0x48EE70, its branch after "is
/// this game's player a white phantom", +0x483240, at +0x48EFA9) also turns the phantom's action
/// buttons off and marks it leaving (+0x485EE0: the flow's +0x53 = 1, +0x54 |= 1, at +0x48EFCB)
/// for its way home; the host's branch doesn't. With the buttons off no prompt shows, the item
/// popups' OK among them: the boss's soul and the phantom's Ember came with no OK until
/// BlockClear2_3 (flow.boss_clear_home) gave the buttons back 8 s later (a co-op player,
/// 2026-10-15: "unable to press ok to clear the item pop ups until like 20 seconds after they
/// appear"). Answered "stay", that call goes (a 5-byte nop); the rest of the branch is the game's.
const BOSS_DEATH_BUTTONS: &str =
    "88 46 0c 8b 57 58 e8 ?? ?? ?? ?? 48 8b cb e8 ?? ?? ?? ?? 48 8b cb e8 ?? ?? ?? ?? ba 10 27 00 00 48 8b cb e8 ?? ?? ?? ?? 3c 01 75 04 83 4b 54 0a";

static BOSS_DEATH_BUTTONS_PATCH: Patch = Patch {
    area: "flow",
    name: "boss death buttons",
    pattern: BOSS_DEATH_BUTTONS,
    offset: 0x16,
    matches: 1,
    from: &[0xE8],
    to: &[0x0F, 0x1F, 0x44, 0x00, 0x00],
    without: "a phantom's item popups after a boss have no OK for a few seconds",
};

/// kindle.game: flow.boss_death_leaving (catalogue.rs).
pub static BOSS_DEATH_LEAVING: HookPoint = HookPoint::hooked(
    "flow.boss_death_leaving",
    "A boss's death turns a white phantom's action buttons off (no prompts, item popups without their OK) and marks it leaving, for its way home: \"game\", or \"stay\" (neither). Answered \"stay\", the game's code is changed until it closes (only a phantom's game takes that branch).",
    &["game", "stay"],
    || BOSS_DEATH_BUTTONS_PATCH.apply().then_some(()).ok_or_else(|| BOSS_DEATH_BUTTONS_PATCH.without.to_string()),
);

/// The flow's "BlockClear2_3Leave", which BlockClear2_2 schedules with BlockClear2_3 for a
/// summoned player (+0x48F630: `mov rcx, rdx; jmp LeaveSession`, +0x4832B0, through the session
/// leave): a player's way out of the session after the boss. The game reaches it through its
/// method invoker, registered by name by a static constructor (+0x265E260; the pattern is the
/// constructors' shape, the name told by the string it registers, its method kept at +0x22's
/// store): the hook is the method's slot, a data swap.
const FLOW_METHOD_REGISTRATION: &str = "48 83 ec 38 48 c7 44 24 20 fe ff ff ff 48 8d 05 ?? ?? ?? ?? 48 89 05 ?? ?? ?? ?? 48 8d 05 ?? ?? ?? ?? 48 89 05 ?? ?? ?? ?? 48 8b 05 ?? ?? ?? ?? 48 85 c0 75 05 e8 ?? ?? ?? ?? 4c 8b 10 4c 8d 0d ?? ?? ?? ?? 4c 8d 05";
/// The registration's `lea r8, [name]`, and its store of the method (`mov [rip+..], rax`).
const FLOW_METHOD_NAME_AT: usize = 0x44;
const FLOW_METHOD_STORE_AT: usize = 0x22;
static BOSS_LEAVE_METHOD: OnceLock<usize> = OnceLock::new();

/// BlockClear2_3Leave's method, as the game's invoker calls it (the flow script, its proxy, the
/// call's parameters).
extern "C" fn boss_leave_method(this: usize, proxy: usize, param: usize) {
    if BOSS_LEAVE_SESSION.switch() == 1 {
        crate::info!("flow: the game would take this player out of the session after the boss; it stays (a mod's answer)");
        return;
    }
    if let Some(&original) = BOSS_LEAVE_METHOD.get() {
        let f: extern "C" fn(usize, usize, usize) = unsafe { std::mem::transmute(original) };
        f(this, proxy, param);
    }
}

fn install_boss_leave() -> Result<(), String> {
    let at = patterns::find_with_string("boss clear leave session", FLOW_METHOD_REGISTRATION, FLOW_METHOD_NAME_AT, "BlockClear2_3Leave")
        .ok_or("the game's boss clear leave wasn't found")?;
    let slot = patterns::rip_target(at + FLOW_METHOD_STORE_AT + 3, at + FLOW_METHOD_STORE_AT + 7).ok_or("the boss clear leave isn't as expected")?;
    let original = memory::read_u64(slot).filter(|&f| f != 0).ok_or("the boss clear leave's method isn't there")?;
    let _ = BOSS_LEAVE_METHOD.set(original as usize);
    if !unsafe { crate::hooks::write_ptr(slot, boss_leave_method as *const () as usize) } {
        return Err("couldn't take the game's boss clear leave".into());
    }
    Ok(())
}

/// kindle.game: flow.boss_leave_session (catalogue.rs).
pub static BOSS_LEAVE_SESSION: HookPoint = HookPoint::hooked(
    "flow.boss_leave_session",
    "After a boss's death the game takes a summoned player out of the session (BlockClear2_3Leave): \"game\", or \"stay\" (it stays in). A switch.",
    &["game", "stay"],
    install_boss_leave,
);

// ── a host's last guest gone ────────────────────────────────────────────────────────────────

/// The multiplayer flow's "OnLeave_Limit" (+0x495CA0, registered by that name with the flow's
/// callbacks): it runs on a host 5 s after its last guest leaves and, through "WarpNextStageKick"
/// (+0x486A00: save the position, request a world reload), sends it to the title screen (verified
/// 2026-10-07 on two games on one PC, 4 of 4 times). Its calls at SEND_HOME_CALLS are the game's
/// own send-home of a player (send_home).
const LEAVE_LIMIT: &str = "48 8b c4 57 48 81 ec 80 00 00 00 48 c7 40 c8 fe ff ff ff 48 89 58 08 48 89 70 18 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 44 24 78 48 8b da 48 8b f9 48 8b ca e8 ?? ?? ?? ?? 84 c0 0f 84 f0 00 00 00 48 8d 44 24 58";

fn leave_limit_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    if LEAVE_LIMIT_ANSWER.switch() == 1 {
        crate::info!("flow: the game would send the host away (its last guest left); it stays (a mod's answer)");
        return 0;
    }
    let f: extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx as usize, r.rdx as usize)
}

static LEAVE_LIMIT_SITE: Site = Site {
    area: "flow",
    name: "leave limit",
    pattern: LEAVE_LIMIT,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(leave_limit_hook),
    without: "a host goes to the title screen after its last guest leaves",
};

fn install_leave_limit() -> Result<(), String> {
    // The send-home's calls are read first: the hook writes over the function's start.
    let _ = send_home_calls();
    LEAVE_LIMIT_SITE.install_or_why()
}

/// kindle.game: flow.leave_limit (catalogue.rs).
pub static LEAVE_LIMIT_ANSWER: HookPoint = HookPoint::hooked(
    "flow.leave_limit",
    "5 s after a host's last guest leaves, the game sends the host to the title screen (OnLeave_Limit): \"game\", or \"skip\" (it stays in its world). A switch.",
    &["game", "skip"],
    install_leave_limit,
);

/// The game's own send-home, as OnLeave_Limit makes it for a player whose host left: the calls at
/// these offsets in it: +0x4860F0 ("returning"), +0x4859B0 with -1 (the return point: the last
/// bonfire), WarpNextStageKick +0x486A00 (the reload), +0x485690 (the home type for the load),
/// +0x485050 ("back home"), +0x4851F0. Asking for the reload alone wasn't enough: after a join the
/// game was set to load the host's world, now gone, and the player went to the title screen
/// (verified 2026-10-09 with two games on one PC: the host's game closed a second after the
/// player's join, 2 of 2 times).
const SEND_HOME_CALLS: [usize; 6] = [0x144, 0x14F, 0x157, 0x15F, 0x167, 0x16F];

fn send_home_calls() -> Option<[usize; 6]> {
    static CALLS: OnceLock<Option<[usize; 6]>> = OnceLock::new();
    *CALLS.get_or_init(|| {
        let at = LEAVE_LIMIT_SITE.find()?[0];
        let calls: Vec<usize> =
            SEND_HOME_CALLS.iter().filter_map(|&off| (memory::read::<u8>(at + off) == Some(0xE8)).then(|| patterns::call_target(at + off)).flatten()).collect();
        let calls = <[usize; 6]>::try_from(calls).ok();
        if calls.is_none() {
            crate::warn!("flow: the game's send-home isn't as expected; Kindle can't send a player home itself");
        }
        calls
    })
}

/// Finds the game's send-home now (before a mod's answer to flow.leave_limit hooks its function).
pub(crate) fn find_send_home() {
    let _ = send_home_calls();
}

/// Sends this game's player home as the game's flow does (see SEND_HOME_CALLS): a reload of its
/// world at its last bonfire, as itself. What a refused reload set for the next load is no longer
/// taken off (world.reload_requested). On the game's thread.
fn send_home() -> Result<(), String> {
    if !crate::on_game_thread() {
        return Err("a player is sent home from the game's thread".into());
    }
    crate::game::player_chr_addr()?;
    let calls = send_home_calls().ok_or("the game's send-home isn't found on this build")?;
    *lock(&REFUSED_AT) = None;
    // None of them uses its rcx (the flow).
    let returning: extern "C" fn() = unsafe { std::mem::transmute(calls[0]) };
    let return_point: extern "C" fn(usize, i32) = unsafe { std::mem::transmute(calls[1]) };
    returning();
    return_point(0, -1);
    for &f in &calls[2..] {
        let f: extern "C" fn() = unsafe { std::mem::transmute(f) };
        f();
    }
    crate::info!("flow: sent this player home");
    Ok(())
}

/// kindle.game: world.send_home (catalogue.rs).
pub static SEND_HOME: crate::catalogue::Function = crate::catalogue::Function {
    name: "world.send_home",
    doc: "Sends the player home as the game's multiplayer flow does when its host is gone: its own world reloads, at the bonfire it last rested at. A session it's in should be left first (kindle.net.leave).",
    args: &[],
    call: |_| send_home().map(|_| None),
};

// ── the world reload ────────────────────────────────────────────────────────────────────────

/// The game's save-and-reload (+0x4D1BB0: GameMan reload request, the position saved), which the
/// flow's WarpNextStageKick calls to send a phantom home (a dead phantom's go-home, a fall out of
/// the world, a host gone, a boss clear).
const WORLD_RELOAD: &str = "48 89 5c 24 10 57 48 83 ec 20 b1 01 e8 ?? ?? ?? ?? 48 8b 05 ?? ?? ?? ?? 32 db 48 8b 78 10 48 85 ff 74 ?? 48 8d 4c 24 30 e8";
/// When the last reload was refused: for HOLD_HOME_RETURN_FOR after it, what the flow set for the
/// load after it (the module's notes) is taken off.
static REFUSED_AT: Mutex<Option<Instant>> = Mutex::new(None);
const HOLD_HOME_RETURN_FOR: Duration = Duration::from_secs(3);
/// GameDataMan: the type the next load gives the player data (-1: none), and "back home" (0: no).
pub(crate) const PENDING_TYPE_AT: usize = 0x9C;
const BACK_HOME_AT: usize = 0x80;

fn world_reload_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let rsp = unsafe { &*reg }.rsp as usize;
    crate::info!("world: the game asks for a world reload (from {})", game_callers(rsp, 8));
    if RELOAD_REQUESTED.decide(&[]) == 1 {
        crate::info!("world: the reload is refused (a mod's answer)");
        *lock(&REFUSED_AT) = Some(Instant::now());
        return 0;
    }
    let f: extern "C" fn() -> usize = unsafe { std::mem::transmute(original) };
    f()
}

static WORLD_RELOAD_SITE: Site = Site {
    area: "world",
    name: "world reload",
    pattern: WORLD_RELOAD,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(world_reload_hook),
    without: "mods can't keep a player in the world the game sends it from",
};

/// kindle.game: world.reload_requested (catalogue.rs).
pub static RELOAD_REQUESTED: HookPoint = HookPoint::hooked(
    "world.reload_requested",
    "The game asks for a world reload to send this player home (its flow's WarpNextStageKick: a phantom's death or fall, a host gone, a boss clear): \"game\", or \"refuse\" (the player stays where it is; what the send-home set for the next load, a home type and \"back home\", is taken off). On the game's thread, rarely.",
    &["game", "refuse"],
    || WORLD_RELOAD_SITE.install_or_why(),
);

/// Each frame: for a while after a refused reload, what the refused send-home set for the next
/// load is taken off (the module's notes).
pub(crate) fn frame() {
    let mut refused = lock(&REFUSED_AT);
    let Some(at) = *refused else { return };
    if at.elapsed() > HOLD_HOME_RETURN_FOR {
        *refused = None;
        return;
    }
    drop(refused);
    let Some(gdm) = crate::ds3::GAME_DATA_MAN.instance() else { return };
    let (Some(pending_type), Some(back_home)) = (memory::read::<i32>(gdm + PENDING_TYPE_AT), memory::read::<u8>(gdm + BACK_HOME_AT)) else { return };
    if pending_type == -1 && back_home == 0 {
        return;
    }
    unsafe {
        std::ptr::write_volatile((gdm + PENDING_TYPE_AT) as *mut i32, -1);
        std::ptr::write_volatile((gdm + BACK_HOME_AT) as *mut u8, 0);
    }
    crate::info!("world: the refused send-home's home type ({pending_type}) and \"back home\" ({back_home}) taken off the next load");
}

// ── network notices ─────────────────────────────────────────────────────────────────────────

/// The game's network notices (+0x474B70; rdx a NetworkMsgParam row, r8 which of its messages):
/// "A connection error occurred. Returning to your world." (row 271), "Being summoned to another
/// world as phantom...", "Critical battle over. Returning to your world." (211), "Critical battle
/// can now be accessed" (261) and the others. Hidden, the notice gets row 0 (no messages) instead;
/// the game frees the notice's text itself either way.
const NETWORK_MSG: &str = "40 55 53 56 57 41 54 41 56 41 57 48 8d 6c 24 f0 48 81 ec 10 01 00 00 48 c7 44 24 38 fe ff ff ff 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 45 00 45 8b e1 41 8b f8 8b da 4c 8b f9 48 8b 75 70";

fn network_msg_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &mut *reg };
    let row = r.rdx as u32;
    if row == 0 || !NETWORK_NOTICE.answered() || NETWORK_NOTICE.decide(&[row as i64]) != 1 {
        return;
    }
    crate::info!("ui: network notice {row} not shown (a mod's answer)");
    r.rdx &= !0xFFFF_FFFF;
}

static NETWORK_MSG_SITE: Site = Site {
    area: "ui",
    name: "network message",
    pattern: NETWORK_MSG,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(network_msg_hook),
    without: "mods can't hide the game's network notices",
};

/// kindle.game: ui.network_notice (catalogue.rs).
pub static NETWORK_NOTICE: HookPoint = HookPoint::hooked(
    "ui.network_notice",
    "The game shows a network notice (its NetworkMsgParam row: 271 \"A connection error occurred. Returning to your world.\", 211 \"Critical battle over...\", 261 \"Critical battle can now be accessed\"...): \"game\", or \"hide\" it. On the game's thread, rarely.",
    &["game", "hide"],
    || NETWORK_MSG_SITE.install_or_why(),
);
