//! Kindle — Lua scripting runtime for Dark Souls III.
//!
//! Loaded into the game as `kindle.dll` (e.g. via Mod Engine 2 `external_dlls`). On load it
//! starts a background thread that waits for the game's task system, then registers one
//! frame task. All mod code runs inside that task, on the game's thread. The optional
//! overlay draws on the render thread and talks to the runtime through `ui_bridge`.

mod anim;
mod api;
mod archive;
mod archive_entries;
mod bonfire;
mod camera;
mod char_make;
mod chars;
mod death;
mod ds3;
mod emevd;
mod enemies;
mod equip;
mod event_patches;
mod files;
mod flag_blocks;
mod frame;
mod front_menu;
mod game;
mod game_events;
mod hks;
mod home_load;
mod hooks;
mod hud_draw;
mod icons;
mod inflate;
mod input_block;
mod lobby;
mod logos;
mod loot;
mod lua_values;
mod map_edits;
mod map_loading;
mod menu_movies;
mod mod_screens;
mod mods_screen;
mod msb;
mod multiplayer;
mod multiplayer_flow;
mod native_mods;
mod native_ui;
mod objects;
mod offline;
mod overlay;
mod p2p;
mod param_rows;
mod params;
mod party_saves;
mod predicate_trace;
mod quick_warp;
#[cfg(feature = "rig")]
mod rig;
mod saves;
mod search;
mod session;
mod session_hud;
mod session_players;
mod shop;
mod spawn;
mod speffect;
mod stubs;
mod talk_menu;
mod text;
mod text_override;
mod watchdog;

// Kindle's parts no game's code is in, at the paths the rest of this crate uses (crate::log, crate::info!).
pub use kindle_core::log;
use kindle_core::{
    bindings, catalogue, clipboard, config, console, crash, deflate, events, fmg, gfx, hook_sites, lock, memory, mods, natives, on_game_thread, patterns,
    runtime, store, trace, ui_bridge, watch, watcher,
};
use kindle_core::{debug, error, info, warn};

use std::ffi::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Duration;

use windows::Win32::Foundation::{HINSTANCE, HMODULE};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::core::BOOL;

use crate::config::KindleConfig;
use crate::runtime::{HALTED, RUNTIME, Runtime, with_runtime};

const DLL_PROCESS_ATTACH: u32 = 1;
const DLL_PROCESS_DETACH: u32 = 0;

/// # Safety
/// Called by the Windows loader.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        let mut buf = [0u16; 1024];
        let len = unsafe { GetModuleFileNameW(Some(HMODULE(module.0)), &mut buf) } as usize;
        let dll_path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        let module_raw = module.0 as usize;
        let _ = std::thread::Builder::new().name("kindle-init".into()).spawn(move || init(dll_path, module_raw));
    }
    if reason == DLL_PROCESS_DETACH {
        crash::exiting();
    }
    BOOL(1)
}

fn init(dll_path: PathBuf, module_raw: usize) {
    let kindle_dir = dll_path.parent().map(PathBuf::from).unwrap_or_default();
    std::sync::LazyLock::force(&mods::START); // uptime counts from here
    log::init(&kindle_dir.join("logs"));
    crash::install(&kindle_dir.join("logs"));
    panic::set_hook(Box::new(|p| crate::error!("panic: {p}")));
    bindings::set(&api::BINDINGS);
    info!("Kindle {} starting ({})", env!("CARGO_PKG_VERSION"), dll_path.display());

    let config = match KindleConfig::load(&kindle_dir.join("kindle.toml")) {
        Ok(c) => c,
        Err(e) => {
            error!("{e}; using defaults");
            KindleConfig::default()
        }
    };
    if let Some(level) = log::Level::parse(&config.runtime.log_level) {
        log::set_level(level);
    }
    patterns::set_cache(kindle_dir.join("cache").join("patterns.json"));
    archive::set_key_cache(kindle_dir.join("cache").join("archive_keys.txt"));
    event_patches::install(kindle_dir.parent().unwrap_or(&kindle_dir));

    match ds3::supported_version() {
        Ok(v) => {
            info!("game version {v} supported");
            #[cfg(feature = "rig")]
            rig::at_start();
        }
        Err(e) => {
            error!("{e}; Kindle is disabled");
            return;
        }
    }
    if config.runtime.skip_logos {
        logos::install();
    }
    // Mods' replacement files and Kindle's menu edits: hooked before the game starts loading
    // what they replace.
    let shift = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x10) } as u16 & 0x8000 != 0;
    // No official online play while Kindle runs: its co-op changes the game's events and params
    // with no mod on (safe mode and Shift leave the game as it is).
    let loads_mods = !config.runtime.safe_mode && !shift;
    offline::start(&kindle_dir, loads_mods);
    // Kindle's own co-op connection (not with Seamless Co-op, which has its own).
    if unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(windows::core::w!("ds3sc.dll")) }.is_err() {
        p2p::install();
        party_saves::install();
    }
    if !config.runtime.safe_mode && !shift {
        let game_dir = kindle_dir.parent().unwrap_or(&kindle_dir).to_path_buf();
        crate::archive::watch_keys();
        let movies = if config.overlay.enabled && config.overlay.game_menus { menu_movies::movies() } else { Vec::new() };
        files::install(&kindle_dir.join("mods"), &runtime::Runtime::read_disabled(&kindle_dir), &game_dir, &kindle_dir.join("cache"), movies);
    }
    if let Err(e) = ds3::wait_for_task_system(Duration::from_secs(300)) {
        error!("{e}; Kindle is disabled");
        return;
    }

    let (reload_tx, reload_rx) = mpsc::channel();
    let (console_tx, console_rx) = mpsc::channel();
    let (ui_tx, ui_rx) = mpsc::channel();
    ui_bridge::install(ui_tx, console_tx.clone());
    let mods_dir = kindle_dir.join("mods");
    let _ = std::fs::create_dir_all(&mods_dir);

    if config.runtime.hot_reload {
        watcher::start(&mods_dir, reload_tx);
    }
    p2p::configure(config.coop.port, &config.coop.public_address);
    quick_warp::install();
    inflate::install();
    text_override::install();
    p2p::install_channel_stop();
    if config.runtime.console {
        console::start(config.runtime.console_port, console_tx);
    }

    front_menu::ENABLED.store(config.overlay.enabled && config.overlay.game_menus, Ordering::Relaxed);
    ui_bridge::DEVELOPER.store(config.overlay.developer, Ordering::Relaxed);
    let overlay_settings = config.overlay.enabled.then(|| (config.overlay.toggle_key.clone(), config.overlay.font_scale, kindle_dir.join("overlay.ini")));

    *RUNTIME.lock().unwrap_or_else(|e| e.into_inner()) = Some(Runtime::new(kindle_dir, config, reload_rx, console_rx, ui_rx));

    // Registered once for the life of the process.
    if let Err(e) = ds3::register_task(tick, ds3::FRAME_BEGIN, Duration::from_secs(300)) {
        error!("{e}; Kindle is disabled");
        return;
    }
    info!("frame task registered; waiting for the first frame");

    if let Some((key_name, font_scale, ini)) = overlay_settings {
        start_overlay(module_raw, &key_name, font_scale, ini);
    }
}

fn start_overlay(module_raw: usize, key_name: &str, font_scale: f32, ini: PathBuf) {
    use hudhook::Hudhook;
    use hudhook::hooks::dx11::ImguiDx11Hooks;

    let (key, key_name) = match overlay::parse_key(key_name) {
        Some(k) => (k, key_name.to_string()),
        None => {
            warn!("overlay: unknown toggle_key {key_name:?}; using F1");
            (hudhook::imgui::Key::F1, "F1".to_string())
        }
    };
    let scale = if font_scale.is_finite() && font_scale > 0.25 { font_scale } else { 1.5 };
    let overlay = overlay::Overlay::new(key, key_name, scale, ini);
    let hinstance = hudhook::windows::Win32::Foundation::HINSTANCE(module_raw as *mut c_void);
    match Hudhook::builder().with::<ImguiDx11Hooks>(overlay).with_hmodule(hinstance).build().apply() {
        Ok(()) => info!("overlay: DirectX 11 hooks applied"),
        Err(e) => error!("overlay: could not hook DirectX 11 ({e:?}); the overlay is disabled"),
    }
}

/// Damage hook: returns true to cancel the hit.
pub(crate) fn dispatch_damage(hit: &mut hooks::Damage) -> bool {
    with_runtime("damage", |rt| frame::dispatch_damage(rt, hit)).unwrap_or(false)
}

pub(crate) fn dispatch_animation(anim: &mut hooks::Animation) {
    with_runtime("animation", |rt| frame::dispatch_animation(rt, anim));
}

pub(crate) fn dispatch_effect(effect: &mut hooks::Effect) {
    with_runtime("effect", |rt| frame::dispatch_effect(rt, effect));
}

pub(crate) fn dispatch_item_use(used: &mut hooks::ItemUse) {
    with_runtime("item use", |rt| frame::dispatch_item_use(rt, used));
}

pub(crate) fn dispatch_item_lot(roll: &mut hooks::ItemLot) {
    with_runtime("item lot", |rt| frame::dispatch_item_lot(rt, roll));
}

/// Talk hook, while the game builds a list menu: which mod options to add.
pub(crate) fn menu_options_for(entries: &[(i32, i32)]) -> Vec<talk_menu::Chosen> {
    with_runtime("menu", |rt| frame::menu_options(rt, entries)).unwrap_or_default()
}

/// Talk query hook, when a mod's menu entry is picked: runs its callback.
pub(crate) fn menu_pick(pick: &talk_menu::Pick) {
    with_runtime("menu", |rt| frame::menu_pick(rt, pick));
}

/// HKS env/act hooks.
pub(crate) fn dispatch_hks(call: &mut hks::Call) {
    with_runtime("hks", |rt| frame::dispatch_hks(rt, call));
}

/// A script's kindle(name, ...) call.
pub(crate) fn dispatch_hks_call(call: &mut hks::ScriptCall) {
    with_runtime("hks call", |rt| frame::dispatch_hks_call(rt, call));
}

/// Trace output for debug.trace_hks (the player's calls): each env id/argument combination
/// once, act calls whenever their arguments change.
pub(crate) fn trace_hks(act: bool, id: &hks::HksId, args: &[hks::HksValue], result: i32) {
    use std::collections::HashSet;
    static SEEN: std::sync::Mutex<Option<HashSet<String>>> = std::sync::Mutex::new(None);
    let shown: Vec<String> = args.iter().skip(1).map(|a| format!("{a:?}")).collect();
    let name = hks::id_name(act, id).map(|n| format!(" {n}")).unwrap_or_default();
    let line = format!("hks {} {id}{name}({})", if act { "act" } else { "env" }, shown.join(", "));
    let key = if act { line.clone() } else { format!("{line} = {result}") };
    if !SEEN.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new).insert(key) {
        return;
    }
    info!("{line} = {result}");
}

fn tick() {
    if HALTED.load(Ordering::Relaxed) {
        return;
    }
    hks::set_game_thread();
    kindle_core::set_tick_thread();
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        hook_sites::keep_frame();
        front_menu::tick();
        char_make::install();
        native_mods::tick();
        text_override::tick();
        session::frame();
        multiplayer_flow::frame();
        multiplayer::frame();
        home_load::frame();
        let mut guard = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rt) = guard.as_mut() {
            frame::tick(rt);
        }
    }));
    if result.is_err() {
        HALTED.store(true, Ordering::Relaxed);
        error!("Kindle hit an internal error and has stopped running mods (the game continues). See the panic above.");
    }
}
