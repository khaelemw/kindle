//! Kindle for Elden Ring: Kindle's core (kindle-core) bound to the game, with the game's params, event
//! flags, items, player and text so far. A mod's Lua state has the sandbox, Kindle's own API (kindle.log,
//! kindle.on, timers, the store, other mods, settings, kindle.game) and the game's modules (prelude.lua):
//! kindle.params (the tables the game has loaded, params.rs, by the paramdefs the DLL embeds; new rows,
//! param_rows.rs), kindle.flags (flags.rs: read from the game's flag store, set through its setter,
//! watched each frame), kindle.items (items.rs: given, taken and counted through the game's own
//! functions; by name through text.rs), kindle.player (player.rs: the character in the world and its
//! saved state, where the game's own code keeps them; its save slot keeps a mod's character store) and
//! kindle.text (text.rs: the game's own text, read in its order, replaced and added to live, its release
//! hooked) and kindle.events (emevd.rs: the game's own event script commands, run through its dispatcher
//! on an event instance of Kindle's; the player's warp to a grace is one) with kindle.ui's messages on
//! them (ui.rs: a mod's text shown by the game's message commands), and kindle.world (world.rs:
//! characters by entity id through the game's own lookup, their HP, position and effects). No other
//! game modules, characters by handle or catalogue entries yet: the port fills them in, with more hooks.
//!
//! Loaded into the game as `kindle\kindle_er.dll` (by the proxy, dinput8.dll), it starts on a thread of
//! its own: its log, the game build check, where the game keeps the managers Kindle will read (er.rs,
//! from the exe's code by singletons.rs), the hooks on the game's text, mods' replacement game files
//! (files.rs: a mod's `files` folder served in place of the archives'), then the mods, the console and
//! hot reload, on Kindle's own frame (frame.rs) until the game runs it as one of its tasks (task.rs), and
//! the overlay (overlay.rs: hudhook's DirectX 12 hooks, kindle.ui's banners and toasts, the developer
//! overlay). Only
//! a mod changes the game (its param edits and rows, its flags, its items, its player, its text).

pub mod archive;
pub mod emevd;
pub mod er;
pub mod files;
pub mod flags;
pub mod frame;
pub mod game_events;
pub mod input_block;
pub mod items;
pub mod openssl;
pub mod overlay;
pub mod param_rows;
pub mod params;
pub mod player;
pub mod search;
pub mod singletons;
pub mod task;
pub mod text;
pub mod ui;
pub mod world;

use std::ffi::c_void;
use std::panic;
use std::path::PathBuf;
use std::sync::mpsc;

use kindle_core::bindings::{self, Bindings};
use kindle_core::catalogue::Catalogue;
use kindle_core::config::KindleConfig;
use kindle_core::mods::{self, ModCtx};
use kindle_core::runtime::{RUNTIME, Runtime};
use kindle_core::{console, crash, error, info, log, ui_bridge, warn, watcher};
use mlua::{Lua, Table};
use windows::Win32::Foundation::{HINSTANCE, HMODULE};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::core::BOOL;

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
    panic::set_hook(Box::new(|p| error!("panic: {p}")));
    bindings::set(&BINDINGS);
    info!("Kindle {} for Elden Ring starting ({})", env!("CARGO_PKG_VERSION"), dll_path.display());

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
    match er::supported_version() {
        Ok(v) => info!("game version {v} supported"),
        Err(e) => {
            error!("{e}; Kindle is disabled");
            return;
        }
    }
    er::log_managers();
    openssl::install();
    text::install();
    // Mods' changes to event files: the maps' files read from the game's archives, and served changed.
    kindle_core::event_patches::set_reader(|map| archive::undcx(&files::read_original(&format!("event/{map}.emevd.dcx"))?));
    if !config.runtime.safe_mode {
        let game_dir = kindle_dir.parent().unwrap_or(&kindle_dir);
        files::install(&kindle_dir.join("mods"), &Runtime::read_disabled(&kindle_dir), game_dir, &kindle_dir.join("cache"));
        kindle_core::event_patches::on_change(files::events_changed);
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
    if config.runtime.console {
        console::start(config.runtime.console_port, console_tx);
    }
    ui_bridge::DEVELOPER.store(config.overlay.developer, std::sync::atomic::Ordering::Relaxed);
    let overlay_settings = config.overlay.enabled.then(|| (config.overlay.toggle_key.clone(), config.overlay.font_scale, kindle_dir.join("overlay.ini")));
    *RUNTIME.lock().unwrap_or_else(|e| e.into_inner()) = Some(Runtime::new(kindle_dir, config, reload_rx, console_rx, ui_rx));
    let _ = std::thread::Builder::new().name("kindle-frame".into()).spawn(frame::run);
    info!("Kindle's frame runs on its own thread until the game runs it as one of its tasks");
    if let Some((key_name, font_scale, ini)) = overlay_settings {
        start_overlay(module_raw, &key_name, font_scale, ini);
    }
}

/// The overlay on the game's DirectX 12 (hudhook's hooks on its swap chain and command queue).
fn start_overlay(module_raw: usize, key_name: &str, font_scale: f32, ini: PathBuf) {
    use hudhook::Hudhook;
    use hudhook::hooks::dx12::ImguiDx12Hooks;

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
    match Hudhook::builder().with::<ImguiDx12Hooks>(overlay).with_hmodule(hinstance).build().apply() {
        Ok(()) => info!("overlay: DirectX 12 hooks applied"),
        Err(e) => error!("overlay: could not hook DirectX 12 ({e:?}); the overlay is disabled"),
    }
}

/// What Kindle's mod runtime asks of Elden Ring (kindle-core's bindings.rs): its params, flags, items, player and text so far.
pub static BINDINGS: Bindings = Bindings {
    game: "er",
    exe: "eldenring.exe",
    natives,
    prelude: include_str!("prelude.lua"),
    raw_state,
    character_slot,
    character_name,
    character,
    unloading,
    unloaded,
    param_caps: params::caps,
    paramdefs: params::PARAMDEFS,
    add_param_rows: param_rows::add_rows,
    catalogue: &CATALOGUE,
    events: &[game_events::EVENTS],
    console_help: HELP,
    console_globals: "events, flags, items, log, params, player, text, ui, world = kindle.events, kindle.flags, kindle.items, kindle.log, kindle.params, kindle.player, kindle.text, kindle.ui, kindle.world",
};

/// The game's catalogue entries: none yet.
static CATALOGUE: Catalogue = Catalogue { hooks: &[], fields: &[], functions: &[], events: &[] };

const HELP: &str = r#"
Kindle console for Elden Ring: type Lua, it runs inside the game with Kindle's own API, its params, flags, items, player, events, text, ui and world.
  kindle.log.info("hello")                     write to Kindle's log
  params.get("EquipParamWeapon", 1000000).weight  a param field (params.tables(), params.fields(t))
  flags.get(71801), flags.set(71801, true)     an event flag (here a grace's)
  items.give("Smithing Stone [1]", 5)          give the player items, by name or items.goods(10100), on a character
  items.find("smithing"), items.name(items.weapon(1000005))  items by name, an item's name
  player.stats(), player.set_runes(10000)      the player's attributes, runes, hp, position ...
  text.get("place_name", 10000), text.set(...)  the game's text (here Stormveil Castle; text.categories)
  player.warp("Church of Elleh")               warp to a site of grace (player.graces())
  events.apply_effect(10000, 4330)             one of the game's event script commands (events.run)
  ui.message("hello"), ui.game_banner("god_slain")  the game's own message and banners
  world.character(1042360800), player.effects()  a character by entity id, active effects
Shorthands: events, flags, items, log, params, player, text, ui and world are globals here.
Commands:
  :mods            list mods and their status
  :reload [id]     reload one mod, or all
  :log [n]         last n log lines (default 30)
  :status          runtime status
  :run <file>      run a Lua file in the console (path relative to the Kindle folder)
  :help            this text
Code can span lines: an unfinished block (function, if, table) waits for the rest.
"#;

/// The game's natives, after Kindle's own (kindle.params' are Kindle's): kindle.flags', kindle.items',
/// kindle.player's, kindle.events', kindle.ui's, kindle.world's and kindle.text's (items by name among them).
fn natives(lua: &Lua, n: &Table) -> mlua::Result<()> {
    flags::register(lua, n)?;
    items::register(lua, n)?;
    player::register(lua, n)?;
    emevd::register(lua, n)?;
    ui::register(lua, n)?;
    world::register(lua, n)?;
    text::register(lua, n)
}

/// A state with raw access: nothing more yet.
fn raw_state(_: &Lua) -> mlua::Result<()> {
    Ok(())
}

/// The loaded character's save slot, for a mod's character store (None with no character loaded).
fn character_slot() -> Result<Option<i32>, String> {
    player::save_slot()
}

fn character_name() -> Result<String, String> {
    player::name()
}

/// Kindle doesn't look up Elden Ring's characters by handle yet.
fn character(_: i64) -> Result<usize, String> {
    Err("Kindle doesn't look up Elden Ring's characters yet".into())
}

/// A mod is unloading: nothing of the game's to give back yet.
fn unloading(_: &str, _: Option<&ModCtx>) {}

/// A mod has unloaded: its param edits are undone (rows it added stay for the session), the game's
/// text is back where it replaced it, and its changes to event files are dropped (the maps' next loads
/// are without them).
fn unloaded(ctx: &mut ModCtx) {
    params::revert_journal(&ctx.id, &mut ctx.journal);
    text::reset_owner(&ctx.id);
    kindle_core::event_patches::remove_owner(&ctx.id);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kindle_core::config::{GAME_MODULES, KindleConfig, ModInfo, ModManifest, Permissions};
    use kindle_core::mods::{ModCtx, create_state};
    use kindle_core::runtime::Runtime;
    use mlua::{Function, Lua};

    use super::BINDINGS;

    /// A mod that asks for every game module (Dark Souls III's names: Elden Ring has params, flags, items, player, events and text so far).
    fn manifest(id: &str) -> ModManifest {
        ModManifest {
            info: ModInfo {
                id: id.into(),
                name: None,
                version: None,
                author: None,
                description: None,
                enabled: true,
                depends: Vec::new(),
                conflicts: Vec::new(),
                games: Some(vec!["er".into()]),
                api: None,
            },
            permissions: Permissions { modules: GAME_MODULES.iter().map(|s| s.to_string()).collect() },
            settings: Vec::new(),
        }
    }

    fn state(id: &str) -> Lua {
        kindle_core::bindings::set(&BINDINGS);
        let dir: PathBuf = std::env::temp_dir().join(format!("kindle-er-test-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        create_state(&manifest(id), &dir, false).unwrap()
    }

    fn eval<T: mlua::FromLuaMulti>(lua: &Lua, src: &str) -> mlua::Result<T> {
        lua.load(src).eval()
    }

    #[test]
    fn a_mod_gets_kindles_own_api_params_flags_items_player_and_text() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let lua = state("er-api");
        let kindle_own = "return type(kindle.log.info) == 'function' and type(kindle.on) == 'function' and type(kindle.every) == 'function' \
                          and type(kindle.store.get_global) == 'function' and type(kindle.mods.get) == 'function' \
                          and type(kindle.settings.get) == 'function' and type(kindle.game.catalogue) == 'function'";
        assert!(eval::<bool>(&lua, kindle_own).unwrap());
        let params = "return type(kindle.params.get) == 'function' and type(kindle.params.add_rows) == 'function' \
                      and type(kindle.params.tables) == 'function' and type(kindle.params.reset) == 'function'";
        assert!(eval::<bool>(&lua, params).unwrap());
        let flags = "return type(kindle.flags.get) == 'function' and type(kindle.flags.set) == 'function' \
                     and type(kindle.flags.block) == 'function' and type(kindle.flags.on_set) == 'function'";
        assert!(eval::<bool>(&lua, flags).unwrap());
        let items = "local i = kindle.items return type(i.give) == 'function' and type(i.remove) == 'function' \
                     and type(i.count) == 'function' and type(i.has) == 'function' and i.ring == i.talisman \
                     and type(i.find) == 'function' and type(i.name) == 'function' and type(i.set_name) == 'function'";
        assert!(eval::<bool>(&lua, items).unwrap());
        let player = "local p = kindle.player return type(p.stats) == 'function' and type(p.set_hp) == 'function' \
                      and type(p.max_stamina) == 'function' and p.souls == p.runes and p.soul_level == p.level";
        assert!(eval::<bool>(&lua, player).unwrap());
        let text = "local t = kindle.text return type(t.get) == 'function' and type(t.set) == 'function' \
                    and type(t.item) == 'function' and t.categories[1] == 'weapon_name'";
        assert!(eval::<bool>(&lua, text).unwrap());
        let ours = "return type(kindle.world.character) == 'function' and type(kindle.world.characters) == 'function' and type(kindle.events.run) == 'function'                     and type(kindle.events.warp_player) == 'function' and type(kindle.ui.message) == 'function'                     and type(kindle.player.warp) == 'function' and type(kindle.player.has_effect) == 'function'";
        assert!(eval::<bool>(&lua, ours).unwrap());
        let ds3 = "return kindle.world.spawn == nil and kindle.debug == nil and kindle.dlc_installed == nil and kindle.flags.boss == nil \
                   and kindle.player.equipment == nil";
        assert!(eval::<bool>(&lua, ds3).unwrap());
        let catalogue = "local c = kindle.game.catalogue() return next(c.hooks) == nil and next(c.fields) == nil and next(c.events) == nil";
        assert!(eval::<bool>(&lua, catalogue).unwrap());
        // Outside the game no params are loaded: each use says so, and a row id must be an integer.
        let e = lua.load("kindle.params.tables()").exec().unwrap_err().to_string();
        assert!(e.contains("params are not loaded yet"), "{e}");
        let e = lua.load("kindle.params.add_row('EquipParamGoods', 3000000, 'Ember')").exec().unwrap_err().to_string();
        assert!(e.contains("row must be an integer id"), "{e}");
        // Nor flags; a watch waits for them.
        let e = lua.load("kindle.flags.get(71801)").exec().unwrap_err().to_string();
        assert!(e.contains("event flags are not available yet"), "{e}");
        let e = lua.load("kindle.flags.get(-1)").exec().unwrap_err().to_string();
        assert!(e.contains("out of range"), "{e}");
        assert!(eval::<u64>(&lua, "return kindle.flags.on_set(71801, function() end)").is_ok());
        // Items are made and read anywhere; giving, taking and counting need the game.
        let made = "local i = kindle.items local k, id = i.info(i.ash(10000)) \
                    return i.weapon(1000000) == 1000000 and i.goods(10100) == 0x40000000 + 10100 and k == 'ash' and id == 10000";
        assert!(eval::<bool>(&lua, made).unwrap());
        let e = lua.load("kindle.items.goods(-1)").exec().unwrap_err().to_string();
        assert!(e.contains("28 bits"), "{e}");
        let e = lua.load("kindle.items.give(kindle.items.goods(10100), 0)").exec().unwrap_err().to_string();
        assert!(e.contains("1 to 999"), "{e}");
        let e = lua.load("kindle.items.count(0x30000000)").exec().unwrap_err().to_string();
        assert!(e.contains("no category"), "{e}");
        let e = lua.load("kindle.items.give(kindle.items.goods(10100), 5)").exec().unwrap_err().to_string();
        assert!(e.contains("weren't found"), "{e}");
        // Nor the game's text, which names items.
        for src in ["kindle.items.name(kindle.items.goods(10100))", "kindle.items.give('Smithing Stone [1]')", "kindle.text.get('place_name', 10000)"] {
            let e = lua.load(src).exec().unwrap_err().to_string();
            assert!(e.contains("text isn't loaded yet"), "{src}: {e}");
        }
        let e = lua.load("kindle.text.get('places', 10000)").exec().unwrap_err().to_string();
        assert!(e.contains("unknown text category"), "{e}");
        // Nor the player, whose layout is checked against the game's code.
        let e = lua.load("kindle.player.stats()").exec().unwrap_err().to_string();
        assert!(e.contains("doesn't know where this build keeps"), "{e}");
    }

    /// A mod's `kindle.player` on a player laid out as the game's: its name, attributes, runes, HP and
    /// position read, and set within the game's bounds; and the save slot and name a mod's character store
    /// is kept by.
    #[test]
    fn a_mod_reads_and_sets_the_player() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let p = crate::player::tests::Player::serve(2);
        let lua = state("er-player");
        let src = r#"
            local p = kindle.player
            assert(p.loaded() and p.name() == "Tarnished")
            local s = p.stats()
            assert(s.vigor == 10 and s.mind == 11 and s.arcane == 17, tostring(s.vigor))
            assert(p.level() == 9 and p.runes() == 1234 and p.total_runes() == 5678)
            p.add_runes(100)
            assert(p.souls() == 1334)
            p.set_stats({ dexterity = 15 })
            assert(p.stats().dexterity == 15 and p.soul_level() == 10)
            assert(p.hp() == 400 and p.max_hp() == 500 and p.max_fp() == 80)
            p.heal()
            assert(p.hp() == 500 and p.fp() == 80 and p.stamina() == 100)
            local x, y, z = p.position()
            assert(x == 1.5 and y == -2.0 and z == 300.25)
        "#;
        lua.load(src).exec().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.pgd[0x6C / 4], 1334);
        assert_eq!(((BINDINGS.character_slot)(), (BINDINGS.character_name)()), (Ok(Some(2)), Ok("Tarnished".into())));
        drop(p);
        let e = lua.load("kindle.player.name()").exec().unwrap_err().to_string();
        assert!(e.contains("doesn't know where this build keeps"), "{e}");
    }

    /// A mod's `kindle.text` and items by name on text laid out as the game's: names in the game's order,
    /// items found by name, an item renamed, text the game hasn't added, and the game's text back as the
    /// mod unloads.
    #[test]
    fn a_mod_reads_and_replaces_text() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = crate::text::tests::Text::items();
        let lua = state("er-text");
        let src = r#"
            local i, t = kindle.items, kindle.text
            assert(i.name(i.weapon(1000005)) == "Dagger +5")
            assert(i.weapon("Dagger +5") == i.weapon(1000005) and i.id("heavy dagger") == i.weapon(1000100))
            assert(i.goods("Longsword") == i.goods(2000000))
            assert(not pcall(i.id, "Longsword"))
            local found = i.find("smithing")
            assert(#found == 2 and found[1].name == "Smithing Stone [1]" and found[1].category == "goods" and found[1].id == 10100)
            assert(t.get("goods_name", 10101) == "Smithing Stone [2] (patched)" and t.get(10, 10101) == "Smithing Stone [2]")
            assert(t.item("Dagger", "description") == "A common dagger." and t.get("place_name", 10000) == nil)
            i.set_name("Smithing Stone [1]", "Kindle Stone")
            assert(i.name(i.goods(10100)) == "Kindle Stone" and i.goods("kindle stone") == i.goods(10100))
            assert(t.set("weapon_name", 1000100, "Hefty Dagger") == 1 and i.name(i.weapon(1000103)) == "Hefty Dagger +3")
            assert(t.set("weapon_name", 1000200, "Keen") == 1 and i.name(i.weapon(1000200)) == "Keen" and i.weapon("keen") == i.weapon(1000200))
            assert(t.reset("weapon_name", 1000100) == 1 and i.name(i.weapon(1000100)) == "Heavy Dagger")
        "#;
        lua.load(src).exec().unwrap_or_else(|e| panic!("{e}"));
        let goods = crate::text::category("goods_name").unwrap();
        assert_eq!(crate::text::get(&goods, 10100).unwrap().as_deref(), Some("Kindle Stone"));
        (BINDINGS.unloaded)(&mut lua.app_data_mut::<ModCtx>().unwrap());
        assert_eq!(crate::text::get(&goods, 10100).unwrap().as_deref(), Some("Smithing Stone [1]"));
        assert_eq!(crate::text::get(&crate::text::category("weapon_name").unwrap(), 1000200).unwrap(), None);
        drop(text);
    }

    /// A mod's event commands, named and by number, reach the game's dispatcher (a fake one here) with
    /// their arguments packed as the game's event files pack them; the player's effects are commands on
    /// entity 10000.
    #[test]
    fn a_mod_runs_the_games_event_commands() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        crate::emevd::tests::serve();
        let lua = state("er-events");
        let src = r#"
            local e = kindle.events
            assert(type(e.run(2007, 4, "i", 100)) == "number")
            e.banner("god_slain")
            e.apply_effect(10000, 4330)
            e.set_ai(2046400800, false, function(done) end)
            assert(not pcall(e.banner, "no_such_banner") and not pcall(e.run, 2007, 4, "x", 1))
        "#;
        lua.load(src).exec().unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(crate::emevd::run_queued().len(), 4);
        let seen: Vec<(i32, i32, Vec<u8>)> = crate::emevd::tests::SEEN.with(|s| s.borrow().iter().map(|(b, i, a, _, _)| (*b, *i, a.clone())).collect());
        let words = |w: &[u32]| -> Vec<u8> { w.iter().flat_map(|v| v.to_le_bytes()).collect() };
        assert_eq!(seen[0], (2007, 4, words(&[100])));
        assert_eq!(seen[1], (2007, 2, words(&[27])));
        assert_eq!(seen[2], (2004, 8, words(&[10000, 4330])));
        assert_eq!(seen[3], (2004, 1, words(&[2046400800, 0])));
        assert_eq!(lua.app_data_ref::<ModCtx>().unwrap().handlers.replies.len(), 1);
    }

    /// A mod's message: its text added to EventTextForMap under an id of Kindle's, the game's message
    /// command run with that id; a banner by name.
    #[test]
    fn a_mod_shows_the_games_own_messages() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = crate::text::tests::Text::serve(&[(10, &[(10100, "Smithing Stone [1]")]), (34, &[(4200, "Somewhere, a heavy door has opened")])]);
        crate::emevd::tests::serve();
        let lua = state("er-ui");
        lua.load(r#"kindle.ui.message("Hello from Kindle"); kindle.ui.message("Saved", "full_screen"); kindle.ui.game_banner("great_enemy_felled")"#)
            .exec()
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(lua.load(r#"kindle.ui.message("x", "sideways")"#).exec().is_err());
        assert_eq!(crate::emevd::run_queued().len(), 3);
        let seen: Vec<(i32, i32, Vec<u8>)> = crate::emevd::tests::SEEN.with(|s| s.borrow().iter().map(|(b, i, a, _, _)| (*b, *i, a.clone())).collect());
        let id = |a: &[u8]| u32::from_le_bytes(a[..4].try_into().unwrap());
        assert_eq!((seen[0].0, seen[0].1, seen[1].0, seen[1].1), (2007, 4, 2007, 9));
        assert_eq!(seen[2], (2007, 2, vec![17, 0, 0, 0]));
        let event_text = crate::text::category("event_text_map").unwrap();
        assert_eq!(crate::text::get(&event_text, id(&seen[0].2)).unwrap().as_deref(), Some("Hello from Kindle"));
        assert_eq!(crate::text::get(&event_text, id(&seen[1].2)).unwrap().as_deref(), Some("Saved"));
        assert_eq!(crate::text::get(&event_text, 4200).unwrap().as_deref(), Some("Somewhere, a heavy door has opened"));
        drop(text);
    }

    /// A mod's `kindle.flags` on a store laid out as the game's: reads, sets read back, a block, and
    /// watches called as their flags change.
    #[test]
    fn a_mod_reads_sets_and_watches_flags() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut store = crate::flags::tests::Store::serve();
        store.blocks[125] = 0b1000_0000;
        let lua = state("er-flags");
        let src = r#"
            local f = kindle.flags
            assert(f.get(20000) and not f.get(20001) and not f.get(25000))
            assert(f.set(20001) and f.get(20001))
            assert(f.set(20001, false) and not f.get(20001))
            assert(f.set(25000) == false and not f.get(25000))
            local words, first = f.block(20999)
            assert(first == 20000 and #words == 32 and words[1] == 0x80000000, tostring(words[1]))
            assert(f.block(25000) == nil)
            fired = {}
            f.on_set(30005, function(id) fired[#fired + 1] = id end)
            f.on_clear(20000, function(id) fired[#fired + 1] = -id end)
        "#;
        lua.load(src).exec().unwrap_or_else(|e| panic!("{e}"));
        // The watches polled as the frame polls them (kindle-core's run_flag_watches): the first poll sees
        // each flag as it is, later ones its changes.
        let poll = |lua: &Lua| {
            let mut c = lua.app_data_mut::<ModCtx>().unwrap();
            let fired: Vec<(i64, Function)> = c
                .handlers
                .flags
                .iter_mut()
                .filter_map(|w| {
                    let now = crate::flags::get_i64(w.flag).unwrap();
                    let fire = w.last.is_some_and(|was| was != now) && now == w.on_set;
                    w.last = Some(now);
                    fire.then(|| (w.flag, w.func.clone()))
                })
                .collect();
            drop(c);
            for (id, f) in fired {
                f.call::<()>(id).unwrap();
            }
        };
        poll(&lua);
        lua.load("kindle.flags.set(30005); kindle.flags.set(20000, false)").exec().unwrap();
        poll(&lua);
        poll(&lua);
        assert_eq!(eval::<Vec<i64>>(&lua, "return fired").unwrap(), [30005, -20000]);
        drop(store);
    }

    /// A mod's `kindle.params` end to end on a repository laid out as the game's, holding every table
    /// of Elden Ring's regulation (the folder KINDLE_ER_PARAMS names) with the paramdefs built in
    /// (KINDLE_ER_PARAMDEFS at build time): reads, a write put back, new rows, and the game's own read
    /// of them (the test passes without them: no game files or Elden Ring paramdefs come with Kindle).
    #[test]
    fn a_mod_reads_writes_and_adds_rows_to_elden_rings_params() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let Some(dir) = std::env::var_os("KINDLE_ER_PARAMS").filter(|_| !crate::params::PARAMDEFS.is_empty()) else {
            eprintln!("KINDLE_ER_PARAMS isn't set or no paramdefs are built in: Elden Ring's params not used through Lua");
            return;
        };
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), std::fs::read(&p).unwrap()))
            .collect();
        files.sort();
        let _repo = crate::params::tests::Repo::serve(&files);
        drop(files);
        let lua = state("er-params");
        let src = r#"
            local p = kindle.params
            assert(#p.tables() > 150, #p.tables())
            local dagger = p.get("EquipParamWeapon", 1000000)
            assert(dagger.weight == 1.5 and dagger.originEquipWep == 1000000, tostring(dagger.weight))
            assert(p.get("SpEffectParam", 3269).soul == 200)
            assert(p.get("EquipParamWeapon", 1) == nil)
            -- A write, read back and put back.
            dagger.weight = 0.5
            assert(dagger.weight == 0.5)
            assert(p.reset() == 1)
            assert(dagger.weight == 1.5)
            -- Two weapons above the table's rows (a weapon row per affinity, 100 apart: the Dagger, the
            -- Heavy Dagger), an effect before SpEffectParam's end rows.
            local function top(t)
              local m = 0
              for _, id in ipairs(p.rows(t)) do if id < 999999000 and id > m then m = id end end
              return m
            end
            local weapon = (top("EquipParamWeapon") // 10000 + 1) * 10000
            local added = p.add_rows("EquipParamWeapon", {{id = weapon, copy_from = 1000000}, {id = weapon + 100, copy_from = 1000100}})
            assert(#added == 2 and added[1].weight == 1.5 and added[2].weight == p.get("EquipParamWeapon", 1000100).weight)
            local effects = p.rows("SpEffectParam")
            local effect = top("SpEffectParam") + 1
            assert(p.add_row("SpEffectParam", effect, 3269).soul == 200)
            local now = p.rows("SpEffectParam")
            assert(#now == #effects + 1 and now[#now] == effects[#effects] and now[#now] >= 999999000)
            -- The old rows read as before; an id the table has is refused; the same row again is kept.
            assert(p.get("EquipParamWeapon", 1000000).weight == 1.5)
            local ok, e = pcall(p.add_row, "SpEffectParam", effect - 1, 3269)
            assert(not ok and tostring(e):find("need ids above"), tostring(e))
            p.add_row("SpEffectParam", effect, 3269)
            assert(#p.rows("SpEffectParam") == #now)
            return weapon, effect
        "#;
        let (weapon, effect): (u64, u64) = lua.load(src).eval().unwrap_or_else(|e| panic!("{e}"));
        // The game's view: each cap back in its slot, the new rows where its read finds them.
        let loaded = crate::params::loaded_in(crate::params::repository().unwrap());
        let registry = crate::params::registry().unwrap();
        for (table, id, copy_from) in [("EquipParamWeapon", weapon, 1000000), ("EquipParamWeapon", weapon + 100, 1000100), ("SpEffectParam", effect, 3269)] {
            let l = loaded.iter().find(|l| l.name == table).unwrap();
            assert_eq!(l.caps_in_slot, 1, "{table}");
            let size = registry.table(table).unwrap().def.size;
            let row = |id: u64| crate::param_rows::tests::game_row(l.table, id as u32, size);
            assert!(row(id).is_some() && row(id) == row(copy_from), "{table} {id}");
        }
    }

    #[test]
    fn the_sandbox_is_kindles() {
        let lua = state("er-sandbox");
        assert!(eval::<bool>(&lua, "return io == nil and os == nil and debug == nil and package == nil and native == nil").unwrap());
        assert!(lua.load("assert(load(string.dump(function() end)))").exec().is_err(), "binary chunk must be rejected");
    }

    #[test]
    fn events_are_the_runtimes_and_the_games() {
        let lua = state("er-events");
        assert!(eval::<u64>(&lua, "return kindle.on('frame', function() end)").is_ok());
        assert!(eval::<u64>(&lua, "return kindle.on('player_died', function() end)").is_ok());
        let e = lua.load("kindle.on('bonfire_rested', function() end)").exec().unwrap_err().to_string();
        assert!(e.contains("unknown event"), "{e}");
    }

    /// dist/stubs-er/kindle.lua (the editor's autocomplete and `kindle check`) names what mods get: every
    /// function and field it declares is there, and its events are the runtime's and the game's.
    #[test]
    fn the_stubs_are_the_api() {
        const STUBS: &str = include_str!("../../../dist/stubs-er/kindle.lua");
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let lua = state("er-stubs");
        let mut checked = 0;
        let mut class = None;
        for line in STUBS.lines() {
            if let Some(rest) = line.strip_prefix("---@class kindle.") {
                class = Some(rest.split_whitespace().next().unwrap_or("").to_string());
            } else if !line.starts_with("---") {
                class = None;
            }
            let field = class.as_ref().zip(line.strip_prefix("---@field ")).map(|(c, f)| format!("{c}.{}", f.split_whitespace().next().unwrap_or("")));
            let path = line.strip_prefix("function kindle.").map(|r| r.split('(').next().unwrap_or("").to_string());
            let Some(path) = path.or(field.filter(|f| ["text.", "events.", "world.", "ui."].iter().any(|m| f.starts_with(m)))) else { continue };
            let ok: bool = eval(&lua, &format!("return kindle.{path} ~= nil")).unwrap_or(false);
            assert!(ok, "the stubs declare kindle.{path}, which mods don't get");
            checked += 1;
        }
        assert!(checked > 150, "{checked}");
        let alias = STUBS.lines().find_map(|l| l.strip_prefix("---@alias kindle.event ")).unwrap();
        let mut stubs: Vec<&str> = alias.split('|').map(|e| e.trim_matches('"')).collect();
        let mut runtime: Vec<&str> = kindle_core::events::CORE.iter().chain(crate::game_events::EVENTS).copied().filter(|e| *e != "map_loading").collect();
        stubs.sort_unstable();
        runtime.sort_unstable();
        assert_eq!(stubs, runtime);
    }

    #[test]
    fn mods_for_another_game_are_left_out() {
        kindle_core::bindings::set(&BINDINGS);
        let kindle_dir = std::env::temp_dir().join(format!("kindle-er-test-{}-games", std::process::id()));
        for (dir, games) in [("a-ds3", ""), ("b-er", "games = [\"er\"]\n"), ("c-both", "games = [\"ds3\", \"er\"]\n")] {
            let d = kindle_dir.join("mods").join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("mod.toml"), format!("[mod]\nid = \"{dir}\"\n{games}")).unwrap();
            std::fs::write(d.join("main.lua"), "kindle.log.info('hi')\n").unwrap();
        }
        let (_reloads, reload_rx) = std::sync::mpsc::channel();
        let (_console, console_rx) = std::sync::mpsc::channel();
        let (_ui, ui_rx) = std::sync::mpsc::channel();
        let mut rt = Runtime::new(kindle_dir.clone(), KindleConfig::default(), reload_rx, console_rx, ui_rx);
        for d in rt.scan_mod_dirs() {
            rt.load_mod_dir(&d);
        }
        let loaded: Vec<(&str, String)> = rt.mods.iter().map(|m| (m.id(), m.status())).collect();
        assert_eq!(loaded, vec![("b-er", "running".to_string()), ("c-both", "running".to_string())]);
        let _ = std::fs::remove_dir_all(kindle_dir);
    }

    #[test]
    fn character_stores_wait_for_a_character() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _title_screen = crate::player::tests::NoPlayer::serve();
        let lua = state("er-store");
        let e = lua.load("kindle.store.get('x')").exec().unwrap_err().to_string();
        assert!(e.contains("no character is loaded"), "{e}");
    }
}
