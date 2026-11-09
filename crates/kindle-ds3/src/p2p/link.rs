//! The TCP links of a Kindle session: the host's listener and its side of each player's link,
//! a player's join and its link to the host, the frames they carry (players' packets, characters,
//! pings, mods' messages and join data, reports), and what a host refuses a player for.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::Duration;

use super::steam::{
    LOBBY_CHAT_UPDATE, MEMBER_ENTERED, MEMBER_LEFT, P2P_SESSION_CONNECT_FAIL, P2P_SESSION_REQUEST, SESSION_ERROR_TIMEOUT, chat_update, my_id, parts_reset,
    payload,
};
use super::{Event, Location, RECEIVED, Session, queue, session, traffic};
use crate::mods::now_ms;

/// The port a host listens on unless kindle.toml says another ([coop] port).
pub const PORT: u16 = 27860;
static PORT_IN_USE: AtomicU16 = AtomicU16::new(PORT);
/// The host's internet address from kindle.toml ([coop] public_address), for its codes: the
/// address itself, or what its name (a dynamic DNS name, as name.duckdns.org) points to,
/// looked up again every PUBLIC_NAME_EVERY.
static PUBLIC_ADDRESS: Mutex<Option<Ipv4Addr>> = Mutex::new(None);
/// A home connection's address changes now and then (a tester's did overnight, 2026-10-14, and
/// its codes named the old one); a name set up for it follows.
const PUBLIC_NAME_EVERY: Duration = Duration::from_secs(300);
/// Reaching the host and its answer: over the internet a connection takes longer than on a LAN.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Lobby ids look like Steam's.
const LOBBY_HIGH: u64 = 0x0186_0000_0000_0000;
const MAX_FRAME: usize = 1 << 20;

const HELLO: u8 = 1;
const WELCOME: u8 = 2;
const MEMBER: u8 = 3;
const PACKET: u8 = 4;
/// A player's character (session_players.rs packs it: look, equipment, stats, name): u64 whose, then it.
const LOOK: u8 = 5;
// 6 was RUN (Kindle's own randomizer's run, which a player in the host's world used); now a
// randomizer mod's own join data (WELCOME_DATA). Not to be used again.
/// The host won't take a player (instead of WELCOME): why, as text.
const REFUSE: u8 = 7;
/// A player is in the host's world (its game's arrival check, session_players.rs): the host waits for the
/// game to build its character from then. No body.
const ARRIVED: u8 = 8;
// 9 was REST (the party rests now) and 11 STATUS (a player's bonfire, out and downed state),
// resting together's; 12 OUTS, 16 DOWNS, 17 REVIVE, 18 REVIVING and 28 STATE the party's deaths';
// now the co-op mod's own messages (MOD). Not to be used again.
// 10 was FOLLOW (the host is going to another place); now the co-op mod's own message (MOD).
// Not to be used again.
// 13 was AWARD (the host's awards forwarded to its players); retired, each player's game awards
// its own (the co-op mod's event edits). Not to be used again.
// 14 and 15 were LIT and LIGHT (bonfires lit for everyone), now the co-op mod's own messages
// (MOD). Not to be used again.
/// Either way, every PING_EVERY: the sender's time (u64 ms), answered at once with PONG (the
/// same body); the answer's delay is that link's round trip (16-18 are the prototype's).
const PING: u8 = 19;
const PONG: u8 = 20;
/// The host to its players: its round trip to each player (u64 id, u32 ms each).
const PINGS: u8 = 21;
// 22 FLAGS, 23 WORLD_BLOCKS and 24 WORLD_STORIES were the session's flags (a player's network
// sets, the host's unshared blocks, its NPC stories); now the co-op mod's own messages (MOD). Not
// to be used again.
/// Either way: take a report now (watchdog.rs), the reason as text; the host passes a player's on
/// to the other players.
const REPORT_NOW: u8 = 25;
/// A player to the host: a report or crash.log of its game (watchdog.rs): u8 name length, the
/// name, then the text.
const REPORT_TEXT: u8 = 26;
/// A mod's message to the same mod on other games of the session (kindle.net, natives/net.rs): u8
/// length and the mod's id, u8 length and the channel, u64 from (the host fills it in for a
/// player's), u64 to (0 the host, MOD_ALL every game but the sender's, else a member), then the
/// message (JSON). A player's go to the host, which passes them on to whom they're for.
const MOD: u8 = 27;
pub const MOD_ALL: u64 = u64::MAX;
/// A host mod's join data (kindle.net.set_welcome), to a player as it's welcomed (before its game
/// loads the host's world) and to every player whenever the mod changes it: u8 length and the
/// mod's id, u8 length and the channel, then the value (JSON; nothing: removed).
const WELCOME_DATA: u8 = 29;
/// A mod message's largest body, and how many wait at most for Kindle's next frame (more are
/// dropped, said once).
pub const MOD_MAX_BYTES: usize = 16 * 1024;
const MOD_INBOX_MAX: usize = 1024;
const PING_EVERY: Duration = Duration::from_secs(1);
/// Each player's round trip to the host, ms (the host's own measures; a player's: its own to the
/// host, the others' as the host sends them).
static PINGS_MS: Mutex<Vec<(u64, u32)>> = Mutex::new(Vec::new());

fn set_ping(id: u64, ms: u32) {
    let mut p = PINGS_MS.lock().unwrap_or_else(|e| e.into_inner());
    match p.iter_mut().find(|e| e.0 == id) {
        Some(e) => e.1 = ms,
        None => p.push((id, ms)),
    }
}

/// Each player's ping to the host (player id, ms): on a player's game, the host's own entry is
/// its ping to the host.
pub fn pings() -> Vec<(u64, u32)> {
    PINGS_MS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Pings the session's links every PING_EVERY while the session lasts; the host sends its pings
/// to its players too.
fn spawn_pinger(lobby: u64) {
    let _ = std::thread::Builder::new().name("kindle-coop-ping".into()).spawn(move || {
        loop {
            std::thread::sleep(PING_EVERY);
            let mut guard = session();
            let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { break };
            let ping = frame(PING, &now_ms().to_le_bytes());
            for l in s.links.values_mut() {
                let _ = l.write_all(&ping);
            }
            if s.host {
                let table: Vec<u8> = pings().iter().flat_map(|&(id, ms)| [&id.to_le_bytes()[..], &ms.to_le_bytes()[..]].concat()).collect();
                let f = frame(PINGS, &table);
                for l in s.links.values_mut() {
                    let _ = l.write_all(&f);
                }
            }
        }
    });
}

/// A connection to another player, written by a thread of its own: the game's thread (and the
/// link threads) only queue frames. The game sends its packets from its main thread, and a
/// write to a slow or stalled connection (a Wi-Fi hiccup, a full send buffer) would have held up
/// that frame until it went through.
pub(super) struct Link {
    queue: std::sync::mpsc::Sender<Vec<u8>>,
    stream: TcpStream,
    failed: std::sync::Arc<AtomicBool>,
}

impl Link {
    fn new(stream: TcpStream) -> std::io::Result<Self> {
        let mut out = stream.try_clone()?;
        let (queue, frames) = std::sync::mpsc::channel::<Vec<u8>>();
        let failed = std::sync::Arc::new(AtomicBool::new(false));
        let gone = failed.clone();
        std::thread::Builder::new().name("kindle-coop-send".into()).spawn(move || {
            // Ends when the link is dropped (its queue closes) or the connection fails.
            for f in frames {
                if out.write_all(&f).is_err() {
                    gone.store(true, Ordering::Relaxed);
                    break;
                }
            }
        })?;
        Ok(Self { queue, stream, failed })
    }

    /// Queues a frame; an error once the connection has failed.
    pub(super) fn write_all(&mut self, frame: &[u8]) -> std::io::Result<()> {
        if self.failed.load(Ordering::Relaxed) || self.queue.send(frame.to_vec()).is_err() {
            return Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "the connection is closed"));
        }
        Ok(())
    }

    fn shutdown(&self, how: std::net::Shutdown) -> std::io::Result<()> {
        self.stream.shutdown(how)
    }
}

// ── frames ────────────────────────────────────────────────────────────────────

pub(crate) fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(5 + body.len());
    f.extend_from_slice(&(body.len() as u32 + 1).to_le_bytes());
    f.push(kind);
    f.extend_from_slice(body);
    f
}

pub(super) fn packet_frame(from: u64, to: u64, channel: i32, data: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(20 + data.len());
    b.extend_from_slice(&from.to_le_bytes());
    b.extend_from_slice(&to.to_le_bytes());
    b.extend_from_slice(&channel.to_le_bytes());
    b.extend_from_slice(data);
    frame(PACKET, &b)
}

pub(crate) fn read_frame(s: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut len = [0u8; 4];
    s.read_exact(&mut len).ok()?;
    let len = u32::from_le_bytes(len) as usize;
    if len == 0 || len > MAX_FRAME {
        return None;
    }
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).ok()?;
    let kind = body.remove(0);
    Some((kind, body))
}

pub(crate) fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// A packet from `from` for this player: into the inbox (asking the game to accept a new
/// sender, as Steam does).
fn deliver(s: &mut Session, from: u64, channel: i32, data: Vec<u8>) {
    traffic(RECEIVED, channel, from, &data);
    if s.requested.insert(from) {
        queue(Event::Callback(P2P_SESSION_REQUEST, payload(&[&from.to_le_bytes()], 8)));
    }
    s.inbox.push_back((from, channel, data));
}

// ── sessions ──────────────────────────────────────────────────────────────────

/// This PC's LAN address (the one its default route uses; nothing is sent).
fn lan_address() -> Option<Ipv4Addr> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) if !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

/// kindle.toml's [coop] settings: the port a host takes players on, and its internet address.
pub fn configure(port: u16, public_address: &str) {
    if port != 0 {
        PORT_IN_USE.store(port, Ordering::Relaxed);
    }
    let public = public_address.trim();
    if public.is_empty() {
        return;
    }
    if let Ok(ip) = public.parse::<Ipv4Addr>() {
        *PUBLIC_ADDRESS.lock().unwrap_or_else(|e| e.into_inner()) = Some(ip);
        return;
    }
    // A name: looked up off the game's thread (a lookup can take seconds), and again now and then.
    let name = public.to_string();
    let _ = std::thread::Builder::new().name("kindle-coop-address".into()).spawn(move || {
        loop {
            let found = std::net::ToSocketAddrs::to_socket_addrs(&(name.as_str(), 0)).ok().and_then(|mut a| {
                a.find_map(|a| match a.ip() {
                    std::net::IpAddr::V4(ip) => Some(ip),
                    _ => None,
                })
            });
            let mut slot = PUBLIC_ADDRESS.lock().unwrap_or_else(|e| e.into_inner());
            match found {
                Some(ip) if *slot != Some(ip) => {
                    crate::info!("co-op: public_address {name} is {ip}");
                    *slot = Some(ip);
                }
                Some(_) => {}
                None if slot.is_none() => {
                    crate::warn!("co-op: kindle.toml's public_address {name} wasn't found; session codes name this PC's network address until it is")
                }
                None => {}
            }
            drop(slot);
            std::thread::sleep(PUBLIC_NAME_EVERY);
        }
    });
}

pub(crate) fn port() -> u16 {
    PORT_IN_USE.load(Ordering::Relaxed)
}

/// Whether an address is on the host's own network (or this PC).
pub(crate) fn is_local(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

/// The address others reach this PC's co-op port at: kindle.toml's public address, else its
/// address on its own network.
pub(crate) fn reachable_address() -> SocketAddrV4 {
    let public = *PUBLIC_ADDRESS.lock().unwrap_or_else(|e| e.into_inner());
    SocketAddrV4::new(public.or_else(lan_address).unwrap_or(Ipv4Addr::LOCALHOST), port())
}

pub(super) fn start_host() -> Result<u64, String> {
    if session().is_some() {
        return Err("there's a session already".into());
    }
    let me = my_id().ok_or("the game's player id isn't there")?;
    parts_reset();
    let port = port();
    // With a party up, its listener has the port and hands the session's joins over.
    let listener = if crate::lobby::listening() { None } else { Some(TcpListener::bind(("0.0.0.0", port)).map_err(|e| format!("port {port}: {e}"))?) };
    let public = *PUBLIC_ADDRESS.lock().unwrap_or_else(|e| e.into_inner());
    let ip = public.or_else(lan_address).unwrap_or(Ipv4Addr::LOCALHOST);
    let lobby = LOBBY_HIGH | (rand_u32() as u64);
    let key = rand_u32().rotate_left(7) ^ (lobby as u32).wrapping_mul(0x9E37_79B9) | 1;
    *session() = Some(Session {
        host: true,
        lobby,
        me,
        owner: me,
        members: vec![me],
        links: HashMap::new(),
        inbox: VecDeque::new(),
        requested: HashSet::new(),
        accepted: HashSet::new(),
        dropped: HashSet::new(),
        left: false,
        address: Some(SocketAddrV4::new(ip, port)),
        key,
        looks: HashMap::new(),
        welcome: HashMap::new(),
        entered: false,
        enter_call: None,
    });
    if let Some(listener) = listener {
        std::thread::Builder::new()
            .name("kindle-coop-host".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if session().as_ref().is_none_or(|s| s.lobby != lobby) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        let _ = std::thread::Builder::new().name("kindle-coop-player".into()).spawn(move || host_player(stream, lobby));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
    }
    PINGS_MS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    spawn_pinger(lobby);
    crate::info!("co-op: hosting on port {port}, code for {}", if public.is_some() { "the internet (kindle.toml's public_address)" } else { "this network" });
    Ok(lobby)
}

pub(crate) fn rand_u32() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    (t as u32) ^ ((t >> 32) as u32) ^ std::process::id().rotate_left(16)
}

/// The host's side of one player's link.
fn host_player(mut stream: TcpStream, lobby: u64) {
    let _ = stream.set_nodelay(true);
    let Some((HELLO, body)) = read_frame(&mut stream) else { return };
    host_player_hello(stream, lobby, body);
}

/// The session's HELLO kind (the party's listener tells a session's join by it).
pub(crate) const SESSION_HELLO: u8 = HELLO;

/// A session join that came through the party's listener (its HELLO already read): the hosted
/// session takes it, as its own listener's; none hosted, it's refused.
pub(crate) fn session_join(mut stream: TcpStream, hello: Vec<u8>) {
    let _ = stream.set_nodelay(true);
    let Some(lobby) = session().as_ref().filter(|s| s.host).map(|s| s.lobby) else {
        let _ = stream.write_all(&frame(REFUSE, b"the host isn't in its world yet"));
        return;
    };
    host_player_hello(stream, lobby, hello);
}

/// The host's side of one player's link, from its HELLO.
fn host_player_hello(mut stream: TcpStream, lobby: u64, body: Vec<u8>) {
    let Some(id) = u64_at(&body, 0) else { return };
    // The session's key, from the code: a player from outside the host's network needs it (one
    // on it may join with the host's address alone).
    let key = body.get(9..13).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")));
    let local = matches!(stream.peer_addr(), Ok(std::net::SocketAddr::V4(a)) if is_local(*a.ip()));
    if !local && (key.is_none() || key != session().as_ref().filter(|s| s.lobby == lobby).map(|s| s.key)) {
        crate::info!("co-op: a player from outside this network without this session's code can't join ({id:#x})");
        let _ = stream.write_all(&frame(REFUSE, b"the session code doesn't match the host's session: copy its code again"));
        return;
    }
    // A Kindle that says which DLCs it has (older ones don't: they're let in).
    if let Some(why) = body.get(8).and_then(|&theirs| refusal(theirs)) {
        crate::info!("co-op: a player can't join ({id:#x}): {why}");
        let _ = stream.write_all(&frame(REFUSE, why.as_bytes()));
        return;
    }
    let Ok(writer) = stream.try_clone() else { return };
    {
        let mut guard = session();
        let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { return };
        if s.members.contains(&id) {
            crate::warn!("co-op: a player with the same id as one in the session tried to join ({id:#x})");
            return;
        }
        let member = frame(MEMBER, &[&id.to_le_bytes()[..], &[1]].concat());
        for l in s.links.values_mut() {
            let _ = l.write_all(&member);
        }
        s.members.push(id);
        let mut welcome = Vec::new();
        welcome.extend_from_slice(&s.lobby.to_le_bytes());
        welcome.extend_from_slice(&s.owner.to_le_bytes());
        welcome.extend_from_slice(&(s.members.len() as u32).to_le_bytes());
        for m in &s.members {
            welcome.extend_from_slice(&m.to_le_bytes());
        }
        if let Some(at) = crate::session::arrival() {
            welcome.push(1);
            welcome.extend_from_slice(&at.to_bytes());
        }
        let mut w = writer;
        if w.write_all(&frame(WELCOME, &welcome)).is_err() {
            s.members.retain(|&m| m != id);
            return;
        }
        // The characters it'll see: the host's own, and the players' already here.
        if let Some(own) = crate::session_players::own_look() {
            let _ = w.write_all(&look_frame(s.me, &own));
        }
        for (who, look) in &s.looks {
            let _ = w.write_all(&look_frame(*who, look));
        }
        for (mod_id, channel, data) in WELCOMES.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            let _ = w.write_all(&welcome_frame(mod_id, channel, data));
        }
        let Ok(link) = Link::new(w) else {
            s.members.retain(|&m| m != id);
            return;
        };
        s.links.insert(id, link);
        queue(Event::Callback(LOBBY_CHAT_UPDATE, chat_update(lobby, id, MEMBER_ENTERED)));
    }
    crate::info!("co-op: a player joined ({id:#x})");
    crate::session_players::player_joined(id);
    while let Some((kind, body)) = read_frame(&mut stream) {
        if kind == LOOK {
            // The player's own character: kept, and passed on to the others.
            let mut guard = session();
            let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { break };
            if u64_at(&body, 0) == Some(id) {
                s.looks.insert(id, body[8..].to_vec());
                for (other, l) in s.links.iter_mut() {
                    if *other != id {
                        let _ = l.write_all(&frame(LOOK, &body));
                    }
                }
                crate::session_players::look_arrived(id);
            }
            continue;
        }
        if kind == ARRIVED {
            crate::session_players::player_arrived(id);
            continue;
        }
        if kind == MOD {
            // From this player whatever it says; kept here or passed on to whom it's for.
            let Some(mut m) = ModFrame::parse(&body) else { continue };
            m.from = id;
            let mut guard = session();
            let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { break };
            if m.to != 0 && m.to != s.me {
                let f = frame(MOD, &m.encode());
                for (other, l) in s.links.iter_mut() {
                    if *other != id && (m.to == MOD_ALL || m.to == *other) {
                        let _ = l.write_all(&f);
                    }
                }
            }
            if m.to == 0 || m.to == s.me || m.to == MOD_ALL {
                drop(guard);
                mod_arrived(m);
            }
            continue;
        }
        if kind == REPORT_NOW {
            let reason = String::from_utf8_lossy(&body).into_owned();
            crate::watchdog::asked(&reason);
            let mut guard = session();
            if let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) {
                let f = frame(REPORT_NOW, &body);
                for (other, l) in s.links.iter_mut() {
                    if *other != id {
                        let _ = l.write_all(&f);
                    }
                }
            }
            continue;
        }
        if kind == REPORT_TEXT {
            if let Some(&n) = body.first()
                && let Some(name) = body.get(1..1 + n as usize)
            {
                let text = body.get(1 + n as usize..).unwrap_or(&[]);
                crate::watchdog::text_arrived(id, &String::from_utf8_lossy(name), &String::from_utf8_lossy(text));
            }
            continue;
        }
        if kind == PING {
            let mut guard = session();
            if let Some(l) = guard.as_mut().filter(|s| s.lobby == lobby).and_then(|s| s.links.get_mut(&id)) {
                let _ = l.write_all(&frame(PONG, &body));
            }
            continue;
        }
        if kind == PONG {
            if let Some(t) = u64_at(&body, 0) {
                set_ping(id, now_ms().saturating_sub(t).min(u32::MAX as u64) as u32);
            }
            continue;
        }
        if kind != PACKET {
            continue;
        }
        let (Some(from), Some(to)) = (u64_at(&body, 0), u64_at(&body, 8)) else { break };
        let channel = i32::from_le_bytes(body.get(16..20).and_then(|b| b.try_into().ok()).unwrap_or([0; 4]));
        let mut guard = session();
        let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { break };
        if from != id {
            continue;
        }
        if to == s.me {
            deliver(s, from, channel, body[20..].to_vec());
        } else if let Some(l) = s.links.get_mut(&to) {
            let _ = l.write_all(&frame(PACKET, &body));
        }
    }
    let mut guard = session();
    if let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) {
        s.links.remove(&id);
        s.forget(id);
        let member = frame(MEMBER, &[&id.to_le_bytes()[..], &[0]].concat());
        for l in s.links.values_mut() {
            let _ = l.write_all(&member);
        }
        queue(Event::Callback(LOBBY_CHAT_UPDATE, chat_update(lobby, id, MEMBER_LEFT)));
    }
    PINGS_MS.lock().unwrap_or_else(|e| e.into_inner()).retain(|e| e.0 != id);
    crate::info!("co-op: a player left ({id:#x})");
    crate::session_players::player_left(id);
}

/// Why a connection to a host didn't make a session.
#[derive(Debug, Clone)]
pub enum ConnectError {
    /// The host turned this player away (REFUSE): why, as the host says.
    Refused(String),
    Other(String),
}

impl From<String> for ConnectError {
    fn from(e: String) -> Self {
        ConnectError::Other(e)
    }
}

impl From<&str> for ConnectError {
    fn from(e: &str) -> Self {
        ConnectError::Other(e.to_string())
    }
}

/// connect, telling a host's refusal apart from the rest.
pub fn connect_detailed(address: SocketAddrV4, key: Option<u32>) -> Result<(u64, Option<Location>), ConnectError> {
    if session().is_some() {
        return Err("there's a session already".into());
    }
    let me = my_id().ok_or("the game's player id isn't there")?;
    parts_reset();
    let mut stream = TcpStream::connect_timeout(&address.into(), CONNECT_TIMEOUT).map_err(|e| ConnectError::Other(format!("couldn't reach {address}: {e}")))?;
    let _ = stream.set_nodelay(true);
    // Who it is, which DLCs it has (the host refuses a player missing one its world needs), and
    // the session's key.
    stream.write_all(&frame(HELLO, &[&me.to_le_bytes()[..], &[dlc_files()], &key.unwrap_or(0).to_le_bytes()].concat())).map_err(|e| e.to_string())?;
    let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
    let body = match read_frame(&mut stream) {
        Some((WELCOME, body)) => body,
        Some((REFUSE, why)) => return Err(ConnectError::Refused(String::from_utf8_lossy(&why).into_owned())),
        _ => return Err("the host didn't answer".into()),
    };
    let _ = stream.set_read_timeout(None);
    let (lobby, owner) = (u64_at(&body, 0).ok_or("bad answer")?, u64_at(&body, 8).ok_or("bad answer")?);
    let n = u32::from_le_bytes(body.get(16..20).and_then(|b| b.try_into().ok()).ok_or("bad answer")?) as usize;
    let members: Vec<u64> = (0..n).filter_map(|i| u64_at(&body, 20 + 8 * i)).collect();
    let rest = 20 + 8 * n;
    let location = (body.get(rest) == Some(&1)).then(|| body.get(rest + 1..).and_then(Location::from_bytes)).flatten();
    if owner == me {
        return Err("the host has the same player id as this game".into());
    }
    let others: Vec<u64> = members.iter().copied().filter(|&m| m != me && m != owner).collect();
    let reader = stream.try_clone().map_err(|e| e.to_string())?;
    let mut links = HashMap::new();
    links.insert(owner, Link::new(stream).map_err(|e| e.to_string())?);
    // The code this player joined with (code()): the host's address and key.
    *session() = Some(Session {
        host: false,
        lobby,
        me,
        owner,
        members,
        links,
        inbox: VecDeque::new(),
        requested: HashSet::new(),
        accepted: HashSet::new(),
        dropped: HashSet::new(),
        left: false,
        address: Some(address),
        key: key.unwrap_or(0),
        looks: HashMap::new(),
        welcome: HashMap::new(),
        entered: false,
        enter_call: None,
    });
    // This player's character, for the others to see.
    if let Some(own) = crate::session_players::own_look()
        && let Some(l) = session().as_mut().and_then(|s| s.links.get_mut(&owner))
    {
        let _ = l.write_all(&look_frame(me, &own));
    }
    // The other players already here (the host's character comes at the game's matching check).
    for m in others {
        crate::session_players::player_joined(m);
    }
    let _ = std::thread::Builder::new().name("kindle-coop-link".into()).spawn(move || player_link(reader, lobby, owner));
    PINGS_MS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    spawn_pinger(lobby);
    crate::info!("co-op: joined {address} (host at {location:?})");
    Ok((lobby, location))
}

/// Whether a join's error says the host isn't there (nothing answers at its address), rather
/// than that it refuses for now.
pub fn host_gone(error: &str) -> bool {
    error.starts_with("couldn't reach") || error == "the host didn't answer"
}

/// A player's side of its link to the host.
fn player_link(mut stream: TcpStream, lobby: u64, owner: u64) {
    while let Some((kind, body)) = read_frame(&mut stream) {
        let mut guard = session();
        let Some(s) = guard.as_mut().filter(|s| s.lobby == lobby) else { return };
        match kind {
            PACKET => {
                let (Some(from), Some(_to)) = (u64_at(&body, 0), u64_at(&body, 8)) else { continue };
                let channel = i32::from_le_bytes(body.get(16..20).and_then(|b| b.try_into().ok()).unwrap_or([0; 4]));
                deliver(s, from, channel, body[20..].to_vec());
            }
            LOOK => {
                if let Some(who) = u64_at(&body, 0) {
                    s.looks.insert(who, body[8..].to_vec());
                    crate::session_players::look_arrived(who);
                }
            }
            PING => {
                if let Some(l) = s.links.get_mut(&owner) {
                    let _ = l.write_all(&frame(PONG, &body));
                }
            }
            PONG => {
                if let Some(t) = u64_at(&body, 0) {
                    set_ping(owner, now_ms().saturating_sub(t).min(u32::MAX as u64) as u32);
                }
            }
            PINGS => {
                for c in body.as_chunks::<12>().0 {
                    let (id, ms) = (u64::from_le_bytes(c[..8].try_into().expect("8 bytes")), u32::from_le_bytes(c[8..].try_into().expect("4 bytes")));
                    if id != s.me {
                        set_ping(id, ms);
                    }
                }
            }
            MOD => {
                if let Some(m) = ModFrame::parse(&body).filter(|m| m.to == MOD_ALL || m.to == s.me) {
                    mod_arrived(m);
                }
            }
            REPORT_NOW => crate::watchdog::asked(&String::from_utf8_lossy(&body)),
            WELCOME_DATA => {
                if let Some((mod_id, channel, data)) = welcome_from(&body) {
                    if data.is_empty() {
                        s.welcome.remove(&(mod_id, channel));
                    } else {
                        s.welcome.insert((mod_id, channel), data);
                    }
                }
            }
            MEMBER => {
                let (Some(id), Some(&joined)) = (u64_at(&body, 0), body.get(8)) else { continue };
                if joined == 1 {
                    if !s.members.contains(&id) {
                        s.members.push(id);
                    }
                    queue(Event::Callback(LOBBY_CHAT_UPDATE, chat_update(lobby, id, MEMBER_ENTERED)));
                    if id != s.me {
                        crate::session_players::player_joined(id);
                    }
                } else {
                    s.forget(id);
                    queue(Event::Callback(LOBBY_CHAT_UPDATE, chat_update(lobby, id, MEMBER_LEFT)));
                    crate::session_players::player_left(id);
                }
            }
            _ => {}
        }
    }
    let mut guard = session();
    if guard.as_ref().is_some_and(|s| s.lobby == lobby) {
        *guard = None;
        queue(Event::Callback(LOBBY_CHAT_UPDATE, chat_update(lobby, owner, MEMBER_LEFT)));
        queue(Event::Callback(P2P_SESSION_CONNECT_FAIL, payload(&[&owner.to_le_bytes(), &[SESSION_ERROR_TIMEOUT]], 9)));
        crate::info!("co-op: the link to the host closed");
        crate::session_players::session_ended();
        crate::session::ended(false);
    }
}

/// Ends Kindle's session (links close; the other side sees this player leave).
pub fn leave() {
    crate::session_players::session_ended();
    if let Some(s) = session().take() {
        for l in s.links.values() {
            let _ = l.shutdown(std::net::Shutdown::Both);
        }
        if s.host {
            // Wakes the accept loop so it sees the session is gone.
            let _ = TcpStream::connect_timeout(&SocketAddrV4::new(Ipv4Addr::LOCALHOST, port()).into(), Duration::from_millis(200));
        }
    }
}

/// A player's character as its Kindle sent it (session_players.rs's packing).
pub fn look(who: u64) -> Option<Vec<u8>> {
    session().as_ref().and_then(|s| s.looks.get(&who).cloned())
}

/// Sends this player's character again (it changed): a host to every player, a player to the
/// host, who passes it on.
pub fn send_look(look: &[u8]) {
    let mut guard = session();
    let Some(s) = guard.as_mut() else { return };
    let f = look_frame(s.me, look);
    let to: Vec<u64> = if s.host { s.links.keys().copied().collect() } else { vec![s.owner] };
    for id in to {
        if let Some(l) = s.links.get_mut(&id) {
            let _ = l.write_all(&f);
        }
    }
}

/// A player in a session: the host's mod `mod_id`'s join data on `channel` (JSON), once it came.
pub fn welcome(mod_id: &str, channel: &str) -> Option<Vec<u8>> {
    session().as_ref().filter(|s| !s.host).and_then(|s| s.welcome.get(&(mod_id.to_string(), channel.to_string())).cloned())
}

/// A mod's message (MOD).
#[derive(Debug, Clone, PartialEq)]
pub struct ModFrame {
    pub mod_id: String,
    pub channel: String,
    pub from: u64,
    pub to: u64,
    pub data: Vec<u8>,
}

impl ModFrame {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let (id, ch) = (self.mod_id.as_bytes(), self.channel.as_bytes());
        [&[id.len() as u8][..], id, &[ch.len() as u8], ch, &self.from.to_le_bytes(), &self.to.to_le_bytes(), &self.data].concat()
    }

    pub(crate) fn parse(body: &[u8]) -> Option<Self> {
        let text = |at: usize| {
            let n = *body.get(at)? as usize;
            Some((String::from_utf8(body.get(at + 1..at + 1 + n)?.to_vec()).ok()?, at + 1 + n))
        };
        let (mod_id, at) = text(0)?;
        let (channel, at) = text(at)?;
        let (from, to) = (u64_at(body, at)?, u64_at(body, at + 8)?);
        let data = body.get(at + 16..)?.to_vec();
        (data.len() <= MOD_MAX_BYTES).then_some(Self { mod_id, channel, from, to, data })
    }
}

static MOD_INBOX: std::sync::Mutex<Vec<ModFrame>> = std::sync::Mutex::new(Vec::new());

pub(crate) fn mod_arrived(m: ModFrame) {
    let mut inbox = MOD_INBOX.lock().unwrap_or_else(|e| e.into_inner());
    if inbox.len() < MOD_INBOX_MAX {
        inbox.push(m);
    } else {
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| crate::warn!("co-op: mods' messages are arriving faster than they're read; some are dropped"));
    }
}

/// The mods' messages that arrived since the last call, in order.
pub fn take_mod_messages() -> Vec<ModFrame> {
    std::mem::take(&mut *MOD_INBOX.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Sends a mod's message to the same mod on other games of the session (of the lobby without
/// one, lobby.rs): `to` 0 the host, MOD_ALL every other game, or a member's id. A player's goes
/// through the host.
pub fn send_mod(mod_id: &str, channel: &str, to: u64, data: &[u8]) -> Result<(), String> {
    if mod_id.len() > 255 || channel.is_empty() || channel.len() > 255 {
        return Err("a channel is 1 to 255 bytes".into());
    }
    if data.len() > MOD_MAX_BYTES {
        return Err(format!("a message is at most {MOD_MAX_BYTES} bytes (this one is {})", data.len()));
    }
    let mut guard = session();
    let Some(s) = guard.as_mut() else {
        drop(guard);
        return crate::lobby::send_mod(mod_id, channel, to, data);
    };
    if to == s.me || (to == 0 && s.host) {
        return Err("that's this game".into());
    }
    if to != 0 && to != MOD_ALL && !s.members.contains(&to) {
        return Err(format!("{to:#x} isn't in the session"));
    }
    let m = ModFrame { mod_id: mod_id.to_string(), channel: channel.to_string(), from: s.me, to, data: data.to_vec() };
    let f = frame(MOD, &m.encode());
    if s.host {
        for (id, l) in s.links.iter_mut() {
            if to == MOD_ALL || to == *id {
                let _ = l.write_all(&f);
            }
        }
    } else {
        let owner = s.owner;
        if let Some(l) = s.links.get_mut(&owner) {
            let _ = l.write_all(&f);
        }
    }
    Ok(())
}

/// The host's id (a player's session), this game's when it hosts.
pub fn host_id() -> Option<u64> {
    session().as_ref().map(|s| if s.host { s.me } else { s.owner })
}

/// This game's Steam id in the session.
pub fn me() -> Option<u64> {
    session().as_ref().map(|s| s.me)
}

/// Asks the other games of the session for a report (REPORT_NOW): the host asks its players, a
/// player the host, which passes it on.
pub fn send_report_now(reason: &str) {
    let f = frame(REPORT_NOW, reason.as_bytes());
    if let Some(s) = session().as_mut() {
        if s.host {
            for l in s.links.values_mut() {
                let _ = l.write_all(&f);
            }
        } else {
            let owner = s.owner;
            if let Some(l) = s.links.get_mut(&owner) {
                let _ = l.write_all(&f);
            }
        }
    }
}

/// A player: sends a report or its crash.log to the host (REPORT_TEXT).
pub fn send_report_text(name: &str, text: &str) {
    let name = &name.as_bytes()[..name.len().min(255)];
    let text = &text.as_bytes()[..text.len().min(MAX_FRAME - 512)];
    let body = [&[name.len() as u8][..], name, text].concat();
    if let Some(s) = session().as_mut().filter(|s| !s.host) {
        let owner = s.owner;
        if let Some(l) = s.links.get_mut(&owner) {
            let _ = l.write_all(&frame(REPORT_TEXT, &body));
        }
    }
}

/// A player: tells the host it's in the host's world (ARRIVED).
pub fn send_arrived() {
    if let Some(s) = session().as_mut().filter(|s| !s.host) {
        let owner = s.owner;
        if let Some(l) = s.links.get_mut(&owner) {
            let _ = l.write_all(&frame(ARRIVED, &[]));
        }
    }
}

/// This game's mods' join data as a host: (mod, channel, JSON).
static WELCOMES: std::sync::Mutex<Vec<(String, String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

/// Sets (None: removes) a mod's join data on `channel`: given to each player as it joins, and to
/// the players in now.
pub fn set_welcome(mod_id: &str, channel: &str, data: Option<Vec<u8>>) -> Result<(), String> {
    if mod_id.len() > 255 || channel.is_empty() || channel.len() > 255 {
        return Err("a channel is 1 to 255 bytes".into());
    }
    if data.as_ref().is_some_and(|d| d.len() > MOD_MAX_BYTES) {
        return Err(format!("join data is at most {MOD_MAX_BYTES} bytes"));
    }
    {
        let mut all = WELCOMES.lock().unwrap_or_else(|e| e.into_inner());
        all.retain(|(m, c, _)| !(m == mod_id && c == channel));
        if let Some(d) = &data {
            all.push((mod_id.to_string(), channel.to_string(), d.clone()));
        }
    }
    if let Some(s) = session().as_mut().filter(|s| s.host) {
        let f = welcome_frame(mod_id, channel, data.as_deref().unwrap_or_default());
        for l in s.links.values_mut() {
            let _ = l.write_all(&f);
        }
    }
    Ok(())
}

/// A mod unloads: its join data goes (players in now keep what they had).
pub fn remove_welcomes(mod_id: &str) {
    WELCOMES.lock().unwrap_or_else(|e| e.into_inner()).retain(|(m, _, _)| m != mod_id);
    REQUIRED_DLC.lock().unwrap_or_else(|e| e.into_inner()).retain(|(m, _)| m != mod_id);
}

fn welcome_frame(mod_id: &str, channel: &str, data: &[u8]) -> Vec<u8> {
    frame(WELCOME_DATA, &[&[mod_id.len() as u8][..], mod_id.as_bytes(), &[channel.len() as u8], channel.as_bytes(), data].concat())
}

fn welcome_from(body: &[u8]) -> Option<(String, String, Vec<u8>)> {
    let id_len = *body.first()? as usize;
    let mod_id = String::from_utf8(body.get(1..1 + id_len)?.to_vec()).ok()?;
    let at = 1 + id_len;
    let ch_len = *body.get(at)? as usize;
    let channel = String::from_utf8(body.get(at + 1..at + 1 + ch_len)?.to_vec()).ok()?;
    Some((mod_id, channel, body.get(at + 1 + ch_len..)?.to_vec()))
}

/// The DLCs a host's mods say its world needs (kindle.net.require_dlc): (mod, bits as in
/// dlc_files).
static REQUIRED_DLC: std::sync::Mutex<Vec<(String, u8)>> = std::sync::Mutex::new(Vec::new());

/// Sets what a mod says players joining this host's world need (bits as in dlc_files; 0 none).
pub fn require_dlc(mod_id: &str, bits: u8) {
    let mut all = REQUIRED_DLC.lock().unwrap_or_else(|e| e.into_inner());
    all.retain(|(m, _)| m != mod_id);
    if bits != 0 {
        all.push((mod_id.to_string(), bits));
    }
}

/// The DLCs whose maps this game has (bit 0 Ashes of Ariandel, bit 1 The Ringed City), read once.
pub fn dlc_files() -> u8 {
    static FOUND: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let Some(game_dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::Path::to_path_buf)) else { return 0 };
        let has = |area: &str| crate::archive::read_any(&game_dir, &format!("/map/mapstudio/{area}.msb.dcx")).is_ok();
        (has("m45_00_00_00") as u8) | ((has("m50_00_00_00") as u8) << 1)
    })
}

const DLC_NAMES: [&str; 2] = ["Ashes of Ariandel", "The Ringed City"];

/// Why a player with these DLCs (dlc_files bits) can't come into this host's world, if it can't:
/// the host is loading, the host is in a DLC's area, or a mod of the host's says its world needs
/// a DLC (a randomizer's enemies from one: the player's game couldn't load them). A base-game
/// player and one with the DLCs play together anywhere else.
fn refusal(theirs: u8) -> Option<String> {
    // The game can't take a player while it loads, until its character is in the new world (a
    // player following the host asks again a moment later).
    if !crate::game::in_world() && crate::session::arrival().is_none() {
        return Some("the host is on a loading screen".into());
    }
    let area = crate::session::arrival().map_or(0, |at| match (at.map as u32) >> 24 {
        45 => 1,
        50 | 51 => 2,
        _ => 0,
    });
    let required = REQUIRED_DLC.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let needed = required.iter().fold(0, |bits, (_, b)| bits | b);
    let missing = |need: u8| (0..2).filter(|b| need & !theirs & (1 << b) != 0).map(|b| DLC_NAMES[b]).collect::<Vec<_>>().join(" and ");
    if area & !theirs != 0 {
        return Some(format!("the host is in {}, which this game doesn't have", missing(area)));
    }
    let mods: Vec<&str> = required.iter().filter(|(_, b)| b & !theirs != 0).map(|(m, _)| m.as_str()).collect();
    (needed & !theirs != 0).then(|| format!("the host's world needs {} (mod {}), which this game doesn't have", missing(needed), mods.join(", ")))
}

fn look_frame(who: u64, look: &[u8]) -> Vec<u8> {
    frame(LOOK, &[&who.to_le_bytes()[..], look].concat())
}

pub fn members() -> Option<Vec<u64>> {
    session().as_ref().map(|s| s.members.clone())
}

/// Whether this game hosts the session (None without one).
pub fn hosting() -> Option<bool> {
    session().as_ref().map(|s| s.host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_messages_read_back_and_have_limits() {
        let m = ModFrame { mod_id: "coop".into(), channel: "lit".into(), from: 0x0110_0001_0000_0001, to: MOD_ALL, data: b"[13000009]".to_vec() };
        assert_eq!(ModFrame::parse(&m.encode()), Some(m.clone()));
        // Cut short, or too long.
        let bytes = m.encode();
        assert_eq!(ModFrame::parse(&bytes[..10]), None);
        let big = ModFrame { data: vec![b' '; MOD_MAX_BYTES + 1], ..m };
        assert_eq!(ModFrame::parse(&big.encode()), None);
        // Without a session nothing is sent.
        assert!(send_mod("coop", "lit", MOD_ALL, b"1").is_err());
        assert!(send_mod("coop", "", MOD_ALL, b"1").is_err());
    }
}
