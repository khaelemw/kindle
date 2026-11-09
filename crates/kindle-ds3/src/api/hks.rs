//! Natives for kindle.hks: the behaviour scripts.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "hks_on",
        lua.create_function(|lua, (act, id, f): (bool, mlua::Value, Function)| {
            let id = match id {
                mlua::Value::Integer(i) => crate::hks::HksId::Num(i as i32),
                mlua::Value::Number(n) => crate::hks::HksId::Num(n as i32),
                mlua::Value::String(s) => crate::hks::HksId::Name(s.to_str()?.to_string()),
                _ => return Err(rt_err("hks: the id must be a number or a name")),
            };
            crate::hks::ensure_hooks().map_err(rt_err)?;
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.hks.push((handle, act, id, f));
            Ok(handle)
        })?,
    )?;
    n.set(
        "hks_names",
        lua.create_function(|lua, ()| {
            let make = |table: &[(&str, crate::hks::KnownId, bool, &str)]| -> mlua::Result<Table> {
                let t = lua.create_table()?;
                for (name, id, _, _) in table {
                    match id {
                        crate::hks::KnownId::Num(n) => t.set(*name, *n)?,
                        crate::hks::KnownId::Name(s) => t.set(*name, *s)?,
                    }
                }
                Ok(t)
            };
            Ok((make(crate::hks::ENV_NAMES)?, make(crate::hks::ACT_NAMES)?))
        })?,
    )?;
    n.set(
        "hks_on_call",
        lua.create_function(|lua, (name, f): (String, Function)| {
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.hks_calls.push((handle, name, f));
            Ok(handle)
        })?,
    )?;
    n.set(
        "hks_eval",
        lua.create_function(|lua, (key, source, reply): (Option<i64>, String, Function)| {
            if crate::hks::precompiled(&source) {
                return Err(rt_err("hks.eval: precompiled chunks are refused (source text only)"));
            }
            let owner = hks_owner(key)?;
            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let frame = FRAME.load(Ordering::Relaxed);
            let kind = crate::hks::RequestKind::Eval(source);
            crate::hks::request(crate::hks::Request { id, owner, kind, args: Vec::new(), frame }).map_err(rt_err)?;
            ctx(lua)?.handlers.replies.push((id, reply));
            Ok(id)
        })?,
    )?;
    n.set(
        "hks_request",
        lua.create_function(|lua, (key, act, values, reply): (Option<i64>, bool, Table, Option<Function>)| {
            let owner = hks_owner(key)?;
            let mut args = Vec::new();
            for v in values.sequence_values::<mlua::Value>() {
                args.push(match v? {
                    mlua::Value::Integer(i) => crate::hks::HksValue::Number(i as f32),
                    mlua::Value::Number(n) => crate::hks::HksValue::Number(n as f32),
                    mlua::Value::Boolean(b) => crate::hks::HksValue::Bool(b),
                    mlua::Value::String(s) => crate::hks::HksValue::Str(s.to_str()?.to_string()),
                    _ => return Err(rt_err("hks: arguments must be numbers, booleans or strings")),
                });
            }
            let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
            let frame = FRAME.load(Ordering::Relaxed);
            let kind = if act { crate::hks::RequestKind::Act } else { crate::hks::RequestKind::Env };
            crate::hks::request(crate::hks::Request { id, owner, kind, args, frame }).map_err(rt_err)?;
            if let Some(f) = reply {
                ctx(lua)?.handlers.replies.push((id, f));
            }
            Ok(id)
        })?,
    )?;
    Ok(())
}
