//! `kindle.toml` (runtime settings). The mod manifest (`mod.toml`) is in kindle-manifest.

use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct KindleConfig {
    pub runtime: RuntimeConfig,
    pub overlay: OverlayConfig,
    pub coop: CoopConfig,
    pub developer: DeveloperConfig,
}

/// `[developer]`: raw access for mods being developed. A distributed mod gets Kindle's named,
/// reviewed functions only, which can change the game but never reach the PC; raw memory reads,
/// traces and paths of its choosing could (a mod able to read any memory reads Kindle's own, its
/// session keys among them). So they exist only for the mods named here, with `raw` on: a mod
/// can't name itself, the player does, in this file. The game shows "developer mode" while any
/// mod has raw access.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DeveloperConfig {
    pub raw: bool,
    /// The ids of the mods that get raw access (mod.toml `id`).
    pub raw_mods: Vec<String>,
}

impl DeveloperConfig {
    /// Whether the mod `id` gets raw access.
    pub fn raw_for(&self, id: &str) -> bool {
        self.raw && self.raw_mods.iter().any(|m| m == id)
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CoopConfig {
    /// The port a host takes players on (TCP). To play over the internet, forward it on the
    /// host's router to the host's PC.
    pub port: u16,
    /// The host's internet address (as a "what's my IP" page shows it), for session codes that
    /// work over the internet. Empty: the code names the host's address on its own network.
    pub public_address: String,
}

impl Default for CoopConfig {
    fn default() -> Self {
        Self { port: 27860, public_address: String::new() }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    /// Draw Kindle's in-game screens (hooks the game's DirectX 11 presentation): the Mods
    /// screen, banners and toasts, and the developer overlay.
    pub enabled: bool,
    /// The developer overlay (console, log, game state, Find) opens with `toggle_key`.
    pub developer: bool,
    /// Key that opens/closes the developer overlay: F1-F12, Insert, Home, End, Delete, PageUp,
    /// PageDown, Backquote.
    pub toggle_key: String,
    /// Text size multiplier.
    pub font_scale: f32,
    /// Add a "Mods" entry to the game's title and pause menus that opens the Mods screen.
    pub game_menus: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self { enabled: true, developer: false, toggle_key: "F1".into(), font_scale: 1.5, game_menus: true }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    /// Reload a mod automatically when one of its files changes.
    pub hot_reload: bool,
    /// Listen for `kindle console` connections on 127.0.0.1. Off unless kindle.toml turns it on: the
    /// console runs any Lua for any program on the PC.
    pub console: bool,
    pub console_port: u16,
    /// debug / info / warn / error
    pub log_level: String,
    /// Longest a single script call (one event handler, one console line) may run before
    /// it is aborted, in milliseconds of the thread's CPU time (mods.rs Deadline). Protects the
    /// game from infinite loops.
    pub script_time_limit_ms: u64,
    /// Load no mods (also: hold Shift while the game starts, until the title screen).
    pub safe_mode: bool,
    /// Save backups to keep in `<kindle>/backups` (made when mods load, if the saves
    /// changed); 0 turns them off.
    pub save_backups: usize,
    /// Skip the logos at the start of the game (logos.rs).
    pub skip_logos: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            hot_reload: true,
            console: false,
            console_port: 27850,
            log_level: "info".into(),
            script_time_limit_ms: 200,
            safe_mode: false,
            save_backups: 10,
            skip_logos: true,
        }
    }
}

impl KindleConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

pub use kindle_manifest::{
    API_VERSION, DEFAULT_GAME, GAME_MODULES, GAMES, ModInfo, ModManifest, Permissions, Setting, SettingKind, SettingValue, game_name, set_toml_value,
};
