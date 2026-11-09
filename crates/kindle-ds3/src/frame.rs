//! Dark Souls III's frame: Kindle's work on the game's thread once per frame, in order (the
//! game's data, characters loading in and out of the world, what the game's parts and hooks have
//! for the mods), and the hooks' dispatch to the mods' handlers (lib.rs's hooks call in here
//! through `runtime::with_runtime`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use mlua::{Function, MultiValue};

use crate::game;
use crate::lua_values;
use crate::mods::{self, LoadedMod, ModState};
use crate::runtime::{Runtime, dispatch_event, run_flag_watches, run_timers};
use crate::ui_bridge::{self, UiRequest};

/// A `map_loading` handler's time limit: the player is on a loading screen, and the game's loader
/// waits for it (a mod working out a character's changes as its world loads).
const LOADING_LIMIT: Duration = Duration::from_secs(10);
/// Longest Kindle waits for the game's params before loading mods regardless.
const DATA_WAIT_LIMIT: Duration = Duration::from_secs(180);

/// What the frame keeps from one frame to the next.
struct FrameState {
    /// Play time when last read, and frames since it last changed.
    play_time: Option<u32>,
    play_time_still: u32,
    /// The world is running (not on a loading screen, warps included).
    world_live: bool,
    /// The editor stubs were generated from complete game data this session.
    stubs_written: bool,
    events: crate::game_events::Tracker,
    /// The keys and buttons the player held last frame (for key_pressed).
    keys_down: Vec<&'static str>,
}

static STATE: LazyLock<Mutex<FrameState>> = LazyLock::new(|| {
    Mutex::new(FrameState {
        play_time: None,
        play_time_still: u32::MAX,
        world_live: false,
        stubs_written: false,
        events: Default::default(),
        keys_down: Vec::new(),
    })
});

/// The player sat down at a bonfire (seen by the animation hook); fired next frame.
static RESTED: AtomicBool = AtomicBool::new(false);

/// Characters for hook dispatch, refreshed at most once per frame.
static CHARS: Mutex<Option<(u64, Arc<Vec<crate::chars::Character>>)>> = Mutex::new(None);

/// Kindle's work for one frame, on the game's thread, in order: the game's data, characters
/// loading in and out of the world, what the game's parts and hooks have for the mods, the mods'
/// handlers. Each step is a function below.
pub fn tick(rt: &mut Runtime) {
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let frame = mods::FRAME.fetch_add(1, Ordering::Relaxed) + 1;
    let now = Instant::now();
    let dt = rt.last_tick.map(|t| (now - t).as_secs_f64()).unwrap_or(0.0);
    rt.last_tick = Some(now);

    load_mods_when_ready(rt, frame, now);
    if rt.mods_loaded {
        rt.process_reloads(now);
        run_map_loading(rt);
    }
    rt.process_console();
    rt.process_ui_requests(ui_request);

    track_player_load(rt, &mut st, frame);
    rt.roll_cpu(now);
    keep_ai_and_states(rt, &st, frame);
    if frame.is_multiple_of(60) {
        rt.flush_stores();
    }
    write_stubs(rt, &mut st, frame);
    deliver_hks_replies(rt, frame);
    deliver_tab_presses(rt);
    deliver_key_presses(rt, &mut st);
    deliver_screen_events(rt);
    deliver_title_picks(rt);
    deliver_session_joins(rt);
    deliver_lobby_joins(rt);
    deliver_net_messages(rt);
    rt.deliver_messages();
    run_event_commands(rt);
    update_wanted_hooks(rt, frame);
    poll_game_events(rt, &mut st, frame);
    dispatch_catalogue_events(rt);
    dispatch_banners(rt);
    dispatch_shop_buys(rt);
    dispatch_travels(rt);
    dispatch_boss_bars(rt);
    dispatch_rested(rt);
    run_frame_handlers(rt, now, dt);
    publish_ui(rt, frame);
}

/// Loads the mods once the game's data is there (or Kindle stops waiting for it).
fn load_mods_when_ready(rt: &mut Runtime, frame: u64, now: Instant) {
    let first = *rt.first_frame.get_or_insert_with(|| {
        crate::info!("first frame (frame {frame}); waiting for the game's data before loading mods");
        now
    });
    if !rt.mods_loaded {
        let ready = crate::params::registry().is_ok();
        let waited = now.duration_since(first);
        if ready || waited > DATA_WAIT_LIMIT {
            if ready {
                crate::info!("game data ready after {:.1}s", waited.as_secs_f64());
                crate::files::find_index_copies();
            } else {
                crate::warn!("game data still not ready after {}s; loading mods anyway", waited.as_secs());
            }
            rt.mods_loaded = true;
            crate::hks::install_bridge();
            rt.pending_reloads.clear();
            while rt.reload_rx.try_recv().is_ok() {}
            rt.load_all();
            if !rt.safe_mode {
                let installed = rt.mods.iter().map(|m| format!("{} {} ({})", m.id(), m.manifest.info.version.as_deref().unwrap_or(""), m.status())).collect();
                crate::saves::backup(&rt.kindle_dir, rt.config.runtime.save_backups, installed);
            }
        }
    }
}

/// Player load/unload transitions. Loaded means in the world: the character exists and play
/// time runs (it stands still on loading screens, and the map's scripts set the world up, e.g.
/// switching dormant enemies' AI off, only when they end).
fn track_player_load(rt: &mut Runtime, st: &mut FrameState, frame: u64) {
    let play_time = game::play_time_ms();
    if let (Some(a), Some(b)) = (st.play_time, play_time)
        && a != b
    {
        st.play_time_still = 0;
    } else {
        st.play_time_still = st.play_time_still.saturating_add(1);
    }
    st.play_time = play_time;
    let exists = game::player_loaded();
    st.world_live = exists && st.play_time_still < 30;
    let loaded = exists && (rt.player_loaded || st.world_live);
    game::set_in_world(loaded);
    if loaded {
        crate::map_edits::frame(frame);
    }
    crate::quick_warp::frame();
    if loaded != rt.player_loaded {
        rt.player_loaded = loaded;
        let event = if loaded { "player_loaded" } else { "player_unloaded" };
        crate::info!("{event}");
        if loaded {
            record_character(rt);
            if crate::p2p::hosting() != Some(false) {
                match crate::game::repair_flasks() {
                    Ok(given) if !given.is_empty() => crate::info!("flasks: {} given back, empty (a rest fills it)", given.join(" and ")),
                    Ok(_) => {}
                    Err(e) => crate::debug!("flasks: not checked ({e})"),
                }
            }
        }
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            if !loaded
                && let Some(lua) = &m.lua
                && let Some(mut c) = lua.app_data_mut::<mods::ModCtx>()
            {
                for w in &mut c.handlers.flags {
                    w.last = None;
                }
                // The next character may be another one: write and close its store.
                c.store_character = None;
            }
            dispatch_event(m, event, limit, ());
        }
    }
}

/// AI that mods turned off stays off: a dormant enemy waking up, or a character reloading, gets
/// its AI state reset by the game without an event command. Character states a mod changed go
/// back to the game's defaults once it unloads, while the world is running (map scripts set
/// characters up when a loading screen ends).
fn keep_ai_and_states(rt: &mut Runtime, st: &FrameState, frame: u64) {
    if frame % 60 == 30 && rt.player_loaded && st.world_live {
        let all = characters();
        crate::chars::keep_ai_off(&all);
        let loaded: std::collections::HashMap<i32, usize> = all.iter().filter_map(|c| Some((c.entity_id?, c.addr))).collect();
        for (entity, owner) in crate::emevd::ai_held_off() {
            let Some(&chr) = loaded.get(&entity) else { continue };
            if game::ai_enabled(chr) == Some(false) {
                continue;
            }
            let mut args = entity.to_le_bytes().to_vec();
            args.extend([0, 0, 0, 0]);
            let _ = crate::emevd::run(crate::emevd::Command { id: 0, owner, bank: 2004, index: 1, args });
        }
        for cmd in crate::emevd::owed_restores(&loaded) {
            let _ = crate::emevd::run(cmd);
        }
    }
}

/// Editor stubs from the game's data, once it's all available (retried every second).
fn write_stubs(rt: &mut Runtime, st: &mut FrameState, frame: u64) {
    if rt.player_loaded && !st.stubs_written && frame.is_multiple_of(60) {
        match crate::stubs::write(&rt.kindle_dir) {
            Ok(complete) => st.stubs_written = complete,
            Err(e) => {
                crate::warn!("editor stubs: {e}");
                st.stubs_written = true;
            }
        }
    }
}

/// The player's character for HKS, and answers to mods' HKS requests.
fn deliver_hks_replies(rt: &mut Runtime, frame: u64) {
    if let Ok(p) = game::player_chr_addr() {
        crate::hks::set_player(p);
    }
    let results = crate::hks::take_results(frame);
    if !results.is_empty() {
        let limit = rt.limit();
        for (id, reply) in results {
            for m in rt.all_mods_mut() {
                let f = m.lua.as_ref().and_then(|l| {
                    let mut c = l.app_data_mut::<mods::ModCtx>()?;
                    let i = c.handlers.replies.iter().position(|(rid, _)| *rid == id)?;
                    Some(c.handlers.replies.remove(i).1)
                });
                if let Some(f) = f {
                    use crate::hks::Reply;
                    m.call(limit, "hks reply", |lua| match &reply {
                        Reply::Int(i) => f.call::<()>(*i),
                        Reply::Value(v) => f.call::<()>(lua_values::hks_to_lua(lua, v)?),
                        Reply::Error(e) => f.call::<()>((mlua::Value::Nil, e.as_str())),
                        Reply::Expired => f.call::<()>((mlua::Value::Nil, "the character's script didn't run in time")),
                    });
                    break;
                }
            }
        }
    }
}

/// Presses of mods' tab buttons (kindle.ui.mod_tab): on_press(pasted text, for a paste). By
/// label: a press on a screen opened before the mod changed its tab goes to the button of that
/// label now, or nowhere.
/// key_pressed: each key or button the player pressed since last frame, by name, while no
/// screen or window of Kindle's has the input (typing in the console isn't a mod's hotkey).
fn deliver_key_presses(rt: &mut Runtime, st: &mut FrameState) {
    let wanted =
        rt.mods.iter().chain(std::iter::once(&rt.console)).any(|m| {
            m.lua.as_ref().and_then(|lua| lua.app_data_ref::<mods::ModCtx>().map(|c| !c.handlers.event_fns("key_pressed").is_empty())).unwrap_or(false)
        });
    if !wanted {
        st.keys_down.clear();
        return;
    }
    let now = crate::input_block::inputs_down();
    let pressed: Vec<&'static str> = now.iter().filter(|k| !st.keys_down.contains(k)).copied().collect();
    st.keys_down = now;
    if pressed.is_empty() || ui_bridge::OVERLAY_VISIBLE.load(Ordering::Relaxed) {
        return;
    }
    let limit = rt.limit();
    for key in pressed {
        for m in rt.all_mods_mut() {
            dispatch_event(m, "key_pressed", limit, key);
        }
    }
}

fn deliver_tab_presses(rt: &mut Runtime) {
    for (owner, label, pasted) in crate::native_mods::take_presses() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { continue };
        let f = m
            .lua
            .as_ref()
            .and_then(|lua| lua.app_data_ref::<mods::ModCtx>().and_then(|c| c.handlers.tab.iter().find(|(l, _)| *l == label).map(|(_, f)| f.clone())));
        let Some(f) = f else {
            crate::debug!("mods screen: {owner} has no button {label:?} now; its press is dropped");
            continue;
        };
        m.call(limit, "ui.mod_tab", |_| f.call::<()>(pasted));
    }
}

/// What the player did on mods' screens (kindle.ui.screen), to each screen's dispatcher in the
/// prelude: (kind, row, value) with kind "select", "change" or "close" and row 1-based.
/// Title entries picked whose mod has a function of its own (kindle.ui.title_entry).
fn deliver_title_picks(rt: &mut Runtime) {
    for owner in crate::front_menu::take_title_picks() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { continue };
        let f = m.lua.as_ref().and_then(|l| l.app_data_ref::<mods::ModCtx>()?.handlers.title.clone());
        if let Some(f) = f {
            m.call(limit, "ui.title_entry", |_| f.call::<()>(()));
        }
    }
}

fn deliver_screen_events(rt: &mut Runtime) {
    use kindle_core::screens::ScreenEvent;
    for (owner, id, event) in kindle_core::screens::take_events() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { continue };
        let f = m.lua.as_ref().and_then(|l| l.app_data_ref::<mods::ModCtx>()?.handlers.screens.iter().find(|(s, _)| *s == id).map(|(_, f)| f.clone()));
        let Some(f) = f else { continue };
        let (kind, row, value, text) = match event {
            ScreenEvent::Selected(row) => ("select", Some(row + 1), None, None),
            ScreenEvent::Changed(row, v) => ("change", Some(row + 1), Some(v), None),
            ScreenEvent::Text(row, text) => ("text", Some(row + 1), None, Some(text)),
            ScreenEvent::Tab(tab) => ("tab", Some(tab + 1), None, None),
            ScreenEvent::Closed => ("close", None, None, None),
        };
        m.call(limit, "ui.screen", |_| f.call::<()>((kind, row, value, text)));
        if let Some(lua) = &m.lua
            && let Some(mut c) = lua.app_data_mut::<mods::ModCtx>()
        {
            c.handlers.screens.retain(|(s, _)| kindle_core::screens::is_open(*s));
        }
    }
}

/// Joins started with kindle.net.join, made in the game once connected: fn(result).
fn deliver_session_joins(rt: &mut Runtime) {
    for (owner, handle, outcome) in crate::session::finished_joins() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { continue };
        let f = match &m.lua {
            Some(lua) => lua.app_data_mut::<mods::ModCtx>().and_then(|mut c| {
                let at = c.handlers.joins.iter().position(|(h, _)| *h == handle)?;
                Some(c.handlers.joins.remove(at).1)
            }),
            None => None,
        };
        let Some(f) = f else { continue };
        let (result, why) = match outcome {
            crate::session::JoinOutcome::Joined => ("joined", None),
            crate::session::JoinOutcome::Gone(e) => ("gone", Some(e)),
            crate::session::JoinOutcome::Refused(e) => ("refused", Some(e)),
            crate::session::JoinOutcome::Failed(e) => ("failed", Some(e)),
        };
        m.call(limit, "net.join", |_| f.call::<()>((result, why)));
    }
}

/// Lobbies joined with kindle.net.join_lobby: fn(result, why).
fn deliver_lobby_joins(rt: &mut Runtime) {
    for (owner, handle, outcome) in crate::lobby::finished_joins() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { continue };
        let f = match &m.lua {
            Some(lua) => lua.app_data_mut::<mods::ModCtx>().and_then(|mut c| {
                let at = c.handlers.joins.iter().position(|(h, _)| *h == handle)?;
                Some(c.handlers.joins.remove(at).1)
            }),
            None => None,
        };
        let Some(f) = f else { continue };
        let (result, why) = match outcome {
            crate::lobby::JoinOutcome::Joined => ("joined", None),
            crate::lobby::JoinOutcome::Refused(e) => ("refused", Some(e)),
            crate::lobby::JoinOutcome::Failed(e) => ("failed", Some(e)),
        };
        m.call(limit, "net.join_lobby", |_| f.call::<()>((result, why)));
    }
}

/// Messages from the same mod on the session's other games (kindle.net): fn(value, from).
fn deliver_net_messages(rt: &mut Runtime) {
    for msg in crate::p2p::take_mod_messages() {
        let limit = rt.limit();
        let Some(m) = rt.all_mods_mut().find(|m| m.id() == msg.mod_id) else { continue };
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua
                .app_data_ref::<mods::ModCtx>()
                .map(|c| c.handlers.net.iter().filter(|(_, ch, _)| *ch == msg.channel).map(|(_, _, f)| f.clone()).collect())
                .unwrap_or_default(),
            None => continue,
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&msg.data) else { continue };
        let from = format!("{:#x}", msg.from);
        for f in fns {
            let ok = m.call(limit, "net", |lua| f.call::<()>((crate::store::to_lua(lua, &value)?, from.as_str())));
            if ok.is_none() {
                break;
            }
        }
    }
}

/// Mods' event commands. Lots mods award themselves go through the item_lot handlers here: the
/// game rolls them while the command runs, when the runtime is busy and its hook can't ask.
/// Then the results: on_done(true) when the game's handler finished one.
fn run_event_commands(rt: &mut Runtime) {
    let mut queued = crate::emevd::take_queued();
    let mut cancelled = Vec::new();
    queued.retain_mut(|cmd| {
        if (cmd.bank, cmd.index) != crate::emevd::AWARD_ITEM_LOT || cmd.args.len() < 4 {
            return true;
        }
        let mut roll = crate::hooks::ItemLot { lot: Some(i32::from_le_bytes([cmd.args[0], cmd.args[1], cmd.args[2], cmd.args[3]])) };
        dispatch_item_lot(rt, &mut roll);
        match roll.lot {
            Some(lot) => {
                cmd.args[..4].copy_from_slice(&lot.to_le_bytes());
                true
            }
            None => {
                cancelled.push((cmd.id, Some(true)));
                false
            }
        }
    });
    for (id, done) in crate::emevd::run_commands(queued).into_iter().chain(cancelled) {
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

/// Hooks are installed lazily, once some mod wants them; the menu options' and HKS watches'
/// flags kept current.
fn update_wanted_hooks(rt: &mut Runtime, frame: u64) {
    if rt.player_loaded && frame.is_multiple_of(60) && rt.subscribed("damage") {
        crate::hooks::ensure_damage_hooks();
    }
    // Menu options: keep the hook flag current.
    let any_options = rt
        .mods
        .iter()
        .chain(std::iter::once(&rt.console))
        .any(|m| m.lua.as_ref().and_then(|l| l.app_data_ref::<mods::ModCtx>().map(|c| !c.handlers.menu_options.is_empty())).unwrap_or(false));
    crate::talk_menu::ACTIVE.store(any_options, Ordering::Relaxed);

    if frame.is_multiple_of(30) {
        let mut env = std::collections::HashSet::new();
        let mut act = std::collections::HashSet::new();
        for m in rt.mods.iter().chain(std::iter::once(&rt.console)) {
            if let Some(c) = m.lua.as_ref().and_then(|l| l.app_data_ref::<mods::ModCtx>()) {
                for (_, is_act, id, _) in &c.handlers.hks {
                    if *is_act {
                        act.insert(id.clone())
                    } else {
                        env.insert(id.clone())
                    };
                }
            }
        }
        crate::hks::set_watched(env, act);
        let wanted = rt.player_loaded && rt.subscribed("item_used");
        crate::hooks::ITEM_USE_WANTED.store(wanted, Ordering::Relaxed);
        if wanted {
            crate::hooks::ensure_item_use_hook();
        }
        let wanted = rt.player_loaded && rt.subscribed("effect");
        crate::hooks::EFFECT_WANTED.store(wanted, Ordering::Relaxed);
        if wanted {
            crate::hooks::ensure_effect_hook();
        }
        // Map pickups are rolled while an area loads, before a character is in the world.
        let wanted = rt.subscribed("item_lot");
        crate::hooks::ITEM_LOT_WANTED.store(wanted, Ordering::Relaxed);
        if wanted {
            crate::hooks::ensure_item_lot_hook();
        }
        if rt.player_loaded
            && (rt.subscribed("banner") || rt.subscribed("boss_defeated"))
            && let Err(e) = crate::emevd::watch_banners()
        {
            crate::warn!("{e}");
        }
        if rt.player_loaded
            && rt.subscribed("boss_bar")
            && let Err(e) = crate::emevd::watch_boss_bars()
        {
            crate::warn!("{e}");
        }
        if rt.player_loaded
            && rt.subscribed("travel")
            && let Err(e) = crate::emevd::watch_travel()
        {
            crate::warn!("{e}");
        }
        if rt.player_loaded
            && rt.subscribed("shop_buy")
            && let Err(e) = crate::shop::watch()
        {
            crate::warn!("{e}");
        }
        let wanted = rt.player_loaded && (rt.subscribed("animation") || rt.subscribed("bonfire_rested"));
        crate::hooks::ANIMATION_WANTED.store(wanted, Ordering::Relaxed);
        if wanted {
            crate::hooks::ensure_animation_hook();
        }
    }
}

/// The game events (game_events.rs) the mods listen for, polled while a character is loaded.
fn poll_game_events(rt: &mut Runtime, st: &mut FrameState, frame: u64) {
    if !rt.player_loaded {
        st.events.reset();
    } else {
        let wants = subscribed_game_events(rt);
        if !wants.is_empty() {
            let fired = st.events.poll(frame, &wants);
            let limit = rt.limit();
            for ev in &fired {
                for m in rt.all_mods_mut() {
                    dispatch_game_event(m, ev, limit);
                }
            }
        }
    }
}

/// The catalogue's events since the last frame (from any thread), in order.
fn dispatch_catalogue_events(rt: &mut Runtime) {
    let queued = crate::catalogue::take_events();
    if !queued.is_empty() {
        let limit = rt.limit();
        for (name, values) in &queued {
            let args = MultiValue::from_vec(values.iter().cloned().map(crate::natives::game::to_lua).collect());
            for m in rt.all_mods_mut() {
                dispatch_event(m, name, limit, args.clone());
            }
        }
    }
}

/// Banners the game showed: banner(name, type) for each, boss_defeated(kind) for the boss ones
/// (HEIR OF FIRE DESTROYED, LORD OF CINDER FALLEN).
fn dispatch_banners(rt: &mut Runtime) {
    for kind in crate::emevd::take_banners() {
        let name = crate::emevd::banner_name(kind);
        let boss = match kind {
            1 => Some("heir_of_fire"),
            22 => Some("lord_of_cinder"),
            _ => None,
        };
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            dispatch_event(m, "banner", limit, (name, kind));
            if let Some(b) = boss {
                dispatch_event(m, "boss_defeated", limit, b);
            }
        }
    }
}

/// Boss bars the game's scripts showed or hid: boss_bar(shown, name, entity).
fn dispatch_boss_bars(rt: &mut Runtime) {
    for (shown, entity, name_id) in crate::emevd::take_boss_bars() {
        let name = u32::try_from(name_id)
            .ok()
            .filter(|&id| id > 0)
            .and_then(|id| crate::text_override::get_any(&crate::text_override::category_ids("npc_name").ok()?, id))
            .filter(|s| !s.trim().is_empty());
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            dispatch_event(m, "boss_bar", limit, (shown, name.clone(), entity));
        }
    }
}

/// Travels the game began: travel(map, entity, how).
fn dispatch_travels(rt: &mut Runtime) {
    for (map, entity, how) in crate::emevd::take_travels() {
        let [a, b, ..] = map.to_be_bytes();
        let map = format!("m{a:02}_{b:02}_00_00");
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            dispatch_event(m, "travel", limit, (map.clone(), entity, how));
        }
    }
}

/// Shop purchases: shop_buy(item, count, row).
fn dispatch_shop_buys(rt: &mut Runtime) {
    for (row, count) in crate::shop::take() {
        let Some(item) = crate::shop::item_of(row) else { continue };
        let limit = rt.limit();
        for m in rt.all_mods_mut() {
            dispatch_event(m, "shop_buy", limit, (item, count, row));
        }
    }
}

/// The player sat down at a bonfire (seen by the animation hook last frame): bonfire_rested.
fn dispatch_rested(rt: &mut Runtime) {
    if RESTED.swap(false, Ordering::Relaxed) && rt.player_loaded {
        let limit = rt.limit();
        let last = crate::bonfire::last_rested().ok().flatten();
        for m in rt.all_mods_mut() {
            let fns: Vec<Function> = match &m.lua {
                Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("bonfire_rested")).unwrap_or_default(),
                None => continue,
            };
            for f in fns {
                let ok = m.call(limit, "bonfire_rested", |lua| {
                    let b = crate::bonfire::all()
                        .ok()
                        .and_then(|all| all.into_iter().find(|b| Some(b.entity_id) == last))
                        .map(|b| lua_values::bonfire_table(lua, b, last))
                        .transpose()?;
                    f.call::<()>(b)
                });
                if ok.is_none() {
                    break;
                }
            }
        }
    }
}

/// Timers, flag watches and the `frame` event.
fn run_frame_handlers(rt: &mut Runtime, now: Instant, dt: f64) {
    let limit = rt.limit();
    let player_loaded = rt.player_loaded;
    for m in rt.all_mods_mut() {
        run_timers(m, now, limit);
        if player_loaded {
            run_flag_watches(m, limit, game::flag_get);
        }
        dispatch_event(m, "frame", limit, dt);
    }
}

/// The mods list for the game menus' Mods screen (twice a second), and the overlay's data (about
/// 6 times a second while it's open, at once when it opens).
fn publish_ui(rt: &mut Runtime, frame: u64) {
    if frame.is_multiple_of(30) {
        let rows = rt.mod_rows();
        ui_bridge::publish_mods(rows);
    }
    ui_bridge::IN_WORLD.store(rt.player_loaded, Ordering::Relaxed);
    // Mods' layers with world points are drawn where this frame's camera sees them.
    if kindle_core::hud::wants_camera() {
        let cam = game::camera().ok().filter(|_| rt.player_loaded);
        kindle_core::hud::set_camera(cam.map(|c| kindle_core::hud::Camera { pos: c.pos, right: c.right, up: c.up, forward: c.forward, fov: c.fov }));
    }

    if ui_bridge::visible() && (frame >= rt.last_snapshot + 10 || rt.last_snapshot == 0) {
        publish_snapshot(rt, frame);
    } else if !ui_bridge::visible() {
        rt.last_snapshot = 0;
    }
}

/// The `map_loading` handlers for the area files the game's loader is waiting to read
/// (map_loading.rs), with the time a loading screen allows; and whether any mod has one.
fn run_map_loading(rt: &mut Runtime) {
    let mut wanted = false;
    for m in rt.all_mods_mut() {
        if let Some(lua) = &m.lua
            && lua.app_data_ref::<mods::ModCtx>().is_some_and(|c| !c.handlers.event_fns("map_loading").is_empty())
        {
            wanted = true;
        }
    }
    crate::map_loading::set_wanted(wanted);
    for request in crate::map_loading::take() {
        for m in rt.all_mods_mut() {
            dispatch_event(m, "map_loading", LOADING_LIMIT, (request.map.clone(), request.file));
        }
        request.finish();
    }
}

/// Notes which mods are running with the character that just loaded (saves.rs).
fn record_character(rt: &mut Runtime) {
    let Ok(slot) = crate::bonfire::save_slot() else { return };
    let Ok(data) = game::player_game_data() else { return };
    let name = data.player_info().name();
    let running = rt
        .mods
        .iter()
        .filter(|m| matches!(m.state, ModState::Running))
        .map(|m| (m.id().to_string(), m.manifest.info.version.clone().unwrap_or_default()))
        .collect();
    rt.character_mods = crate::saves::record_character(&rt.kindle_dir, slot, &name, &running);
}

fn publish_snapshot(rt: &mut Runtime, frame: u64) {
    rt.last_snapshot = frame;
    let mods = rt.mod_rows();
    let player = if rt.player_loaded {
        match (game::chr_data(), game::player_game_data()) {
            (Ok(d), Ok(g)) => {
                let i = g.player_info();
                Some(ui_bridge::PlayerSnap {
                    name: i.name(),
                    hp: (d.hp, d.max_hp),
                    fp: (d.fp, d.max_fp),
                    stamina: (d.stamina, d.max_stamina),
                    stats: [
                        ("Vigor", i.vigor),
                        ("Attunement", i.attunement),
                        ("Endurance", i.endurance),
                        ("Vitality", i.vitality),
                        ("Strength", i.strength),
                        ("Dexterity", i.dexterity),
                        ("Intelligence", i.intelligence),
                        ("Faith", i.faith),
                        ("Luck", i.luck),
                    ],
                })
            }
            _ => None,
        }
    } else {
        None
    };
    let inventory = if rt.player_loaded {
        let names = crate::text::item_names().ok();
        game::inventory()
            .unwrap_or_default()
            .into_iter()
            .map(|(cat, id, qty)| {
                let kind = crate::text::Kind::parse(cat).unwrap_or(crate::text::Kind::Goods);
                let name = match game::make_item(kind.category(), id as i64).ok().and_then(|raw| game::item_id(raw).ok()) {
                    Some(item) => lua_values::display_name(item),
                    None => names.as_ref().and_then(|n| n.name(kind, id)),
                }
                .unwrap_or_default();
                (cat, id, qty, name)
            })
            .collect()
    } else {
        Vec::new()
    };
    let flags = ui_bridge::watched_flags().into_iter().map(|f| (f, if rt.player_loaded { game::flag_get(f as i64).ok() } else { None })).collect();
    let param_tables = crate::params::registry().map(|r| r.tables().filter(|t| t.valid).map(|t| t.name.clone()).collect()).unwrap_or_default();
    let nearby = if rt.player_loaded {
        match crate::chars::all() {
            Ok(all) => {
                let me = all.iter().find(|c| c.is_player).and_then(|c| c.pos);
                let mut rows: Vec<ui_bridge::NearbyRow> = all
                    .iter()
                    .filter(|c| !c.is_player && c.hp > 0 && c.kind() != "helper")
                    .filter_map(|c| {
                        let d = crate::chars::distance(me?, c.pos?);
                        (d <= 40.0).then(|| ui_bridge::NearbyRow {
                            handle: c.handle,
                            label: crate::chars::name(c.npc_param_id).unwrap_or_else(|| c.model.clone()),
                            kind: c.kind(),
                            entity_id: c.entity_id,
                            hp: (c.hp, c.max_hp),
                            distance: d,
                        })
                    })
                    .collect();
                rows.sort_by(|a, b| a.distance.total_cmp(&b.distance));
                rows.truncate(60);
                rows
            }
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };
    ui_bridge::publish(ui_bridge::Snapshot {
        frame,
        uptime: mods::START.elapsed().as_secs_f64(),
        mods_loaded: rt.mods_loaded,
        safe_mode: rt.safe_mode,
        character_mods: rt.character_mods.clone(),
        mods,
        player,
        inventory,
        flags,
        param_tables,
        nearby,
    });
}

/// The overlay's requests for the game (the runtime does its own).
fn ui_request(_rt: &mut Runtime, req: UiRequest) {
    match req {
        UiRequest::SetFlag(flag, state) => match game::flag_set(flag as i64, state) {
            Ok(_) => crate::info!("overlay: flag {flag} set {}", if state { "ON" } else { "off" }),
            Err(e) => crate::warn!("overlay: flag {flag}: {e}"),
        },
        UiRequest::SetChrHp(handle, hp) => match crate::chars::find(handle as i64) {
            Ok(c) => {
                let hp = if hp == i32::MAX { c.max_hp } else { hp };
                let _ = crate::chars::set_hp(c.addr, hp as i64);
                crate::info!("overlay: {} ({}) hp -> {hp}", c.model, handle);
            }
            Err(e) => crate::warn!("overlay: {e}"),
        },
        UiRequest::ChrEffect(handle, id) => {
            let result = crate::game::checked_effect_id(id).and_then(|id| crate::chars::find(handle as i64).and_then(|c| crate::speffect::apply(c.addr, id)));
            match result {
                Ok(()) => crate::info!("overlay: applied SpEffect {id} to {handle}"),
                Err(e) => crate::warn!("overlay: SpEffect {id} on {handle}: {e}"),
            }
        }
        UiRequest::ParamRow { table, row, reply } => {
            let _ = reply.send(param_row(&table, row));
        }
        UiRequest::ParamSet { table, row, field, value, reply } => {
            let result = param_set(&table, row, &field, &value);
            if let Err(e) = &result {
                crate::warn!("overlay: {table}[{row}].{field}: {e}");
            }
            let _ = reply.send(result);
        }
        UiRequest::Find { query, reply } => {
            let _ = reply.send(crate::search::search(&query));
        }
        _ => {}
    }
}

/// Game events at least one running mod (or the console) listens to.
fn subscribed_game_events(rt: &Runtime) -> std::collections::HashSet<&'static str> {
    let mut wants = std::collections::HashSet::new();
    for m in rt.mods.iter().chain(std::iter::once(&rt.console)) {
        let Some(lua) = &m.lua else { continue };
        let Some(c) = lua.app_data_ref::<mods::ModCtx>() else { continue };
        for ev in crate::game_events::EVENTS {
            if !c.handlers.event_fns(ev).is_empty() {
                wants.insert(*ev);
            }
        }
    }
    wants
}

/// Runs mods' `damage` handlers for one hit. Handlers get a table they can edit
/// (`e.attack_power`) and cancel the hit by returning false. Returns true to cancel.
pub fn dispatch_damage(rt: &mut Runtime, hit: &mut crate::hooks::Damage) -> bool {
    if !rt.player_loaded || !rt.subscribed("damage") {
        return false;
    }
    let list = characters();
    let target = list.iter().find(|c| c.addr == hit.target);
    let attacker = list.iter().find(|c| c.addr == hit.attacker);
    let me = lua_values::player_pos();
    let limit = rt.limit();
    let mut cancel = false;
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("damage")).unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let amount = hit.amount;
            let result = m.call(limit, "damage", |lua| {
                let e = lua.create_table()?;
                if let Some(t) = target {
                    e.set("target", lua_values::character_table(lua, t, me)?)?;
                }
                if let Some(a) = attacker {
                    e.set("attacker", lua_values::character_table(lua, a, me)?)?;
                }
                e.set("attack_power", amount)?;
                e.set("attack_id", hit.attack_id)?;
                let keep: mlua::Value = f.call(e.clone())?;
                Ok((matches!(keep, mlua::Value::Boolean(false)), e.get::<f32>("attack_power").unwrap_or(amount)))
            });
            match result {
                Some((true, _)) => cancel = true,
                Some((false, new)) => hit.amount = new,
                None => break,
            }
        }
    }
    cancel
}

/// Runs mods' `animation` handlers for one animation event. Handlers can replace it
/// (`e.name` / `e.id`) or cancel it by returning false.
pub fn dispatch_animation(rt: &mut Runtime, anim: &mut crate::hooks::Animation) {
    if !rt.player_loaded {
        return;
    }
    let Some(id) = anim.id else { return };
    let list = characters();
    let Some(chr) = list.iter().find(|c| crate::anim::hkb_character(c.addr) == Some(anim.hkb)) else { return };
    let name = crate::anim::name_of(chr.addr, id);
    // Sitting down at a bonfire (the start of resting).
    if chr.is_player && name.as_deref().is_some_and(|n| SIT_DOWN.contains(&n)) {
        RESTED.store(true, Ordering::Relaxed);
    }
    let me = lua_values::player_pos();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("animation")).unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let result = m.call(limit, "animation", |lua| {
                let e = lua.create_table()?;
                e.set("character", lua_values::character_table(lua, chr, me)?)?;
                e.set("id", id)?;
                e.set("name", name.clone())?;
                let keep: mlua::Value = f.call(e.clone())?;
                if matches!(keep, mlua::Value::Boolean(false)) {
                    return Ok(None);
                }
                let new_name: Option<String> = e.get("name")?;
                if new_name != name
                    && let Some(n) = new_name
                {
                    return crate::anim::id_of(chr.addr, &n).map(Some).map_err(mlua::Error::runtime);
                }
                Ok(Some(e.get::<i32>("id").unwrap_or(id)))
            });
            match result {
                Some(new) => anim.id = new,
                None => return,
            }
            if anim.id.is_none() {
                return;
            }
        }
    }
}

/// Loaded characters, cached for the current frame (hooks can fire many times a frame).
fn characters() -> Arc<Vec<crate::chars::Character>> {
    let frame = mods::FRAME.load(Ordering::Relaxed);
    let mut cache = CHARS.lock().unwrap_or_else(|e| e.into_inner());
    match cache.as_ref() {
        Some((f, list)) if *f == frame && !list.is_empty() => list.clone(),
        _ => {
            let list = Arc::new(crate::chars::all().unwrap_or_default());
            *cache = Some((frame, list.clone()));
            list
        }
    }
}

/// Runs mods' `effect` handlers for an SpEffect about to be applied to any character.
/// Handlers can swap it (`e.id`) or cancel it by returning false.
pub fn dispatch_effect(rt: &mut Runtime, effect: &mut crate::hooks::Effect) {
    if !rt.player_loaded {
        return;
    }
    let Some(id) = effect.id else { return };
    let list = characters();
    let Some(target) = list.iter().find(|c| c.addr == effect.target) else { return };
    let source = list.iter().find(|c| c.addr == effect.source);
    let me = lua_values::player_pos();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("effect")).unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let current = effect.id.unwrap_or(id);
            let result = m.call(limit, "effect", |lua| {
                let e = lua.create_table()?;
                e.set("target", lua_values::character_table(lua, target, me)?)?;
                if let Some(s) = source {
                    e.set("source", lua_values::character_table(lua, s, me)?)?;
                }
                e.set("id", current)?;
                let keep: mlua::Value = f.call(e.clone())?;
                if matches!(keep, mlua::Value::Boolean(false)) {
                    return Ok(None);
                }
                Ok(Some(e.get::<i32>("id").unwrap_or(current)))
            });
            match result {
                Some(new) => effect.id = new,
                None => return,
            }
            if effect.id.is_none() {
                return;
            }
        }
    }
}

/// Runs mods' `item_lot` handlers for an item lot about to be rolled. Handlers can swap it
/// (`e.lot`) or cancel it by returning false.
pub fn dispatch_item_lot(rt: &mut Runtime, roll: &mut crate::hooks::ItemLot) {
    let Some(lot) = roll.lot else { return };
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("item_lot")).unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let current = roll.lot.unwrap_or(lot);
            let result = m.call(limit, "item_lot", |lua| {
                let e = lua.create_table()?;
                e.set("lot", current)?;
                let keep: mlua::Value = f.call(e.clone())?;
                if matches!(keep, mlua::Value::Boolean(false)) {
                    return Ok(None);
                }
                Ok(Some(e.get::<i32>("lot").unwrap_or(current)))
            });
            match result {
                Some(new) => roll.lot = new,
                None => return,
            }
            if roll.lot.is_none() {
                return;
            }
        }
    }
}

/// Runs mods' `item_used` handlers. Returning false keeps the item in the inventory.
pub fn dispatch_item_use(rt: &mut Runtime, used: &mut crate::hooks::ItemUse) {
    if !rt.player_loaded {
        return;
    }
    let list = characters();
    let Some(player) = list.iter().find(|c| c.addr == used.player) else { return };
    let Ok(item) = game::make_item(crate::ds3::ItemCategory::Goods, used.goods_id as i64) else { return };
    let name = game::item_id(item).ok().and_then(lua_values::display_name);
    let me = lua_values::player_pos();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns("item_used")).unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let result = m.call(limit, "item_used", |lua| {
                let e = lua.create_table()?;
                e.set("player", lua_values::character_table(lua, player, me)?)?;
                e.set("item", item)?;
                e.set("name", name.clone())?;
                let keep: mlua::Value = f.call(e)?;
                Ok(matches!(keep, mlua::Value::Boolean(false)))
            });
            match result {
                Some(true) => used.keep = true,
                Some(false) => {}
                None => return,
            }
        }
    }
}

/// Runs the callback of a picked mod menu entry (from the talk query hook).
pub fn menu_pick(rt: &mut Runtime, pick: &crate::talk_menu::Pick) {
    use crate::talk_menu::Pick;
    let owner = match pick {
        Pick::Option(o, _) | Pick::Entry(o, _, _) => o.clone(),
    };
    let limit = rt.limit();
    let Some(m) = rt.all_mods_mut().find(|m| m.id() == owner) else { return };
    let f = m.lua.as_ref().and_then(|l| {
        let c = l.app_data_ref::<mods::ModCtx>()?;
        match pick {
            Pick::Option(_, h) => c.handlers.menu_options.iter().find(|o| o.handle == *h).map(|o| o.on_select.clone()),
            Pick::Entry(_, h, i) => c.handlers.menus.iter().find(|(mh, _)| mh == h).and_then(|(_, fns)| fns.get(*i).cloned().flatten()),
        }
    });
    if let Some(f) = f {
        m.call(limit, "menu", |_| f.call::<()>(()));
    }
}

/// Which mod options belong in a list menu with these entries (slot, text id).
pub fn menu_options(rt: &mut Runtime, entries: &[(i32, i32)]) -> Vec<crate::talk_menu::Chosen> {
    // A bonfire's menu has Travel, except before the player reaches Firelink (only Leave):
    // the player sits at a bonfire then.
    let is_bonfire = entries.iter().any(|&(_, t)| t == crate::talk_menu::TRAVEL_TEXT) || crate::talk_menu::sitting_at_bonfire();
    let limit = rt.limit();
    let mut out = Vec::new();
    for m in rt.all_mods_mut() {
        let id = m.id().to_string();
        let options: Vec<(u64, u32, Result<String, Function>)> = match &m.lua {
            Some(lua) => lua
                .app_data_ref::<mods::ModCtx>()
                .map(|c| c.handlers.menu_options.iter().map(|o| (o.handle, o.text_id, o.filter.clone())).collect())
                .unwrap_or_default(),
            None => continue,
        };
        for (handle, text_id, filter) in options {
            let wanted = match filter {
                Ok(kind) => match kind.as_str() {
                    "bonfire" => is_bonfire,
                    "npc" => !is_bonfire,
                    _ => true,
                },
                Err(f) => m
                    .call(limit, "menu option filter", |lua| {
                        let list = lua.create_table()?;
                        for &(slot, text) in entries {
                            let e = lua.create_table()?;
                            e.set("slot", slot)?;
                            e.set("text_id", text)?;
                            e.set("text", crate::text_override::get(30, text as u32))?;
                            list.push(e)?;
                        }
                        let menu = lua.create_table()?;
                        menu.set("entries", list)?;
                        menu.set("bonfire", is_bonfire)?;
                        f.call::<bool>(menu)
                    })
                    .unwrap_or(false),
            };
            if wanted {
                out.push((id.clone(), handle, text_id));
            }
        }
    }
    out
}

/// Runs mods' HKS env/act handlers for one call. env handlers return a new answer (number
/// or boolean; nil keeps it); act handlers return false to block the action.
pub fn dispatch_hks(rt: &mut Runtime, call: &mut crate::hks::Call) {
    if !rt.player_loaded {
        return;
    }
    let list = characters();
    let chr = call.character.and_then(|owner| list.iter().find(|c| c.addr == owner));
    let me = lua_values::player_pos();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua
                .app_data_ref::<mods::ModCtx>()
                .map(|c| c.handlers.hks.iter().filter(|(_, a, id, _)| *a == call.act && *id == call.id).map(|(_, _, _, f)| f.clone()).collect())
                .unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let result = m.call(limit, "hks", |lua| {
                let e = lua.create_table()?;
                if let Some(c) = chr {
                    e.set("character", lua_values::character_table(lua, c, me)?)?;
                }
                match &call.id {
                    crate::hks::HksId::Num(n) => e.set("id", *n)?,
                    crate::hks::HksId::Name(s) => e.set("id", s.as_str())?,
                }
                let args = lua.create_table()?;
                for (i, a) in call.args.iter().skip(1).enumerate() {
                    args.raw_set(i + 1, lua_values::hks_to_lua(lua, a)?)?;
                }
                e.set("args", args)?;
                if !call.act {
                    e.set("result", call.result)?;
                }
                f.call::<mlua::Value>(e)
            });
            match result {
                Some(mlua::Value::Boolean(false)) if call.act => call.allow = false,
                Some(mlua::Value::Boolean(b)) if !call.act => call.result = b as i32,
                Some(mlua::Value::Integer(i)) if !call.act => call.result = i as i32,
                Some(mlua::Value::Number(n)) if !call.act => call.result = n as i32,
                None => return,
                _ => {}
            }
        }
    }
}

/// Runs mods' hks.on_call handlers for a script's kindle(name, ...) call. Every handler
/// runs; the first value one returns is what the script gets.
pub fn dispatch_hks_call(rt: &mut Runtime, call: &mut crate::hks::ScriptCall) {
    if !rt.player_loaded {
        return;
    }
    let list = characters();
    let chr = call.character.and_then(|owner| list.iter().find(|c| c.addr == owner));
    let me = lua_values::player_pos();
    let limit = rt.limit();
    for m in rt.all_mods_mut() {
        let fns: Vec<Function> = match &m.lua {
            Some(lua) => lua
                .app_data_ref::<mods::ModCtx>()
                .map(|c| c.handlers.hks_calls.iter().filter(|(_, n, _)| *n == call.name).map(|(_, _, f)| f.clone()).collect())
                .unwrap_or_default(),
            None => continue,
        };
        for f in fns {
            let result = m.call(limit, "hks call", |lua| {
                let e = lua.create_table()?;
                if let Some(c) = chr {
                    e.set("character", lua_values::character_table(lua, c, me)?)?;
                }
                e.set("name", call.name.as_str())?;
                let args = lua.create_table()?;
                for (i, a) in call.args.iter().enumerate() {
                    args.raw_set(i + 1, lua_values::hks_to_lua(lua, a)?)?;
                }
                e.set("args", args)?;
                f.call::<mlua::Value>(e)
            });
            if call.result.is_some() {
                continue;
            }
            call.result = match result {
                Some(mlua::Value::Boolean(b)) => Some(crate::hks::HksValue::Bool(b)),
                Some(mlua::Value::Integer(i)) => Some(crate::hks::HksValue::Number(i as f32)),
                Some(mlua::Value::Number(n)) => Some(crate::hks::HksValue::Number(n as f32)),
                Some(mlua::Value::String(s)) => Some(crate::hks::HksValue::Str(s.to_string_lossy())),
                _ => None,
            };
        }
    }
}

fn dispatch_game_event(m: &mut LoadedMod, ev: &crate::game_events::Fired, limit: Duration) {
    use crate::game_events::Fired;
    use mlua::IntoLuaMulti;
    let fns: Vec<Function> = match &m.lua {
        Some(lua) => lua.app_data_ref::<mods::ModCtx>().map(|c| c.handlers.event_fns(ev.name())).unwrap_or_default(),
        None => return,
    };
    for f in fns {
        let ok = m.call(limit, ev.name(), |lua| {
            let args: MultiValue = match ev {
                Fired::PlayerDied => ().into_lua_multi(lua)?,
                Fired::RegionChanged { new, old } => (*new, *old).into_lua_multi(lua)?,
                Fired::CharacterKilled(c) | Fired::CharacterSpawned(c) => lua_values::character_table(lua, c, lua_values::player_pos())?.into_lua_multi(lua)?,
                Fired::ItemGained { item, count, name } | Fired::ItemLost { item, count, name } => (*item, *count, name.clone()).into_lua_multi(lua)?,
                Fired::EffectApplied(id) | Fired::EffectExpired(id) => (*id).into_lua_multi(lua)?,
                Fired::LevelUp { level, old, raised, souls } => {
                    let t = lua.create_table()?;
                    for (name, value) in raised {
                        t.set(*name, *value)?;
                    }
                    (*level, *old, t, *souls).into_lua_multi(lua)?
                }
                Fired::EquipmentChanged { slot, item, old } => (*slot, *item, *old).into_lua_multi(lua)?,
                Fired::SpellsChanged { spells, old } => (spells.clone(), old.clone()).into_lua_multi(lua)?,
                Fired::SoulsChanged { souls, old } => (*souls, *old).into_lua_multi(lua)?,
                Fired::BonfireLit(b) => lua_values::bonfire_table(lua, b.clone(), None)?.into_lua_multi(lua)?,
                Fired::MenuOpened(kind) | Fired::MenuClosed(kind) => (*kind).into_lua_multi(lua)?,
                Fired::EmberChanged(embered) => (*embered).into_lua_multi(lua)?,
            };
            f.call::<()>(args)
        });
        if ok.is_none() {
            return;
        }
    }
}

/// Animation events that start resting at a bonfire (sitting down).
const SIT_DOWN: &[&str] = &["W_Event68010", "W_BonfireStart"];

fn param_row(table: &str, row: u64) -> Result<Vec<ui_bridge::ParamField>, String> {
    let reg = crate::params::registry()?;
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
    let reg = crate::params::registry()?;
    let t = reg.table(table)?;
    let old = t.write(row, field, &value)?;
    crate::info!("overlay: {}[{row}].{field} = {value} (was {old})", t.name);
    Ok(())
}
