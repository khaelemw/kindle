//! Natives for kindle.store and kindle.settings.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "settings_get",
        lua.create_function(|lua, name: String| {
            let v = ctx(lua)?.setting(&name).map_err(rt_err)?;
            setting_to_lua(lua, &v)
        })?,
    )?;
    n.set(
        "settings_set",
        lua.create_function(|lua, (name, value): (String, LuaValue)| {
            let v = setting_from_lua(&value).ok_or_else(|| rt_err(format!("setting {name}: values are booleans, numbers or strings")))?;
            ctx(lua)?.set_setting(&name, v).map_err(rt_err)?;
            Ok(())
        })?,
    )?;
    n.set("settings_names", lua.create_function(|lua, ()| Ok(ctx(lua)?.settings.iter().map(|s| s.name.clone()).collect::<Vec<_>>()))?)?;
    n.set(
        "store_get",
        lua.create_function(|lua, (global, key): (bool, String)| {
            let value = ctx(lua)?.store(global).map_err(rt_err)?.get(&key).cloned();
            match value {
                Some(v) => crate::store::to_lua(lua, &v),
                None => Ok(LuaValue::Nil),
            }
        })?,
    )?;
    n.set(
        "store_set",
        lua.create_function(|lua, (global, key, value): (bool, String, LuaValue)| {
            let json = crate::store::to_json(&value).map_err(|e| rt_err(format!("store.set {key:?}: {e}")))?;
            ctx(lua)?.store(global).map_err(rt_err)?.set(&key, json);
            Ok(())
        })?,
    )?;
    n.set(
        "keep",
        lua.create_function(|lua, (key, default): (String, LuaValue)| {
            let id = ctx(lua)?.id.clone();
            let value = match crate::mods::kept(&id, &key) {
                Some(v) => crate::store::to_lua(lua, &v)?,
                None => default,
            };
            let handle = lua.create_registry_value(value.clone())?;
            let mut c = ctx(lua)?;
            c.keeps.retain(|(k, _)| *k != key);
            c.keeps.push((key, handle));
            Ok(value)
        })?,
    )?;
    n.set("store_keys", lua.create_function(|lua, global: bool| Ok(ctx(lua)?.store(global).map_err(rt_err)?.keys()))?)?;
    Ok(())
}
