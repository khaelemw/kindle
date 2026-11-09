//! Session watchdog: what a real session saw, kept for whoever fixes it.
//!
//! - A report on demand: F9 (with the game's window in front), or Back with Down on the
//!   controller, saves a snapshot of this game (where the player is, its character's state, its
//!   Estus, its effects, the other players' characters, the session's state, the log's last lines)
//!   in `kindle\logs\reports`, and asks every other game of the session for its own at that
//!   moment. A player's reports go to the host as well, so they're all on one PC.
//! - Checks, each second in a session, of states a session is known to get wrong: another
//!   player's character not drawn (or fading out as a phantom does) while it's in the world and
//!   not out, this game's own character not drawn while it isn't out. Each is logged once while
//!   it lasts (WARN "watchdog: ..."), and makes a report (at most one a minute for each kind).
//! - A player's new crash.log entries go to the host as it joins.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::lock;
use crate::memory;

/// F9.
const REPORT_KEY: i32 = 0x78;
/// Back and Down on the controller.
const REPORT_PAD: u16 = 0x0020 | crate::input_block::PAD_DOWN;
const CHECK_EVERY: Duration = Duration::from_secs(1);
const AUTO_REPORT_EVERY: Duration = Duration::from_secs(60);
const LOG_LINES: usize = 150;
/// A problem counts once it's been seen at this many checks in a row (a moment of a join or a
/// load isn't one).
const SEEN_FOR: u32 = 3;
/// ChrIns: the byte with "drawn" (bit 5), and the flags the phantom's departure sets (0x20).
const DRAW_AT: usize = 0x1A09;
const DRAWN: u8 = 1 << 5;
const DEPARTURE_AT: usize = 0x1A0C;
const DEPARTING: u8 = 0x20;

#[derive(Default)]
struct State {
    key_down: bool,
    checked_at: Option<Instant>,
    /// The problems seen at the last check (kind and subject), so each is logged once.
    seen: Vec<String>,
    /// How many checks in a row each problem has been seen: it counts from SEEN_FOR.
    streak: Vec<(String, u32)>,
    /// When each kind last made a report.
    auto: Vec<(String, Instant)>,
    crash_sent: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
/// A report asked for by another game (its reason), taken on the game's thread.
static ASKED: Mutex<Option<String>> = Mutex::new(None);
static ASKED_ANY: AtomicBool = AtomicBool::new(false);

fn kindle_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("kindle"))
}

fn reports_dir() -> Option<PathBuf> {
    let d = kindle_dir()?.join("logs").join("reports");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

fn clean(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).take(40).collect()
}

/// Whether this game's window is the one in front (the key is for the game being played).
fn in_front() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid)) };
    pid == std::process::id()
}

fn character_lines(label: &str, chr: usize) -> Vec<String> {
    let pos = crate::chars::position(chr);
    let anim = crate::chars::animation_name(chr);
    let drawn = memory::read::<u8>(chr + DRAW_AT).map(|b| b & DRAWN != 0);
    let departing = memory::read::<u8>(chr + DEPARTURE_AT).map(|b| b & DEPARTING != 0);
    let data = unsafe { crate::ds3::ChrIns::at(chr) }.data();
    let hp = data.as_ref().map(|d| (d.hp, d.max_hp));
    vec![format!("{label} {chr:#x}: at {pos:?}, animation {anim:?}, HP {hp:?}, drawn {drawn:?}, departing {departing:?}")]
}

/// The snapshot of this game.
fn snapshot(reason: &str) -> String {
    let mut out = vec![format!("Kindle session report: {reason}"), format!("log time {:.3} s", crate::log::elapsed())];
    out.extend(crate::session::summary());
    out.push(format!("Kindle's hooks on the game: {}", crate::hook_sites::installed().join(", ")));
    out.extend(crate::predicate_trace::report_lines());
    if let Ok(main) = crate::game::player_chr_addr() {
        out.extend(character_lines("this player", main));
        out.push(format!("embered {:?}, Estus {:?}", crate::game::embered().ok(), crate::game::estus().ok().map(|e| (e.hp, e.hp_max, e.fp, e.fp_max))));
        if let Ok(pgd) = crate::game::player_game_data().map(|p| p.addr()) {
            out.push(format!(
                "player data: type {:?}, invade type {:?}, Estus rate {:?}+{:?}, ashen rate {:?}+{:?}",
                memory::read::<i32>(pgd + 0x84),
                memory::read::<u8>(pgd + 0xFC),
                memory::read::<f32>(pgd + 0x7FC),
                memory::read::<i32>(pgd + 0x800),
                memory::read::<f32>(pgd + 0x804),
                memory::read::<i32>(pgd + 0x808)
            ));
        }
        if let Ok(effects) = crate::game::player_effects() {
            out.push(format!("effects: {:?}", effects.iter().map(|e| e.id).collect::<Vec<_>>()));
        }
        for (chr, _, _) in crate::session_players::teammates(main) {
            out.extend(character_lines("other player", chr));
        }
    } else {
        out.push("no character in the world".into());
    }
    out.push(String::new());
    out.push(format!("last {LOG_LINES} log lines:"));
    let lines: Vec<String> =
        crate::log::recent(LOG_LINES * 2).into_iter().filter(|l| !l.contains("co-op: packets:") && !l.contains("console client")).collect();
    out.extend(lines[lines.len().saturating_sub(LOG_LINES)..].iter().cloned());
    out.join("\r\n")
}

/// Saves a report here and, for a player, sends it to the host.
fn save(reason: &str) {
    let text = snapshot(reason);
    let name = format!("{}_{}.txt", crate::saves::timestamp(), clean(reason));
    if let Some(path) = reports_dir().map(|d| d.join(&name)) {
        match std::fs::write(&path, &text) {
            Ok(()) => crate::info!("watchdog: report saved ({reason}): {}", path.display()),
            Err(e) => crate::warn!("watchdog: couldn't save a report ({e})"),
        }
    }
    if crate::p2p::hosting() == Some(false) {
        crate::p2p::send_report_text(reason, &text);
    }
}

/// Another game asked for a report (the link thread).
pub fn asked(reason: &str) {
    *lock(&ASKED) = Some(reason.to_string());
    ASKED_ANY.store(true, Ordering::Relaxed);
}

/// The host: a player's report or crash entries came (the link thread).
pub fn text_arrived(from: u64, name: &str, text: &str) {
    let file = format!("{}_{from:016x}_{}.txt", crate::saves::timestamp(), clean(name));
    if let Some(path) = reports_dir().map(|d| d.join(&file)) {
        match std::fs::write(&path, text) {
            Ok(()) => crate::info!("watchdog: a player's {name} kept: {}", path.display()),
            Err(e) => crate::warn!("watchdog: couldn't keep a player's {name} ({e})"),
        }
    }
}

/// A report here and from every other game of the session.
fn report(reason: &str, ask_others: bool) {
    save(reason);
    if ask_others && crate::p2p::hosting().is_some() {
        crate::p2p::send_report_now(reason);
    }
}

/// The checks: (kind, subject) of each problem seen now.
fn problems() -> Vec<(String, String)> {
    let mut found = Vec::new();
    if !crate::game::in_world() || crate::game::in_world_for().is_none_or(|t| t < Duration::from_secs(5)) {
        return found;
    }
    let Ok(main) = crate::game::player_chr_addr() else { return found };
    if !crate::chars::hidden_by_mod(main) && memory::read::<u8>(main + DRAW_AT).is_some_and(|b| b & DRAWN == 0) {
        found.push(("own-not-drawn".into(), "this player's character isn't drawn".into()));
    }
    for (chr, _, hp) in crate::session_players::teammates(main) {
        // (One the game is still making as its player joins: Init, at the origin.)
        if crate::chars::hidden_by_mod(chr) || hp <= 0 || crate::chars::animation_name(chr).is_none_or(|a| a == "Init") {
            continue;
        }
        if memory::read::<u8>(chr + DRAW_AT).is_some_and(|b| b & DRAWN == 0) {
            found.push(("player-not-drawn".into(), format!("another player's character {chr:#x} isn't drawn")));
        }
        if memory::read::<u8>(chr + DEPARTURE_AT).is_some_and(|b| b & DEPARTING != 0) {
            found.push(("player-departing".into(), format!("another player's character {chr:#x} is fading out as a phantom does")));
        }
    }
    found
}

/// A player's crash.log goes to the host once per launch, as it's in a session.
fn send_crashes(st: &mut State) {
    if st.crash_sent || crate::p2p::hosting() != Some(false) || crate::session::state() != Ok("joined") {
        return;
    }
    st.crash_sent = true;
    let Some(path) = kindle_dir().map(|d| d.join("logs").join("crash.log")) else { return };
    if let Ok(text) = std::fs::read_to_string(path)
        && !text.trim().is_empty()
    {
        crate::p2p::send_report_text("crash.log", &text);
    }
}

/// Each frame (the game's thread).
pub fn frame() {
    let mut guard = lock(&STATE);
    let st = guard.get_or_insert_with(State::default);
    // The key, on its press.
    let key = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(REPORT_KEY) } as u16 & 0x8000 != 0 && in_front();
    let pad = (crate::input_block::PAD_RAW.load(Ordering::Relaxed) | crate::input_block::pad_buttons()) & REPORT_PAD == REPORT_PAD;
    let down = key || pad;
    let pressed = down && !st.key_down;
    st.key_down = down;
    send_crashes(st);
    let asked = ASKED_ANY.swap(false, Ordering::Relaxed).then(|| lock(&ASKED).take()).flatten();
    if !pressed && asked.is_none() && st.checked_at.is_some_and(|t| t.elapsed() < CHECK_EVERY) {
        return;
    }
    if pressed {
        drop(guard);
        let text = "Report saved: send Game\\kindle\\logs\\reports to the developer";
        crate::ui_bridge::push_notice(crate::ui_bridge::NoticeKind::Toast, text.into(), [1.0, 0.82, 0.5, 1.0], 4.0);
        report("report key", true);
        return;
    }
    if let Some(reason) = asked {
        drop(guard);
        // (The host's link passed a player's request on to the other players already.)
        report(&format!("asked by another game: {reason}"), false);
        return;
    }
    st.checked_at = Some(Instant::now());
    if crate::p2p::hosting().is_none() {
        st.seen.clear();
        return;
    }
    let now = problems();
    let streak: Vec<(String, u32)> = now
        .iter()
        .map(|(k, s)| {
            let key = format!("{k}: {s}");
            let n = st.streak.iter().find(|(x, _)| *x == key).map_or(0, |(_, n)| *n);
            (key, n + 1)
        })
        .collect();
    st.streak = streak;
    let found: Vec<(String, String)> = now.into_iter().filter(|(k, s)| st.streak.iter().any(|(x, n)| *x == format!("{k}: {s}") && *n >= SEEN_FOR)).collect();
    let mut due = None;
    for (kind, subject) in &found {
        let key = format!("{kind}: {subject}");
        if st.seen.contains(&key) {
            continue;
        }
        crate::warn!("watchdog: {subject}");
        let recent = st.auto.iter().any(|(k, t)| k == kind && t.elapsed() < AUTO_REPORT_EVERY);
        if !recent {
            st.auto.retain(|(k, _)| k != kind);
            st.auto.push((kind.clone(), Instant::now()));
            due.get_or_insert_with(|| kind.clone());
        }
    }
    st.seen = found.iter().map(|(k, s)| format!("{k}: {s}")).collect();
    drop(guard);
    if let Some(kind) = due {
        report(&kind, false);
    }
}
