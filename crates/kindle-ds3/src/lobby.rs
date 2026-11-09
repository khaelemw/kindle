//! Kindle's lobby: players gather in it before (and alongside) a session, from the title screen on.
//! A mod opens one (its code names the host's address, as a session's), the others join it with the
//! code; everyone sees who is in it and where they are (at the title, loading, in the world, with
//! their character's name). Mods' messages go between the same mod on the lobby's games while there
//! is no session (kindle.net.send and on), and a host's mod sends each member its party save for
//! the host's world (party_saves.rs); what the member's game writes back is kept by the host. What a
//! lobby is for (co-op: the host opening its world to it, members joining it) is the mod's.
//!
//! The lobby has the co-op port (kindle.toml's [coop] port) from the moment it opens, and its
//! listener tells a session's join (p2p's HELLO as the first frame) from a lobby's: the
//! session's joins are handed to p2p, so one port is still all a host forwards. Frames as
//! p2p's (u32 length, u8 kind, fields), kinds of their own:
//! - HELLO (member to host): u64 id, u32 the lobby's key, u8 place, the name (UTF-8);
//! - WELCOME (host to member): u64 the host's id; REFUSE: why (UTF-8);
//! - STATUS (member to host): u8 place, the name;
//! - MEMBERS (host to all): u32 count, then per member u64 id, u8 place, u16 name length, name;
//! - SAVE_PART / SAVE_DONE (host to member): its party save for the host's world, in parts (u32
//!   offset, bytes), then u32 its length (0: the host has none yet);
//! - SAVED_PART / SAVED_DONE (member to host): each time the member's game wrote it, the pieces
//!   that changed since the host's copy (u32 offset, bytes), then u32 its length and u64 its
//!   checksum (FNV-1a); the host puts them on its copy and checks the sum;
//! - RESEND (host to member): the pieces didn't make the member's save (its copy wasn't the one the
//!   member thought): the next goes whole;
//! - MOD (either way): a mod's message, as p2p's MOD frame; a member's go through the host.
//!
//! Each link has a writer thread taking whole frames in order, so a save's parts never mix with
//! the other frames and the game's thread never waits on the network.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const HELLO: u8 = 100;
const WELCOME: u8 = 101;
const REFUSE: u8 = 102;
const STATUS: u8 = 103;
const MEMBERS: u8 = 104;
// 105 was WORLD (the host's session code), now the co-op mod's own message. Not to be used again.
const SAVE_PART: u8 = 106;
const SAVE_DONE: u8 = 107;
const SAVED_PART: u8 = 108;
const SAVED_DONE: u8 = 109;
const RESEND: u8 = 110;
const MOD: u8 = 111;
/// A save's part (within p2p's frame limit).
const PART: usize = 512 * 1024;
/// A member's save goes to the host in pieces of this size, only those that changed since the
/// host's copy: the game rewrites an entry or two (a character's slot, the profile) of the 9 MB
/// file at a save, several times as a character loads, and each write went whole.
const PIECE: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

/// FNV-1a over a save, for the host to check the pieces made the member's.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// A member's save for the host: the pieces that differ from `base` (all of it without one), then
/// its length and checksum.
fn diff_frames(base: Option<&[u8]>, file: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (k, piece) in file.chunks(PIECE).enumerate() {
        let at = k * PIECE;
        if base.and_then(|b| b.get(at..at + piece.len())) != Some(piece) {
            out.push(crate::p2p::frame(SAVED_PART, &[&(at as u32).to_le_bytes()[..], piece].concat()));
        }
    }
    out.push(crate::p2p::frame(SAVED_DONE, &[&(file.len() as u32).to_le_bytes()[..], &checksum(file).to_le_bytes()[..]].concat()));
    out
}

/// What a link's writer thread is given: a frame, or a flag to clear once what came before is sent.
enum Out {
    Frame(Vec<u8>),
    Sent(Arc<AtomicBool>),
}

struct Link {
    id: u64,
    out: Sender<Out>,
    stream: TcpStream,
}

/// A link to `id` over `stream`, with its writer thread.
fn link(id: u64, stream: TcpStream) -> Option<Link> {
    let mut writer = stream.try_clone().ok()?;
    let (out, rx) = std::sync::mpsc::channel::<Out>();
    std::thread::Builder::new()
        .name("kindle-lobby-out".into())
        .spawn(move || {
            for o in rx {
                match o {
                    Out::Frame(f) => {
                        if writer.write_all(&f).is_err() {
                            break;
                        }
                    }
                    Out::Sent(flag) => flag.store(false, Ordering::Relaxed),
                }
            }
        })
        .ok()?;
    Some(Link { id, out, stream })
}

/// A save's frames: its parts, then its length (0 for none).
fn save_frames(part: u8, done: u8, file: Option<&[u8]>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let bytes = file.unwrap_or_default();
    for (k, chunk) in bytes.chunks(PART).enumerate() {
        out.push(crate::p2p::frame(part, &[&((k * PART) as u32).to_le_bytes()[..], chunk].concat()));
    }
    out.push(crate::p2p::frame(done, &(bytes.len() as u32).to_le_bytes()));
    out
}

/// A save coming in parts, put together.
#[derive(Default)]
struct Incoming(Vec<u8>);

impl Incoming {
    fn part(&mut self, body: &[u8]) {
        let Some(at) = body.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize) else { return };
        let bytes = &body[4..];
        if self.0.len() < at + bytes.len() {
            self.0.resize(at + bytes.len(), 0);
        }
        self.0[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// The whole save at its end (None: none, or not all of it came).
    fn done(&mut self, body: &[u8]) -> Option<Vec<u8>> {
        let len = body.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize)?;
        let got = std::mem::take(&mut self.0);
        (len > 0 && got.len() == len).then_some(got)
    }
}

/// Where a member is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    Title = 0,
    Loading = 1,
    World = 2,
}

impl Place {
    fn from_byte(b: u8) -> Self {
        match b {
            2 => Place::World,
            1 => Place::Loading,
            _ => Place::Title,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Place::Title => "title",
            Place::Loading => "loading",
            Place::World => "world",
        }
    }
}

#[derive(Clone)]
pub struct Member {
    pub id: u64,
    pub name: String,
    pub place: Place,
}

struct Lobby {
    host: bool,
    me: u64,
    /// The lobby's key, in its code (as a session's): a member from outside the host's network is
    /// taken only with it.
    key: u32,
    /// Host: the address and key its code names.
    code: Option<String>,
    /// Everyone, the host first.
    members: Vec<Member>,
    /// Host: a link per member; a member: the host's.
    links: Vec<Link>,
    /// What this game last told the lobby about itself.
    told: Option<(Place, String)>,
    /// Host: the world each member was last sent its party save for (what comes back is kept there).
    save_worlds: Vec<(u64, String)>,
    /// A member: its party save is on its way to the host (cleared once sent).
    uploading: Arc<AtomicBool>,
    /// A member: its game wrote its party save while one was on its way: send it after.
    upload_due: bool,
    /// A member: its party save as the host has it (what it sent, or the last one sent to it),
    /// which the next one is sent against.
    sent_base: Arc<Mutex<Option<Vec<u8>>>>,
}

static LOBBY: Mutex<Option<Lobby>> = Mutex::new(None);
/// The lobby's listener is up (p2p's host then leaves the port to it).
static LISTENING: AtomicBool = AtomicBool::new(false);

fn lobby() -> MutexGuard<'static, Option<Lobby>> {
    LOBBY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether the lobby's listener has the co-op port.
pub fn listening() -> bool {
    LISTENING.load(Ordering::Relaxed)
}

/// The lobby's members (empty without one).
pub fn members() -> Vec<Member> {
    lobby().as_ref().map(|p| p.members.clone()).unwrap_or_default()
}

/// Whether this game is in a lobby, and hosts it.
pub fn hosting() -> Option<bool> {
    lobby().as_ref().map(|p| p.host)
}

/// The lobby's code (host).
pub fn code() -> Option<String> {
    lobby().as_ref().and_then(|p| p.code.clone())
}

/// This game's name in the lobby: its character's, else (at the title) "Player" and the end of
/// its player id. Never the PC's user name: the lobby shows it to everyone in it.
fn my_name(me: u64) -> String {
    if crate::game::player_loaded()
        && let Ok(d) = crate::game::player_game_data()
    {
        let n = d.player_info().name();
        if !n.is_empty() {
            return n;
        }
    }
    format!("Player {:04X}", me & 0xFFFF)
}

fn my_place() -> Place {
    if crate::game::in_world() {
        Place::World
    } else if crate::game::player_loaded() {
        Place::Loading
    } else {
        Place::Title
    }
}

fn new_lobby(host: bool, me: u64, key: u32, code: Option<String>, links: Vec<Link>, told: Option<(Place, String)>) -> Lobby {
    Lobby {
        host,
        me,
        key,
        code,
        members: if host { vec![Member { id: me, name: my_name(me), place: my_place() }] } else { Vec::new() },
        links,
        told,
        save_worlds: Vec::new(),
        uploading: Arc::new(AtomicBool::new(false)),
        upload_due: false,
        sent_base: Arc::new(Mutex::new(None)),
    }
}

/// Opens a lobby: this game hosts it. Its code is returned (and its listener has the port).
pub fn open() -> Result<String, String> {
    if lobby().is_some() {
        return Err("this game is in a lobby already".into());
    }
    if crate::p2p::hosting().is_some() {
        return Err("leave the session first".into());
    }
    let me = crate::p2p::my_id().ok_or("the game's player id isn't there")?;
    let port = crate::p2p::port();
    let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|e| format!("port {port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let key = crate::p2p::rand_u32() | 1;
    let code = crate::p2p::code_for(crate::p2p::reachable_address(), key);
    *lobby() = Some(new_lobby(true, me, key, Some(code.clone()), Vec::new(), None));
    LISTENING.store(true, Ordering::Relaxed);
    let spawned = std::thread::Builder::new().name("kindle-lobby".into()).spawn(move || accept_loop(listener));
    if let Err(e) = spawned {
        LISTENING.store(false, Ordering::Relaxed);
        *lobby() = None;
        return Err(e.to_string());
    }
    crate::info!("lobby: hosting on port {port}");
    Ok(code)
}

/// The host's listener: lobby members and the session's joins, until the lobby ends.
fn accept_loop(listener: TcpListener) {
    while lobby().as_ref().is_some_and(|p| p.host) {
        match listener.accept() {
            Ok((stream, from)) => {
                let _ = std::thread::Builder::new().name("kindle-lobby-in".into()).spawn(move || arrived(stream, from));
            }
            // Non-blocking: nothing waiting (the loop also notices the lobby's end).
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                crate::warn!("lobby: the listener stopped ({e})");
                break;
            }
        }
    }
    LISTENING.store(false, Ordering::Relaxed);
}

/// A connection to the host's port: a session's join goes to the session; a member's HELLO makes
/// it a member.
fn arrived(mut stream: TcpStream, from: SocketAddr) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let Some((kind, body)) = crate::p2p::read_frame(&mut stream) else { return };
    if kind == crate::p2p::SESSION_HELLO {
        return crate::p2p::session_join(stream, body);
    }
    if kind != HELLO {
        return;
    }
    let Some(id) = crate::p2p::u64_at(&body, 0) else { return };
    let key = body.get(8..12).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")));
    let place = Place::from_byte(body.get(12).copied().unwrap_or(0));
    let name = String::from_utf8_lossy(body.get(13..).unwrap_or_default()).into_owned();
    let local = matches!(from, SocketAddr::V4(a) if crate::p2p::is_local(*a.ip()));
    let refuse = |mut s: TcpStream, why: &str| {
        let _ = s.write_all(&crate::p2p::frame(REFUSE, why.as_bytes()));
    };
    let Ok(reader) = stream.try_clone() else { return };
    {
        let mut guard = lobby();
        let Some(p) = guard.as_mut().filter(|p| p.host) else { return refuse(stream, "the lobby is closed") };
        if !local && key != Some(p.key) {
            crate::info!("lobby: someone from outside this network without the lobby's code ({id:#x})");
            return refuse(stream, "the code doesn't match: copy it again");
        }
        if p.members.iter().any(|m| m.id == id) {
            return refuse(stream, "someone with your player id is in the lobby already");
        }
        if stream.write_all(&crate::p2p::frame(WELCOME, &p.me.to_le_bytes())).is_err() {
            return;
        }
        let Some(l) = link(id, stream) else { return };
        crate::info!("lobby: {name} joined");
        p.members.push(Member { id, name, place });
        p.links.push(l);
        broadcast_members(p);
    }
    member_link(reader, id);
}

/// The host's side of a member's link: its status, its save and its mod messages, until it goes.
fn member_link(mut stream: TcpStream, id: u64) {
    // A save coming in: the host's copy with the pieces that came put on it.
    let mut save: Option<Vec<u8>> = None;
    let world = || lobby().as_ref().and_then(|p| p.save_worlds.iter().find(|(m, _)| *m == id).map(|(_, w)| w.clone()));
    let copy = |w: &Option<String>| w.as_deref().and_then(|w| crate::party_saves::copy_of(w, id)).unwrap_or_default();
    while let Some((kind, body)) = crate::p2p::read_frame(&mut stream) {
        if kind == SAVED_PART {
            let Some(at) = body.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize) else { continue };
            let file = save.get_or_insert_with(|| copy(&world()));
            let piece = &body[4..];
            if file.len() < at + piece.len() {
                file.resize(at + piece.len(), 0);
            }
            file[at..at + piece.len()].copy_from_slice(piece);
        } else if kind == SAVED_DONE {
            let w = world();
            let changed = save.is_some();
            let mut file = save.take().unwrap_or_else(|| copy(&w));
            let Some(len) = body.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")) as usize) else { continue };
            file.resize(len, 0);
            let sum = body.get(4..12).map(|b| u64::from_le_bytes(b.try_into().expect("8 bytes")));
            if len == 0 || sum.is_some_and(|s| s != checksum(&file)) {
                crate::warn!("lobby: {id:#x}'s save didn't fit this copy; asking for all of it");
                if let Some(p) = lobby().as_mut() {
                    send_to(p, id, &crate::p2p::frame(RESEND, &[]));
                }
                continue;
            }
            match w {
                Some(w) if changed => crate::party_saves::keep(&w, id, &file),
                Some(_) => {}
                None => crate::warn!("lobby: a member's save came before it was sent one; not kept"),
            }
        } else if kind == STATUS {
            let place = Place::from_byte(body.first().copied().unwrap_or(0));
            let name = String::from_utf8_lossy(body.get(1..).unwrap_or_default()).into_owned();
            let mut guard = lobby();
            let Some(p) = guard.as_mut() else { return };
            if let Some(m) = p.members.iter_mut().find(|m| m.id == id) {
                (m.place, m.name) = (place, name);
            }
            broadcast_members(p);
        } else if kind == MOD {
            // From this member whatever it says; kept here or passed on to whom it's for.
            let Some(mut m) = crate::p2p::ModFrame::parse(&body) else { continue };
            m.from = id;
            let mut guard = lobby();
            let Some(p) = guard.as_mut() else { return };
            if m.to != 0 && m.to != p.me {
                let f = crate::p2p::frame(MOD, &m.encode());
                for l in &p.links {
                    if l.id != id && (m.to == crate::p2p::MOD_ALL || m.to == l.id) {
                        let _ = l.out.send(Out::Frame(f.clone()));
                    }
                }
            }
            if m.to == 0 || m.to == p.me || m.to == crate::p2p::MOD_ALL {
                drop(guard);
                crate::p2p::mod_arrived(m);
            }
        }
    }
    let mut guard = lobby();
    let Some(p) = guard.as_mut().filter(|p| p.host) else { return };
    let name = p.members.iter().find(|m| m.id == id).map(|m| m.name.clone()).unwrap_or_default();
    p.members.retain(|m| m.id != id);
    p.links.retain(|l| l.id != id);
    p.save_worlds.retain(|(m, _)| *m != id);
    broadcast_members(p);
    drop(guard);
    crate::info!("lobby: {name} left");
}

fn send_to(p: &mut Lobby, id: u64, f: &[u8]) {
    if let Some(l) = p.links.iter().find(|l| l.id == id) {
        let _ = l.out.send(Out::Frame(f.to_vec()));
    }
}

fn broadcast(p: &mut Lobby, f: &[u8]) {
    for l in &p.links {
        let _ = l.out.send(Out::Frame(f.to_vec()));
    }
}

fn broadcast_members(p: &mut Lobby) {
    let mut body = (p.members.len() as u32).to_le_bytes().to_vec();
    for m in &p.members {
        body.extend_from_slice(&m.id.to_le_bytes());
        body.push(m.place as u8);
        let name = m.name.as_bytes();
        body.extend_from_slice(&(name.len().min(u16::MAX as usize) as u16).to_le_bytes());
        body.extend_from_slice(&name[..name.len().min(u16::MAX as usize)]);
    }
    let f = crate::p2p::frame(MEMBERS, &body);
    broadcast(p, &f);
}

fn read_members(body: &[u8]) -> Vec<Member> {
    let mut out = Vec::new();
    let n = body.get(..4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes"))).unwrap_or(0);
    let mut at = 4;
    for _ in 0..n {
        let (Some(id), Some(&place), Some(len)) = (crate::p2p::u64_at(body, at), body.get(at + 8), body.get(at + 9..at + 11)) else { break };
        let len = u16::from_le_bytes(len.try_into().expect("2 bytes")) as usize;
        let Some(name) = body.get(at + 11..at + 11 + len) else { break };
        out.push(Member { id, name: String::from_utf8_lossy(name).into_owned(), place: Place::from_byte(place) });
        at += 11 + len;
    }
    out
}

// ── joining ──────────────────────────────────────────────────────────────────────────────────

/// How a mod's join of a lobby ended (join_later).
pub enum JoinOutcome {
    Joined,
    /// The host turned it away: why.
    Refused(String),
    Failed(String),
}

/// A join under way (join_later): who asked, and how its connection ended once its thread knows.
struct PendingJoin {
    owner: String,
    handle: u64,
    outcome: Option<JoinOutcome>,
}

static PENDING: Mutex<Option<PendingJoin>> = Mutex::new(None);

fn pending() -> MutexGuard<'static, Option<PendingJoin>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Connects to a lobby (blocks up to CONNECT_TIMEOUT; off the game's thread, with this game's id,
/// name and place read on it).
fn connect(code: &str, me: u64, name: String, place: Place) -> Result<(), JoinOutcome> {
    if lobby().is_some() {
        return Err(JoinOutcome::Failed("this game is in a lobby already".into()));
    }
    if crate::p2p::hosting().is_some() {
        return Err(JoinOutcome::Failed("leave the session first".into()));
    }
    let (address, key) = crate::p2p::parse(code).ok_or_else(|| JoinOutcome::Failed("that isn't a lobby code".into()))?;
    let failed = |e: String| JoinOutcome::Failed(e);
    let mut stream = TcpStream::connect_timeout(&address.into(), CONNECT_TIMEOUT).map_err(|e| failed(format!("couldn't reach {address}: {e}")))?;
    let _ = stream.set_nodelay(true);
    let hello = [&me.to_le_bytes()[..], &key.unwrap_or(0).to_le_bytes(), &[place as u8], name.as_bytes()].concat();
    stream.write_all(&crate::p2p::frame(HELLO, &hello)).map_err(|e| failed(e.to_string()))?;
    let _ = stream.set_read_timeout(Some(CONNECT_TIMEOUT));
    let host = match crate::p2p::read_frame(&mut stream) {
        Some((WELCOME, body)) => crate::p2p::u64_at(&body, 0).ok_or_else(|| failed("bad answer".into()))?,
        Some((REFUSE, why)) => return Err(JoinOutcome::Refused(String::from_utf8_lossy(&why).into_owned())),
        _ => return Err(failed("the host didn't answer".into())),
    };
    let _ = stream.set_read_timeout(None);
    let reader = stream.try_clone().map_err(|e| failed(e.to_string()))?;
    let to_host = link(host, stream).ok_or_else(|| failed("couldn't start the link".into()))?;
    *lobby() = Some(new_lobby(false, me, 0, None, vec![to_host], Some((place, name))));
    std::thread::Builder::new().name("kindle-lobby-link".into()).spawn(move || host_link(reader)).map_err(|e| failed(e.to_string()))?;
    crate::info!("lobby: joined {address}");
    Ok(())
}

/// Starts joining a lobby without blocking the game: Kindle connects on a thread of its own; how
/// it ended comes from finished_joins. One at a time.
pub fn join_later(code: &str, owner: &str, handle: u64) -> Result<(), String> {
    crate::p2p::parse(code).ok_or("that isn't a lobby code")?;
    let me = crate::p2p::my_id().ok_or("the game's player id isn't there")?;
    let (name, place) = (my_name(me), my_place());
    let mut p = pending();
    if p.is_some() {
        return Err("a join is under way already".into());
    }
    *p = Some(PendingJoin { owner: owner.to_string(), handle, outcome: None });
    drop(p);
    let code = code.to_string();
    let spawned = std::thread::Builder::new().name("kindle-lobby-join".into()).spawn(move || {
        let outcome = match connect(&code, me, name, place) {
            Ok(()) => JoinOutcome::Joined,
            Err(e) => e,
        };
        if let Some(p) = pending().as_mut() {
            p.outcome = Some(outcome);
        }
    });
    if spawned.is_err() {
        *pending() = None;
        return Err("couldn't start the join".into());
    }
    Ok(())
}

/// Each frame: a lobby join that ended, who asked and how.
pub fn finished_joins() -> Vec<(String, u64, JoinOutcome)> {
    let mut p = pending();
    if p.as_ref().is_none_or(|j| j.outcome.is_none()) {
        return Vec::new();
    }
    let j = p.take().expect("checked above");
    vec![(j.owner, j.handle, j.outcome.expect("checked above"))]
}

/// A member's side of its link to the host: the members, its party save and mod messages.
fn host_link(mut stream: TcpStream) {
    let mut save = Incoming::default();
    while let Some((kind, body)) = crate::p2p::read_frame(&mut stream) {
        match kind {
            SAVE_PART => save.part(&body),
            SAVE_DONE => {
                let file = save.done(&body);
                if let Some(p) = lobby().as_ref() {
                    *p.sent_base.lock().unwrap_or_else(|e| e.into_inner()) = file.clone();
                }
                crate::party_saves::arrived(file);
            }
            RESEND => {
                if let Some(p) = lobby().as_mut() {
                    *p.sent_base.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    p.upload_due = true;
                }
            }
            MEMBERS => {
                if let Some(p) = lobby().as_mut() {
                    p.members = read_members(&body);
                }
            }
            MOD => {
                if let Some(m) = crate::p2p::ModFrame::parse(&body) {
                    crate::p2p::mod_arrived(m);
                }
            }
            _ => {}
        }
    }
    if lobby().take().is_some() {
        crate::party_saves::left();
        crate::info!("lobby: the host closed the lobby");
        crate::catalogue::emit(&LOBBY_CLOSED, Vec::new());
    }
}

/// kindle.game: the host closed the lobby this game was in (catalogue.rs).
pub static LOBBY_CLOSED: crate::catalogue::Event =
    crate::catalogue::Event::new("lobby.closed", "The lobby's host closed it (or its link ended) while this game was a member.", &[]);

/// Leaves the lobby (a host closes it: its members' links end).
pub fn leave() {
    let Some(p) = lobby().take() else { return };
    if !p.host {
        crate::party_saves::left();
    }
    for l in p.links {
        let _ = l.stream.shutdown(std::net::Shutdown::Both);
    }
    crate::info!("lobby: left");
}

// ── mods' messages ───────────────────────────────────────────────────────────────────────────

/// Sends a mod's message over the lobby (kindle.net.send without a session): `to` 0 the host,
/// p2p::MOD_ALL every other game, or a member's id. A member's goes through the host.
pub fn send_mod(mod_id: &str, channel: &str, to: u64, data: &[u8]) -> Result<(), String> {
    let mut guard = lobby();
    let p = guard.as_mut().ok_or("no co-op session or lobby")?;
    if to == p.me || (to == 0 && p.host) {
        return Err("that's this game".into());
    }
    if to != 0 && to != crate::p2p::MOD_ALL && !p.members.iter().any(|m| m.id == to) {
        return Err(format!("{to:#x} isn't in the lobby"));
    }
    let m = crate::p2p::ModFrame { mod_id: mod_id.to_string(), channel: channel.to_string(), from: p.me, to, data: data.to_vec() };
    let f = crate::p2p::frame(MOD, &m.encode());
    if p.host {
        for l in &p.links {
            if to == crate::p2p::MOD_ALL || to == l.id {
                let _ = l.out.send(Out::Frame(f.clone()));
            }
        }
    } else if let Some(l) = p.links.first() {
        let _ = l.out.send(Out::Frame(f));
    }
    Ok(())
}

/// This game's id in the lobby, and the host's.
pub fn ids() -> Option<(u64, u64)> {
    lobby().as_ref().map(|p| (p.me, if p.host { p.me } else { p.links.first().map_or(0, |l| l.id) }))
}

// ── party saves ──────────────────────────────────────────────────────────────────────────────

/// The host: sends a member its party save for the host's world (the host's copy, or none yet):
/// the world its character is in, or at the title the one its Continue will load (so the member's
/// is in place before the host loads); keeps what the member's game writes back there. The world it
/// was for.
pub fn send_party_save(member: u64) -> Result<String, String> {
    let world = crate::party_saves::host_world().ok_or("the host's world isn't known: it has played none of its characters with Kindle yet")?;
    let mut guard = lobby();
    let p = guard.as_mut().filter(|p| p.host).ok_or("only the lobby's host sends party saves")?;
    if !p.links.iter().any(|l| l.id == member) {
        return Err(format!("{member:#x} isn't in the lobby"));
    }
    let file = crate::party_saves::copy_of(&world, member);
    crate::info!("lobby: sending {member:#x} its party save for {world} ({})", file.as_ref().map_or("none yet".to_string(), |f| format!("{} bytes", f.len())));
    for f in save_frames(SAVE_PART, SAVE_DONE, file.as_deref()) {
        send_to(p, member, &f);
    }
    p.save_worlds.retain(|(m, _)| *m != member);
    p.save_worlds.push((member, world.clone()));
    Ok(world)
}

/// Each frame (the game's thread): this game's place and name go to the lobby; a member's party
/// save goes to the host as its game writes it.
pub fn frame() {
    let mut guard = lobby();
    let Some(p) = guard.as_mut() else { return };
    let now = (my_place(), my_name(p.me));
    if p.told.as_ref() != Some(&now) {
        p.told = Some(now.clone());
        if p.host {
            if let Some(m) = p.members.iter_mut().find(|m| m.id == p.me) {
                (m.place, m.name) = (now.0, now.1.clone());
            }
            broadcast_members(p);
        } else {
            let f = crate::p2p::frame(STATUS, &[&[now.0 as u8][..], now.1.as_bytes()].concat());
            broadcast(p, &f);
        }
    }
    if !p.host {
        upload_save(p);
    }
}

/// A member: what its game wrote to its party save goes to the host (one at a time: a write
/// while one is on its way is sent after it), the pieces that changed since the host's copy. The
/// file is read and compared off the game's thread.
fn upload_save(p: &mut Lobby) {
    if crate::party_saves::take_written() {
        p.upload_due = true;
    }
    if !p.upload_due || p.uploading.load(Ordering::Relaxed) {
        return;
    }
    let Some(l) = p.links.first() else { return };
    let (out, uploading, base) = (l.out.clone(), p.uploading.clone(), p.sent_base.clone());
    p.uploading.store(true, Ordering::Relaxed);
    p.upload_due = false;
    let spawned = std::thread::Builder::new().name("kindle-lobby-save".into()).spawn(move || {
        let Some(file) = crate::party_saves::read() else {
            uploading.store(false, Ordering::Relaxed);
            return;
        };
        let mut base = base.lock().unwrap_or_else(|e| e.into_inner());
        let frames = diff_frames(base.as_deref(), &file);
        let pieces = frames.len() - 1;
        for f in frames {
            let _ = out.send(Out::Frame(f));
        }
        let _ = out.send(Out::Sent(uploading));
        crate::debug!("lobby: party save sent to the host ({pieces} piece(s) of {})", file.len().div_ceil(PIECE));
        *base = Some(file);
    });
    if spawned.is_err() {
        p.uploading.store(false, Ordering::Relaxed);
        p.upload_due = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pieces in `frames` put on `base`, as the host does; and the length and checksum sent.
    fn apply(mut base: Vec<u8>, frames: &[Vec<u8>]) -> (Vec<u8>, usize, u64) {
        let mut done = (0, 0);
        for f in frames {
            let (kind, body) = (f[4], &f[5..]);
            let at = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
            if kind == SAVED_PART {
                let piece = &body[4..];
                if base.len() < at + piece.len() {
                    base.resize(at + piece.len(), 0);
                }
                base[at..at + piece.len()].copy_from_slice(piece);
            } else if kind == SAVED_DONE {
                done = (at, u64::from_le_bytes(body[4..12].try_into().unwrap()));
            }
        }
        base.resize(done.0, 0);
        (base, done.0, done.1)
    }

    #[test]
    fn a_save_goes_as_the_pieces_that_changed() {
        let base: Vec<u8> = (0..PIECE * 5 + 100).map(|i| (i * 7) as u8).collect();
        // The same: only its end.
        assert_eq!(diff_frames(Some(&base), &base).len(), 1);
        // One byte in the third piece: that piece.
        let mut file = base.clone();
        file[PIECE * 2 + 9] ^= 0xFF;
        let frames = diff_frames(Some(&base), &file);
        assert_eq!(frames.len(), 2);
        let (made, len, sum) = apply(base.clone(), &frames);
        assert_eq!((made.clone(), len, sum), (file.clone(), file.len(), checksum(&file)));
        // No copy at the host: all of it.
        assert_eq!(diff_frames(None, &file).len(), 6 + 1);
        // Longer and shorter files.
        let longer = [&file[..], &[1, 2, 3][..]].concat();
        assert_eq!(apply(base.clone(), &diff_frames(Some(&base), &longer)).0, longer);
        let shorter = file[..PIECE * 3].to_vec();
        assert_eq!(apply(base.clone(), &diff_frames(Some(&base), &shorter)).0, shorter);
    }
}
