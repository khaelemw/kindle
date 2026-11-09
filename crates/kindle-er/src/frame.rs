//! Elden Ring's frame: the mods load once the game has loaded its params and text, then each frame the
//! reloads the watcher saw, the console's requests, the mods' event commands, messages between mods,
//! each mod's timers, flag watches
//! and `frame` handlers, the game events mods listen to (game_events.rs), its stores written about once
//! a second, the mods' CPU time, and the overlay's
//! requests and what it shows (overlay.rs, through kindle_core's ui_bridge). It runs on a
//! thread of Kindle's own (`run`) until the game runs Kindle's frame task (task.rs), then on the game's
//! thread.

use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use kindle_core::mods;
use kindle_core::runtime::{HALTED, RUNTIME, Runtime, dispatch_event, run_flag_watches, run_timers};
use kindle_core::ui_bridge::{self, UiRequest};

/// How often `run` ticks: the game's 60 frames a second.
const FRAME: Duration = Duration::from_micros(16_667);
/// Longest Kindle waits for the game's params and text before loading mods regardless.
const DATA_WAIT_LIMIT: Duration = Duration::from_secs(180);
/// Frames a character is in the world before the mods hear of it (half a second): past the frames in
/// which the game is still putting it together as a load ends.
const SETTLE_FRAMES: u32 = 30;

/// Nearby characters the overlay lists: within this many metres, this many at most.
const NEARBY: f32 = 40.0;
const MOST_NEARBY: usize = 60;

/// The game state the game events are found by, from frame to frame.
static EVENTS: Mutex<Option<crate::game_events::Tracker>> = Mutex::new(None);

/// Frames the player's character has been in the world, in a row.
static IN_WORLD_FOR: AtomicU32 = AtomicU32::new(0);

/// Kindle's work for one frame.
pub fn tick(rt: &mut Runtime) {
    let frame = mods::FRAME.fetch_add(1, Ordering::Relaxed) + 1;
    let now = Instant::now();
    let dt = rt.last_tick.map(|t| (now - t).as_secs_f64()).unwrap_or(0.0);
    rt.last_tick = Some(now);
    // A hook's code the game put back gets Kindle's again.
    kindle_core::hook_sites::keep_frame();
    let first = *rt.first_frame.get_or_insert_with(|| {
        kindle_core::info!("first frame (frame {frame}); waiting for the game's params and text before loading mods");
        now
    });
    if !rt.mods_loaded {
        // Kindle's frame starts well before the game loads its data (seen live: the frame at 0.2 s, the
        // text at 10-25 s), and a mod's own start often adds rows and names them.
        let ready = kindle_core::params::registry().is_ok() && crate::text::loaded().is_ok();
        let waited = now.duration_since(first);
        if ready || waited > DATA_WAIT_LIMIT {
            if ready {
                kindle_core::info!("game data ready after {:.1}s", waited.as_secs_f64());
            } else {
                kindle_core::warn!("game data still not ready after {}s; loading mods anyway", waited.as_secs());
            }
            rt.mods_loaded = true;
            rt.pending_reloads.clear();
            while rt.reload_rx.try_recv().is_ok() {}
            rt.load_all();
        }
    }
    if !rt.mods_loaded {
        rt.process_console();
        return;
    }
    rt.process_reloads(now);
    rt.process_console();
    rt.process_ui_requests(ui_request);
    crate::text::tick();
    event_commands(rt);
    player_comes_and_goes(rt);
    game_events(rt, frame);
    rt.roll_cpu(now);
    if frame.is_multiple_of(60) {
        rt.flush_stores();
    }
    rt.deliver_messages();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        run_timers(m, now, limit);
        run_flag_watches(m, limit, crate::flags::get_i64);
        dispatch_event(m, "frame", limit, dt);
    }
    // The mods list twice a second; the overlay's data about 6 times a second while it's open.
    if frame.is_multiple_of(30) {
        let rows = rt.mod_rows();
        ui_bridge::publish_mods(rows);
    }
    ui_bridge::IN_WORLD.store(rt.player_loaded, Ordering::Relaxed);
    if ui_bridge::visible() && (frame >= rt.last_snapshot + 10 || rt.last_snapshot == 0) {
        publish_snapshot(rt, frame);
    } else if !ui_bridge::visible() {
        rt.last_snapshot = 0;
    }
}

/// The game events some mod listens to, found and sent to every mod that does.
fn game_events(rt: &mut Runtime, frame: u64) {
    use crate::game_events::{EVENTS as NAMES, Tracker};
    let mut tracker = EVENTS.lock().unwrap_or_else(|e| e.into_inner());
    let tracker = tracker.get_or_insert_with(Tracker::default);
    if !rt.player_loaded {
        tracker.reset();
        return;
    }
    let wants: std::collections::HashSet<&'static str> = NAMES.iter().copied().filter(|e| rt.subscribed(e)).collect();
    if wants.is_empty() {
        return;
    }
    let fired = tracker.poll(frame, &wants);
    if fired.is_empty() {
        return;
    }
    let from = crate::world::player_map().zip(crate::player::position().ok());
    let limit = rt.limit();
    for ev in &fired {
        for m in rt.all_mods_mut() {
            dispatch_game_event(m, ev, from, limit);
        }
    }
}

fn dispatch_game_event(m: &mut mods::LoadedMod, ev: &crate::game_events::Fired, from: Option<(u32, (f32, f32, f32))>, limit: Duration) {
    use crate::game_events::Fired;
    use mlua::IntoLuaMulti;
    let fns: Vec<mlua::Function> = match &m.lua {
        Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns(ev.name())).unwrap_or_default(),
        None => return,
    };
    for f in fns {
        let ok = m.call(limit, ev.name(), |lua| {
            let args: mlua::MultiValue = match ev {
                Fired::PlayerDied => ().into_lua_multi(lua)?,
                Fired::MapChanged { new, old } => (new.as_str(), old.as_str()).into_lua_multi(lua)?,
                Fired::CharacterKilled(c) | Fired::CharacterSpawned(c) => crate::world::listed_table(lua, c, from)?.into_lua_multi(lua)?,
                Fired::ItemGained { item, count } | Fired::ItemLost { item, count } => {
                    (*item, *count, crate::text::item(*item, "name").ok().flatten()).into_lua_multi(lua)?
                }
                Fired::EffectApplied(id) | Fired::EffectExpired(id) => (*id).into_lua_multi(lua)?,
                Fired::LevelUp { level, old, raised, runes } => {
                    let t = lua.create_table()?;
                    for (name, value) in raised {
                        t.set(*name, *value)?;
                    }
                    (*level, *old, t, *runes).into_lua_multi(lua)?
                }
            };
            f.call::<()>(args)
        });
        if ok.is_none() {
            return;
        }
    }
}

/// What the overlay shows: the mods, the player (the attributes and level), watched flags, the param
/// tables and the characters near the player.
fn publish_snapshot(rt: &mut Runtime, frame: u64) {
    rt.last_snapshot = frame;
    let mods = rt.mod_rows();
    let player = if rt.player_loaded { player_snap() } else { None };
    let flags = ui_bridge::watched_flags().into_iter().map(|f| (f, if rt.player_loaded { crate::flags::get_i64(f as i64).ok() } else { None })).collect();
    let param_tables = kindle_core::params::registry().map(|r| r.tables().filter(|t| t.valid).map(|t| t.name.clone()).collect()).unwrap_or_default();
    let nearby = if rt.player_loaded { nearby().unwrap_or_default() } else { Vec::new() };
    ui_bridge::publish(ui_bridge::Snapshot {
        frame,
        uptime: mods::START.elapsed().as_secs_f64(),
        mods_loaded: rt.mods_loaded,
        safe_mode: rt.safe_mode,
        character_mods: rt.character_mods.clone(),
        mods,
        player,
        inventory: if rt.player_loaded { inventory() } else { Vec::new() },
        flags,
        param_tables,
        nearby,
    });
}

fn player_snap() -> Option<ui_bridge::PlayerSnap> {
    let pool = |what: &str| (crate::player::get(what).unwrap_or(0), crate::player::get(&format!("max_{what}")).unwrap_or(0));
    let mut stats = [("", 0u32); 9];
    for (slot, (name, value)) in stats.iter_mut().zip(crate::player::stats().ok()?) {
        let label = crate::player::ATTRIBUTE_LABELS.iter().find(|(n, _)| *n == name).map_or(name, |(_, label)| label);
        *slot = (label, value.max(0) as u32);
    }
    stats[8] = ("Level", crate::player::level().ok()?.max(0) as u32);
    Some(ui_bridge::PlayerSnap { name: crate::player::name().ok()?, hp: pool("hp"), fp: pool("fp"), stamina: pool("stamina"), stats })
}

/// What the player holds: (category, row, quantity, name).
fn inventory() -> Vec<(&'static str, u32, u32, String)> {
    crate::items::held()
        .unwrap_or_default()
        .into_iter()
        .map(|(item, qty)| {
            let name = crate::text::item(item, "name").ok().flatten().unwrap_or_default();
            (crate::items::category(item).unwrap_or("?"), item & crate::items::ID_BITS, qty, name)
        })
        .collect()
}

/// The living characters near the player, nearest first (helpers left out).
fn nearby() -> Result<Vec<ui_bridge::NearbyRow>, String> {
    let all = crate::world::characters()?;
    let Some(me) = all.iter().find(|c| c.is_player) else { return Ok(Vec::new()) };
    let Some(at) = me.position else { return Ok(Vec::new()) };
    let mut rows: Vec<ui_bridge::NearbyRow> = all
        .iter()
        .filter(|c| !c.is_player && c.hp > 0 && c.kind != "helper")
        .filter_map(|c| {
            let (x, y, z) = crate::world::relative(c.map, me.map, c.position?)?;
            let distance = ((x - at.0).powi(2) + (y - at.1).powi(2) + (z - at.2).powi(2)).sqrt();
            (distance <= NEARBY).then(|| ui_bridge::NearbyRow {
                handle: c.handle as u32,
                label: c.name.clone().or_else(|| c.model.clone()).unwrap_or_else(|| "?".into()),
                kind: c.kind,
                entity_id: c.entity.map(|e| e as i32),
                hp: (c.hp, c.max_hp),
                distance,
            })
        })
        .collect();
    rows.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    rows.truncate(MOST_NEARBY);
    Ok(rows)
}

/// A character the overlay lists, by the low half of its handle (the overlay's rows keep that).
fn listed(handle: u32) -> Result<i64, String> {
    crate::world::characters()?.into_iter().find(|c| c.handle as u32 == handle).map(|c| c.handle).ok_or_else(|| format!("character {handle:#x} is gone"))
}

/// The overlay's requests for the game (the runtime does its own).
fn ui_request(_rt: &mut Runtime, req: UiRequest) {
    match req {
        UiRequest::SetFlag(flag, state) => match crate::flags::set(flag, state) {
            Ok(true) => kindle_core::info!("overlay: flag {flag} set {}", if state { "ON" } else { "off" }),
            Ok(false) => kindle_core::warn!("overlay: flag {flag} isn't one of the game's"),
            Err(e) => kindle_core::warn!("overlay: flag {flag}: {e}"),
        },
        UiRequest::SetChrHp(handle, hp) => {
            let result = listed(handle).and_then(|key| {
                let hp = if hp == i32::MAX { crate::world::hp(key)?.1 } else { hp };
                crate::world::set_hp(key, hp as i64).map(|_| hp)
            });
            match result {
                Ok(hp) => kindle_core::info!("overlay: character {handle:#x} hp -> {hp}"),
                Err(e) => kindle_core::warn!("overlay: {e}"),
            }
        }
        UiRequest::ChrEffect(handle, id) => match listed(handle).and_then(|key| crate::world::apply_effect(key, id)) {
            Ok(()) => kindle_core::info!("overlay: applied SpEffect {id} to {handle:#x}"),
            Err(e) => kindle_core::warn!("overlay: SpEffect {id} on {handle:#x}: {e}"),
        },
        UiRequest::ParamRow { table, row, reply } => {
            let _ = reply.send(param_row(&table, row));
        }
        UiRequest::ParamSet { table, row, field, value, reply } => {
            let result = param_set(&table, row, &field, &value);
            if let Err(e) = &result {
                kindle_core::warn!("overlay: {table}[{row}].{field}: {e}");
            }
            let _ = reply.send(result);
        }
        UiRequest::Find { query, reply } => {
            let _ = reply.send(crate::search::search(&query));
        }
        _ => {}
    }
}

fn param_row(table: &str, row: u64) -> Result<Vec<ui_bridge::ParamField>, String> {
    let reg = kindle_core::params::registry()?;
    let t = reg.table(table)?;
    if !t.has_row(row) {
        return Err(format!("{} has no row {row}", t.name));
    }
    t.def
        .fields
        .iter()
        .map(|f| {
            Ok(ui_bridge::ParamField { name: f.name.clone(), ty: f.ty.name(), value: t.read(row, &f.name)?.to_string(), writable: f.is_scalar() && t.valid })
        })
        .collect()
}

fn param_set(table: &str, row: u64, field: &str, value: &str) -> Result<(), String> {
    let v = value.trim();
    let value = if let Ok(i) = v.parse::<i64>() {
        kindle_paramdef::Value::Int(i)
    } else if let Ok(f) = v.parse::<f32>() {
        kindle_paramdef::Value::Float(f)
    } else {
        return Err(format!("{value:?} is not a number"));
    };
    let reg = kindle_core::params::registry()?;
    let t = reg.table(table)?;
    let old = t.write(row, field, &value)?;
    kindle_core::info!("overlay: {}[{row}].{field} = {value} (was {old})", t.name);
    Ok(())
}

/// Mods' event commands run (on the game's thread), each reply to the mod that ran it: on_done(true)
/// when the game's handler finished the command, (nil, why) when it couldn't run.
fn event_commands(rt: &mut Runtime) {
    for (id, done) in crate::emevd::run_queued() {
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            let f = m.lua.as_ref().and_then(|l| {
                let mut c = l.app_data_mut::<mods::ModCtx>()?;
                let i = c.handlers.replies.iter().position(|(rid, _)| *rid == id)?;
                Some(c.handlers.replies.remove(i).1)
            });
            if let Some(f) = f {
                m.call(limit, "event command reply", |_| match done {
                    Some(d) => f.call::<()>(d),
                    None => f.call::<()>((mlua::Value::Nil, "the command couldn't run (see the log)")),
                });
                break;
            }
        }
    }
}

/// The mods' `player_loaded` once the player's character has been in the world for SETTLE_FRAMES frames,
/// `player_unloaded` as it leaves (to the title, or a load); as Dark Souls III's, a mod's flag watches start
/// over and its character store is written and closed then (the next character may be another).
fn player_comes_and_goes(rt: &mut Runtime) {
    let exists = crate::player::is_loaded().unwrap_or(false);
    let seen = if exists {
        IN_WORLD_FOR.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    } else {
        IN_WORLD_FOR.store(0, Ordering::Relaxed);
        0
    };
    let loaded = exists && (rt.player_loaded || seen >= SETTLE_FRAMES);
    if loaded == rt.player_loaded {
        return;
    }
    rt.player_loaded = loaded;
    let event = if loaded { "player_loaded" } else { "player_unloaded" };
    kindle_core::info!("{event}");
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        if !loaded
            && let Some(lua) = &m.lua
            && let Some(mut c) = lua.app_data_mut::<mods::ModCtx>()
        {
            for w in &mut c.handlers.flags {
                w.last = None;
            }
            c.store_character = None;
        }
        dispatch_event(m, event, limit, ());
    }
}

/// Runs the frame 60 times a second on this thread until the game runs Kindle's frame task (registered
/// from here once the game can take it). A panic in it stops the mods (the game continues).
pub fn run() {
    kindle_core::set_tick_thread();
    loop {
        let start = Instant::now();
        crate::task::try_register();
        if crate::task::taken() {
            kindle_core::info!("Kindle's own frame thread stops: the game runs the frame now");
            return;
        }
        if !HALTED.load(Ordering::Relaxed) {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                let mut guard = RUNTIME.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(rt) = guard.as_mut() {
                    tick(rt);
                }
            }));
            if result.is_err() {
                HALTED.store(true, Ordering::Relaxed);
                kindle_core::error!("Kindle hit an internal error and has stopped running mods (the game continues). See the panic above.");
            }
        }
        std::thread::sleep(FRAME.saturating_sub(start.elapsed()));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use kindle_core::config::KindleConfig;
    use kindle_core::runtime::Runtime;

    use super::{SETTLE_FRAMES, tick};
    use crate::params::tests::{Repo, TEST_LOCK};
    use crate::player::tests::{NoPlayer, Player};
    use crate::text::tests::Text;

    /// A runtime on mods in a folder of its own: (id, main.lua) each.
    fn runtime(name: &str, mods: &[(&str, &str)]) -> (Runtime, PathBuf) {
        let kindle_dir: PathBuf = std::env::temp_dir().join(format!("kindle-er-test-{}-{name}", std::process::id()));
        for (id, main) in mods {
            let d = kindle_dir.join("mods").join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("mod.toml"), format!("[mod]\nid = \"{id}\"\ngames = [\"er\"]\n")).unwrap();
            std::fs::write(d.join("main.lua"), main).unwrap();
        }
        let (_reloads, reload_rx) = std::sync::mpsc::channel();
        let (_console, console_rx) = std::sync::mpsc::channel();
        let (_ui, ui_rx) = std::sync::mpsc::channel();
        (Runtime::new(kindle_dir.clone(), KindleConfig::default(), reload_rx, console_rx, ui_rx), kindle_dir)
    }

    /// The game's params (a table of one row) and text (its goods names), as the game has them loaded.
    fn game_data() -> (Repo, Text) {
        let file = crate::param_rows::tests::file(&[(10, [1, 2, 3, 4])]);
        (Repo::serve(&[("EquipParamGoods".to_string(), file)]), Text::serve(&[(10, &[(10000, "Ruin Fragment")])]))
    }

    #[test]
    fn mods_wait_for_the_games_params_and_text() {
        let _one_at_a_time = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        kindle_core::bindings::set(&crate::BINDINGS);
        let _title_screen = NoPlayer::serve();
        let (mut rt, kindle_dir) = runtime("wait", &[("er-wait", "kindle.export { up = function() return true end }\n")]);
        for _ in 0..3 {
            tick(&mut rt);
        }
        assert!(rt.mods.is_empty() && !rt.mods_loaded, "the mods loaded before the game's data");
        let file = crate::param_rows::tests::file(&[(10, [1, 2, 3, 4])]);
        let repo = Repo::serve(&[("EquipParamGoods".to_string(), file)]);
        tick(&mut rt);
        assert!(rt.mods.is_empty(), "the mods loaded before the game's text");
        let text = Text::serve(&[(10, &[(10000, "Ruin Fragment")])]);
        tick(&mut rt);
        let running: Vec<(&str, String)> = rt.mods.iter().map(|m| (m.id(), m.status())).collect();
        assert_eq!(running, vec![("er-wait", "running".to_string())]);
        drop((text, repo));
        let _ = std::fs::remove_dir_all(kindle_dir);
    }

    #[test]
    fn mods_load_and_run_on_kindles_frame() {
        let _one_at_a_time = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        kindle_core::bindings::set(&crate::BINDINGS);
        let _game_data = game_data();
        let _title_screen = NoPlayer::serve();
        let mods = [
            (
                "er-a",
                r#"local frames, fired, got = 0, false, nil
kindle.on("frame", function(dt) frames = frames + 1 end)
kindle.after(0, function() fired = true end)
kindle.on_message("ping", function(data, from) got = from .. ":" .. tostring(data) end)
kindle.export { state = function() return string.format("%d %s %s", frames, tostring(fired), tostring(got)) end }
"#,
            ),
            ("er-b", "kindle.after(0, function() kindle.send(\"ping\", 7) end)\n"),
        ];
        let (mut rt, kindle_dir) = runtime("frame", &mods);
        // A mod of game files only: no main.lua, yet it runs.
        let files_only = kindle_dir.join("mods").join("er-c");
        std::fs::create_dir_all(files_only.join("files").join("menu")).unwrap();
        std::fs::write(files_only.join("mod.toml"), "[mod]\nid = \"er-c\"\ngames = [\"er\"]\n").unwrap();
        // The first frame loads the mods (the game's data is there); er-b's timer sends on the second, er-a
        // gets it on the third.
        for _ in 0..5 {
            tick(&mut rt);
        }
        let running: Vec<(&str, String)> = rt.mods.iter().map(|m| (m.id(), m.status())).collect();
        assert_eq!(running, vec![("er-a", "running".to_string()), ("er-b", "running".to_string()), ("er-c", "running".to_string())]);
        let state = rt.console_lua("kindle.mods.get('er-a').state()", "test");
        assert!(state.contains("5 true er-b:7"), "{state}");
        let _ = std::fs::remove_dir_all(kindle_dir);
    }

    #[test]
    fn mods_hear_of_the_player_coming_into_the_world_and_leaving() {
        let _one_at_a_time = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        kindle_core::bindings::set(&crate::BINDINGS);
        let _game_data = game_data();
        let title_screen = NoPlayer::serve();
        let main = r#"local seen = {}
kindle.on("player_loaded", function() seen[#seen + 1] = "loaded" end)
kindle.on("player_unloaded", function() seen[#seen + 1] = "unloaded" end)
kindle.export { seen = function() return "[" .. table.concat(seen, " ") .. "]" end }
"#;
        let (mut rt, kindle_dir) = runtime("player", &[("er-player", main)]);
        let seen = |rt: &mut Runtime| rt.console_lua("kindle.mods.get('er-player').seen()", "test");
        tick(&mut rt);
        drop(title_screen);
        let player = Player::serve(2);
        for _ in 1..SETTLE_FRAMES {
            tick(&mut rt);
        }
        assert!(seen(&mut rt).contains("[]"), "{}", seen(&mut rt));
        tick(&mut rt);
        assert!(seen(&mut rt).contains("[loaded]"), "{}", seen(&mut rt));
        tick(&mut rt);
        drop(player);
        let _title_screen = NoPlayer::serve();
        tick(&mut rt);
        assert!(seen(&mut rt).contains("[loaded unloaded]"), "{}", seen(&mut rt));
        let _ = std::fs::remove_dir_all(kindle_dir);
    }
}
