//! One loaded mod = one sandboxed Lua state plus its manifest. This module owns creating
//! the state (sandbox, natives, prelude), the per-mod handler lists, the param journal,
//! and the time-limit watchdog.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use mlua::{Function, HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Table, Value as LuaValue, VmState};

use crate::config::{GAME_MODULES, ModManifest};
use crate::log::{self, Level};

/// The sandbox and Kindle's own API, in Lua; it runs the game's prelude (bindings.rs) last.
const PRELUDE: &str = include_str!("prelude.lua");
pub const MEMORY_LIMIT: usize = 128 * 1024 * 1024;

pub static START: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

/// Milliseconds since Kindle started.
pub fn now_ms() -> u64 {
    START.elapsed().as_millis() as u64
}
pub static FRAME: AtomicU64 = AtomicU64::new(0);

// ── per-mod state stored in the Lua state's app data ───────────────────────

pub struct FlagWatch {
    pub handle: u64,
    pub flag: i64,
    pub on_set: bool,
    pub last: Option<bool>,
    pub func: Function,
}

pub struct Timer {
    pub handle: u64,
    pub due: Instant,
    pub every: Option<Duration>,
    pub func: Function,
}

/// A mod's option in the game's list menus (see talk_menu.rs).
pub struct MenuOption {
    pub handle: u64,
    pub text_id: u32,
    /// "any", "bonfire", "npc", or a Lua function deciding from the menu's entries.
    pub filter: Result<String, Function>,
    pub on_select: Function,
}

/// An HKS env/act id: scripts call most by number, some by (Japanese) name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HksId {
    Num(i32),
    Name(String),
}

impl std::fmt::Display for HksId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HksId::Num(n) => write!(f, "{n}"),
            HksId::Name(s) => write!(f, "{s:?}"),
        }
    }
}

#[derive(Default)]
pub struct Handlers {
    /// HKS env/act handlers: (handle, is act, id, function).
    pub hks: Vec<(u64, bool, HksId, Function)>,
    /// Callbacks for answers that arrive on a later frame (HKS requests, event commands):
    /// (request id, function).
    pub replies: Vec<(u64, Function)>,
    /// Handlers for scripts' kindle(name, ...) calls: (handle, name, function).
    pub hks_calls: Vec<(u64, String, Function)>,
    pub menu_options: Vec<MenuOption>,
    /// Sub-menus shown with ui.menu: (handle, each entry's on_select). Only recent ones kept.
    pub menus: Vec<(u64, Vec<Option<Function>>)>,
    pub events: Vec<(u64, String, Function)>,
    /// kindle.on_message handlers: (handle, message name, function).
    pub messages: Vec<(u64, String, Function)>,
    pub flags: Vec<FlagWatch>,
    pub timers: Vec<Timer>,
    /// kindle.game.on_decide answers: (handle, hook point name, function).
    pub decides: Vec<(u64, String, Function)>,
    /// kindle.net.on handlers: (handle, channel, function).
    pub net: Vec<(u64, String, Function)>,
    /// kindle.net.join callbacks waiting for their join (session::join_later), by handle.
    pub joins: Vec<(u64, Function)>,
    /// The mod's tab's buttons' functions (kindle.ui.mod_tab), by label.
    pub tab: Vec<(String, Function)>,
    /// Mod screens open (kindle.ui.screen): (screen id, the prelude's function that hands each event to the row's callback).
    pub screens: Vec<(u64, Function)>,
    /// The mod's title entry's own function (kindle.ui.title_entry with one), run at the frame
    /// after it's picked, in place of opening its Mods tab.
    pub title: Option<Function>,
    next_handle: u64,
}

impl Handlers {
    pub fn handle(&mut self) -> u64 {
        self.next_handle += 1;
        self.next_handle
    }

    pub fn cancel(&mut self, handle: u64) -> bool {
        let count = |h: &Self| {
            h.events.len()
                + h.flags.len()
                + h.timers.len()
                + h.menu_options.len()
                + h.hks.len()
                + h.hks_calls.len()
                + h.messages.len()
                + h.decides.len()
                + h.net.len()
        };
        let before = count(self);
        self.events.retain(|(h, _, _)| *h != handle);
        self.decides.retain(|(h, _, _)| *h != handle);
        self.flags.retain(|w| w.handle != handle);
        self.timers.retain(|t| t.handle != handle);
        self.menu_options.retain(|o| o.handle != handle);
        self.hks.retain(|(h, _, _, _)| *h != handle);
        self.hks_calls.retain(|(h, _, _)| *h != handle);
        self.messages.retain(|(h, _, _)| *h != handle);
        self.net.retain(|(h, _, _)| *h != handle);
        before != count(self)
    }

    pub fn event_fns(&self, event: &str) -> Vec<Function> {
        self.events.iter().filter(|(_, e, _)| e == event).map(|(_, _, f)| f.clone()).collect()
    }
}

/// A file in a mod's folder by the path the mod gave ("art/icon.png"): letters, digits, spaces and
/// _ - . in names, / between folders, never .. or an absolute path.
pub fn mod_file(dir: &Path, path: &str) -> Result<PathBuf, String> {
    let valid = !path.is_empty()
        && path.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ' '))
        && !path.split('/').any(|part| part == ".." || part.is_empty());
    if !valid {
        return Err(format!("{path:?} isn't a path in the mod's folder (letters, digits, _ - . and / between folders)"));
    }
    Ok(dir.join(path))
}

pub struct ModCtx {
    pub id: String,
    pub dir: PathBuf,
    /// Raw access (kindle.toml [developer]: the console, and mods the player names there).
    pub raw: bool,
    pub handlers: Handlers,
    /// (table, row, field) → value before this mod first changed it.
    pub journal: HashMap<(String, u64, String), kindle_paramdef::Value>,
    /// When set, `print` output is captured here (console) instead of logged.
    pub capture: Option<Vec<String>>,
    /// kindle.store: the mod's global store and the loaded character's (by save slot),
    /// opened on first use.
    pub store_global: Option<crate::store::Store>,
    pub store_character: Option<(i32, crate::store::Store)>,
    /// Settings declared in mod.toml, and where their values are kept.
    pub settings: Vec<crate::config::Setting>,
    pub settings_store: Option<crate::store::Store>,
    /// The player's HUD setting before this mod first changed it (restored on unload).
    pub hud_original: Option<u8>,
    /// kindle.keep: the values handed out, by key, saved when the mod unloads.
    pub keeps: Vec<(String, mlua::RegistryKey)>,
}

/// kindle.keep's values while their mods are unloaded (reloads), by mod id and key; for the
/// session only.
static KEPT: std::sync::Mutex<Option<HashMap<String, HashMap<String, serde_json::Value>>>> = std::sync::Mutex::new(None);

/// The value `id` kept under `key` when it last unloaded.
pub fn kept(id: &str, key: &str) -> Option<serde_json::Value> {
    KEPT.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(id)?.get(key).cloned()
}

/// Saves a mod's kept values as its Lua state goes (they must be plain data, as for
/// kindle.store; others are reported and dropped).
pub fn save_kept(id: &str, lua: &Lua) {
    let keeps = lua.app_data_mut::<ModCtx>().map(|mut c| std::mem::take(&mut c.keeps)).unwrap_or_default();
    if keeps.is_empty() {
        return;
    }
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    let mine = kept.get_or_insert_with(HashMap::new).entry(id.to_string()).or_default();
    for (key, value) in keeps {
        let json = lua.registry_value::<LuaValue>(&value).map_err(|e| e.to_string()).and_then(|v| crate::store::to_json(&v));
        match json {
            Ok(Some(v)) => {
                mine.insert(key, v);
            }
            Ok(None) => {
                mine.remove(&key);
            }
            Err(e) => log::write(Level::Warn, id, &format!("keep {key:?} not kept: {e}")),
        }
    }
}

impl ModCtx {
    /// Writes changed stores to disk.
    pub fn flush_stores(&mut self) {
        let character = self.store_character.as_mut().map(|(_, s)| s);
        for s in self.store_global.iter_mut().chain(character).chain(self.settings_store.iter_mut()) {
            if let Err(e) = s.flush() {
                crate::warn!("[{}] store: {e}", self.id);
            }
        }
    }

    /// A setting's current value: the saved one if it still fits the declaration, else the
    /// default.
    pub fn setting(&mut self, name: &str) -> Result<crate::config::SettingValue, String> {
        let decl = self.settings.iter().find(|s| s.name == name).cloned().ok_or_else(|| self.unknown_setting(name))?;
        let saved = self.settings_store()?.get(name).and_then(crate::config::SettingValue::from_json);
        Ok(saved.and_then(|v| decl.check(v).ok()).unwrap_or(decl.default))
    }

    /// Changes a setting (checked against its declaration); returns the value kept.
    pub fn set_setting(&mut self, name: &str, value: crate::config::SettingValue) -> Result<crate::config::SettingValue, String> {
        let decl = self.settings.iter().find(|s| s.name == name).cloned().ok_or_else(|| self.unknown_setting(name))?;
        let v = decl.check(value).map_err(|e| format!("setting {name}: {e}"))?;
        self.settings_store()?.set(name, Some(v.to_json()));
        Ok(v)
    }

    fn unknown_setting(&self, name: &str) -> String {
        let names: Vec<&str> = self.settings.iter().map(|s| s.name.as_str()).collect();
        if names.is_empty() {
            format!("no setting {name:?}: this mod declares no settings ([settings.<name>] in mod.toml)")
        } else {
            format!("no setting {name:?} (settings: {})", names.join(", "))
        }
    }

    fn settings_store(&mut self) -> Result<&mut crate::store::Store, String> {
        if self.settings_store.is_none() {
            self.settings_store = Some(crate::store::Store::named(&self.id, "settings")?);
        }
        Ok(self.settings_store.as_mut().unwrap())
    }

    pub fn store(&mut self, global: bool) -> Result<&mut crate::store::Store, String> {
        use crate::store::Store;
        if global {
            if self.store_global.is_none() {
                self.store_global = Some(Store::global(&self.id)?);
            }
            return Ok(self.store_global.as_mut().unwrap());
        }
        let game = crate::bindings::get().ok_or("no game is bound to keep a character's values")?;
        let Some(slot) = (game.character_slot)()? else {
            return Err("no character is loaded (store.get_global and store.set_global keep values that belong to no character)".into());
        };
        if self.store_character.as_ref().is_none_or(|(s, _)| *s != slot) {
            let name = (game.character_name)()?;
            self.store_character = Some((slot, Store::character(&self.id, slot, &name)?));
        }
        Ok(&mut self.store_character.as_mut().unwrap().1)
    }
}

/// Separate app-data slot so the watchdog hook never contends with ModCtx borrows. The deadline is
/// in the calling thread's CPU time (100 ns units), not the clock's: a script is stopped for what
/// it runs, not for a stall of the game's thread while it runs (the PC busy with another program
/// or the disk, the thread waiting in the game). With two games on one PC a co-op handler at the title was
/// stopped, and the mod with it, 0.6 s after a save backup while a second game loaded, 2026-10-18.
pub struct Deadline(pub std::cell::Cell<Option<u64>>);

impl Deadline {
    /// The calling thread's CPU time so far (user and kernel), in 100 ns units.
    pub fn thread_cpu() -> u64 {
        use windows::Win32::Foundation::FILETIME;
        use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
        let (mut created, mut exited, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetThreadTimes(GetCurrentThread(), &mut created, &mut exited, &mut kernel, &mut user) }.is_err() {
            return 0;
        }
        let ticks = |t: FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
        ticks(kernel) + ticks(user)
    }

    /// A deadline `limit` of this thread's CPU time from now.
    pub fn after(limit: Duration) -> u64 {
        Self::thread_cpu() + (limit.as_nanos() / 100) as u64
    }

    fn passed(deadline: u64) -> bool {
        Self::thread_cpu() > deadline
    }
}

/// Runs `work`, Kindle's own for a mod's call (a param table rebuilt for its new rows), with the
/// CPU time it takes kept off the mod's deadline: the time limit is for what a script runs, and a
/// table of thousands of rows rebuilt for a mod's add_rows took the dsr-world mod past it, which
/// undid all its changes (2026-10-26).
pub fn kindles_own<T>(lua: &Lua, work: impl FnOnce() -> T) -> T {
    let start = Deadline::thread_cpu();
    let out = work();
    if let Some(d) = lua.app_data_ref::<Deadline>()
        && let Some(deadline) = d.0.get()
    {
        d.0.set(Some(deadline + Deadline::thread_cpu().saturating_sub(start)));
    }
    out
}

/// Tells the player in game that a mod stopped (the full error is in the log and the
/// overlay), so a broken mod doesn't just go quiet.
pub fn notify_stopped(name: &str, error: &str) {
    let first = error.lines().next().unwrap_or(error);
    let first: String = if first.chars().count() > 120 { first.chars().take(117).chain("...".chars()).collect() } else { first.to_string() };
    let text = format!("{name} stopped: {first} (F1 > Log)");
    // A mod saved twice in a row fails twice: one message is enough.
    if crate::ui_bridge::notices().iter().any(|n| n.text == text) {
        return;
    }
    crate::ui_bridge::push_notice(crate::ui_bridge::NoticeKind::Toast, text, [1.0, 0.5, 0.45, 1.0], 10.0);
}

pub fn ctx(lua: &Lua) -> mlua::Result<mlua::AppDataRefMut<'_, ModCtx>> {
    lua.app_data_mut::<ModCtx>().ok_or_else(|| mlua::Error::runtime("kindle: mod context missing"))
}

pub fn rt_err(e: impl std::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(e.to_string())
}

// ── between mods ───────────────────────────────────────────────────────────
//
// Each mod has its own Lua state, so nothing is shared directly: kindle.export keeps a
// table of functions in the exporting mod's state, and kindle.mods.get(id) gives others a
// proxy whose calls run that function in its own state, with arguments and results copied
// across (plain data only). The running mods' states are listed here for that.

pub static LIVE: std::sync::Mutex<Option<HashMap<String, Lua>>> = std::sync::Mutex::new(None);

pub fn set_live(id: &str, lua: Option<&Lua>) {
    let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    let map = live.get_or_insert_with(HashMap::new);
    match lua {
        Some(l) => {
            map.insert(id.to_string(), l.clone());
        }
        None => {
            map.remove(id);
        }
    }
}

pub fn live(id: &str) -> Option<Lua> {
    LIVE.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(id).cloned()
}

/// Ids of the running mods.
pub fn live_ids() -> Vec<String> {
    let mut ids: Vec<String> = LIVE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|m| m.keys().cloned().collect()).unwrap_or_default();
    ids.sort();
    ids
}

/// Copies plain data (nil, booleans, numbers, strings, tables of those) into another state.
pub fn copy_value(v: &LuaValue, to: &Lua, depth: usize) -> mlua::Result<LuaValue> {
    if depth > 32 {
        return Err(rt_err("tables passed between mods nest too deep (or contain themselves)"));
    }
    Ok(match v {
        LuaValue::Nil => LuaValue::Nil,
        LuaValue::Boolean(b) => LuaValue::Boolean(*b),
        LuaValue::Integer(i) => LuaValue::Integer(*i),
        LuaValue::Number(n) => LuaValue::Number(*n),
        LuaValue::String(s) => LuaValue::String(to.create_string(s.as_bytes())?),
        LuaValue::Table(t) => {
            let out = to.create_table()?;
            for pair in t.pairs::<LuaValue, LuaValue>() {
                let (k, item) = pair?;
                out.raw_set(copy_value(&k, to, depth + 1)?, copy_value(&item, to, depth + 1)?)?;
            }
            LuaValue::Table(out)
        }
        other => {
            return Err(rt_err(format!("a {} can't be passed between mods (only nil, booleans, numbers, strings and tables)", other.type_name())));
        }
    })
}

/// A message sent with kindle.send, delivered to every other mod on the next frame.
pub struct Message {
    pub from: String,
    pub name: String,
    pub data: Option<serde_json::Value>,
}

pub static MESSAGES: std::sync::Mutex<Vec<Message>> = std::sync::Mutex::new(Vec::new());

/// Ids for requests answered on a later frame, unique across mods.
pub static REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn setting_to_lua(lua: &Lua, v: &crate::config::SettingValue) -> mlua::Result<LuaValue> {
    use crate::config::SettingValue as V;
    Ok(match v {
        V::Bool(b) => LuaValue::Boolean(*b),
        V::Int(i) => LuaValue::Integer(*i),
        V::Float(f) => LuaValue::Number(*f),
        V::Choice(c) => LuaValue::String(lua.create_string(c)?),
    })
}

pub fn setting_from_lua(v: &LuaValue) -> Option<crate::config::SettingValue> {
    use crate::config::SettingValue as V;
    Some(match v {
        LuaValue::Boolean(b) => V::Bool(*b),
        LuaValue::Integer(i) => V::Int(*i),
        LuaValue::Number(n) => V::Float(*n),
        LuaValue::String(s) => V::Choice(s.to_string_lossy()),
        _ => return None,
    })
}

pub fn parse_color(name: &str) -> mlua::Result<[f32; 4]> {
    Ok(match name {
        "gold" => [1.0, 0.82, 0.5, 1.0],
        "white" => [0.95, 0.93, 0.88, 1.0],
        "red" => [0.95, 0.35, 0.3, 1.0],
        "blue" => [0.55, 0.75, 1.0, 1.0],
        "green" => [0.6, 0.9, 0.55, 1.0],
        hex if hex.len() == 7 && hex.starts_with('#') => {
            let v = u32::from_str_radix(&hex[1..], 16).map_err(|_| rt_err(format!("bad color {hex:?}")))?;
            [((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0, 1.0]
        }
        other => return Err(rt_err(format!("unknown color {other:?} (gold, white, red, blue, green or #rrggbb)"))),
    })
}

// ── value conversion for params ────────────────────────────────────────────

pub fn to_param_value(v: &LuaValue) -> mlua::Result<kindle_paramdef::Value> {
    use kindle_paramdef::Value as P;
    Ok(match v {
        LuaValue::Integer(i) => P::Int(*i),
        LuaValue::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => P::Int(*n as i64),
        LuaValue::Number(n) => P::Float(*n as f32),
        LuaValue::Boolean(b) => P::Int(*b as i64),
        other => return Err(rt_err(format!("param fields take numbers or booleans, got {}", other.type_name()))),
    })
}

pub fn from_param_value(lua: &Lua, v: kindle_paramdef::Value) -> mlua::Result<LuaValue> {
    use kindle_paramdef::Value as P;
    Ok(match v {
        P::Int(i) => LuaValue::Integer(i),
        // Round-trip through the shortest f32 representation so 1.2f32 reads as 1.2.
        P::Float(f) => LuaValue::Number(f.to_string().parse::<f64>().unwrap_or(f as f64)),
        P::Str(s) => LuaValue::String(lua.create_string(&s)?),
        P::Bytes(b) => LuaValue::Table(lua.create_sequence_from(b)?),
    })
}

// ── loaded mod ─────────────────────────────────────────────────────────────

pub enum ModState {
    Running,
    Failed(String),
    Disabled,
}

pub struct LoadedMod {
    pub manifest: ModManifest,
    pub dir: PathBuf,
    pub lua: Option<Lua>,
    pub state: ModState,
    pub reloads: u32,
    pub cpu: CpuTime,
}

/// Time spent running a mod's Lua, for the overlay.
#[derive(Default)]
pub struct CpuTime {
    /// Since the last roll.
    pub current: std::cell::Cell<Duration>,
    /// Milliseconds per second over the last measured window.
    pub ms_per_second: f64,
    /// Since the last roll, by handler (what `LoadedMod::call` was told it runs).
    pub current_by_handler: std::cell::RefCell<HashMap<String, Duration>>,
    /// Milliseconds per second by handler over the last window, slowest first.
    pub by_handler: Vec<(String, f64)>,
}

impl CpuTime {
    pub fn roll(&mut self, window: Duration) {
        let per_second = |spent: Duration| spent.as_secs_f64() * 1000.0 / window.as_secs_f64().max(0.001);
        self.ms_per_second = per_second(self.current.take());
        let mut by: Vec<(String, f64)> = self.current_by_handler.get_mut().drain().map(|(what, spent)| (what, per_second(spent))).collect();
        by.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.by_handler = by;
    }

    fn add(&self, what: &str, spent: Duration) {
        let mut by = self.current_by_handler.borrow_mut();
        match by.get_mut(what) {
            Some(d) => *d += spent,
            None => {
                by.insert(what.to_string(), spent);
            }
        }
    }
}

impl LoadedMod {
    pub fn id(&self) -> &str {
        &self.manifest.info.id
    }

    pub fn status(&self) -> String {
        match &self.state {
            ModState::Running => "running".into(),
            ModState::Disabled => "disabled".into(),
            ModState::Failed(e) => format!("FAILED: {e}"),
        }
    }

    /// Calls Lua with the time limit armed, without changing the mod's state on error.
    pub fn try_call<R>(&self, limit: Duration, f: impl FnOnce(&Lua) -> mlua::Result<R>) -> Result<R, String> {
        let lua = self.lua.as_ref().ok_or("mod is not running")?;
        if let Some(d) = lua.app_data_ref::<Deadline>() {
            d.0.set(Some(Deadline::after(limit)));
        }
        let started = Instant::now();
        let result = f(lua);
        self.cpu.current.set(self.cpu.current.get() + started.elapsed());
        if let Some(d) = lua.app_data_ref::<Deadline>() {
            d.0.set(None);
        }
        result.map_err(|e| clean_error(&e))
    }

    /// Calls Lua with the time limit armed. On error the mod is unloaded and marked failed.
    pub fn call<R>(&mut self, limit: Duration, what: &str, f: impl FnOnce(&Lua) -> mlua::Result<R>) -> Option<R> {
        self.lua.as_ref()?;
        let before = self.cpu.current.get();
        let result = self.try_call(limit, f);
        self.cpu.add(what, self.cpu.current.get().saturating_sub(before));
        match result {
            Ok(r) => Some(r),
            Err(e) => {
                let msg = format!("{what}: {e}");
                log::write(Level::Error, self.id(), &msg);
                if self.id() != "console" {
                    notify_stopped(self.manifest.info.name.as_deref().unwrap_or(self.id()), &msg);
                }
                self.unload(false);
                self.state = ModState::Failed(msg);
                None
            }
        }
    }

    /// Drops the Lua state. With `graceful`, runs the mod's unload handlers first.
    /// Always undoes what the mod changed in the game (the bindings' unloading and unloaded).
    pub fn unload(&mut self, graceful: bool) {
        let Some(lua) = self.lua.take() else { return };
        save_kept(self.id(), &lua);
        set_live(self.id(), None);
        crate::watch::stop_owned_by(self.id());
        let game = crate::bindings::get();
        if let Some(g) = game {
            (g.unloading)(self.id(), lua.app_data_ref::<ModCtx>().as_deref());
        }
        if graceful {
            if let Some(d) = lua.app_data_ref::<Deadline>() {
                d.0.set(Some(Deadline::after(Duration::from_millis(500))));
            }
            let fns = lua.app_data_ref::<ModCtx>().map(|c| c.handlers.event_fns("unload")).unwrap_or_default();
            let explicit: Option<Function> = lua.globals().get::<Table>("kindle").ok().and_then(|k| k.get::<Function>("on_unload").ok());
            for f in fns.into_iter().chain(explicit) {
                if let Err(e) = f.call::<()>(()) {
                    log::write(Level::Warn, self.id(), &format!("unload handler: {}", clean_error(&e)));
                }
            }
        }
        if let Some(mut ctx) = lua.remove_app_data::<ModCtx>() {
            if let Some(g) = game {
                (g.unloaded)(&mut ctx);
            }
            ctx.handlers = Handlers::default();
        }
        crate::ui_bridge::remove_owner(self.id());
        drop(lua);
    }
}

/// Shortens mlua errors to the useful part (Lua message + traceback first line).
pub fn clean_error(e: &mlua::Error) -> String {
    let s = e.to_string();
    let s = s.strip_prefix("runtime error: ").unwrap_or(&s);
    let s = s.strip_prefix("callback error: ").unwrap_or(s);
    s.lines().take_while(|l| !l.trim_start().starts_with("stack traceback")).collect::<Vec<_>>().join(" ")
}

/// Creates a sandboxed Lua state for a mod, installs natives and runs the prelude.
/// `all_modules` grants every game module and raw access (used by the console).
pub fn create_state(manifest: &ModManifest, dir: &Path, all_modules: bool) -> mlua::Result<Lua> {
    create_state_with(manifest, dir, all_modules, all_modules)
}

/// create_state, with raw access (kindle.toml [developer]) given apart from the modules.
pub fn create_state_with(manifest: &ModManifest, dir: &Path, all_modules: bool, raw: bool) -> mlua::Result<Lua> {
    let game = crate::bindings::get().ok_or_else(|| mlua::Error::runtime("kindle: no game is bound (bindings::set)"))?;
    let lua = Lua::new_with(StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE, LuaOptions::new())?;
    lua.set_memory_limit(MEMORY_LIMIT)?;

    lua.set_app_data(ModCtx {
        id: manifest.info.id.clone(),
        dir: dir.to_path_buf(),
        raw,
        handlers: Handlers::default(),
        journal: HashMap::new(),
        capture: None,
        store_global: None,
        store_character: None,
        settings: manifest.settings.clone(),
        settings_store: None,
        hud_original: None,
        keeps: Vec::new(),
    });
    lua.set_app_data(Deadline(std::cell::Cell::new(None)));

    // Watchdog: abort scripts that run past their deadline (infinite loops etc).
    lua.set_hook(HookTriggers::new().every_nth_instruction(10_000), |lua, _| {
        if let Some(d) = lua.app_data_ref::<Deadline>()
            && let Some(deadline) = d.0.get()
            && Deadline::passed(deadline)
        {
            return Err(mlua::Error::runtime("script exceeded its time limit (see script_time_limit_ms)"));
        }
        Ok(VmState::Continue)
    })?;

    let native = natives(&lua)?;

    let allowed = lua.create_table()?;
    for module in GAME_MODULES {
        if all_modules || manifest.allows(module) {
            allowed.set(*module, true)?;
        }
    }

    let info = lua.create_table()?;
    info.set("id", manifest.info.id.as_str())?;
    info.set("name", manifest.info.name.clone().unwrap_or_else(|| manifest.info.id.clone()))?;
    info.set("version", manifest.info.version.clone().unwrap_or_default())?;
    info.set("raw", raw)?;

    let game_modules = lua.load(game.prelude).set_name("=kindle game prelude").into_function()?;
    lua.load(PRELUDE).set_name("=kindle prelude").call::<()>((native, allowed, info, game_modules))?;
    if raw {
        (game.raw_state)(&lua)?;
    }
    Ok(lua)
}

pub fn natives(lua: &Lua) -> mlua::Result<Table> {
    crate::natives::build(lua)
}

/// Formats Lua values for the console, one level of table nesting.
/// How many entries of one table `inspect` shows.
pub const INSPECT_ENTRIES: usize = 60;

/// A readable rendering of a Lua value. Tables show their contents down to `max_depth`
/// levels: lists as `{ 1, 2, 3 }`, others as `{ key = value }` with sorted keys.
pub fn inspect(v: &LuaValue, max_depth: usize) -> String {
    inspect_at(v, 0, max_depth)
}

pub fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn inspect_at(v: &LuaValue, depth: usize, max_depth: usize) -> String {
    match v {
        LuaValue::Nil => "nil".into(),
        LuaValue::Boolean(b) => b.to_string(),
        LuaValue::Integer(i) => i.to_string(),
        LuaValue::Number(n) => n.to_string(),
        LuaValue::String(s) => format!("{:?}", s.to_string_lossy()),
        LuaValue::Table(t) => {
            if depth >= max_depth {
                return "{...}".into();
            }
            let mut entries: Vec<(LuaValue, LuaValue)> = t.pairs::<LuaValue, LuaValue>().flatten().collect();
            if entries.is_empty() {
                return "{}".into();
            }
            let len = t.raw_len();
            let is_list = entries.len() == len && entries.iter().all(|(k, _)| matches!(k, LuaValue::Integer(i) if *i >= 1 && *i as usize <= len));
            let mut parts = Vec::new();
            if is_list {
                for i in 1..=len.min(INSPECT_ENTRIES) {
                    let v: LuaValue = t.raw_get(i).unwrap_or(LuaValue::Nil);
                    parts.push(inspect_at(&v, depth + 1, max_depth));
                }
            } else {
                // Numbers first (in order), then names alphabetically.
                entries.sort_by(|(a, _), (b, _)| match (a, b) {
                    (LuaValue::Integer(x), LuaValue::Integer(y)) => x.cmp(y),
                    (LuaValue::Integer(_), _) => std::cmp::Ordering::Less,
                    (_, LuaValue::Integer(_)) => std::cmp::Ordering::Greater,
                    _ => inspect_at(a, 0, 0).cmp(&inspect_at(b, 0, 0)),
                });
                for (k, v) in entries.iter().take(INSPECT_ENTRIES) {
                    let key = match k {
                        LuaValue::String(s) if is_identifier(&s.to_string_lossy()) => s.to_string_lossy().to_string(),
                        other => format!("[{}]", inspect_at(other, max_depth, max_depth)),
                    };
                    parts.push(format!("{key} = {}", inspect_at(v, depth + 1, max_depth)));
                }
            }
            let count = if is_list { len } else { entries.len() };
            if count > INSPECT_ENTRIES {
                parts.push(format!("... ({} more)", count - INSPECT_ENTRIES));
            }
            format!("{{ {} }}", parts.join(", "))
        }
        LuaValue::Function(_) => "<function>".into(),
        other => format!("<{}>", other.type_name()),
    }
}

pub fn inspect_multi(values: &MultiValue) -> String {
    values.iter().map(|v| inspect(v, 3)).collect::<Vec<_>>().join("\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_shows_lists_and_sorted_keys() {
        let lua = Lua::new();
        let v = |code: &str| lua.load(code).eval::<LuaValue>().unwrap();
        assert_eq!(inspect(&v("return {3, 1, 2}"), 3), "{ 3, 1, 2 }");
        assert_eq!(inspect(&v("return {b = 1, a = {1, 2}, ['x y'] = true}"), 3), r#"{ a = { 1, 2 }, b = 1, ["x y"] = true }"#);
        assert_eq!(inspect(&v("return {a = {b = {c = {}}}}"), 2), "{ a = { b = {...} } }");
        assert_eq!(inspect(&v("return {}"), 3), "{}");
        assert_eq!(inspect(&v("return {[2] = 'x', [1] = 'y', k = 1}"), 3), r#"{ [1] = "y", [2] = "x", k = 1 }"#);
    }
}
