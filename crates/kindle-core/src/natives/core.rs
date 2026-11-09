//! Natives for kindle core: logging, events, timers, time.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set("version", env!("CARGO_PKG_VERSION"))?;
    n.set("api", crate::config::API_VERSION)?;
    n.set(
        "log",
        lua.create_function(|lua, (level, msg): (String, String)| {
            let id = ctx(lua)?.id.clone();
            log::write(Level::parse(&level).unwrap_or(Level::Info), &id, &msg);
            Ok(())
        })?,
    )?;
    n.set("inspect", lua.create_function(|_, (v, depth): (LuaValue, Option<usize>)| Ok(inspect(&v, depth.unwrap_or(3).clamp(1, 10))))?)?;
    n.set(
        "print",
        lua.create_function(|lua, msg: String| {
            let mut c = ctx(lua)?;
            match c.capture.as_mut() {
                Some(buf) => buf.push(msg),
                None => {
                    let id = c.id.clone();
                    drop(c);
                    log::write(Level::Info, &id, &msg);
                }
            }
            Ok(())
        })?,
    )?;
    n.set(
        "on",
        lua.create_function(|lua, (event, f): (String, Function)| {
            if !crate::events::is_known(&event) {
                let all: Vec<&str> = crate::events::all().collect();
                return Err(rt_err(format!("unknown event {event:?} (events: {})", all.join(", "))));
            }
            let mut c = ctx(lua)?;
            let h = c.handlers.handle();
            c.handlers.events.push((h, event, f));
            Ok(h)
        })?,
    )?;
    n.set(
        "timer",
        lua.create_function(|lua, (seconds, repeat, f): (f64, bool, Function)| {
            if !seconds.is_finite() || seconds < 0.0 || (repeat && seconds < 0.001) {
                return Err(rt_err("timer interval must be a positive number of seconds"));
            }
            let d = Duration::from_secs_f64(seconds);
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.timers.push(Timer { handle, due: Instant::now() + d, every: repeat.then_some(d), func: f });
            Ok(handle)
        })?,
    )?;
    n.set("cancel", lua.create_function(|lua, h: u64| Ok(ctx(lua)?.handlers.cancel(h)))?)?;
    n.set("time", lua.create_function(|_, ()| Ok(START.elapsed().as_secs_f64()))?)?;
    n.set("frame", lua.create_function(|_, ()| Ok(FRAME.load(Ordering::Relaxed)))?)?;
    n.set("clock", lua.create_function(|_, ()| Ok(crate::clock::now()))?)?;
    n.set(
        "date",
        lua.create_function(|lua, (seconds, utc): (Option<f64>, Option<bool>)| {
            let seconds = seconds.unwrap_or_else(crate::clock::now);
            if !seconds.is_finite() {
                return Err(rt_err("kindle.date: seconds must be a number"));
            }
            let offset = if utc == Some(true) { 0 } else { crate::clock::local_offset() };
            let d = crate::clock::date(seconds, offset);
            let t = lua.create_table()?;
            for (k, v) in [
                ("year", d.year),
                ("month", d.month),
                ("day", d.day),
                ("hour", d.hour),
                ("minute", d.minute),
                ("second", d.second),
                ("weekday", d.weekday),
                ("yearday", d.yearday),
            ] {
                t.set(k, v)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "json_encode",
        lua.create_function(|_, (v, pretty): (LuaValue, Option<bool>)| {
            crate::json::encode(&v, pretty.unwrap_or(false)).map_err(|e| rt_err(format!("kindle.json.encode: {e}")))
        })?,
    )?;
    n.set("json_decode", lua.create_function(|lua, text: String| crate::json::decode(lua, &text))?)?;
    n.set(
        "read_data",
        lua.create_function(|lua, path: String| {
            let full = crate::mods::mod_file(&ctx(lua)?.dir, &path).map_err(|e| rt_err(format!("kindle.read_data: {e}")))?;
            let len = std::fs::metadata(&full).map_err(|e| rt_err(format!("kindle.read_data {path:?}: {e}")))?.len();
            if len > 16 << 20 {
                return Err(rt_err(format!("kindle.read_data {path:?}: {len} bytes, more than 16 MB")));
            }
            let bytes = std::fs::read(&full).map_err(|e| rt_err(format!("kindle.read_data {path:?}: {e}")))?;
            lua.create_string(&bytes)
        })?,
    )?;
    n.set(
        "read_mod_file",
        lua.create_function(|lua, name: String| {
            let valid = !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
                && !name.contains("..")
                && !name.starts_with('/');
            if !valid {
                return Err(rt_err(format!("require: invalid module name {name:?}")));
            }
            let rel = format!("{}.lua", name.replace('.', "/"));
            let path = ctx(lua)?.dir.join(&rel);
            let src = std::fs::read_to_string(&path).map_err(|e| rt_err(format!("require {name:?}: {rel}: {e}")))?;
            Ok((src, format!("@{rel}")))
        })?,
    )?;
    n.set(
        "on_message",
        lua.create_function(|lua, (name, f): (String, Function)| {
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.messages.push((handle, name, f));
            Ok(handle)
        })?,
    )?;
    Ok(())
}
