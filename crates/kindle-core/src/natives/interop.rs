//! Natives for kindle.export, kindle.mods, messages between mods.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set("export", lua.create_function(|lua, t: Table| lua.set_named_registry_value("kindle_exports", t))?)?;
    n.set(
        "mod_call",
        lua.create_function(|lua, (target, name, args): (String, String, MultiValue)| {
            let me = ctx(lua)?.id.clone();
            if target == me {
                return Err(rt_err("a mod can't call its own exports through kindle.mods"));
            }
            let other = live(&target).ok_or_else(|| rt_err(format!("mod {target:?} isn't running")))?;
            let exports: Option<Table> = other.named_registry_value("kindle_exports")?;
            let exports = exports.ok_or_else(|| rt_err(format!("mod {target:?} exports nothing (see kindle.export)")))?;
            let f: Option<Function> = exports.get(name.as_str())?;
            let f = f.ok_or_else(|| rt_err(format!("mod {target:?} exports no function {name:?}")))?;
            let copied = args.iter().map(|v| copy_value(v, &other, 0)).collect::<mlua::Result<Vec<_>>>()?;
            // The callee runs under the caller's time limit.
            let deadline = lua.app_data_ref::<Deadline>().and_then(|d| d.0.get());
            let previous = other.app_data_ref::<Deadline>().map(|d| d.0.replace(deadline));
            let result = f.call::<MultiValue>(MultiValue::from_vec(copied));
            if let Some(d) = other.app_data_ref::<Deadline>() {
                d.0.set(previous.flatten());
            }
            let result = result.map_err(|e| rt_err(format!("{target}.{name}: {}", clean_error(&e))))?;
            let back = result.iter().map(|v| copy_value(v, lua, 0)).collect::<mlua::Result<Vec<_>>>()?;
            Ok(MultiValue::from_vec(back))
        })?,
    )?;
    n.set(
        "mod_exports",
        lua.create_function(|_, target: String| {
            let Some(other) = live(&target) else { return Ok(None) };
            let exports: Option<Table> = other.named_registry_value("kindle_exports")?;
            Ok(Some(match exports {
                Some(t) => t.pairs::<String, LuaValue>().filter_map(|p| p.ok()).filter(|(_, v)| v.is_function()).map(|(k, _)| k).collect(),
                None => Vec::<String>::new(),
            }))
        })?,
    )?;
    n.set("mods_running", lua.create_function(|_, ()| Ok(live_ids()))?)?;
    n.set(
        "send",
        lua.create_function(|lua, (name, data): (String, LuaValue)| {
            let data = crate::store::to_json(&data).map_err(|e| rt_err(format!("kindle.send {name:?}: {e}")))?;
            let from = ctx(lua)?.id.clone();
            MESSAGES.lock().unwrap_or_else(|e| e.into_inner()).push(Message { from, name, data });
            Ok(())
        })?,
    )?;
    Ok(())
}
