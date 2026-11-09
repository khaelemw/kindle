//! Natives for kindle.debug: memory inspection for reverse engineering.

use super::prelude::*;

/// Raw access (memory reads, code traces, files anywhere): only for the console and the mods the
/// player names in kindle.toml [developer] (config.rs DeveloperConfig).
fn raw(lua: &Lua, what: &str) -> mlua::Result<()> {
    if ctx(lua)?.raw { Ok(()) } else { Err(rt_err(format!("debug.{what}: raw access only (kindle.toml [developer])"))) }
}

/// Where debug.export writes: a plain file name in `<kindle>/exports/<mod id>/`; with raw access,
/// any path.
fn export_path(lua: &Lua, game_dir: &std::path::Path, dest: &str) -> mlua::Result<std::path::PathBuf> {
    let c = ctx(lua)?;
    if c.raw {
        return Ok(dest.into());
    }
    let plain = !dest.is_empty() && dest != "." && dest != ".." && !dest.contains(['/', '\\', ':']);
    if !plain {
        return Err(rt_err(format!("debug.export: {dest:?} isn't a plain file name (it's written in kindle/exports/{}/)", c.id)));
    }
    let dir = game_dir.join("kindle").join("exports").join(&c.id);
    std::fs::create_dir_all(&dir).map_err(|e| rt_err(format!("{}: {e}", dir.display())))?;
    Ok(dir.join(dest))
}

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "debug_watch",
        lua.create_function(|lua, (addr, len, max): (u64, Option<u64>, Option<u64>)| {
            raw(lua, "watch")?;
            let owner = ctx(lua)?.id.clone();
            crate::watch::start(&owner, addr as usize, len.unwrap_or(1) as usize, max.unwrap_or(16) as usize).map_err(rt_err)
        })?,
    )?;
    n.set(
        "debug_press",
        lua.create_function(|_, (key, seconds): (mlua::Value, Option<f64>)| {
            let key = match key {
                mlua::Value::Integer(i) => i.to_string(),
                mlua::Value::String(s) => s.to_str()?.to_string(),
                _ => return Err(rt_err("debug.press: key must be a name (\"enter\") or a key code")),
            };
            crate::input_block::press(&key, seconds.unwrap_or(0.1)).map_err(rt_err)
        })?,
    )?;
    n.set(
        "debug_profile",
        lua.create_function(|lua, ()| {
            let out = lua.create_table()?;
            for m in crate::ui_bridge::mods() {
                let row = lua.create_table()?;
                row.set("total", m.cpu_ms)?;
                for (what, ms) in m.cpu_by_handler {
                    row.set(what, ms)?;
                }
                out.set(m.id, row)?;
            }
            Ok(out)
        })?,
    )?;
    n.set(
        "debug_class_name",
        lua.create_function(|lua, addr: u64| {
            raw(lua, "class_name")?;
            Ok(memory::class_name(addr as usize))
        })?,
    )?;
    n.set(
        "debug_input_stats",
        lua.create_function(|_, ()| Ok(crate::input_block::STATS.iter().map(|s| s.load(std::sync::atomic::Ordering::Relaxed) as i64).collect::<Vec<_>>()))?,
    )?;
    n.set(
        "debug_trace_next",
        lua.create_function(|lua, (rva, max): (i64, Option<i64>)| {
            raw(lua, "trace_next")?;
            crate::trace::trace_next(rva as usize, max.unwrap_or(2_000_000) as usize).map_err(rt_err)
        })?,
    )?;
    n.set(
        "debug_watch_stop",
        lua.create_function(|lua, ()| {
            raw(lua, "watch_stop")?;
            crate::watch::stop();
            Ok(())
        })?,
    )?;
    n.set(
        "debug_watch_hits",
        lua.create_function(|lua, ()| {
            raw(lua, "watch_hits")?;
            let t = lua.create_table()?;
            for (rip, addr, returns) in crate::watch::hits() {
                let h = lua.create_table()?;
                h.set("rip", rip)?;
                h.set("address", addr)?;
                h.set("returns", returns)?;
                t.push(h)?;
            }
            Ok(t)
        })?,
    )?;
    n.set("debug_trace_events", lua.create_function(|_, on: bool| crate::emevd::set_trace(on).map_err(rt_err))?)?;
    n.set("debug_trace_predicates", lua.create_function(|_, on: bool| crate::predicate_trace::set(on).map_err(rt_err))?)?;
    n.set(
        "debug_predicates",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            for (name, caller, ty, calls, yes) in crate::predicate_trace::counts() {
                let row = lua.create_table()?;
                row.set("predicate", name)?;
                row.set("caller", caller)?;
                row.set("type", (ty >= 0).then_some(ty))?;
                row.set("calls", calls)?;
                row.set("yes", yes)?;
                t.push(row)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "debug_trace_hks",
        lua.create_function(|_, on: bool| {
            crate::hks::ensure_hooks().map_err(rt_err)?;
            crate::hks::TRACE.store(on, Ordering::Relaxed);
            Ok(())
        })?,
    )?;
    n.set("debug_trace_talk", lua.create_function(|_, on: Option<bool>| crate::hooks::set_talk_trace(on.unwrap_or(true)).map_err(rt_err))?)?;
    n.set(
        "debug_census",
        lua.create_function(|lua, ()| {
            raw(lua, "census")?;
            use std::sync::atomic::Ordering::Relaxed;
            let out = lua.create_table()?;
            for c in crate::hook_sites::census() {
                let row = lua.create_table()?;
                row.set("area", c.area)?;
                row.set("name", c.name)?;
                let calls = c.calls.load(Relaxed);
                row.set("calls", calls)?;
                row.set("off_game_thread", c.off_game_thread.load(Relaxed))?;
                row.set("threads", c.threads.iter().map(|t| t.load(Relaxed)).filter(|&t| t != 0).count())?;
                row.set("per_second", c.seconds().filter(|&s| s > 0.0).map(|s| calls as f64 / s))?;
                out.push(row)?;
            }
            Ok(out)
        })?,
    )?;
    n.set(
        "debug_singletons",
        lua.create_function(|lua, ()| {
            raw(lua, "singletons")?;
            let t = lua.create_table()?;
            for (name, addr) in crate::ds3::singletons() {
                t.set(name, addr as i64)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "debug_singleton",
        lua.create_function(|lua, name: String| {
            raw(lua, "singleton")?;
            Ok(crate::ds3::singleton_instance(&name).map(|a| a as i64))
        })?,
    )?;
    n.set(
        "debug_module",
        lua.create_function(|lua, ()| {
            raw(lua, "module")?;
            Ok(memory::game_module().map(|(b, s)| (b as i64, s as i64)).unwrap_or((0, 0)))
        })?,
    )?;
    n.set(
        "debug_read",
        lua.create_function(|lua, (addr, ty): (i64, Option<String>)| {
            raw(lua, "read")?;
            let a = addr as usize;
            let v = match ty.as_deref().unwrap_or("u64") {
                "u8" => memory::read::<u8>(a).map(|v| LuaValue::Integer(v as i64)),
                "i8" | "s8" => memory::read::<i8>(a).map(|v| LuaValue::Integer(v as i64)),
                "u16" => memory::read::<u16>(a).map(|v| LuaValue::Integer(v as i64)),
                "i16" | "s16" => memory::read::<i16>(a).map(|v| LuaValue::Integer(v as i64)),
                "u32" => memory::read::<u32>(a).map(|v| LuaValue::Integer(v as i64)),
                "i32" | "s32" => memory::read::<i32>(a).map(|v| LuaValue::Integer(v as i64)),
                "u64" | "ptr" | "i64" => memory::read::<u64>(a).map(|v| LuaValue::Integer(v as i64)),
                "f32" => memory::read::<f32>(a).map(|v| LuaValue::Number(v as f64)),
                "f64" => memory::read::<f64>(a).map(LuaValue::Number),
                other => return Err(rt_err(format!("unknown type {other:?}"))),
            };
            let _ = lua;
            Ok(v.unwrap_or(LuaValue::Nil))
        })?,
    )?;
    n.set(
        "debug_export",
        lua.create_function(|lua, (path, dest): (String, String)| {
            let exe = std::env::current_exe().map_err(|e| rt_err(e.to_string()))?;
            let game_dir = exe.parent().ok_or_else(|| rt_err("the game's folder isn't known"))?;
            let dest = export_path(lua, game_dir, &dest)?;
            let (archive, data) = crate::archive::read_any(game_dir, &path).map_err(rt_err)?;
            std::fs::write(&dest, &data).map_err(|e| rt_err(format!("{}: {e}", dest.display())))?;
            Ok(format!("{archive}: {} bytes", data.len()))
        })?,
    )?;
    n.set(
        "debug_bytes",
        lua.create_function(|lua, (addr, len): (i64, usize)| {
            raw(lua, "bytes")?;
            let len = len.min(4096);
            Ok(memory::read_bytes(addr as usize, len).map(|b| {
                b.chunks(16)
                    .enumerate()
                    .map(|(i, c)| {
                        let hex: Vec<String> = c.iter().map(|x| format!("{x:02x}")).collect();
                        let txt: String = c.iter().map(|&x| if (0x20..0x7f).contains(&x) { x as char } else { '.' }).collect();
                        format!("{:#014x}  {:<47}  {txt}", addr as usize + i * 16, hex.join(" "))
                    })
                    .collect::<Vec<_>>()
                    .join(
                        "
",
                    )
            }))
        })?,
    )?;
    n.set(
        "debug_wstr",
        lua.create_function(|lua, (addr, max): (i64, Option<usize>)| {
            raw(lua, "wstr")?;
            Ok(memory::read_wstr(addr as usize, max.unwrap_or(256).min(4096)))
        })?,
    )?;
    Ok(())
}
