//! Kindle's stand-ins for the game's Steam interfaces: the swapped steam_api64 imports, the
//! thunks forwarding the slots Kindle doesn't take, the callbacks and call results it delivers,
//! and ISteamMatchmaking's lobby and ISteamNetworking's packet slots while a Kindle session is up
//! (the module doc in mod.rs has the slots and payloads). Also the channel manager's exit fix.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::link::{leave, packet_frame, start_host};
use super::{EVENTS, Event, READ, SENT, log_totals, polled, queue, session, traffic};
use crate::memory;

const MATCHMAKING_SLOTS: usize = 40;
const NETWORKING_SLOTS: usize = 16;
/// Kindle's call handles (the game's are Steam's): a marker in the top half.
const CALL_MARK: u64 = 0x4B49_0000_0000_0000;
const CALL_MARK_MASK: u64 = 0xFFFF_0000_0000_0000;

pub(super) const LOBBY_CHAT_UPDATE: i32 = 506;
pub(super) const P2P_SESSION_REQUEST: i32 = 1202;
pub(super) const P2P_SESSION_CONNECT_FAIL: i32 = 1203;
/// ValidateAuthTicketResponse_t {u64 who, i32 response, u64 owner}: 24 bytes.
const VALIDATE_AUTH_TICKET_RESPONSE: i32 = 143;
/// k_EBeginAuthSessionResultOK, k_EAuthSessionResponseOK.
const AUTH_OK: u32 = 0;
const RESULT_OK: i32 = 1;
const ENTER_SUCCESS: u32 = 1;
pub(super) const MEMBER_ENTERED: u32 = 1;
pub(super) const MEMBER_LEFT: u32 = 2;
/// k_EP2PSessionErrorTimeout.
pub(super) const SESSION_ERROR_TIMEOUT: u8 = 4;

/// The game's imports Kindle swaps, and what they pointed at.
struct Imports {
    matchmaking: usize,
    networking: usize,
    user: usize,
    register_callback: usize,
    unregister_callback: usize,
    register_call_result: usize,
    unregister_call_result: usize,
    run_callbacks: usize,
}

static ORIGINAL: OnceLock<Imports> = OnceLock::new();
/// Kindle's interface objects: (proxy, the real interface).
static MATCHMAKING: OnceLock<(Box<usize>, usize)> = OnceLock::new();
static NETWORKING: OnceLock<(Box<usize>, usize)> = OnceLock::new();

/// Registered callback objects by callback id, and Kindle's pending call results' objects.
static CALLBACKS: Mutex<Vec<(i32, usize)>> = Mutex::new(Vec::new());
static RESULTS: Mutex<Vec<(u64, usize)>> = Mutex::new(Vec::new());
static NEXT_CALL: AtomicU64 = AtomicU64::new(1);

/// The next CreateLobby is Kindle's (set by co-op's host).
static HOSTING: AtomicBool = AtomicBool::new(false);

// ── installing ────────────────────────────────────────────────────────────────

/// The game's network channel thread (DLNR3D::SocketChannelManager, +0x198E910) loops while its
/// manager's "running" byte (+0x164) is set, locking the manager's lock (+0x170) each pass. The
/// manager's destructor (+0x198DEF0) destroys that lock and then the thread object without
/// stopping the thread: it trusts that the network's shutdown already called the manager's Stop
/// (+0x198E1E0: running off, then it joins the thread). At every Kindle game exit the thread
/// still ran and locked a destroyed lock, whose base vtable's slot is the deleting destructor:
/// heap corruption (exit code 0x40000015). Kindle's hook calls the game's Stop first when the
/// thread still runs, as the shutdown would have.
const CHANNEL_MANAGER_DTOR: &str =
    "48 89 4c 24 08 57 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 50 48 8b f9 48 8d 05 ?? ?? ?? ?? 48 89 01 48 8d 99 70 01 00 00";
const CHANNEL_MANAGER_STOP: &str =
    "40 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 60 48 c7 44 24 20 fe ff ff ff 48 89 9c 24 b8 00 00 00 48 8b e9 32 db 48 8d b1 70 01 00 00";
/// The manager's "running" byte.
const CHANNEL_RUNNING_AT: usize = 0x164;
static CHANNEL_STOP: OnceLock<usize> = OnceLock::new();

static CHANNEL_MANAGER_DTOR_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "network",
    name: "channel manager destructor",
    pattern: CHANNEL_MANAGER_DTOR,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(channel_manager_dtor_hook),
    without: "its thread may outlive it at exit",
};

/// Hooks the channel manager's destructor (see CHANNEL_MANAGER_DTOR). Not co-op: every exit.
pub fn install_channel_stop() {
    let (Some(dtor), Some(stop)) = (
        crate::patterns::find_unique("channel manager destructor", CHANNEL_MANAGER_DTOR),
        crate::patterns::find_unique("channel manager stop", CHANNEL_MANAGER_STOP),
    ) else {
        return crate::warn!("network: the game's channel manager wasn't found; its thread may outlive it at exit");
    };
    let _ = CHANNEL_STOP.set(stop);
    if let Ok(h) = CHANNEL_MANAGER_DTOR_SITE.hook_at(dtor) {
        std::mem::forget(h);
    }
}

fn channel_manager_dtor_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let manager = unsafe { &*reg }.rcx as usize;
    if memory::read::<u8>(manager + CHANNEL_RUNNING_AT).is_some_and(|r| r != 0)
        && let Some(&stop) = CHANNEL_STOP.get()
    {
        crate::info!("network: the channel thread still ran as its manager was freed; stopped first");
        let stop: extern "C" fn(usize) = unsafe { std::mem::transmute(stop) };
        stop(manager);
    }
    let f: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(original) };
    f(manager)
}

/// Points the game's Steam imports at Kindle's (once, early). Nothing changes for the game
/// until a Kindle session is up.
pub fn install() -> bool {
    if ORIGINAL.get().is_some() {
        return true;
    }
    let slots = memory::game_imports("steam_api64.dll");
    let names = [
        "SteamMatchmaking",
        "SteamNetworking",
        "SteamUser",
        "SteamAPI_RegisterCallback",
        "SteamAPI_UnregisterCallback",
        "SteamAPI_RegisterCallResult",
        "SteamAPI_UnregisterCallResult",
        "SteamAPI_RunCallbacks",
    ];
    let Some(at) = names.iter().map(|n| slots.get(*n).copied()).collect::<Option<Vec<usize>>>() else {
        crate::warn!("co-op: the game's Steam imports weren't found; Kindle's connection is off");
        return false;
    };
    let read = |i: usize| memory::read_u64(at[i]).unwrap_or(0) as usize;
    let original = Imports {
        matchmaking: read(0),
        networking: read(1),
        user: read(2),
        register_callback: read(3),
        unregister_callback: read(4),
        register_call_result: read(5),
        unregister_call_result: read(6),
        run_callbacks: read(7),
    };
    if [original.matchmaking, original.networking, original.register_callback, original.run_callbacks].contains(&0) {
        crate::warn!("co-op: the game's Steam imports aren't filled in; Kindle's connection is off");
        return false;
    }
    let _ = ORIGINAL.set(original);
    let ours: [usize; 8] = [
        matchmaking as *const () as usize,
        networking as *const () as usize,
        if read(2) != 0 { user as *const () as usize } else { read(2) },
        register_callback as *const () as usize,
        unregister_callback as *const () as usize,
        register_call_result as *const () as usize,
        unregister_call_result as *const () as usize,
        run_callbacks as *const () as usize,
    ];
    for (slot, value) in at.iter().zip(ours) {
        if !unsafe { crate::hooks::write_ptr(*slot, value) } {
            crate::warn!("co-op: couldn't swap the game's Steam imports");
            return false;
        }
    }
    true
}

fn original() -> &'static Imports {
    ORIGINAL.get().expect("p2p imports used before install")
}

/// Code forwarding each slot to `real`'s (replacing `this`).
fn thunks(real: usize, count: usize) -> Option<Vec<usize>> {
    use windows::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE, VirtualAlloc};
    const SIZE: usize = 24;
    let page = unsafe { VirtualAlloc(None, SIZE * count, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as usize;
    if page == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // mov rcx, real; mov rax, [rcx]; jmp [rax + 8 * i]
        let mut code = Vec::with_capacity(SIZE);
        code.extend_from_slice(&[0x48, 0xB9]);
        code.extend_from_slice(&(real as u64).to_le_bytes());
        code.extend_from_slice(&[0x48, 0x8B, 0x01, 0xFF, 0xA0]);
        code.extend_from_slice(&((8 * i) as u32).to_le_bytes());
        let at = page + SIZE * i;
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(), at as *mut u8, code.len()) };
        out.push(at);
    }
    Some(out)
}

/// An interface object of Kindle's: a vtable of thunks to `real`, with `overrides`.
fn proxy(real: usize, count: usize, overrides: &[(usize, usize)]) -> Option<Box<usize>> {
    let mut slots = thunks(real, count)?;
    for &(i, f) in overrides {
        slots[i] = f;
    }
    let vtable = Box::leak(slots.into_boxed_slice()).as_ptr() as usize;
    Some(Box::new(vtable))
}

type Getter = unsafe extern "C" fn() -> usize;

/// What a getter gave the game before. The Steam of a test install now and then answers
/// SteamUser() and SteamNetworking() with null for a moment, and the game never checks: its voice
/// thread read through it (+0x19987D0, +0x1998B3F, +0x19974B2: 11 crashes in three test games'
/// logs, 6 of them a host's). Steam's interfaces last as long as the game, and Kindle's own
/// forward to the one Steam gave first, so that one is given again.
fn given_before(name: &str, made: &OnceLock<(Box<usize>, usize)>) -> usize {
    static TOLD: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let given = made.get().map_or(0, |(ours, real)| if **ours == 0 { *real } else { &**ours as *const usize as usize });
    let mut told = TOLD.lock().unwrap_or_else(|e| e.into_inner());
    if !told.iter().any(|t| t == name) {
        told.push(name.to_string());
        crate::warn!(
            "network: Steam answered {name}() with nothing; the game gets {}",
            if given == 0 { "nothing too (none before)" } else { "the one from before" }
        );
    }
    given
}

extern "C" fn matchmaking() -> usize {
    let real = unsafe { std::mem::transmute::<usize, Getter>(original().matchmaking)() };
    if real == 0 {
        return given_before("SteamMatchmaking", &MATCHMAKING);
    }
    let made = MATCHMAKING.get_or_init(|| {
        let overrides = [
            (13, create_lobby as *const () as usize),
            (14, join_lobby as *const () as usize),
            (15, leave_lobby as *const () as usize),
            (17, member_count as *const () as usize),
            (18, member_by_index as *const () as usize),
            (35, lobby_owner as *const () as usize),
        ];
        (proxy(real, MATCHMAKING_SLOTS, &overrides).unwrap_or_default(), real)
    });
    if *made.0 == 0 { real } else { &*made.0 as *const usize as usize }
}

extern "C" fn networking() -> usize {
    let real = unsafe { std::mem::transmute::<usize, Getter>(original().networking)() };
    if real == 0 {
        return given_before("SteamNetworking", &NETWORKING);
    }
    let made = NETWORKING.get_or_init(|| {
        let overrides = [
            (0, send_packet as *const () as usize),
            (1, packet_available as *const () as usize),
            (2, read_packet as *const () as usize),
            (3, accept_session as *const () as usize),
            (4, close_session as *const () as usize),
        ];
        (proxy(real, NETWORKING_SLOTS, &overrides).unwrap_or_default(), real)
    });
    if *made.0 == 0 { real } else { &*made.0 as *const usize as usize }
}

/// Set while mods are on (offline.rs, before `install`): the game's `SteamUser()->BLoggedOn()`
/// (slot 1) answers false. The game marks itself connected to Steam only when that's true (at
/// start, +0x1968737, and on Steam's servers-connected callback, +0x1967AE0, both then
/// +0x1968880), and only a connected game logs in to FromSoftware's server after "press any
/// button" (the "Logging in" wait and its failure dialogs). Everything else in SteamUser goes
/// to Steam (Kindle's co-op uses its GetSteamID), except the session's tickets (below).
pub static STEAM_OFFLINE: AtomicBool = AtomicBool::new(false);
static USER: OnceLock<(Box<usize>, usize)> = OnceLock::new();
const USER_SLOTS: usize = 32;

extern "C" fn user() -> usize {
    let real = unsafe { std::mem::transmute::<usize, Getter>(original().user)() };
    if real == 0 {
        return given_before("SteamUser", &USER);
    }
    let made = USER.get_or_init(|| {
        let mut overrides = vec![(14, begin_auth as *const () as usize), (15, end_auth as *const () as usize)];
        if STEAM_OFFLINE.load(Ordering::Relaxed) {
            overrides.push((1, logged_on as *const () as usize));
        }
        (proxy(real, USER_SLOTS, &overrides).unwrap_or_default(), real)
    });
    if *made.0 == 0 { real } else { &*made.0 as *const usize as usize }
}

/// The game checks each player in its session with Steam: it sends its ticket
/// (`GetAuthSessionTicket`), checks the others' (`BeginAuthSession`, slot 14), and waits for
/// Steam's answer (ValidateAuthTicketResponse_t, 5 s: its "SteamValidateAuthTicketTimeout"), and
/// ends it when they go (`EndAuthSession`, slot 15). Steam can't vouch for a player whose game
/// isn't on a real Steam account (or isn't logged in), so for players in Kindle's session Kindle
/// answers: the ticket is good (the players are the ones the host let in). Others go to Steam.
static AUTHED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

extern "C" fn begin_auth(this: usize, ticket: usize, size: i32, who: u64) -> u32 {
    let ours = session().as_ref().is_some_and(|s| s.members.contains(&who) || s.links.contains_key(&who));
    if !ours {
        let real = USER.get().map(|u| u.1).unwrap_or(0);
        let f = real_slot(real, 14);
        if f == 0 {
            return AUTH_OK;
        }
        let f: extern "C" fn(usize, usize, i32, u64) -> u32 = unsafe { std::mem::transmute(f) };
        return f(real, ticket, size, who);
    }
    let _ = this;
    let mut authed = AUTHED.lock().unwrap_or_else(|e| e.into_inner());
    if !authed.contains(&who) {
        authed.push(who);
        crate::info!("co-op: a player's ticket is checked by Kindle, not Steam ({who:#x})");
    }
    queue(Event::Callback(VALIDATE_AUTH_TICKET_RESPONSE, payload(&[&who.to_le_bytes(), &AUTH_OK.to_le_bytes(), &[0; 4], &who.to_le_bytes()], 24)));
    AUTH_OK
}

extern "C" fn end_auth(this: usize, who: u64) {
    let _ = this;
    let mut authed = AUTHED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = authed.iter().position(|&a| a == who) {
        authed.remove(i);
        return;
    }
    drop(authed);
    let real = USER.get().map(|u| u.1).unwrap_or(0);
    let f = real_slot(real, 15);
    if f != 0 {
        let f: extern "C" fn(usize, u64) = unsafe { std::mem::transmute(f) };
        f(real, who);
    }
}

extern "C" fn logged_on(_this: usize) -> bool {
    false
}

/// The real interface's vtable slot `i`.
fn real_slot(real: usize, i: usize) -> usize {
    memory::read_u64(real).and_then(|vt| memory::read_u64(vt as usize + 8 * i)).unwrap_or(0) as usize
}

fn real_matchmaking() -> usize {
    MATCHMAKING.get().map(|m| m.1).unwrap_or(0)
}

fn real_networking() -> usize {
    NETWORKING.get().map(|m| m.1).unwrap_or(0)
}

/// The id the game knows this player by (`SteamUser()->GetSteamID()`).
pub fn my_id() -> Option<u64> {
    let getter = original().user;
    if getter == 0 {
        return None;
    }
    let user = unsafe { std::mem::transmute::<usize, Getter>(getter)() };
    let f = real_slot(user, 2);
    if f == 0 {
        return None;
    }
    let get: extern "C" fn(usize, *mut u64) -> *mut u64 = unsafe { std::mem::transmute(f) };
    let mut id = 0u64;
    get(user, &mut id);
    (id != 0).then_some(id)
}

// ── callbacks ─────────────────────────────────────────────────────────────────

type RegisterFn = unsafe extern "C" fn(usize, i32);
type UnregisterFn = unsafe extern "C" fn(usize);
type CallResultFn = unsafe extern "C" fn(usize, u64);
type RunFn = unsafe extern "C" fn();

extern "C" fn register_callback(object: usize, id: i32) {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).push((id, object));
    unsafe { std::mem::transmute::<usize, RegisterFn>(original().register_callback)(object, id) };
}

extern "C" fn unregister_callback(object: usize) {
    CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).retain(|&(_, o)| o != object);
    unsafe { std::mem::transmute::<usize, UnregisterFn>(original().unregister_callback)(object) };
}

extern "C" fn register_call_result(object: usize, call: u64) {
    if call & CALL_MARK_MASK == CALL_MARK {
        RESULTS.lock().unwrap_or_else(|e| e.into_inner()).push((call, object));
        return;
    }
    unsafe { std::mem::transmute::<usize, CallResultFn>(original().register_call_result)(object, call) };
}

extern "C" fn unregister_call_result(object: usize, call: u64) {
    if call & CALL_MARK_MASK == CALL_MARK {
        RESULTS.lock().unwrap_or_else(|e| e.into_inner()).retain(|&(c, o)| c != call || o != object);
        return;
    }
    unsafe { std::mem::transmute::<usize, CallResultFn>(original().unregister_call_result)(object, call) };
}

/// A new session: nothing made yet.
pub(super) fn parts_reset() {
    crate::session_players::reset();
    // Before a mod's answer to flow.leave_limit hooks the function it's read from.
    crate::multiplayer_flow::find_send_home();
}

/// Each frame (the game's thread), the parts of Kindle's sessions in this order: the lobby and its
/// party saves, the watchdog, the players' pings, the other players' gear and characters.
fn parts_frame() {
    crate::lobby::frame();
    crate::party_saves::frame();
    crate::watchdog::frame();
    crate::session_hud::frame();
    crate::session_players::gear_frame();
    crate::session_players::characters_frame();
}

/// The real RunCallbacks, then Kindle's events (on the game's thread, no lock held while the
/// game runs them).
extern "C" fn run_callbacks() {
    unsafe { std::mem::transmute::<usize, RunFn>(original().run_callbacks)() };
    parts_frame();
    if session().is_some() {
        log_totals();
        if session().as_ref().is_some_and(|s| s.left) {
            crate::info!("co-op: the game left the session; Kindle's ends too");
            leave();
        }
    }
    let events = std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()));
    let mut later = Vec::new();
    for e in events {
        match e {
            Event::Callback(id, data) => {
                let objects: Vec<usize> = CALLBACKS.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|&&(i, _)| i == id).map(|&(_, o)| o).collect();
                for o in objects {
                    let run = real_slot(o, 1);
                    if run != 0 {
                        let run: extern "C" fn(usize, *const u8) = unsafe { std::mem::transmute(run) };
                        run(o, data.as_ptr());
                    }
                }
            }
            Event::Result(call, data) => {
                let object = {
                    let mut r = RESULTS.lock().unwrap_or_else(|e| e.into_inner());
                    r.iter().position(|&(c, _)| c == call).map(|i| r.remove(i).1)
                };
                match object {
                    Some(o) => {
                        let run = real_slot(o, 0);
                        if run != 0 {
                            let run: extern "C" fn(usize, *const u8, bool, u64) = unsafe { std::mem::transmute(run) };
                            run(o, data.as_ptr(), false, call);
                        }
                        if let Some(s) = session().as_mut().filter(|s| s.enter_call == Some(call)) {
                            s.entered = true;
                        }
                    }
                    // Registered right after the call returns; wait for it.
                    None => later.push(Event::Result(call, data)),
                }
            }
        }
    }
    if !later.is_empty() {
        EVENTS.lock().unwrap_or_else(|e| e.into_inner()).extend(later);
    }
}

fn new_call() -> u64 {
    CALL_MARK | NEXT_CALL.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn payload(fields: &[&[u8]], size: usize) -> Vec<u8> {
    let mut v = vec![0u8; size.max(16)];
    let mut at = 0;
    for f in fields {
        v[at..at + f.len()].copy_from_slice(f);
        at += f.len();
    }
    v
}

fn lobby_created(lobby: u64) -> Vec<u8> {
    payload(&[&RESULT_OK.to_le_bytes(), &[0; 4], &lobby.to_le_bytes()], 16)
}

fn lobby_enter(lobby: u64) -> Vec<u8> {
    payload(&[&lobby.to_le_bytes(), &0u32.to_le_bytes(), &[0; 4], &ENTER_SUCCESS.to_le_bytes()], 24)
}

pub(super) fn chat_update(lobby: u64, who: u64, state: u32) -> Vec<u8> {
    payload(&[&lobby.to_le_bytes(), &who.to_le_bytes(), &who.to_le_bytes(), &state.to_le_bytes()], 32)
}

// ── ISteamMatchmaking ─────────────────────────────────────────────────────────

extern "C" fn create_lobby(this: usize, kind: i32, max: i32) -> u64 {
    if HOSTING.swap(false, Ordering::Relaxed) {
        match start_host() {
            Ok(lobby) => {
                let call = new_call();
                queue(Event::Result(call, lobby_created(lobby)));
                return call;
            }
            Err(e) => crate::warn!("co-op: couldn't host: {e}"),
        }
        return 0;
    }
    let f: extern "C" fn(usize, i32, i32) -> u64 = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 13)) };
    let _ = this;
    crate::info!("co-op: the game creates a Steam lobby (type {kind}, {max} players)");
    f(real_matchmaking(), kind, max)
}

extern "C" fn join_lobby(_this: usize, lobby: u64) -> u64 {
    if let Some(s) = session().as_mut().filter(|s| s.lobby == lobby) {
        if std::mem::take(&mut s.left) {
            crate::info!("co-op: the game joined Kindle's lobby again");
        } else {
            crate::info!("co-op: the game joins Kindle's lobby");
        }
        let call = new_call();
        s.enter_call = Some(call);
        queue(Event::Result(call, lobby_enter(lobby)));
        return call;
    }
    let f: extern "C" fn(usize, u64) -> u64 = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 14)) };
    crate::info!("co-op: the game joins Steam lobby {lobby:#x}");
    f(real_matchmaking(), lobby)
}

extern "C" fn leave_lobby(_this: usize, lobby: u64) {
    if let Some(s) = session().as_mut().filter(|s| s.lobby == lobby) {
        // (The game leaves the lobby in its session's leave: a summoned player's game left it as
        // it arrived in early builds, and the link outlived a leave by 10 s to see whether it
        // joined again; with the summon's join none does, in any log since. Ended from the next
        // frame's callbacks, not inside the game's own call.)
        crate::info!("co-op: the game left Kindle's lobby");
        s.left = true;
        return;
    }
    let f: extern "C" fn(usize, u64) = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 15)) };
    crate::info!("co-op: the game leaves Steam lobby {lobby:#x}");
    f(real_matchmaking(), lobby)
}

extern "C" fn member_count(_this: usize, lobby: u64) -> i32 {
    if let Some(s) = session().as_ref().filter(|s| s.lobby == lobby) {
        return s.members.len() as i32;
    }
    let f: extern "C" fn(usize, u64) -> i32 = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 17)) };
    f(real_matchmaking(), lobby)
}

extern "C" fn member_by_index(_this: usize, out: *mut u64, lobby: u64, i: i32) -> *mut u64 {
    if let Some(s) = session().as_ref().filter(|s| s.lobby == lobby) {
        unsafe { *out = s.members.get(i as usize).copied().unwrap_or(0) };
        return out;
    }
    let f: extern "C" fn(usize, *mut u64, u64, i32) -> *mut u64 = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 18)) };
    f(real_matchmaking(), out, lobby, i)
}

extern "C" fn lobby_owner(_this: usize, out: *mut u64, lobby: u64) -> *mut u64 {
    if let Some(s) = session().as_ref().filter(|s| s.lobby == lobby) {
        unsafe { *out = s.owner };
        return out;
    }
    let f: extern "C" fn(usize, *mut u64, u64) -> *mut u64 = unsafe { std::mem::transmute(real_slot(real_matchmaking(), 35)) };
    f(real_matchmaking(), out, lobby)
}

// ── ISteamNetworking ──────────────────────────────────────────────────────────

extern "C" fn send_packet(_this: usize, to: u64, data: *const u8, size: u32, kind: i32, channel: i32) -> bool {
    {
        let mut guard = session();
        if let Some(s) = guard.as_mut() {
            if !s.members.contains(&to) || to == s.me {
                // Someone outside the session (a player who just left, say): not for Steam's
                // networking either, which crashed on such a packet (a test install's,
                // verified 2026-10-09 with two games on one PC, the host as a player joined).
                if s.dropped.insert(to) {
                    crate::info!("co-op: the game sent a packet to {to:#x}, who isn't in the session; dropped");
                }
                return false;
            }
            let bytes = unsafe { std::slice::from_raw_parts(data, size as usize) };
            traffic(SENT, channel, to, bytes);
            s.accepted.insert(to);
            let frame = packet_frame(s.me, to, channel, bytes);
            // A player sends everything through the host.
            let via = if s.host { to } else { s.owner };
            return s.links.get_mut(&via).is_some_and(|l| l.write_all(&frame).is_ok());
        }
    }
    let f: extern "C" fn(usize, u64, *const u8, u32, i32, i32) -> bool = unsafe { std::mem::transmute(real_slot(real_networking(), 0)) };
    f(real_networking(), to, data, size, kind, channel)
}

extern "C" fn packet_available(_this: usize, size: *mut u32, channel: i32) -> bool {
    if session().is_some() {
        polled(channel);
    }
    if let Some(s) = session().as_ref()
        && let Some((_, _, data)) = s.inbox.iter().find(|(from, c, _)| *c == channel && s.readable(from))
    {
        unsafe { *size = data.len() as u32 };
        return true;
    }
    let f: extern "C" fn(usize, *mut u32, i32) -> bool = unsafe { std::mem::transmute(real_slot(real_networking(), 1)) };
    f(real_networking(), size, channel)
}

extern "C" fn read_packet(_this: usize, dest: *mut u8, cap: u32, size: *mut u32, from: *mut u64, channel: i32) -> bool {
    {
        let mut guard = session();
        if let Some(s) = guard.as_mut()
            && let Some(i) = s.inbox.iter().position(|(from, c, _)| *c == channel && s.readable(from))
            && let Some((sender, _, data)) = s.inbox.remove(i)
        {
            let n = data.len().min(cap as usize);
            traffic(READ, channel, sender, &data);
            unsafe {
                std::ptr::copy_nonoverlapping(data.as_ptr(), dest, n);
                *size = n as u32;
                *from = sender;
            }
            return true;
        }
    }
    let f: extern "C" fn(usize, *mut u8, u32, *mut u32, *mut u64, i32) -> bool = unsafe { std::mem::transmute(real_slot(real_networking(), 2)) };
    f(real_networking(), dest, cap, size, from, channel)
}

extern "C" fn accept_session(_this: usize, id: u64) -> bool {
    if let Some(s) = session().as_mut().filter(|s| s.members.contains(&id)) {
        if s.accepted.insert(id) {
            crate::info!("co-op: the game accepted {id:#x}");
        }
        return true;
    }
    let f: extern "C" fn(usize, u64) -> bool = unsafe { std::mem::transmute(real_slot(real_networking(), 3)) };
    f(real_networking(), id)
}

extern "C" fn close_session(_this: usize, id: u64) -> bool {
    if let Some(s) = session().as_mut().filter(|s| s.members.contains(&id) || s.accepted.contains(&id) || s.requested.contains(&id)) {
        // As with Steam, the peer's next packets wait for the game to accept it again.
        s.accepted.remove(&id);
        s.requested.remove(&id);
        crate::info!("co-op: the game closed its link to {id:#x}");
        return true;
    }
    let f: extern "C" fn(usize, u64) -> bool = unsafe { std::mem::transmute(real_slot(real_networking(), 4)) };
    f(real_networking(), id)
}

/// The next CreateLobby (the game's, from co-op's host) makes a Kindle session.
pub fn arm_host() {
    HOSTING.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payloads_have_the_games_sizes() {
        assert_eq!(&lobby_created(7)[8..16], &7u64.to_le_bytes());
        assert_eq!(&lobby_enter(7)[16..20], &ENTER_SUCCESS.to_le_bytes());
        let u = chat_update(1, 2, MEMBER_LEFT);
        assert_eq!((u.len(), &u[24..28]), (32, &MEMBER_LEFT.to_le_bytes()[..]));
    }
}
