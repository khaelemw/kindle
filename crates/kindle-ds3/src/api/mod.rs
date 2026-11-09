//! Dark Souls III's half of the Lua API: the natives for its modules (one file per area), the
//! prelude that wraps them into kindle.flags, kindle.items and the rest (prelude.lua), its catalogue,
//! and the rest of what Kindle's mod runtime asks of the game (`BINDINGS`).

mod debug;
mod events;
mod flags;
mod hks;
pub(crate) mod input;
mod items;
mod maps;
mod net;
mod player;
mod text;
mod ui;
mod world;

use mlua::{Lua, Table};

use crate::bindings::Bindings;
use crate::catalogue::Catalogue;
use crate::log::{self, Level};
use crate::mods::ModCtx;

/// Everything the natives use, in one place.
mod prelude {
    #![allow(unused_imports)]
    pub(super) use std::sync::atomic::Ordering;
    pub(super) use std::time::{Duration, Instant};

    pub(super) use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};

    pub(super) use crate::ds3::ItemCategory;
    pub(super) use crate::log::{self, Level};
    pub(super) use crate::lua_values::*;
    pub(super) use crate::mods::*;
    pub(super) use crate::ui_bridge::{self, NoticeKind};
    pub(super) use crate::{anim, bonfire, chars, equip, game, memory, objects, params, speffect, text, text_override};
}

/// What Kindle's mod runtime asks of Dark Souls III (bindings.rs); set as Kindle starts (lib.rs).
pub static BINDINGS: Bindings = Bindings {
    game: "ds3",
    exe: "DarkSoulsIII.exe",
    natives,
    prelude: include_str!("../prelude.lua"),
    raw_state,
    character_slot,
    character_name,
    character,
    unloading,
    unloaded,
    param_caps: crate::params::caps,
    paramdefs: crate::params::PARAMDEFS,
    add_param_rows: crate::param_rows::add_rows,
    catalogue: &CATALOGUE,
    events: &[crate::game_events::EVENTS, crate::hooks::EVENTS],
    console_help: HELP,
    console_globals: "flags, items, player, world, params, log, debug, text, ui = kindle.flags, kindle.items, kindle.player, kindle.world, kindle.params, kindle.log, kindle.debug, kindle.text, kindle.ui",
};

/// The game's natives, after Kindle's own.
fn natives(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set("dlc_installed", lua.create_function(|_, dlc: u32| Ok(crate::ds3::dlc_installed(dlc)))?)?;
    flags::register(lua, n)?;
    items::register(lua, n)?;
    player::register(lua, n)?;
    world::register(lua, n)?;
    events::register(lua, n)?;
    maps::register(lua, n)?;
    text::register(lua, n)?;
    ui::register(lua, n)?;
    hks::register(lua, n)?;
    debug::register(lua, n)?;
    net::register(lua, n)?;
    input::register(lua, n)?;
    Ok(())
}

/// A state with raw access: the test rig's kindle.rig, in a rig build.
#[cfg(feature = "rig")]
fn raw_state(lua: &Lua) -> mlua::Result<()> {
    crate::rig::install(lua)
}

#[cfg(not(feature = "rig"))]
fn raw_state(_: &Lua) -> mlua::Result<()> {
    Ok(())
}

/// The loaded character's save slot; None with no character in the world.
fn character_slot() -> Result<Option<i32>, String> {
    if !crate::game::player_loaded() {
        return Ok(None);
    }
    crate::bonfire::save_slot().map(Some)
}

fn character_name() -> Result<String, String> {
    Ok(crate::game::player_game_data()?.player_info().name())
}

fn character(handle: i64) -> Result<usize, String> {
    crate::chars::find(handle).map(|c| c.addr)
}

/// A mod is unloading: the characters its event commands hold go back to the game, and the
/// player gets their HUD setting back.
fn unloading(id: &str, ctx: Option<&ModCtx>) {
    crate::emevd::release_characters(id);
    crate::chars::release_ai(id, &crate::chars::all().unwrap_or_default());
    if let Some(v) = ctx.and_then(|c| c.hud_original)
        && let Err(e) = crate::game::set_hud(v)
    {
        log::write(Level::Warn, id, &format!("HUD setting not restored: {e}"));
    }
}

/// A mod has unloaded: its param edits, catalogue answers, input, menu entries, event script and
/// map changes and texts are undone.
fn unloaded(ctx: &mut ModCtx) {
    crate::params::revert_journal(&ctx.id, &mut ctx.journal);
    crate::catalogue::release(&CATALOGUE, &ctx.id);
    input::release(&ctx.id);
    crate::camera::release(&ctx.id);
    crate::native_mods::remove_mod_tab(&ctx.id);
    crate::front_menu::remove_title_entry(&ctx.id);
    crate::char_make::remove_rows(&ctx.id);
    crate::p2p::remove_welcomes(&ctx.id);
    let patches = crate::event_patches::remove_owner(&ctx.id);
    if patches > 0 {
        log::write(Level::Info, &ctx.id, &format!("dropped {patches} event script change(s) (from the next area load)"));
    }
    let map_changes = crate::map_edits::reset(&ctx.id, None);
    if map_changes > 0 {
        log::write(Level::Info, &ctx.id, &format!("dropped {map_changes} map change(s) (from the next area load)"));
    }
    let texts = crate::text_override::reset_owner(&ctx.id);
    if texts > 0 {
        log::write(Level::Info, &ctx.id, &format!("reverted {texts} text override(s)"));
    }
}

/// Kindle's catalogue. Entries come with the parts of Kindle that implement them (co-op's move to
/// a Lua mod added them subsystem by subsystem).
pub static CATALOGUE: Catalogue = Catalogue {
    hooks: &[
        &crate::bonfire::REST_REQUESTED,
        &crate::bonfire::TRAVEL_REQUESTED,
        &crate::death::DEPARTURE_HIT,
        &crate::death::LOAD_BLOODSTAIN,
        &crate::multiplayer::CHR_TYPE_TEST,
        &crate::multiplayer::OWN_WORLD_TEST,
        &crate::multiplayer::MP_STATE_TEST,
        &crate::multiplayer::NPC_GUEST_LOOK,
        &crate::multiplayer::CUTSCENE_SKIP,
        &crate::multiplayer::EMBER_LOOK,
        &crate::multiplayer::ESTUS_ALLOTMENT,
        &crate::multiplayer::BONFIRE_LEVEL,
        &crate::multiplayer::BONFIRE_SESSION_LIST,
        &crate::multiplayer::TALK_CHECK,
        &crate::multiplayer::IN_SESSION,
        &crate::multiplayer::NPC_READY,
        &crate::loot::GUEST_LOOT,
        &crate::loot::GUEST_ITEM_IDS,
        &crate::loot::TREASURE_FLAG,
        &crate::loot::ITEM_MESSAGES_ANSWER,
        &crate::loot::GUEST_AWARDS,
        &crate::loot::HOST_HP_DEATH,
        &crate::multiplayer_flow::BOSS_CLEAR_FADE,
        &crate::multiplayer_flow::BOSS_CLEAR_HOME,
        &crate::multiplayer_flow::BOSS_DEATH_LEAVING,
        &crate::multiplayer_flow::BOSS_LEAVE_SESSION,
        &crate::multiplayer_flow::LEAVE_LIMIT_ANSWER,
        &crate::multiplayer_flow::RELOAD_REQUESTED,
        &crate::multiplayer_flow::NETWORK_NOTICE,
        &crate::home_load::PHANTOM_COPY,
        &crate::home_load::PLAYER_TYPE,
        &crate::home_load::KEEP_HOME_TYPE,
        &crate::session::LEAVE_REQUESTED,
        &crate::session::CLOSE_REQUESTED,
        &crate::session_hud::HIDDEN_NAME_TAGS,
        &crate::emevd::BANNER_SHOWING,
    ],
    fields: &[
        &crate::bonfire::SITTING,
        &crate::native_ui::MENU_OPEN,
        &crate::native_ui::ACTION_PROMPT,
        &crate::native_ui::ESC_MENU_OPEN,
        &crate::native_ui::ITEM_POPUP,
        &crate::death::FALL_TIME,
        &crate::equip::OWN_KEY_ITEMS,
        &crate::party_saves::TITLE_SCREEN,
    ],
    functions: &[
        &crate::bonfire::LIGHT_FIRE,
        &crate::bonfire::REST,
        &crate::death::MARK,
        &crate::death::LEAVE,
        &crate::death::STAND_ON,
        &crate::equip::USE_OWN_KEY_ITEMS,
        &crate::flag_blocks::USE_OWN_TABLE,
        &crate::flag_blocks::SEPARATE_BLOCK,
        &crate::multiplayer::CHR_TYPE_GET,
        &crate::multiplayer::CHR_TYPE_SET,
        &crate::multiplayer_flow::SEND_HOME,
        &crate::native_ui::QUIT_TO_TITLE,
        &crate::party_saves::TITLE_BACK,
    ],
    events: &[
        &crate::death::FALL_DEATH_BLOCKED,
        &crate::flag_blocks::NETWORK_SET,
        &crate::flag_blocks::PLAIN_SET,
        &crate::flag_blocks::CHANGED,
        &crate::multiplayer_flow::BOSS_CLEAR_STAYED,
        &crate::session::LEFT,
        &crate::lobby::LOBBY_CLOSED,
        &crate::party_saves::READY,
        &crate::party_saves::WAITING,
        &crate::party_saves::LEFT_PLAYING,
        &crate::party_saves::OWN_BACK,
    ],
};

const HELP: &str = r#"
Kindle console — type Lua, it runs inside the game (all modules available).
  flags.get(flags.boss.iudex_gundyr)   read an event flag (bosses, bonfires by name)
  items.give("Ember", 2)               give an item
  player.hp()                          player stats (needs a loaded character)
  params.get("EquipParamGoods", "Ember").weight
Shorthands: flags, items, player, params, log are globals here.
Commands:
  :mods            list mods and their status
  :reload [id]     reload one mod, or all
  :log [n]         last n log lines (default 30)
  :status          runtime status
  :run <file>      run a Lua file in the console (path relative to the Kindle folder)
  :help            this text
Code can span lines: an unfinished block (function, if, table) waits for the rest.
"#;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use mlua::{Function, Lua, Value as LuaValue};

    use super::{BINDINGS, CATALOGUE};
    use crate::config::{ModInfo, ModManifest, Permissions};
    use crate::mods::*;

    fn manifest(modules: &[&str]) -> ModManifest {
        manifest_for("test", modules)
    }

    fn manifest_for(id: &str, modules: &[&str]) -> ModManifest {
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
                games: None,
                api: None,
            },
            permissions: Permissions { modules: modules.iter().map(|s| s.to_string()).collect() },
            settings: Vec::new(),
        }
    }

    fn temp_mod_dir(files: &[(&str, &str)]) -> PathBuf {
        // Every test's Lua state has Dark Souls III's API.
        crate::bindings::set(&BINDINGS);
        let dir = std::env::temp_dir().join(format!("kindle-test-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, body) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    fn eval<T: mlua::FromLuaMulti>(lua: &Lua, src: &str) -> mlua::Result<T> {
        lua.load(src).eval()
    }

    #[test]
    fn settings_come_from_mod_toml() {
        // A directory of its own: saved settings from an earlier run with the same process id
        // (Windows reuses them) made "chance" start at 1.0.
        crate::store::set_root(&std::env::temp_dir().join(format!("kindle-settings-test-{}-{}", std::process::id(), rand_suffix())));
        let dir = temp_mod_dir(&[(
            "mod.toml",
            r#"
[mod]
id = "settings-test"

[settings.chance]
type = "float"
label = "Chance"
default = 0.25
min = 0.0
max = 1.0

[settings.mode]
type = "choice"
choices = ["easy", "hard"]
default = "easy"

[settings.loud]
type = "bool"
default = false
"#,
        )]);
        let m = ModManifest::load(&dir.join("mod.toml")).unwrap();
        let names: Vec<&str> = m.settings.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["chance", "mode", "loud"]); // file order
        let lua = create_state(&m, &dir, false).unwrap();
        assert_eq!(eval::<f64>(&lua, r#"return kindle.settings.get("chance")"#).unwrap(), 0.25);
        eval::<()>(&lua, r#"kindle.settings.set("chance", 5)"#).unwrap();
        assert_eq!(eval::<f64>(&lua, r#"return kindle.settings.get("chance")"#).unwrap(), 1.0); // clamped
        assert!(eval::<()>(&lua, r#"kindle.settings.set("mode", "medium")"#).is_err());
        assert!(eval::<()>(&lua, r#"kindle.settings.set("loud", "yes")"#).is_err());
        assert!(eval::<()>(&lua, r#"return kindle.settings.get("missing")"#).is_err());

        let bad = temp_mod_dir(&[("mod.toml", "[mod]\nid = \"x\"\n[settings.n]\ntype = \"choice\"\ndefault = \"a\"\n")]);
        let e = ModManifest::load(&bad.join("mod.toml")).unwrap_err();
        assert!(e.contains("settings.n") && e.contains("choices"), "{e}");
    }

    #[test]
    fn mods_call_each_others_exports() {
        let dir = temp_mod_dir(&[]);
        let a = create_state(&manifest_for("interop-a", &[]), &dir, false).unwrap();
        let b = create_state(&manifest_for("interop-b", &[]), &dir, false).unwrap();
        set_live("interop-a", Some(&a));
        set_live("interop-b", Some(&b));
        a.load(
            r#"
            local count = 0
            kindle.export {
              greet = function(name) count = count + 1; return "hello " .. name, count end,
              sum = function(t) local s = 0 for _, v in ipairs(t) do s = s + v end return { total = s } end,
              fail = function() error("nope") end,
            }"#,
        )
        .exec()
        .unwrap();
        let (text, n): (String, i64) = eval(&b, r#"return kindle.mods.get("interop-a").greet("ashen one")"#).unwrap();
        assert_eq!((text.as_str(), n), ("hello ashen one", 1));
        let total: i64 = eval(&b, r#"return kindle.mods.get("interop-a").sum({1, 2, 3}).total"#).unwrap();
        assert_eq!(total, 6);
        // errors, missing functions, functions as arguments, unknown mods
        let e = eval::<()>(&b, r#"kindle.mods.get("interop-a").fail()"#).unwrap_err().to_string();
        assert!(e.contains("interop-a.fail") && e.contains("nope"), "{e}");
        assert!(eval::<()>(&b, r#"kindle.mods.get("interop-a").nothing()"#).is_err());
        assert!(eval::<()>(&b, r#"kindle.mods.get("interop-a").greet(print)"#).is_err());
        assert!(eval::<bool>(&b, r#"return kindle.mods.get("no-such-mod") == nil"#).unwrap());
        let running: Vec<String> = eval(&b, "return kindle.mods.running()").unwrap();
        assert!(running.contains(&"interop-a".to_string()));
        set_live("interop-a", None);
        set_live("interop-b", None);
        assert!(eval::<()>(&b, r#"local m = kindle.mods.get("interop-a") assert(m == nil)"#).is_ok());
    }

    #[test]
    fn kept_values_survive_a_reload() {
        let dir = temp_mod_dir(&[]);
        let first = create_state(&manifest_for("keep-test", &[]), &dir, false).unwrap();
        let code = "local s = kindle.keep('state', { n = 1 }) s.n = s.n + 1 return s.n";
        assert_eq!(eval::<i64>(&first, code).unwrap(), 2);
        assert_eq!(eval::<i64>(&first, "return kindle.keep('plain', 5)").unwrap(), 5);
        save_kept("keep-test", &first);
        let second = create_state(&manifest_for("keep-test", &[]), &dir, false).unwrap();
        assert_eq!(eval::<i64>(&second, code).unwrap(), 3);
        assert_eq!(eval::<i64>(&second, "return kindle.keep('plain', 7)").unwrap(), 5);
        // Another mod's keys are its own.
        let other = create_state(&manifest_for("keep-other", &[]), &dir, false).unwrap();
        assert_eq!(eval::<i64>(&other, code).unwrap(), 2);
    }

    #[test]
    fn permissions_gate_modules() {
        let lua = create_state(&manifest(&["flags"]), &temp_mod_dir(&[]), false).unwrap();
        assert!(eval::<bool>(&lua, "return kindle.flags ~= nil").unwrap());
        assert!(eval::<bool>(&lua, "return kindle.items == nil and kindle.player == nil and kindle.params == nil").unwrap());
        assert!(eval::<bool>(&lua, "return type(kindle.log.info) == 'function' and type(kindle.on) == 'function'").unwrap());
    }

    #[test]
    fn raw_access_only_when_given() {
        let lua = create_state(&manifest(&["debug"]), &temp_mod_dir(&[]), false).unwrap();
        assert!(eval::<bool>(&lua, "return type(kindle.debug.press) == 'function' and type(kindle.debug.export) == 'function'").unwrap());
        assert!(eval::<bool>(&lua, "return kindle.debug.read == nil and kindle.debug.bytes == nil and kindle.debug.watch == nil").unwrap());
        assert!(eval::<bool>(&lua, "return kindle.debug.singleton == nil and kindle.debug.trace_next == nil").unwrap());
        // Not even through a name the prelude doesn't expose: the native refuses too.
        let raw_native = natives(&lua).unwrap().get::<Function>("debug_read").unwrap();
        assert!(raw_native.call::<LuaValue>((0i64, "u8")).is_err());
        // export: a plain file name only.
        assert!(lua.load("kindle.debug.export('/event/x.emevd.dcx', '../escape.bin')").exec().is_err());
        assert!(lua.load("kindle.debug.export('/event/x.emevd.dcx', 'C:/Windows/x.bin')").exec().is_err());
        let dev = create_state_with(&manifest(&["debug"]), &temp_mod_dir(&[]), false, true).unwrap();
        assert!(eval::<bool>(&dev, "return type(kindle.debug.read) == 'function' and type(kindle.debug.watch) == 'function'").unwrap());
    }

    #[test]
    fn sandbox_has_no_escape_hatches() {
        let lua = create_state(&manifest(&[]), &temp_mod_dir(&[]), false).unwrap();
        assert!(eval::<bool>(&lua, "return io == nil and os == nil and debug == nil and package == nil").unwrap());
        assert!(eval::<bool>(&lua, "return dofile == nil and loadfile == nil").unwrap());
        // the private natives table is not reachable
        assert!(eval::<bool>(&lua, "return native == nil").unwrap());
        // binary chunks are refused even though string.dump exists
        let r: mlua::Result<()> = lua.load("assert(load(string.dump(function() end)))").exec();
        assert!(r.is_err(), "binary chunk must be rejected");
    }

    #[test]
    fn require_is_mod_local() {
        let dir = temp_mod_dir(&[("util.lua", "return { n = 41 }"), ("lib/deep.lua", "return 'deep'")]);
        let lua = create_state(&manifest(&[]), &dir, false).unwrap();
        assert_eq!(eval::<i64>(&lua, "return require('util').n + 1").unwrap(), 42);
        assert_eq!(eval::<String>(&lua, "return require('lib.deep')").unwrap(), "deep");
        assert!(eval::<bool>(&lua, "return require('util') == require('util')").unwrap(), "cached");
        let e = lua.load("require('../secret')").exec().unwrap_err().to_string();
        assert!(e.contains("invalid module name"), "{e}");
    }

    #[test]
    fn circular_require_is_an_error() {
        let dir = temp_mod_dir(&[("a.lua", "return require('b')"), ("b.lua", "return require('a')")]);
        let lua = create_state(&manifest(&[]), &dir, false).unwrap();
        let e = lua.load("require('a')").exec().unwrap_err().to_string();
        assert!(e.contains("circular require"), "{e}");
    }

    #[test]
    fn timers_check_their_arguments() {
        let lua = create_state(&manifest(&[]), &temp_mod_dir(&[]), false).unwrap();
        let e = lua.load("kindle.after('soon', function() end)").exec().unwrap_err().to_string();
        assert!(e.contains("seconds must be a number"), "{e}");
    }

    #[test]
    fn watchdog_stops_infinite_loops() {
        let mut m = LoadedMod {
            manifest: manifest(&[]),
            dir: temp_mod_dir(&[]),
            lua: Some(create_state(&manifest(&[]), &temp_mod_dir(&[]), false).unwrap()),
            state: ModState::Running,
            reloads: 0,
            cpu: CpuTime::default(),
        };
        let started = Instant::now();
        let r = m.call(Duration::from_millis(50), "test", |lua| lua.load("while true do end").exec());
        assert!(r.is_none());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(matches!(&m.state, ModState::Failed(e) if e.contains("time limit")));
        assert!(m.lua.is_none(), "failed mod is unloaded");
    }

    #[test]
    fn watchdog_counts_the_scripts_own_time_not_a_stall() {
        let mut m = LoadedMod {
            manifest: manifest(&[]),
            dir: temp_mod_dir(&[]),
            lua: Some(create_state(&manifest(&[]), &temp_mod_dir(&[]), false).unwrap()),
            state: ModState::Running,
            reloads: 0,
            cpu: CpuTime::default(),
        };
        let lua = m.lua.as_ref().unwrap();
        let stall = lua
            .create_function(|_, ms: u64| {
                std::thread::sleep(Duration::from_millis(ms));
                Ok(())
            })
            .unwrap();
        lua.globals().set("stall", stall).unwrap();
        // The thread waits 300 ms (as the game's thread does when the PC is busy), then the script
        // runs on: under its 50 ms, it isn't stopped.
        let r = m.call(Duration::from_millis(50), "test", |lua| lua.load("stall(300) for i = 1, 100000 do end").exec());
        assert!(r.is_some(), "{}", m.status());
        assert!(matches!(m.state, ModState::Running));
    }

    #[test]
    fn handlers_register_and_cancel() {
        let lua = create_state(&manifest(&["flags"]), &temp_mod_dir(&[]), false).unwrap();
        let h: u64 = eval(&lua, "return kindle.on('frame', function(dt) end)").unwrap();
        eval::<u64>(&lua, "return kindle.every(1, function() end)").unwrap();
        eval::<u64>(&lua, "return kindle.flags.on_set(14000800, function() end)").unwrap();
        {
            let c = lua.app_data_ref::<ModCtx>().unwrap();
            assert_eq!(c.handlers.event_fns("frame").len(), 1);
            assert_eq!(c.handlers.timers.len(), 1);
            assert_eq!(c.handlers.flags.len(), 1);
        }
        assert!(eval::<bool>(&lua, &format!("return kindle.cancel({h})")).unwrap());
        assert_eq!(lua.app_data_ref::<ModCtx>().unwrap().handlers.event_fns("frame").len(), 0);
        let e = lua.load("kindle.on('not_an_event', print)").exec().unwrap_err().to_string();
        assert!(e.contains("unknown event"), "{e}");
    }

    #[test]
    fn print_and_log_formatting() {
        let lua = create_state(&manifest(&[]), &temp_mod_dir(&[]), false).unwrap();
        lua.app_data_mut::<ModCtx>().unwrap().capture = Some(Vec::new());
        lua.load("print('a', 1, nil, true)").exec().unwrap();
        let out = lua.app_data_mut::<ModCtx>().unwrap().capture.take().unwrap();
        assert_eq!(out, vec!["a\t1\tnil\ttrue"]);
        lua.load("kindle.log.info('hp=%d of %s', 5, 'x')").exec().unwrap();
    }

    /// Function names under `kindle` that the editor stubs declare.
    fn stub_functions() -> std::collections::BTreeSet<String> {
        include_str!("../../../../dist/stubs/kindle.lua")
            .lines()
            .filter_map(|l| l.strip_prefix("function kindle."))
            .filter_map(|l| l.split('(').next())
            .map(|name| format!("kindle.{name}"))
            .collect()
    }

    /// The stubs (docs, editor completion, `kindle check`) and the runtime's `kindle` table
    /// list the same functions.
    #[test]
    fn stubs_match_the_runtime_api() {
        let lua = create_state(&manifest(&[]), &temp_mod_dir(&[]), true).unwrap();
        let runtime: String = eval(
            &lua,
            r#"
            local out, seen = {}, {}
            local function walk(t, path)
              if seen[t] then return end
              seen[t] = true
              for k, v in pairs(t) do
                if type(k) == "string" then
                  if type(v) == "function" then out[#out + 1] = path .. "." .. k
                  elseif type(v) == "table" then walk(v, path .. "." .. k) end
                end
              end
            end
            walk(kindle, "kindle")
            return table.concat(out, "\n")
            "#,
        )
        .unwrap();
        let runtime: std::collections::BTreeSet<String> = runtime.lines().map(String::from).collect();
        let stubs = stub_functions();
        let undocumented: Vec<&String> = runtime.difference(&stubs).collect();
        let missing: Vec<&String> = stubs.difference(&runtime).collect();
        assert!(undocumented.is_empty(), "in the runtime but not in dist/stubs/kindle.lua: {undocumented:?}");
        assert!(missing.is_empty(), "in dist/stubs/kindle.lua but not in the runtime: {missing:?}");
    }

    #[test]
    fn catalogue_names_are_unique() {
        assert!(CATALOGUE.duplicate_names().is_empty());
    }
}
