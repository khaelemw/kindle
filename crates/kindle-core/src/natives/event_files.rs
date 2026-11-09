//! Natives for kindle.events' changes to the game's event files (event_patches.rs): reading a map's
//! events as the game has them and as it will run them, and each mod's changes, dropped when it
//! unloads (the game's side calls `event_patches::remove_owner`).

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    let op = |lua: &Lua, map: &str, event: i64, op: crate::event_patches::Op| -> mlua::Result<()> {
        let owner = ctx(lua)?.id.clone();
        crate::event_patches::add(&owner, map, event, op).map_err(rt_err)
    };
    let cmd = |bank: u32, index: u32, format: &str, values: &[f64]| -> mlua::Result<crate::event_patches::Command> {
        Ok(crate::event_patches::Command { bank, index, args: crate::emevd::pack(format, values).map_err(rt_err)? })
    };
    n.set(
        "events_script",
        lua.create_function(|lua, (map, event): (String, i64)| {
            crate::event_patches::script(&map, event).map_err(rt_err)?.map(|e| event_table(lua, &e)).transpose()
        })?,
    )?;
    n.set(
        "events_view",
        lua.create_function(|lua, (map, event): (String, i64)| {
            crate::event_patches::view(&map, event).map_err(rt_err)?.map(|e| event_table(lua, &e)).transpose()
        })?,
    )?;
    n.set(
        "events_find",
        lua.create_function(|lua, (map, bank, index): (String, u32, u32)| {
            let list = lua.create_table()?;
            for (event, at, c) in crate::event_patches::find(&map, bank, index).map_err(rt_err)? {
                let r = lua.create_table()?;
                r.set("event", event)?;
                r.set("command", at + 1)?;
                r.set("args", words(&c.args))?;
                r.set("bytes", c.args.clone())?;
                list.push(r)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "events_reset",
        lua.create_function(|lua, map: String| {
            let owner = ctx(lua)?.id.clone();
            Ok(crate::event_patches::remove_owner_map(&owner, &map))
        })?,
    )?;
    n.set("events_disable", lua.create_function(move |lua, (map, event): (String, i64)| op(lua, &map, event, crate::event_patches::Op::Disable))?)?;
    n.set(
        "events_remove",
        lua.create_function(move |lua, (map, event, at): (String, i64, usize)| op(lua, &map, event, crate::event_patches::Op::Remove(at)))?,
    )?;
    n.set(
        "events_insert",
        lua.create_function(move |lua, (map, event, at, bank, index, format, values): (String, i64, usize, u32, u32, String, Vec<f64>)| {
            op(lua, &map, event, crate::event_patches::Op::Insert(at, cmd(bank, index, &format, &values)?))
        })?,
    )?;
    n.set(
        "events_replace",
        lua.create_function(move |lua, (map, event, at, bank, index, format, values): (String, i64, usize, u32, u32, String, Vec<f64>)| {
            op(lua, &map, event, crate::event_patches::Op::Replace(at, cmd(bank, index, &format, &values)?))
        })?,
    )?;
    n.set(
        "events_add",
        lua.create_function(move |lua, (map, event, rest, commands): (String, i64, u32, Vec<Table>)| {
            let mut list = Vec::new();
            for c in commands {
                let values: Vec<f64> = c.get::<Option<Vec<f64>>>("values")?.unwrap_or_default();
                let format: String = c.get::<Option<String>>("format")?.unwrap_or_default();
                list.push(cmd(c.get("bank")?, c.get("index")?, &format, &values)?);
            }
            op(lua, &map, event, crate::event_patches::Op::Add { rest, commands: list })
        })?,
    )?;
    n.set(
        "events_start",
        lua.create_function(move |lua, (map, event, slot, format, values): (String, i64, i32, String, Vec<f64>)| {
            let args = crate::emevd::pack(&format, &values).map_err(rt_err)?;
            op(lua, &map, event, crate::event_patches::Op::Start { slot, args })
        })?,
    )?;
    n.set(
        "events_edit",
        lua.create_function(move |lua, (map, event, steps, why): (String, i64, Vec<Table>, Option<String>)| {
            let steps =
                steps.iter().enumerate().map(|(i, t)| step(t).map_err(|e| rt_err(format!("events.edit: step {}: {e}", i + 1)))).collect::<mlua::Result<_>>()?;
            op(lua, &map, event, crate::event_patches::Op::Edit { steps, why: why.unwrap_or_default() })
        })?,
    )?;
    Ok(())
}

/// A command's arguments as 32-bit words.
fn words(args: &[u8]) -> Vec<i64> {
    args.chunks(4)
        .map(|w| {
            let mut b = [0u8; 4];
            b[..w.len()].copy_from_slice(w);
            i32::from_le_bytes(b) as i64
        })
        .collect()
}

/// An event for Lua: {id, rest, commands = {{bank, index, args = {32-bit words}, bytes, layer}},
/// parameters = {{command, target, source, length}}} (commands numbered from 1).
fn event_table(lua: &Lua, e: &crate::evd::Event) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", e.id)?;
    t.set("rest", e.rest)?;
    let cmds = lua.create_table()?;
    for (i, c) in e.instructions.iter().enumerate() {
        let r = lua.create_table()?;
        r.set("bank", c.bank)?;
        r.set("index", c.index)?;
        r.set("args", words(&c.args))?;
        r.set("bytes", c.args.clone())?;
        r.set("layer", c.layer)?;
        cmds.set(i + 1, r)?;
    }
    t.set("commands", cmds)?;
    let params = lua.create_table()?;
    for p in &e.parameters {
        let r = lua.create_table()?;
        r.set("command", p.instruction + 1)?;
        r.set("target", p.target)?;
        r.set("source", p.source)?;
        r.set("length", p.length)?;
        params.push(r)?;
    }
    t.set("parameters", params)?;
    Ok(t)
}

/// One step of events.edit: {check|remove|replace|insert = n, is = {bank, index, bytes...}, and
/// for a new command bank, index, format, values, params = {{target, source, length}}, layer_of,
/// keep_params (a replacement keeping the replaced command's parameter entries)}.
fn step(t: &Table) -> Result<crate::event_patches::Step, String> {
    use crate::event_patches::{Action, Expect, New, NewParam, Step};
    let get = |k: &str| t.get::<Option<i64>>(k).map_err(|e| e.to_string());
    let place = |n: i64| usize::try_from(n - 1).map_err(|_| format!("command numbers start at 1 (got {n})"));
    let new = || -> Result<New, String> {
        let bank: u32 = t.get("bank").map_err(|_| "a new command needs bank and index")?;
        let index: u32 = t.get("index").map_err(|_| "a new command needs bank and index")?;
        let format: String = t.get::<Option<String>>("format").map_err(|e| e.to_string())?.unwrap_or_default();
        let values: Vec<f64> = t.get::<Option<Vec<f64>>>("values").map_err(|e| e.to_string())?.unwrap_or_default();
        let args = crate::emevd::pack(&format, &values)?;
        let mut params = Vec::new();
        for p in t.get::<Option<Vec<Table>>>("params").map_err(|e| e.to_string())?.unwrap_or_default() {
            let f = |k: &str| p.get::<i64>(k).map_err(|_| format!("a parameter needs target, source and length ({k})"));
            let (target, source, length) = (f("target")?, f("source")?, f("length")?);
            if target < 0 || source < 0 || length <= 0 || target + length > args.len() as i64 {
                return Err(format!("parameter {target}, {source}, {length} is outside the command's {} argument bytes", args.len()));
            }
            params.push(NewParam { target, source, length });
        }
        let layer_of = get("layer_of")?.map(place).transpose()?;
        let keep_params = t.get::<Option<bool>>("keep_params").map_err(|e| e.to_string())?.unwrap_or(false);
        if keep_params && !params.is_empty() {
            return Err("keep_params and params both given".into());
        }
        Ok(New { command: crate::event_patches::Command { bank, index, args }, params, layer_of, keep_params })
    };
    let (at, action) = if let Some(n) = get("check")? {
        (n, Action::Check)
    } else if let Some(n) = get("remove")? {
        (n, Action::Remove)
    } else if let Some(n) = get("replace")? {
        (n, Action::Replace(new()?))
    } else if let Some(n) = get("insert")? {
        (n, Action::Insert(new()?))
    } else {
        return Err("needs check, remove, replace or insert = <command number>".into());
    };
    let expect = match t.get::<Option<Vec<i64>>>("is").map_err(|e| e.to_string())? {
        None => None,
        Some(v) if v.len() >= 2 && v[2..].iter().all(|b| (0..=255).contains(b)) => {
            let n = |x: i64| u32::try_from(x).map_err(|_| format!("bad bank or index {x}"));
            Some(Expect { bank: n(v[0])?, index: n(v[1])?, head: v[2..].iter().map(|&b| b as u8).collect() })
        }
        Some(_) => return Err("is = {bank, index, then bytes 0-255}".into()),
    };
    if matches!(action, Action::Check) && expect.is_none() {
        return Err("a check needs is = {bank, index, bytes...}".into());
    }
    Ok(Step { at: place(at)?, expect, action })
}
