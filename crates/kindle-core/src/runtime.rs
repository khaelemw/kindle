//! The mod runtime: the mods' Lua states (loading, reloads, turning them on and off), the console,
//! and running mods' handlers. A game's frame (frame.rs in the game's crate) drives it once per frame
//! on the game's thread, the only place Lua ever executes; its hooks reach it through `with_runtime`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use mlua::{Function, MultiValue, Value as LuaValue};

use crate::config::{KindleConfig, ModInfo, ModManifest, Permissions};
use crate::console::ConsoleRequest;
use crate::log::{self, Level};
use crate::mods::{self, LoadedMod, ModState};
use crate::ui_bridge::{self, UiRequest};

const RELOAD_DEBOUNCE: Duration = Duration::from_millis(250);
const LOAD_TIME_LIMIT: Duration = Duration::from_secs(2);
/// The runtime, once the game's runtime has made it: its frame task and its hooks use it.
pub static RUNTIME: Mutex<Option<Runtime>> = Mutex::new(None);
/// Set when Kindle hit an internal error: no mod code runs again this session.
pub static HALTED: AtomicBool = AtomicBool::new(false);

pub struct Runtime {
    pub kindle_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub config: KindleConfig,
    pub mods: Vec<LoadedMod>,
    pub console: LoadedMod,
    pub reload_rx: Receiver<String>,
    pub pending_reloads: HashMap<String, Instant>,
    pub console_rx: Receiver<ConsoleRequest>,
    pub player_loaded: bool,
    pub last_tick: Option<Instant>,
    /// When the first frame ran.
    pub first_frame: Option<Instant>,
    /// Mods are loaded once the game's data (params) is available, so main.lua can use it.
    pub mods_loaded: bool,
    /// When mods' CPU time was last rolled into per-second figures.
    pub cpu_rolled: Option<Instant>,
    /// Safe mode: no mods are loaded this session.
    pub safe_mode: bool,
    /// Every mod that has run with the loaded character ("id version"), from the game's saves.
    pub character_mods: Vec<String>,
    /// Mods turned off in the overlay (kept in data/disabled.json).
    pub disabled: std::collections::BTreeSet<String>,
    pub ui_rx: Receiver<ui_bridge::UiRequest>,
    /// Frame of the last snapshot published for the overlay.
    pub last_snapshot: u64,
}

impl Runtime {
    pub fn new(
        kindle_dir: PathBuf,
        config: KindleConfig,
        reload_rx: Receiver<String>,
        console_rx: Receiver<ConsoleRequest>,
        ui_rx: Receiver<ui_bridge::UiRequest>,
    ) -> Self {
        crate::store::set_root(&kindle_dir);
        let disabled = Self::read_disabled(&kindle_dir);
        let console_manifest = ModManifest {
            info: ModInfo {
                id: "console".into(),
                name: Some("Kindle console".into()),
                version: None,
                author: None,
                description: None,
                enabled: true,
                depends: Vec::new(),
                conflicts: Vec::new(),
                games: None,
                api: None,
            },
            permissions: Permissions::default(),
            settings: Vec::new(),
        };
        let mods_dir = kindle_dir.join("mods");
        let raw = config.developer.raw && !config.developer.raw_mods.is_empty();
        crate::ui_bridge::RAW_ACCESS.store(raw, std::sync::atomic::Ordering::Relaxed);
        if raw {
            crate::warn!("developer mode: raw access for {} (kindle.toml [developer])", config.developer.raw_mods.join(", "));
        }
        Self {
            console: LoadedMod { manifest: console_manifest, dir: mods_dir.clone(), lua: None, state: ModState::Running, reloads: 0, cpu: Default::default() },
            kindle_dir,
            mods_dir,
            config,
            mods: Vec::new(),
            reload_rx,
            pending_reloads: HashMap::new(),
            console_rx,
            player_loaded: false,
            last_tick: None,
            first_frame: None,
            cpu_rolled: None,
            safe_mode: false,
            character_mods: Vec::new(),
            disabled,
            mods_loaded: false,
            ui_rx,
            last_snapshot: 0,
        }
    }

    pub fn limit(&self) -> Duration {
        Duration::from_millis(self.config.runtime.script_time_limit_ms.max(10))
    }

    // ── loading ─────────────────────────────────────────────────────────────

    pub fn scan_mod_dirs(&self) -> Vec<String> {
        let mut dirs: Vec<String> = std::fs::read_dir(&self.mods_dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().join("mod.toml").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.') && !n.starts_with('_'))
            .collect();
        dirs.sort();
        dirs
    }

    pub fn load_all(&mut self) {
        let _ = std::fs::create_dir_all(&self.mods_dir);
        let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) } as u16 & 0x8000 != 0;
        if self.config.runtime.safe_mode || shift {
            self.safe_mode = true;
            let why = if shift { "Shift was held" } else { "safe_mode is set in kindle.toml" };
            crate::warn!("safe mode: {why}, so no mods are loaded this session");
            return;
        }
        let dirs = self.scan_mod_dirs();
        crate::info!("loading {} mod(s) from {}", dirs.len(), self.mods_dir.display());
        for dir in self.load_order(dirs) {
            self.load_mod_dir(&dir);
        }
        let running = self.mods.iter().filter(|m| matches!(m.state, ModState::Running)).count();
        crate::info!("{running}/{} mod(s) running", self.mods.len());
    }

    fn disabled_path(kindle_dir: &Path) -> PathBuf {
        kindle_dir.join("data").join("disabled.json")
    }

    pub fn read_disabled(kindle_dir: &Path) -> std::collections::BTreeSet<String> {
        std::fs::read_to_string(Self::disabled_path(kindle_dir))
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .map(|v| v.into_iter().collect())
            .unwrap_or_default()
    }

    /// Turns a mod on or off from the overlay (remembered across sessions).
    /// Turns the developer overlay on or off and saves it to kindle.toml.
    pub fn set_developer(&mut self, on: bool) {
        ui_bridge::DEVELOPER.store(on, Ordering::Relaxed);
        let path = self.kindle_dir.join("kindle.toml");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let saved = std::fs::write(&path, crate::config::set_toml_value(&text, "overlay", "developer", if on { "true" } else { "false" }));
        match saved {
            Ok(()) => crate::info!("developer overlay {}", if on { "on" } else { "off" }),
            Err(e) => crate::warn!("developer overlay: couldn't save to {}: {e}", path.display()),
        }
    }

    pub fn set_enabled(&mut self, id: &str, on: bool) {
        let changed = if on { self.disabled.remove(id) } else { self.disabled.insert(id.to_string()) };
        if changed {
            let path = Self::disabled_path(&self.kindle_dir);
            let list: Vec<&String> = self.disabled.iter().collect();
            let written = path
                .parent()
                .map(std::fs::create_dir_all)
                .transpose()
                .and_then(|_| std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap_or_default()));
            if let Err(e) = written {
                crate::warn!("{}: {e}", path.display());
            }
        }
        if let Some(dir) = self.mods.iter().find(|m| m.id() == id).and_then(|m| m.dir.file_name()) {
            let dir = dir.to_string_lossy().to_string();
            crate::info!("{id} {} in the overlay", if on { "enabled" } else { "disabled" });
            self.load_mod_dir(&dir);
        }
    }

    /// Mod folders ordered so each mod loads after the mods it depends on (otherwise by
    /// folder name; a dependency cycle keeps folder order and fails the dependency check).
    pub fn load_order(&self, dirs: Vec<String>) -> Vec<String> {
        let manifests: Vec<(String, Option<ModManifest>)> =
            dirs.into_iter().map(|d| (d.clone(), ModManifest::load(&self.mods_dir.join(&d).join("mod.toml")).ok())).collect();
        let dir_of: HashMap<String, usize> = manifests.iter().enumerate().filter_map(|(i, (_, m))| m.as_ref().map(|m| (m.info.id.clone(), i))).collect();
        fn visit(i: usize, ms: &[(String, Option<ModManifest>)], dir_of: &HashMap<String, usize>, state: &mut [u8], out: &mut Vec<usize>) {
            if state[i] != 0 {
                return; // done, or a cycle
            }
            state[i] = 1;
            if let Some(m) = &ms[i].1 {
                for dep in &m.info.depends {
                    if let Some(&j) = dir_of.get(dep) {
                        visit(j, ms, dir_of, state, out);
                    }
                }
            }
            state[i] = 2;
            out.push(i);
        }
        let mut state = vec![0u8; manifests.len()];
        let mut order = Vec::new();
        for i in 0..manifests.len() {
            visit(i, &manifests, &dir_of, &mut state, &mut order);
        }
        order.into_iter().map(|i| manifests[i].0.clone()).collect()
    }

    /// Why a mod can't start next to the running ones (missing dependencies, conflicts).
    pub fn interop_problem(&self, manifest: &ModManifest) -> Option<String> {
        let running = |id: &str| self.mods.iter().any(|m| m.id() == id && matches!(m.state, ModState::Running));
        let missing: Vec<&str> = manifest.info.depends.iter().map(String::as_str).filter(|d| !running(d)).collect();
        if !missing.is_empty() {
            return Some(format!("needs {} running first (depends in mod.toml)", missing.join(", ")));
        }
        let me = manifest.info.id.as_str();
        let clash: Vec<&str> = self
            .mods
            .iter()
            .filter(|m| matches!(m.state, ModState::Running))
            .filter(|m| manifest.info.conflicts.iter().any(|c| c == m.id()) || m.manifest.info.conflicts.iter().any(|c| c == me))
            .map(|m| m.id())
            .collect();
        (!clash.is_empty()).then(|| format!("conflicts with {} (conflicts in mod.toml)", clash.join(", ")))
    }

    /// (Re)loads the mod in `mods/<dir_name>`, or unloads it if it no longer exists or is now for
    /// another game (`games` in its mod.toml).
    pub fn load_mod_dir(&mut self, dir_name: &str) {
        if self.safe_mode {
            return;
        }
        let dir = self.mods_dir.join(dir_name);
        let reloads = match self.mods.iter().position(|m| m.dir == dir) {
            Some(i) => {
                let mut old = self.mods.remove(i);
                old.unload(true);
                old.reloads + 1
            }
            None => 0,
        };

        if !dir.join("mod.toml").is_file() {
            if reloads > 0 {
                crate::info!("mod in {dir_name} removed");
            }
            return;
        }

        let manifest = match ModManifest::load(&dir.join("mod.toml")) {
            Ok(m) => m,
            Err(e) => {
                log::write(Level::Error, dir_name, &e);
                return;
            }
        };
        let game = crate::bindings::get().map_or(crate::config::DEFAULT_GAME, |b| b.game);
        if !manifest.info.for_game(game) {
            let msg = format!("not loaded: it's for {}, not {} (games in its mod.toml)", manifest.info.game_names(), crate::config::game_name(game));
            log::write(Level::Info, dir_name, &msg);
            return;
        }
        if let Some(other) = self.mods.iter().find(|m| m.id() == manifest.info.id) {
            log::write(Level::Error, dir_name, &format!("mod id {:?} is already used by {}", manifest.info.id, other.dir.display()));
            return;
        }

        let problem = self.interop_problem(&manifest);
        let mut m = LoadedMod { manifest, dir: dir.clone(), lua: None, state: ModState::Running, reloads, cpu: Default::default() };
        if !m.manifest.info.enabled || self.disabled.contains(m.id()) {
            m.state = ModState::Disabled;
        } else if let Some(p) = problem {
            log::write(Level::Error, m.id(), &format!("not started: {p}"));
            mods::notify_stopped(m.manifest.info.name.as_deref().unwrap_or(m.id()), &p);
            m.state = ModState::Failed(p);
        } else {
            match self.start_mod(&mut m) {
                Ok(()) => {
                    let verb = if reloads > 0 { "reloaded" } else { "loaded" };
                    log::write(
                        Level::Info,
                        m.id(),
                        &format!(
                            "{verb} {} {} (modules: {})",
                            m.manifest.info.name.as_deref().unwrap_or(m.id()),
                            m.manifest.info.version.as_deref().unwrap_or(""),
                            if m.manifest.permissions.modules.is_empty() { "none".into() } else { m.manifest.permissions.modules.join(", ") }
                        ),
                    );
                }
                // start_mod has already logged the error.
                Err(e) => {
                    mods::notify_stopped(m.manifest.info.name.as_deref().unwrap_or(m.id()), &e);
                    m.state = ModState::Failed(e);
                }
            }
        }
        let pos = self.mods.partition_point(|x| x.dir < m.dir);
        self.mods.insert(pos, m);
    }

    pub fn start_mod(&mut self, m: &mut LoadedMod) -> Result<(), String> {
        let id = m.id().to_string();
        let logged = |e: String| {
            log::write(Level::Error, &id, &e);
            e
        };
        let main = m.dir.join("main.lua");
        // A mod of game files only (files\, no main.lua) has no script to run.
        let files_only = !main.exists() && m.dir.join("files").is_dir();
        let src = if files_only { String::new() } else { std::fs::read_to_string(&main).map_err(|e| logged(format!("main.lua: {e}")))? };
        let raw = self.config.developer.raw_for(&id);
        if raw {
            crate::warn!("{id}: raw access (kindle.toml [developer]): memory reads and traces, files where it says");
        }
        let lua = mods::create_state_with(&m.manifest, &m.dir, false, raw).map_err(|e| logged(mods::clean_error(&e)))?;
        mods::set_live(&id, Some(&lua));
        m.lua = Some(lua);
        m.state = ModState::Running;
        // On failure `call` logs the error itself.
        m.call(LOAD_TIME_LIMIT, "main.lua", |lua| lua.load(&src).set_name("@main.lua").exec()).ok_or_else(|| match &m.state {
            ModState::Failed(e) => e.clone(),
            _ => "failed to start".into(),
        })?;
        // A mod (re)loaded while a character is in the world gets player_loaded right away,
        // so hot reload behaves like a fresh load.
        if self.player_loaded {
            dispatch_event(m, "player_loaded", self.limit(), ());
        }
        Ok(())
    }

    fn ensure_console(&mut self) {
        if self.console.lua.is_none() {
            let created = mods::create_state(&self.console.manifest, &self.mods_dir, true).and_then(|lua| {
                // Shorthand globals for interactive use.
                let globals = crate::bindings::get().map_or("", |b| b.console_globals);
                lua.load(globals).set_name("=console globals").exec()?;
                Ok(lua)
            });
            match created {
                Ok(lua) => {
                    self.console.lua = Some(lua);
                    self.console.state = ModState::Running;
                }
                Err(e) => crate::error!("console state: {}", mods::clean_error(&e)),
            }
        }
    }

    /// Each mod with its settings (current values, for loaded mods).
    pub fn mod_rows(&mut self) -> Vec<ui_bridge::ModRow> {
        self.mods
            .iter()
            .map(|m| {
                let settings = m
                    .lua
                    .as_ref()
                    .and_then(|l| l.app_data_mut::<mods::ModCtx>())
                    .map(|mut c| {
                        let decls = c.settings.clone();
                        decls
                            .into_iter()
                            .map(|d| ui_bridge::SettingRow {
                                value: c.setting(&d.name).unwrap_or_else(|_| d.default.clone()),
                                default: d.default,
                                name: d.name,
                                label: d.label,
                                description: d.description,
                                kind: d.kind,
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                (m, settings)
            })
            .map(|(m, settings)| ui_bridge::ModRow {
                settings,
                cpu_ms: m.cpu.ms_per_second,
                cpu_by_handler: m.cpu.by_handler.clone(),
                disabled: matches!(m.state, ModState::Disabled),
                disabled_in_toml: !m.manifest.info.enabled,
                id: m.id().to_string(),
                name: m.manifest.info.name.clone().unwrap_or_else(|| m.id().to_string()),
                version: m.manifest.info.version.clone().unwrap_or_default(),
                author: m.manifest.info.author.clone().unwrap_or_default(),
                modules: m.manifest.permissions.modules.join(", "),
                status: m.status(),
                failed: matches!(m.state, ModState::Failed(_)),
                reloads: m.reloads,
            })
            .collect()
    }

    /// The overlay's requests: the runtime's own (reloads, turning mods on and off, settings) here,
    /// the rest (the game's) to `game`.
    pub fn process_ui_requests(&mut self, game: fn(&mut Runtime, UiRequest)) {
        while let Ok(req) = self.ui_rx.try_recv() {
            match req {
                UiRequest::Reload(None) => {
                    for d in self.scan_mod_dirs() {
                        self.load_mod_dir(&d);
                    }
                }
                UiRequest::Reload(Some(id)) => {
                    if let Some(dir) = self.mods.iter().find(|m| m.id() == id).and_then(|m| m.dir.file_name()) {
                        let dir = dir.to_string_lossy().to_string();
                        self.load_mod_dir(&dir);
                    }
                }
                UiRequest::SetEnabled(id, on) => self.set_enabled(&id, on),
                UiRequest::SetDeveloper(on) => self.set_developer(on),
                UiRequest::SetSetting(id, name, value) => {
                    let limit = self.limit();
                    let Some(m) = self.mods.iter_mut().find(|m| m.id() == id) else { continue };
                    let set = m.lua.as_ref().and_then(|l| l.app_data_mut::<mods::ModCtx>()).map(|mut c| c.set_setting(&name, value));
                    match set {
                        Some(Ok(v)) => {
                            crate::info!("overlay: {id} setting {name} = {v:?}");
                            let fns: Vec<Function> = m
                                .lua
                                .as_ref()
                                .and_then(|l| l.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("setting_changed")))
                                .unwrap_or_default();
                            for f in fns {
                                let ok = m.call(limit, "setting_changed", |lua| f.call::<()>((name.as_str(), mods::setting_to_lua(lua, &v)?)));
                                if ok.is_none() {
                                    break;
                                }
                            }
                        }
                        Some(Err(e)) => crate::warn!("overlay: {id}: {e}"),
                        None => {}
                    }
                }
                other => game(self, other),
            }
        }
    }

    pub fn subscribed(&self, event: &str) -> bool {
        self.mods
            .iter()
            .chain(std::iter::once(&self.console))
            .any(|m| m.lua.as_ref().and_then(|lua| lua.app_data_ref::<mods::ModCtx>().map(|c| !c.handlers.event_fns(event).is_empty())).unwrap_or(false))
    }

    pub fn all_mods_mut(&mut self) -> impl Iterator<Item = &mut LoadedMod> {
        self.mods.iter_mut().chain(std::iter::once(&mut self.console))
    }

    pub fn process_reloads(&mut self, now: Instant) {
        while let Ok(dir) = self.reload_rx.try_recv() {
            self.pending_reloads.insert(dir, now);
        }
        let due: Vec<String> = self.pending_reloads.iter().filter(|(_, t)| now.duration_since(**t) >= RELOAD_DEBOUNCE).map(|(d, _)| d.clone()).collect();
        for dir in due {
            self.pending_reloads.remove(&dir);
            self.load_mod_dir(&dir);
        }
    }

    pub fn process_console(&mut self) {
        while let Ok(req) = self.console_rx.try_recv() {
            let out = self.console_command(req.line.trim());
            let _ = req.reply.send(out);
        }
    }

    /// The mods' CPU time, as milliseconds per second: rolled over once a second (each game's frame
    /// calls this every frame).
    pub fn roll_cpu(&mut self, now: Instant) {
        let window = self.cpu_rolled.map(|t| now.duration_since(t));
        if window.is_none_or(|w| w >= Duration::from_secs(1)) {
            if let Some(w) = window {
                for m in self.all_mods_mut() {
                    m.cpu.roll(w);
                }
            }
            self.cpu_rolled = Some(now);
        }
    }

    /// Writes what changed in each mod's stores.
    pub fn flush_stores(&mut self) {
        for m in self.all_mods_mut() {
            if let Some(lua) = &m.lua
                && let Some(mut c) = lua.app_data_mut::<mods::ModCtx>()
            {
                c.flush_stores();
            }
        }
    }

    /// Messages between mods (kindle.send) since the last call, delivered to every other mod's handlers.
    pub fn deliver_messages(&mut self) {
        let messages = std::mem::take(&mut *mods::MESSAGES.lock().unwrap_or_else(|e| e.into_inner()));
        for msg in messages {
            let limit = self.limit();
            for m in self.all_mods_mut() {
                if m.id() == msg.from {
                    continue;
                }
                let fns: Vec<Function> = match &m.lua {
                    Some(lua) => lua
                        .app_data_ref::<mods::ModCtx>()
                        .map(|c| c.handlers.messages.iter().filter(|(_, n, _)| *n == msg.name).map(|(_, _, f)| f.clone()).collect())
                        .unwrap_or_default(),
                    None => continue,
                };
                for f in fns {
                    let ok = m.call(limit, "message", |lua| {
                        let data = match &msg.data {
                            Some(d) => crate::store::to_lua(lua, d)?,
                            None => mlua::Value::Nil,
                        };
                        f.call::<()>((data, msg.from.as_str()))
                    });
                    if ok.is_none() {
                        break;
                    }
                }
            }
        }
    }

    pub fn console_command(&mut self, line: &str) -> String {
        // Requests are one line; multi-line code arrives with its newlines as \u{1}.
        let line = &line.replace(crate::console::NEWLINE, "\n");
        if let Some(path) = line.strip_prefix(":run ") {
            let path = std::path::Path::new(path.trim().trim_matches('"'));
            let path = if path.is_absolute() { path.to_path_buf() } else { self.kindle_dir.join(path) };
            return match std::fs::read_to_string(&path) {
                Ok(code) => self.console_lua(&code, &format!("@{}", path.display())),
                Err(e) => format!("error: {}: {e}", path.display()),
            };
        }
        if let Some(cmd) = line.strip_prefix(':') {
            let mut parts = cmd.split_whitespace();
            return match (parts.next().unwrap_or(""), parts.next()) {
                ("help", _) | ("", _) => crate::bindings::get().map_or("", |b| b.console_help).trim().to_string(),
                ("mods", _) => {
                    if self.mods.is_empty() {
                        return format!("no mods in {}", self.mods_dir.display());
                    }
                    self.mods
                        .iter()
                        .map(|m| {
                            format!(
                                "{:<24} {:<10} {:<16} reloads={}  {}",
                                m.id(),
                                m.manifest.info.version.as_deref().unwrap_or("-"),
                                m.manifest.info.author.as_deref().unwrap_or("-"),
                                m.reloads,
                                m.status()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                ("reload", Some("all")) | ("reload", None) => {
                    let dirs: Vec<String> = self.scan_mod_dirs();
                    for d in &dirs {
                        self.load_mod_dir(d);
                    }
                    format!("reloaded {} mod(s)", dirs.len())
                }
                ("reload", Some(id)) => match self.mods.iter().find(|m| m.id() == id) {
                    Some(m) => {
                        let dir = m.dir.file_name().unwrap().to_string_lossy().to_string();
                        self.load_mod_dir(&dir);
                        self.mods.iter().find(|m| m.id() == id).map(|m| m.status()).unwrap_or_default()
                    }
                    None => format!("no mod with id {id:?}"),
                },
                ("log", n) => log::recent(n.and_then(|n| n.parse().ok()).unwrap_or(30)).join("\n"),
                ("status", _) => format!(
                    "Kindle {} | frame {} | uptime {:.0}s | player loaded: {} | mods: {}{} | dir: {}",
                    env!("CARGO_PKG_VERSION"),
                    mods::FRAME.load(Ordering::Relaxed),
                    mods::START.elapsed().as_secs_f64(),
                    self.player_loaded,
                    self.mods.len(),
                    if self.mods_loaded { "" } else { " (waiting for game data)" },
                    self.kindle_dir.display()
                ),
                (other, _) => format!("unknown command :{other} (try :help)"),
            };
        }
        if line.is_empty() {
            return String::new();
        }
        self.console_lua(line, "=console")
    }

    /// Runs Lua in the console's state and formats what it printed and returned.
    pub fn console_lua(&mut self, code: &str, name: &str) -> String {
        self.ensure_console();
        let limit = self.limit();
        let Some(lua) = self.console.lua.as_ref() else { return "console unavailable".into() };
        // Try as an expression first so `flags.get(123)` prints its value.
        let func = lua.load(format!("return {code}")).set_name(name).into_function().or_else(|_| lua.load(code).set_name(name).into_function());
        // An unfinished chunk (an open function/if/table): the client sends more lines.
        if let Err(mlua::Error::SyntaxError { incomplete_input: true, .. }) = &func {
            return crate::console::INCOMPLETE.to_string();
        }
        if let Some(mut c) = lua.app_data_mut::<mods::ModCtx>() {
            c.capture = Some(Vec::new());
        }
        let result = match func {
            Err(e) => Err(mods::clean_error(&e)),
            // try_call: a console error is reported but keeps the console's variables.
            Ok(f) => self.console.try_call(limit, |_| f.call::<MultiValue>(())),
        };
        let mut out: Vec<String> = Vec::new();
        if let Some(lua) = self.console.lua.as_ref()
            && let Some(mut c) = lua.app_data_mut::<mods::ModCtx>()
        {
            out.extend(c.capture.take().unwrap_or_default());
        }
        match result {
            Ok(values) if !values.is_empty() => out.push(mods::inspect_multi(&values)),
            Ok(_) => {}
            Err(e) => out.push(format!("error: {e}")),
        }
        out.join("\n")
    }
}

// ── dispatch helpers ───────────────────────────────────────────────────────

impl Runtime {
    /// A hook point's answer from the mods' Lua callbacks (kindle.game.on_decide), in load order:
    /// the first that names one of its answers; None if none does (the switch answers).
    pub fn dispatch_decide(&mut self, hook: &'static crate::catalogue::HookPoint, args: &[i64]) -> Option<usize> {
        let limit = self.limit();
        for m in self.all_mods_mut() {
            let fns: Vec<Function> = match &m.lua {
                Some(lua) => lua
                    .app_data_ref::<mods::ModCtx>()
                    .map(|c| c.handlers.decides.iter().filter(|(_, n, _)| n == hook.name).map(|(_, _, f)| f.clone()).collect())
                    .unwrap_or_default(),
                None => continue,
            };
            for f in fns {
                let a = MultiValue::from_vec(args.iter().map(|&v| LuaValue::Integer(v)).collect());
                if let Some(Some(answer)) = m.call(limit, hook.name, |_| f.call::<Option<String>>(a)) {
                    match hook.choices.iter().position(|c| *c == answer) {
                        Some(i) => return Some(i),
                        None => log::write(Level::Warn, m.id(), &format!("{}: no answer {answer:?} (answers: {})", hook.name, hook.choices.join(", "))),
                    }
                }
            }
        }
        None
    }
}

pub fn dispatch_event<A: mlua::IntoLuaMulti + Clone>(m: &mut LoadedMod, event: &str, limit: Duration, args: A) {
    let fns: Vec<Function> = match &m.lua {
        Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns(event)).unwrap_or_default(),
        None => return,
    };
    for f in fns {
        let a = args.clone();
        if m.call(limit, event, |_| f.call::<()>(a)).is_none() {
            return;
        }
    }
}

pub fn run_timers(m: &mut LoadedMod, now: Instant, limit: Duration) {
    let fns: Vec<Function> = {
        let Some(lua) = &m.lua else { return };
        let Some(mut c) = lua.app_data_mut::<mods::ModCtx>() else { return };
        let mut due = Vec::new();
        c.handlers.timers.retain_mut(|t| {
            if t.due > now {
                return true;
            }
            due.push(t.func.clone());
            match t.every {
                Some(every) => {
                    t.due = now + every;
                    true
                }
                None => false,
            }
        });
        due
    };
    for f in fns {
        if m.call(limit, "timer", |_| f.call::<()>(())).is_none() {
            return;
        }
    }
}

/// Runs the flag watchers (kindle.flags.on_set) whose flag changed to what they wait for; `flag`
/// reads one of the game's event flags.
pub fn run_flag_watches(m: &mut LoadedMod, limit: Duration, flag: fn(i64) -> Result<bool, String>) {
    let fired: Vec<(i64, Function)> = {
        let Some(lua) = &m.lua else { return };
        let Some(mut c) = lua.app_data_mut::<mods::ModCtx>() else { return };
        let mut fired = Vec::new();
        for w in &mut c.handlers.flags {
            let Ok(now) = flag(w.flag) else { continue };
            match w.last {
                None => w.last = Some(now),
                Some(prev) if prev != now => {
                    w.last = Some(now);
                    if now == w.on_set {
                        fired.push((w.flag, w.func.clone()));
                    }
                }
                _ => {}
            }
        }
        fired
    };
    for (flag, f) in fired {
        if m.call(limit, "flag watcher", |_| f.call::<()>(flag)).is_none() {
            return;
        }
    }
}

/// Runs `f` with the runtime, for game hooks. Hooks often fire on Havok worker threads while
/// Kindle's frame task holds the runtime, so other threads wait briefly for it (Kindle's
/// frame task never waits on game work, so this can't deadlock). On the frame task's own
/// thread the runtime is busy only because Kindle itself caused the hook (Lua calling into
/// the game): then mods are skipped at once. Returns None if mods were skipped.
pub fn with_runtime<R>(what: &str, f: impl FnOnce(&mut Runtime) -> R) -> Option<R> {
    if HALTED.load(Ordering::Relaxed) {
        return None;
    }
    let own_thread = crate::on_game_thread();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(8);
    let mut guard = loop {
        match RUNTIME.try_lock() {
            Ok(g) => break g,
            Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                if own_thread {
                    return None;
                }
                if std::time::Instant::now() >= deadline {
                    static WARNED: AtomicBool = AtomicBool::new(false);
                    if !WARNED.swap(true, Ordering::Relaxed) {
                        crate::warn!("{what}: a game call waited too long for Kindle and went unhandled");
                    }
                    return None;
                }
                std::thread::yield_now();
            }
        }
    };
    guard.as_mut().map(f)
}

/// A hook point's answer from the mods' Lua callbacks (catalogue.rs), or None.
pub fn dispatch_decide(hook: &'static crate::catalogue::HookPoint, args: &[i64]) -> Option<usize> {
    with_runtime(hook.name, |rt| rt.dispatch_decide(hook, args)).flatten()
}
