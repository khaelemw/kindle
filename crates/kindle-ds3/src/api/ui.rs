//! Natives for kindle.ui.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "ui_set_hud",
        lua.create_function(|lua, value: u8| {
            let current = game::hud().map_err(rt_err)?;
            let mut c = ctx(lua)?;
            c.hud_original.get_or_insert(current);
            game::set_hud(value).map_err(rt_err)
        })?,
    )?;
    n.set("ui_hud", lua.create_function(|_, ()| game::hud().map_err(rt_err))?)?;
    n.set(
        "ui_screen_open",
        lua.create_function(|lua, (spec, dispatch): (Table, Function)| {
            let mut c = ctx(lua)?;
            let dir = c.dir.clone();
            let screen = kindle_core::screens::from_lua(&c.id, &spec, &|p| layer_image(&dir, p)).map_err(|e| rt_err(format!("ui.screen: {e}")))?;
            let id = kindle_core::screens::open(screen);
            c.handlers.screens.push((id, dispatch));
            Ok(id)
        })?,
    )?;
    n.set(
        "ui_screen_update",
        lua.create_function(|lua, (id, spec): (u64, Table)| {
            let c = ctx(lua)?;
            let dir = c.dir.clone();
            let screen = kindle_core::screens::from_lua(&c.id, &spec, &|p| layer_image(&dir, p)).map_err(|e| rt_err(format!("screen:update: {e}")))?;
            kindle_core::screens::update(&c.id, id, screen).map_err(|e| rt_err(format!("screen:update: {e}")))
        })?,
    )?;
    n.set(
        "ui_screen_close",
        lua.create_function(|lua, id: u64| {
            let mut c = ctx(lua)?;
            let closed = kindle_core::screens::close(&c.id, id);
            c.handlers.screens.retain(|(open, _)| kindle_core::screens::is_open(*open));
            Ok(closed)
        })?,
    )?;
    n.set("ui_screen_is_open", lua.create_function(|_, id: u64| Ok(kindle_core::screens::is_open(id)))?)?;
    n.set(
        "ui_layer_add",
        lua.create_function(|lua, elements: Table| {
            let c = ctx(lua)?;
            let dir = c.dir.clone();
            let elements = kindle_core::hud::from_lua(&elements, &|p| layer_image(&dir, p)).map_err(|e| rt_err(format!("ui.layer: {e}")))?;
            Ok(kindle_core::hud::add(&c.id, elements))
        })?,
    )?;
    n.set(
        "ui_layer_set",
        lua.create_function(|lua, (id, elements): (u64, Table)| {
            let c = ctx(lua)?;
            let dir = c.dir.clone();
            let elements = kindle_core::hud::from_lua(&elements, &|p| layer_image(&dir, p)).map_err(|e| rt_err(format!("layer:set: {e}")))?;
            kindle_core::hud::set(&c.id, id, elements).map_err(|e| rt_err(format!("layer:set: {e}")))
        })?,
    )?;
    n.set(
        "ui_layer_show",
        lua.create_function(|lua, (id, on): (u64, bool)| {
            let c = ctx(lua)?;
            kindle_core::hud::show(&c.id, id, on).map_err(|e| rt_err(format!("layer:show: {e}")))
        })?,
    )?;
    n.set(
        "ui_layer_remove",
        lua.create_function(|lua, id: u64| {
            let c = ctx(lua)?;
            Ok(kindle_core::hud::remove(&c.id, id))
        })?,
    )?;
    n.set(
        "ui_mod_tab",
        lua.create_function(|lua, (title, buttons): (String, Vec<Table>)| {
            let mut specs = Vec::new();
            let mut fns = Vec::new();
            for b in &buttons {
                let label: String = b.get("label")?;
                let help: Option<String> = b.get("help")?;
                let paste: Option<bool> = b.get("paste")?;
                let info: Option<bool> = b.get("info")?;
                let info = info.unwrap_or(false);
                if !info {
                    let on_press: Function = b.get("on_press").map_err(|_| rt_err("ui.mod_tab: each button needs on_press (or info = true)"))?;
                    fns.push((label.clone(), on_press));
                }
                specs.push(crate::native_mods::ModTabButton { label, help: help.unwrap_or_default(), pastes: paste.unwrap_or(false), info });
            }
            let mut c = ctx(lua)?;
            crate::native_mods::set_mod_tab(&c.id, &title, specs).map_err(|e| rt_err(format!("ui.mod_tab: {e}")))?;
            c.handlers.tab = fns;
            Ok(())
        })?,
    )?;
    n.set(
        "ui_title_entry",
        lua.create_function(|lua, (label, help, on_select): (Option<String>, Option<String>, Option<Function>)| {
            let mut c = ctx(lua)?;
            let owner = c.id.clone();
            match label {
                Some(label) => {
                    crate::front_menu::set_title_entry(&owner, &label, &help.unwrap_or_default(), on_select.is_some())
                        .map_err(|e| rt_err(format!("ui.title_entry: {e}")))?;
                    c.handlers.title = on_select;
                    Ok(())
                }
                None => {
                    crate::front_menu::remove_title_entry(&owner);
                    c.handlers.title = None;
                    Ok(())
                }
            }
        })?,
    )?;
    n.set(
        "ui_character_rows",
        lua.create_function(|lua, rows: Vec<Table>| {
            let rows = rows.iter().enumerate().map(|(i, r)| character_row(r).map_err(|e| format!("row {}: {e}", i + 1))).collect::<Result<Vec<_>, _>>();
            let owner = ctx(lua)?.id.clone();
            rows.and_then(|rows| crate::char_make::set_rows(&owner, rows)).map_err(|e| rt_err(format!("ui.character_rows: {e}")))
        })?,
    )?;
    n.set(
        "ui_character_choices",
        lua.create_function(|lua, ()| {
            let owner = ctx(lua)?.id.clone();
            let t = lua.create_table()?;
            for (key, choice) in crate::char_make::choices(&owner) {
                match choice {
                    crate::char_make::Choice::Switch(on) => t.set(key, on)?,
                    crate::char_make::Choice::Text(text) => t.set(key, text)?,
                }
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "ui_reset_character_choices",
        lua.create_function(|lua, ()| {
            let owner = ctx(lua)?.id.clone();
            crate::char_make::reset_choices(&owner);
            Ok(())
        })?,
    )?;
    n.set(
        "ui_copy",
        lua.create_function(|_, text: String| {
            if text.chars().count() > 1024 {
                return Err(rt_err("ui.copy: 1024 characters at most"));
            }
            Ok(crate::clipboard::set_text(&text))
        })?,
    )?;
    n.set("ui_message", lua.create_function(|_, text: String| crate::native_ui::message(&text).map_err(rt_err))?)?;
    n.set(
        "ui_add_menu_option",
        lua.create_function(|lua, (text, filter, on_select): (String, mlua::Value, Function)| {
            static NEXT_TEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(69_000_000);
            let filter = match filter {
                mlua::Value::Nil => Ok("any".to_string()),
                mlua::Value::String(s) => {
                    let s = s.to_str()?.to_string();
                    if !["any", "bonfire", "npc"].contains(&s.as_str()) {
                        return Err(rt_err(format!("unknown menu {s:?} (any, bonfire, npc, or a function)")));
                    }
                    Ok(s)
                }
                mlua::Value::Function(f) => Err(f),
                _ => return Err(rt_err("where must be \"any\", \"bonfire\", \"npc\" or a function")),
            };
            let text_id = NEXT_TEXT.fetch_add(1, Ordering::Relaxed);
            let owner = ctx(lua)?.id.clone();
            // Menu text lives in the event-text FMG (30).
            text_override::set_or_add(&[30], text_id, &text, &owner).map_err(rt_err)?;
            crate::hooks::ensure_talk_hooks().map_err(rt_err)?;
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.menu_options.push(MenuOption { handle, text_id, filter, on_select });
            Ok(handle)
        })?,
    )?;
    n.set(
        "ui_set_menu_option_text",
        lua.create_function(|lua, (handle, text): (u64, String)| {
            let c = ctx(lua)?;
            let text_id = c.handlers.menu_options.iter().find(|o| o.handle == handle).map(|o| o.text_id);
            let owner = c.id.clone();
            drop(c);
            let text_id = text_id.ok_or_else(|| rt_err("ui.set_menu_option_text: no such menu option"))?;
            text_override::set_or_add(&[30], text_id, &text, &owner).map_err(rt_err)?;
            Ok(())
        })?,
    )?;
    n.set(
        "ui_menu",
        lua.create_function(|lua, entries: Table| {
            let mut texts = Vec::new();
            let mut fns = Vec::new();
            for e in entries.sequence_values::<Table>() {
                let e = e?;
                texts.push(e.get::<String>("text")?);
                fns.push(e.get::<Option<Function>>("on_select")?);
            }
            if texts.is_empty() {
                return Err(rt_err("ui.menu: needs at least one entry"));
            }
            if texts.len() > crate::talk_menu::POOL_SIZE {
                return Err(rt_err(format!("ui.menu: at most {} entries", crate::talk_menu::POOL_SIZE)));
            }
            let mut c = ctx(lua)?;
            let owner = c.id.clone();
            let handle = c.handlers.handle();
            c.handlers.menus.push((handle, fns));
            let excess = c.handlers.menus.len().saturating_sub(8);
            c.handlers.menus.drain(..excess);
            drop(c);
            crate::talk_menu::request(crate::talk_menu::Request::Menu(owner, handle, texts)).map_err(rt_err)
        })?,
    )?;
    n.set(
        "ui_shop",
        lua.create_function(|_, (first, last): (i32, i32)| {
            if first > last {
                return Err(rt_err("ui.shop: first row must not be after last"));
            }
            crate::talk_menu::request(crate::talk_menu::Request::Shop(first, last)).map_err(rt_err)
        })?,
    )?;
    n.set(
        "ui_open",
        lua.create_function(|_, name: String| {
            let screen = crate::talk_menu::Screen::from_name(&name).ok_or_else(|| {
                let names: Vec<&str> = crate::talk_menu::Screen::NAMES.iter().map(|(n, _)| *n).collect();
                rt_err(format!("ui.open: no screen {name:?} (one of {})", names.join(", ")))
            })?;
            crate::talk_menu::request(crate::talk_menu::Request::Screen(screen)).map_err(rt_err)
        })?,
    )?;
    n.set("ui_game_banner", lua.create_function(|_, kind: LuaValue| crate::emevd::banner(banner_kind(kind)?).map_err(rt_err))?)?;
    n.set(
        "ui_hide_game_banner",
        lua.create_function(|_, (kind, seconds): (LuaValue, f64)| {
            let d = Duration::from_secs_f64(if seconds.is_finite() { seconds.clamp(0.0, 600.0) } else { 0.0 });
            crate::emevd::hide_banner_for(banner_kind(kind)?, d).map_err(rt_err)
        })?,
    )?;
    n.set(
        "ui_place_banner",
        lua.create_function(|lua, text: String| {
            let id = ctx(lua)?.id.clone();
            crate::native_ui::place_banner(&id, &text).map_err(rt_err)
        })?,
    )?;
    n.set(
        "ui_prompt",
        lua.create_function(|lua, text: Option<String>| {
            ui_bridge::set_prompt(&ctx(lua)?.id, text);
            Ok(())
        })?,
    )?;
    n.set(
        "ui_status",
        lua.create_function(|lua, (text, progress): (Option<String>, Option<f32>)| {
            let status = text.map(|t| (t, progress.filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0))));
            ui_bridge::set_status(&ctx(lua)?.id, status);
            Ok(())
        })?,
    )?;
    n.set(
        "ui_notice",
        lua.create_function(|lua, (kind, msg, seconds, color): (String, String, f32, String)| {
            let kind = if kind == "banner" { NoticeKind::Banner } else { NoticeKind::Toast };
            let seconds = if seconds.is_finite() { seconds.clamp(0.5, 60.0) } else { 3.0 };
            let color = parse_color(&color)?;
            if !ui_bridge::RENDERER_ACTIVE.load(Ordering::Relaxed) {
                // Overlay disabled: at least put it in the log.
                let id = ctx(lua)?.id.clone();
                log::write(Level::Info, &id, &format!("[{}] {msg}", if kind == NoticeKind::Banner { "banner" } else { "toast" }));
            }
            ui_bridge::push_notice(kind, msg, color, seconds);
            Ok(())
        })?,
    )?;
    Ok(())
}

/// One row of ui.character_rows.
fn character_row(t: &Table) -> Result<crate::char_make::ModRow, String> {
    use crate::char_make::{ModRow, PageTab, RowKind, Switch};
    let text = |t: &Table, k: &str| t.get::<Option<String>>(k).map_err(|_| format!("{k} must be text")).map(Option::unwrap_or_default);
    let label = text(t, "label")?;
    let help = text(t, "help")?;
    let kind = if let Some(page) = t.get::<Option<Vec<Table>>>("page").map_err(|_| "page must be a list of tabs".to_string())? {
        let mut tabs = Vec::new();
        for tab in &page {
            let icon = match tab.get::<Option<String>>("icon").map_err(|_| "a tab's icon must be text".to_string())? {
                None => None,
                Some(name) => Some(
                    crate::menu_movies::TAB_ICON_NAMES
                        .iter()
                        .position(|n| *n == name)
                        .ok_or_else(|| format!("no icon {name:?} ({})", crate::menu_movies::TAB_ICON_NAMES.join(", ")))?,
                ),
            };
            let mut switches = Vec::new();
            for w in tab.get::<Vec<Table>>("switches").map_err(|_| "a tab needs switches = {...}".to_string())? {
                let default = w.get::<Option<bool>>("default").map_err(|_| "a switch's default must be true or false".to_string())?.unwrap_or(false);
                switches.push(Switch { key: text(&w, "key")?, label: text(&w, "label")?, help: text(&w, "help")?, default });
            }
            tabs.push(PageTab { title: text(tab, "title")?, icon, switches });
        }
        RowKind::Page(tabs)
    } else if let Some(box_) = t.get::<Option<Table>>("text").map_err(|_| "text must be {key =, empty =}".to_string())? {
        RowKind::Text { key: text(&box_, "key")?, empty: text(&box_, "empty")? }
    } else {
        return Err("a row needs page = {...} or text = {...}".into());
    };
    Ok(ModRow { owner: String::new(), label, help, kind })
}

/// A banner type of the game's, by name ("bonfire_lit") or number.
fn banner_kind(kind: LuaValue) -> mlua::Result<i32> {
    Ok(match kind {
        LuaValue::Integer(i) => i as i32,
        LuaValue::Number(n) => n as i32,
        LuaValue::String(s) => {
            let name = s.to_str()?.to_lowercase().replace([' ', '-'], "_");
            crate::emevd::BANNER_NAMES.iter().find(|(n, _)| *n == name).map(|(_, k)| *k).ok_or_else(|| {
                let names: Vec<&str> = crate::emevd::BANNER_NAMES.iter().map(|(n, _)| *n).collect();
                rt_err(format!("unknown banner {name:?} (banners: {})", names.join(", ")))
            })?
        }
        _ => return Err(rt_err("give a banner name or type number")),
    })
}

/// A layer's or a screen row's picture: a PNG in the mod's folder, or a number: the game's icon
/// of that id (Lua gives a number as its digits).
fn layer_image(dir: &std::path::Path, path: &str) -> Result<std::sync::Arc<kindle_core::hud::Image>, String> {
    if let Ok(id) = path.parse::<u32>() {
        return crate::icons::icon(id);
    }
    let full = kindle_core::mods::mod_file(dir, path)?;
    kindle_core::hud::load_image(&full)
}
