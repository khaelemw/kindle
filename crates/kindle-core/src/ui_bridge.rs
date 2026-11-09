//! The only channel between the overlay (render thread) and the runtime (game task thread).
//!
//! The overlay never touches game memory or Lua. The runtime publishes a [`Snapshot`] while
//! the overlay is visible, and the overlay sends [`UiRequest`]s that the runtime executes on
//! the game thread at the start of the next frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

/// Set by the overlay while it is open; the runtime only publishes snapshots then.
pub static OVERLAY_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Set when "Mods" is picked in the game's own title or pause menu; the overlay opens the
/// Mods screen (mods_screen.rs) and clears it.
pub static OPEN_MODS_SCREEN: AtomicBool = AtomicBool::new(false);
/// The mod whose settings the Mods screen opens at (with OPEN_MODS_SCREEN), else the first.
pub static MODS_SCREEN_AT: Mutex<Option<String>> = Mutex::new(None);

/// The developer overlay opens with its key (kindle.toml `developer`).
pub static DEVELOPER: AtomicBool = AtomicBool::new(false);
/// A mod has raw access (kindle.toml [developer]): the overlay says so on screen at all times.
pub static RAW_ACCESS: AtomicBool = AtomicBool::new(false);

/// Set by the runtime while a character is loaded.
pub static IN_WORLD: AtomicBool = AtomicBool::new(false);

/// The status lines (drawn each frame until cleared), one per mod in the order they were first
/// set: the mod's id, its text and, if any, a progress bar (0 to 1) under it. For a state the
/// player has to see the whole time (co-op revive, a run's clock).
static STATUS: Mutex<Vec<(String, String, Option<f32>)>> = Mutex::new(Vec::new());

/// Sets `owner`'s status line (in its place, if it has one) or clears it; the other mods' lines
/// stay as they are.
pub fn set_status(owner: &str, status: Option<(String, Option<f32>)>) {
    let mut lines = STATUS.lock().unwrap_or_else(|e| e.into_inner());
    let at = lines.iter().position(|(o, ..)| o == owner);
    match (at, status) {
        (Some(i), Some((text, progress))) => lines[i] = (owner.into(), text, progress),
        (None, Some((text, progress))) => lines.push((owner.into(), text, progress)),
        (Some(i), None) => {
            lines.remove(i);
        }
        (None, None) => {}
    }
}

/// The status lines, top to bottom.
pub fn status_lines() -> Vec<(String, Option<f32>)> {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, text, progress)| (text.clone(), *progress)).collect()
}

/// The action prompts at the bottom of the screen, as the game's own ("Rest at bonfire"), until
/// cleared: one per mod, the one set last shown (the one before it comes back when it's cleared).
static PROMPTS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

pub fn set_prompt(owner: &str, text: Option<String>) {
    let mut prompts = PROMPTS.lock().unwrap_or_else(|e| e.into_inner());
    prompts.retain(|(o, _)| o != owner);
    if let Some(text) = text {
        prompts.push((owner.into(), text));
    }
}

pub fn prompt() -> Option<String> {
    PROMPTS.lock().unwrap_or_else(|e| e.into_inner()).last().map(|(_, text)| text.clone())
}

/// A mod has unloaded: its status line, prompt, screens and layers go.
pub fn remove_owner(owner: &str) {
    set_status(owner, None);
    set_prompt(owner, None);
    crate::screens::close_owner(owner);
    crate::hud::remove_owner(owner);
}
/// How black the screen is drawn over the game (0 none, 1 black; f32 bits), for Kindle's own
/// transitions (quick_warp.rs).
pub static SCREEN_FADE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);
/// The mods and their settings, kept current by the runtime (for the native Mods screen).
static MODS: Mutex<Vec<ModRow>> = Mutex::new(Vec::new());
static WATCHED_FLAGS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static REQUESTS: OnceLock<Mutex<Sender<UiRequest>>> = OnceLock::new();
static CONSOLE: OnceLock<Mutex<Sender<crate::console::ConsoleRequest>>> = OnceLock::new();

#[derive(Clone, Default)]
pub struct Snapshot {
    pub frame: u64,
    pub uptime: f64,
    pub mods_loaded: bool,
    /// No mods are loaded this session (Shift held at startup, or safe_mode in kindle.toml).
    pub safe_mode: bool,
    /// Every mod that has run with the loaded character ("id version").
    pub character_mods: Vec<String>,
    pub mods: Vec<ModRow>,
    pub player: Option<PlayerSnap>,
    /// (category, param id, quantity, name)
    pub inventory: Vec<(&'static str, u32, u32, String)>,
    /// (flag, value or None if unreadable)
    pub flags: Vec<(u32, Option<bool>)>,
    pub param_tables: Vec<String>,
    pub nearby: Vec<NearbyRow>,
}

#[derive(Clone)]
pub struct NearbyRow {
    pub handle: u32,
    pub label: String,
    pub kind: &'static str,
    pub entity_id: Option<i32>,
    pub hp: (i32, i32),
    pub distance: f32,
}

#[derive(Clone)]
pub struct ModRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub modules: String,
    pub status: String,
    pub failed: bool,
    pub reloads: u32,
    pub settings: Vec<SettingRow>,
    /// Milliseconds per second spent in the mod's Lua.
    pub cpu_ms: f64,
    /// The same by handler (event name, "message", "hks call", ...), slowest first.
    pub cpu_by_handler: Vec<(String, f64)>,
    pub disabled: bool,
    /// Turned off with `enabled = false` in mod.toml (the overlay can't turn it on).
    pub disabled_in_toml: bool,
}

#[derive(Clone)]
pub struct SettingRow {
    pub name: String,
    pub label: String,
    pub description: String,
    pub kind: crate::config::SettingKind,
    pub value: crate::config::SettingValue,
    pub default: crate::config::SettingValue,
}

#[derive(Clone)]
pub struct PlayerSnap {
    pub name: String,
    pub hp: (i32, i32),
    pub fp: (i32, i32),
    pub stamina: (i32, i32),
    pub stats: [(&'static str, u32); 9],
}

pub struct ParamField {
    pub name: String,
    pub ty: &'static str,
    pub value: String,
    pub writable: bool,
}

pub enum UiRequest {
    /// Reload one mod by id, or all mods.
    Reload(Option<String>),
    SetFlag(u32, bool),
    /// Set a character's HP (0 kills, i32::MAX heals to full).
    SetChrHp(u32, i32),
    ChrEffect(u32, i64),
    ParamRow {
        table: String,
        row: u64,
        reply: Sender<Result<Vec<ParamField>, String>>,
    },
    ParamSet {
        table: String,
        row: u64,
        field: String,
        value: String,
        reply: Sender<Result<(), String>>,
    },
    /// Turn a mod on or off (remembered across sessions).
    SetEnabled(String, bool),
    /// Change a mod's setting (mod id, setting name, value).
    SetSetting(String, String, crate::config::SettingValue),
    /// Turn the developer overlay on or off (saved to kindle.toml).
    SetDeveloper(bool),
    /// Search every name Kindle knows (search.rs).
    Find {
        query: String,
        reply: Sender<Vec<FindHit>>,
    },
}

/// One Find result: what it is, its name, its id, and Lua that uses it.
#[derive(Clone, Debug)]
pub struct FindHit {
    pub kind: &'static str,
    pub name: String,
    pub id: String,
    pub lua: String,
    pub note: String,
}

pub fn install(requests: Sender<UiRequest>, console: Sender<crate::console::ConsoleRequest>) {
    let _ = REQUESTS.set(Mutex::new(requests));
    let _ = CONSOLE.set(Mutex::new(console));
}

pub fn send(req: UiRequest) {
    if let Some(tx) = REQUESTS.get() {
        let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(req);
    }
}

pub fn console(req: crate::console::ConsoleRequest) {
    if let Some(tx) = CONSOLE.get() {
        let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(req);
    }
}

pub fn visible() -> bool {
    OVERLAY_VISIBLE.load(Ordering::Relaxed)
}

pub fn publish(s: Snapshot) {
    *SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
}

pub fn snapshot() -> Option<Snapshot> {
    SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn publish_mods(mods: Vec<ModRow>) {
    *MODS.lock().unwrap_or_else(|e| e.into_inner()) = mods;
}

pub fn mods() -> Vec<ModRow> {
    MODS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn watched_flags() -> Vec<u32> {
    WATCHED_FLAGS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

pub fn set_watched_flags(flags: Vec<u32>) {
    *WATCHED_FLAGS.lock().unwrap_or_else(|e| e.into_inner()) = flags;
}

// ── on-screen notices (ui.banner / ui.toast) ────────────────────────────────

/// Whether the overlay renderer is running, so notices will actually be drawn.
pub static RENDERER_ACTIVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    /// Large centred text on a dark band, like the game's own banners.
    Banner,
    /// Small stacked notification in the top-right corner.
    Toast,
}

#[derive(Clone)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
    pub color: [f32; 4],
    pub created: std::time::Instant,
    pub seconds: f32,
}

static NOTICES: Mutex<Vec<Notice>> = Mutex::new(Vec::new());

pub fn push_notice(kind: NoticeKind, text: String, color: [f32; 4], seconds: f32) {
    let mut n = NOTICES.lock().unwrap_or_else(|e| e.into_inner());
    if kind == NoticeKind::Banner {
        // One banner at a time: a new one replaces the current one.
        n.retain(|x| x.kind != NoticeKind::Banner);
    }
    n.push(Notice { kind, text, color, created: std::time::Instant::now(), seconds });
    let toasts = n.iter().filter(|x| x.kind == NoticeKind::Toast).count();
    if toasts > 6
        && let Some(i) = n.iter().position(|x| x.kind == NoticeKind::Toast)
    {
        n.remove(i);
    }
}

/// Live notices (expired ones are dropped).
pub fn notices() -> Vec<Notice> {
    let mut n = NOTICES.lock().unwrap_or_else(|e| e.into_inner());
    n.retain(|x| x.created.elapsed().as_secs_f32() < x.seconds);
    n.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each mod keeps its own status line in its place, and an unloaded mod's line and prompt go.
    #[test]
    fn status_and_prompt_per_mod() {
        let ours = |lines: Vec<(String, Option<f32>)>| lines.into_iter().filter(|(t, _)| t.starts_with("test:")).map(|(t, _)| t).collect::<Vec<_>>();
        set_status("test-timer", Some(("test: 0:41".into(), None)));
        set_status("test-streak", Some(("test: streak 1".into(), None)));
        set_status("test-timer", Some(("test: 0:42".into(), None)));
        assert_eq!(ours(status_lines()), ["test: 0:42", "test: streak 1"]);
        set_status("test-streak", None);
        assert_eq!(ours(status_lines()), ["test: 0:42"]);

        set_prompt("test-door", Some("test: open".into()));
        set_prompt("test-revive", Some("test: revive".into()));
        assert_eq!(prompt().as_deref(), Some("test: revive"));
        remove_owner("test-revive");
        assert_eq!(prompt().as_deref(), Some("test: open"));
        remove_owner("test-door");
        remove_owner("test-timer");
        assert!(ours(status_lines()).is_empty());
    }
}
