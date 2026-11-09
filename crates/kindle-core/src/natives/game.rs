//! Natives for kindle.game: the catalogue's entries (catalogue.rs), by name.

use super::prelude::*;
use crate::catalogue::{self, Value};

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "game_catalogue",
        lua.create_function(|lua, ()| {
            let out = lua.create_table()?;
            let hooks = lua.create_table()?;
            for h in catalogue::current().hooks {
                let t = lua.create_table()?;
                t.set("doc", h.doc)?;
                t.set("answers", h.choices.to_vec())?;
                t.set("answer", h.choices[h.switch()])?;
                hooks.set(h.name, t)?;
            }
            out.set("hooks", hooks)?;
            let fields = lua.create_table()?;
            for f in catalogue::current().fields {
                let t = lua.create_table()?;
                t.set("doc", f.doc)?;
                t.set("kind", f.kind.describe())?;
                t.set("character", f.per_character)?;
                t.set("writable", f.set.is_some())?;
                fields.set(f.name, t)?;
            }
            out.set("fields", fields)?;
            let functions = lua.create_table()?;
            for f in catalogue::current().functions {
                let t = lua.create_table()?;
                t.set("doc", f.doc)?;
                t.set("args", f.args.iter().map(|(n, k)| format!("{n}: {}", k.describe())).collect::<Vec<_>>())?;
                functions.set(f.name, t)?;
            }
            out.set("functions", functions)?;
            let events = lua.create_table()?;
            for e in catalogue::current().events {
                let t = lua.create_table()?;
                t.set("doc", e.doc)?;
                t.set("args", e.args.to_vec())?;
                events.set(e.name, t)?;
            }
            out.set("events", events)?;
            Ok(out)
        })?,
    )?;
    n.set(
        "game_decide",
        lua.create_function(|lua, (name, answer): (String, String)| {
            let hook = catalogue::current().hook(&name).map_err(rt_err)?;
            let owner = ctx(lua)?.id.clone();
            catalogue::set_switch(hook, &owner, &answer).map_err(rt_err)
        })?,
    )?;
    n.set(
        "game_on_decide",
        lua.create_function(|lua, (name, f): (String, Function)| {
            let hook = catalogue::current().hook(&name).map_err(rt_err)?;
            catalogue::add_callback(hook).map_err(rt_err)?;
            let mut c = ctx(lua)?;
            let h = c.handlers.handle();
            c.handlers.decides.push((h, hook.name.to_string(), f));
            Ok(h)
        })?,
    )?;
    n.set(
        "game_listen",
        lua.create_function(|_, name: String| {
            let event = catalogue::current().event(&name).map_err(rt_err)?;
            event.listen().map_err(rt_err)
        })?,
    )?;
    n.set(
        "game_get",
        lua.create_function(|_, (name, handle): (String, Option<i64>)| {
            let field = catalogue::current().field(&name).map_err(rt_err)?;
            let chr = character(handle)?;
            catalogue::get(field, chr).map(to_lua).map_err(rt_err)
        })?,
    )?;
    n.set(
        "game_set",
        lua.create_function(|_, (name, value, handle): (String, LuaValue, Option<i64>)| {
            let field = catalogue::current().field(&name).map_err(rt_err)?;
            let chr = character(handle)?;
            catalogue::set(field, chr, &from_lua(&value)?).map_err(rt_err)
        })?,
    )?;
    n.set(
        "game_call",
        lua.create_function(|_, (name, args): (String, MultiValue)| {
            let f = catalogue::current().function(&name).map_err(rt_err)?;
            let args: Vec<Value> = args.iter().map(from_lua).collect::<mlua::Result<_>>()?;
            Ok(catalogue::call(f, &args).map_err(rt_err)?.map(to_lua).unwrap_or(LuaValue::Nil))
        })?,
    )?;
    Ok(())
}

/// A character's address from its handle, looked up in the game's lists now.
fn character(handle: Option<i64>) -> mlua::Result<Option<usize>> {
    let Some(h) = handle else { return Ok(None) };
    let game = crate::bindings::get().ok_or_else(|| rt_err("no game is bound"))?;
    (game.character)(h).map(Some).map_err(rt_err)
}

pub fn to_lua(v: Value) -> LuaValue {
    match v {
        Value::Bool(b) => LuaValue::Boolean(b),
        Value::Int(i) => LuaValue::Integer(i),
        Value::Float(f) => LuaValue::Number(f),
    }
}

fn from_lua(v: &LuaValue) -> mlua::Result<Value> {
    Ok(match v {
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(i) => Value::Int(*i),
        LuaValue::Number(f) => Value::Float(*f),
        other => return Err(rt_err(format!("expected a boolean or a number, got {}", other.type_name()))),
    })
}
