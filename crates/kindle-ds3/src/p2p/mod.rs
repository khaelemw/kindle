//! Kindle's own connection for co-op: the game's multiplayer runs over Steam lobbies and Steam's
//! peer-to-peer packets, and Kindle carries both itself, straight between the players' PCs (no
//! Steam or FromSoftware server; a TCP link per player, on a LAN or over the internet to the host's
//! public address).
//!
//! The game (1.15.2) calls steam_api64's `SteamMatchmaking()` and `SteamNetworking()` before
//! every use (never keeping the interface), so Kindle points those imports at its own objects.
//! Their vtables forward every slot to the real interface (a thunk replacing `this`), except the
//! ones the game's sessions use, while a Kindle session is up:
//! - ISteamMatchmaking: 13 CreateLobby(type, max) -> call, 14 JoinLobby(id) -> call,
//!   15 LeaveLobby(id), 17 GetNumLobbyMembers(id), 18 GetLobbyMemberByIndex(id, i) and
//!   35 GetLobbyOwner(id) (the last two return the id through a hidden pointer, rdx);
//! - ISteamNetworking: 0 SendP2PPacket(id, data, size, type, channel), 1
//!   IsP2PPacketAvailable(&size, channel), 2 ReadP2PPacket(dest, cap, &size, &from, channel),
//!   3 AcceptP2PSessionWithUser(id), 4 CloseP2PSessionWithUser(id).
//!
//! Steam reports back through callbacks (`SteamAPI_RegisterCallback(object, id)`) and call
//! results (`SteamAPI_RegisterCallResult(object, call)`), delivered in `SteamAPI_RunCallbacks`.
//! Kindle swaps those imports too: it notes the objects, and after the real RunCallbacks
//! delivers its own on the game's thread: a call result through vtable slot 0
//! `Run(param, io_failure, call)`, a callback through slot 1 `Run(param)` (MSVC lists the two
//! overloads reversed; slot 2 is the payload size). Payloads, as the game's objects size them:
//! LobbyCreated_t 513 {i32 result, u64 lobby} 16 bytes; LobbyEnter_t 504 {u64 lobby, u32
//! permissions, bool locked, u32 response} 24; LobbyChatUpdate_t 506 {u64 lobby, u64 changed,
//! u64 making, u32 state} 32; P2PSessionRequest_t 1202 {u64 remote} 8;
//! P2PSessionConnectFail_t 1203 {u64 remote, u8 error} 9.
//!
//! As with Steam, a new peer's packets wait until the game accepts it
//! (`AcceptP2PSessionWithUser`, which it does from the P2PSessionRequest callback once it
//! knows the peer, i.e. after joining the lobby) or this player has sent to it: handed over at
//! once, a joining game read the host's first handshake packet before its join had set up the
//! host as a member, dropped it and never answered, so the host never set up the link's
//! encryption and dropped everything after (2026-10-06).
//!
//! A session: the host listens; a player joins with the host's code (its address and the
//! session's key; kindle.toml's [coop] public_address makes it the host's internet address),
//! says who it is (the id `SteamUser()->GetSteamID()` gives the game), and gets the lobby and its
//! members, and where the host is (so the game can summon the player there, session.rs). Players'
//! packets go through the host (it passes on what isn't for it), so a lobby
//! of four needs only the host reachable. Frames: u32 length, u8 kind, then the kind's fields.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::SocketAddrV4;
use std::sync::Mutex;
use std::time::Duration;

mod codes;
mod link;
mod steam;

pub(crate) use codes::code_for;
pub use codes::{code, code_is_local, parse};
pub use link::{
    ConnectError, MOD_ALL, ModFrame, configure, connect_detailed, host_gone, host_id, hosting, leave, look, me, members, pings, remove_welcomes, require_dlc,
    send_arrived, send_look, send_mod, send_report_now, send_report_text, set_welcome, take_mod_messages, welcome,
};
pub(crate) use link::{SESSION_HELLO, frame, is_local, mod_arrived, port, rand_u32, reachable_address, read_frame, session_join, u64_at};
pub use steam::{STEAM_OFFLINE, arm_host, install, install_channel_stop, my_id};

use link::Link;

/// What Kindle tells the game in its next RunCallbacks: a callback (id, payload) or a call's result.
enum Event {
    Callback(i32, Vec<u8>),
    Result(u64, Vec<u8>),
}
static EVENTS: Mutex<Vec<Event>> = Mutex::new(Vec::new());

/// Where a player is: the map (packed, 0xAABB0000 for mAA_BB) and the position and facing in it,
/// and the map's state in that player's world (its ceremony, a CeremonyParam id; -1 unknown).
#[derive(Clone, Copy, Debug)]
pub struct Location {
    pub map: i32,
    pub pos: [f32; 3],
    pub angle: f32,
    pub ceremony: i32,
}

impl Location {
    fn to_bytes(self) -> Vec<u8> {
        let mut b = self.map.to_le_bytes().to_vec();
        for f in [self.pos[0], self.pos[1], self.pos[2], self.angle] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&self.ceremony.to_le_bytes());
        b
    }

    /// (A host whose Kindle predates the ceremony sends none.)
    fn from_bytes(b: &[u8]) -> Option<Self> {
        let f = |i: usize| -> Option<f32> { Some(f32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
        let i = |at: usize| -> Option<i32> { Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?)) };
        Some(Self { map: i(0)?, pos: [f(4)?, f(8)?, f(12)?], angle: f(16)?, ceremony: i(20).unwrap_or(-1) })
    }
}

struct Session {
    host: bool,
    lobby: u64,
    me: u64,
    owner: u64,
    members: Vec<u64>,
    /// Host: a link per player. A player: the host's only.
    links: HashMap<u64, Link>,
    inbox: VecDeque<(u64, i32, Vec<u8>)>,
    requested: HashSet<u64>,
    /// Peers whose packets the game may read: accepted by it, or sent to by this player.
    accepted: HashSet<u64>,
    /// Peers outside the session the game sent packets to (dropped; logged once each).
    dropped: HashSet<u64>,
    /// The game left the lobby: its session is over, and Kindle's ends at the next frame.
    left: bool,
    /// The address players join: the host's own, or (a player) the one it joined.
    address: Option<SocketAddrV4>,
    /// The session's key, in its code: a player from outside the host's network is taken only
    /// with it, so nobody who finds the open port gets into the game.
    key: u32,
    /// Players' characters as their Kindles sent them (session_players.rs).
    looks: HashMap<u64, Vec<u8>>,
    /// A player: the host's mods' join data, by (mod, channel) (WELCOME_DATA).
    welcome: HashMap<(String, String), Vec<u8>>,
    /// A player: its game has its entry into the lobby (the LobbyEnter result delivered to it),
    /// and the call that brings it. Until then the game reads no packet of the session's (see
    /// `readable`).
    entered: bool,
    enter_call: Option<u64>,
}

impl Session {
    /// Whether the game may read a packet from `from` now: one it accepted (or sent to), and, on
    /// a player, once its lobby entry was delivered. With Steam a player's game is in the lobby
    /// before the host hears of it, so the host's first packets come after; Kindle tells the host
    /// as the link is made, before the player's game even joins the lobby, so the host's
    /// handshake could come first (seen 2026-10-13: received 1 ms before the player's game
    /// joined). The game accepted the host (its session request ran first) and read the
    /// handshake from a peer it didn't know yet: dropped, never answered; on its lobby entry it
    /// started a handshake of its own, which crossed the host's, and neither game checked the
    /// other's ticket until the join timed out 30 s later, then joined again (the joins that took
    /// 70 s and more after a load). Held until the entry, the host's
    /// handshake is read by a game that knows the host.
    fn readable(&self, from: &u64) -> bool {
        self.accepted.contains(from) && (self.host || self.entered)
    }

    /// A player left: they're no longer a member, and if they come back their packets wait,
    /// as a new peer's do, until the game accepts them or sends to them (which it does once it
    /// has their lobby entry). Kept as accepted, a returning player's first handshake packet
    /// reached the host's game before their lobby entry; the game drops a handshake from a peer it doesn't know ("control
    /// packet from an unknown peer"), never answers it, and the player can't read the host
    /// (every packet fails its check) until its join times out (verified 2026-10-07 with the
    /// game's matching log, three games on one PC, both players back after a boss died).
    fn forget(&mut self, id: u64) {
        self.members.retain(|&m| m != id);
        self.accepted.remove(&id);
        self.requested.remove(&id);
        self.inbox.retain(|(from, _, _)| *from != id);
    }
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);

// ── traffic log (for working out the game's multiplay handshake) ──────────────────────────
// The first packets each way on each channel, the channels the game reads, and the totals
// every few seconds while they change.

const SENT: u8 = 0;
const RECEIVED: u8 = 1;
const READ: u8 = 2;
const LOGGED_PER_CHANNEL: u32 = 4;
const TOTALS_EVERY: Duration = Duration::from_secs(5);

/// Packets and bytes by (direction, channel).
type Traffic = Vec<((u8, i32), (u32, u64))>;
static TRAFFIC: Mutex<Traffic> = Mutex::new(Vec::new());
static POLLED: Mutex<Vec<i32>> = Mutex::new(Vec::new());
static TOTALS_AT: Mutex<Option<(std::time::Instant, String)>> = Mutex::new(None);

fn traffic(dir: u8, channel: i32, peer: u64, data: &[u8]) {
    let n = {
        let mut t = TRAFFIC.lock().unwrap_or_else(|e| e.into_inner());
        let i = match t.iter().position(|(k, _)| *k == (dir, channel)) {
            Some(i) => i,
            None => {
                t.push(((dir, channel), (0, 0)));
                t.len() - 1
            }
        };
        t[i].1.0 += 1;
        t[i].1.1 += data.len() as u64;
        t[i].1.0
    };
    if n <= LOGGED_PER_CHANNEL && dir != READ {
        let hex: String = data.iter().take(24).map(|b| format!("{b:02x}")).collect();
        let what = if dir == SENT { "sent" } else { "received" };
        crate::info!("co-op: {what} channel {channel} {} B {} {peer:#x}: {hex}", data.len(), if dir == SENT { "to" } else { "from" });
    }
}

fn polled(channel: i32) {
    let mut p = POLLED.lock().unwrap_or_else(|e| e.into_inner());
    if !p.contains(&channel) {
        p.push(channel);
        crate::info!("co-op: the game reads channel {channel}");
    }
}

/// The totals, logged every few seconds while they change (from the game's thread).
fn log_totals() {
    let mut at = TOTALS_AT.lock().unwrap_or_else(|e| e.into_inner());
    if at.as_ref().is_some_and(|(t, _)| t.elapsed() < TOTALS_EVERY) {
        return;
    }
    let mut t = TRAFFIC.lock().unwrap_or_else(|e| e.into_inner()).clone();
    t.sort();
    let text = t.iter().map(|((d, c), (n, b))| format!("{} ch{c} {n}/{b}B", ["sent", "recv", "read"][*d as usize])).collect::<Vec<_>>().join(", ");
    let changed = at.as_ref().is_none_or(|(_, last)| *last != text);
    if changed && !text.is_empty() {
        crate::info!("co-op: packets: {text}");
    }
    *at = Some((std::time::Instant::now(), text));
}

fn session() -> std::sync::MutexGuard<'static, Option<Session>> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
}

fn queue(e: Event) {
    EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(e);
}
