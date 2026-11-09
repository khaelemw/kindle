//! Natives for kindle.flags.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set("flag_bosses", lua.create_function(|lua, ()| lua.create_table_from(game::BOSSES.iter().map(|(name, flag, _)| (*name, *flag))))?)?;
    n.set(
        "flag_bonfires",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            for (name, flag) in bonfire_flags() {
                t.set(name, flag)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "flag_get",
        lua.create_function(|_, (id, set): (i64, Option<String>)| match flag_set_named(set)? {
            None => game::flag_get(id).map_err(rt_err),
            Some(s) => {
                let id = crate::ds3::check_flag(id).map_err(rt_err)?;
                crate::ds3::with_flag_set(s, id, || crate::ds3::get_flag(id)).map_err(rt_err)
            }
        })?,
    )?;
    n.set(
        "flag_set",
        lua.create_function(|_, (id, state, set): (i64, bool, Option<String>)| match flag_set_named(set)? {
            None => game::flag_set(id, state).map_err(rt_err),
            Some(s) => {
                let id = crate::ds3::check_flag(id).map_err(rt_err)?;
                crate::ds3::with_flag_set(s, id, || {
                    crate::ds3::set_flag(id, state)?;
                    Ok(crate::ds3::get_flag(id)? == state)
                })
                .map_err(rt_err)
            }
        })?,
    )?;
    n.set(
        "flag_block",
        lua.create_function(|_, (first, set): (i64, String)| {
            let set = flag_set_named(Some(set))?.expect("named");
            Ok(crate::flag_blocks::block(set, first).map_err(rt_err)?.map(|w| w.to_vec()))
        })?,
    )?;
    n.set(
        "flag_set_block",
        lua.create_function(|_, (first, words, set): (i64, Vec<i64>, String)| {
            let set = flag_set_named(Some(set))?.expect("named");
            if words.len() != crate::flag_blocks::WORDS {
                return Err(rt_err(format!("flags.set_block: {} words, not {}", crate::flag_blocks::WORDS, words.len())));
            }
            let mut w = [0u32; crate::flag_blocks::WORDS];
            for (slot, &v) in w.iter_mut().zip(&words) {
                *slot = u32::try_from(v).map_err(|_| rt_err(format!("flags.set_block: {v} isn't a 32-bit word")))?;
            }
            crate::flag_blocks::set_block(set, first, &w).map_err(rt_err)
        })?,
    )?;
    n.set(
        "flag_set_in_use",
        lua.create_function(|_, ()| {
            Ok(match crate::ds3::flag_set_in_use().map_err(rt_err)? {
                crate::ds3::FlagSet::Own => "own",
                crate::ds3::FlagSet::Session => "session",
            })
        })?,
    )?;
    n.set(
        "flag_watch",
        lua.create_function(|lua, (flag, on_set, f): (i64, bool, Function)| {
            u32::try_from(flag).map_err(|_| rt_err(format!("event flag {flag} is out of range")))?;
            let mut c = ctx(lua)?;
            let handle = c.handlers.handle();
            c.handlers.flags.push(FlagWatch { handle, flag, on_set, last: None, func: f });
            Ok(handle)
        })?,
    )?;
    Ok(())
}

/// A flag set by name: "own" (the character's) or "session" (a player's in a session); nil, the one
/// in use.
fn flag_set_named(set: Option<String>) -> mlua::Result<Option<crate::ds3::FlagSet>> {
    match set.as_deref() {
        None => Ok(None),
        Some("own") => Ok(Some(crate::ds3::FlagSet::Own)),
        Some("session") => Ok(Some(crate::ds3::FlagSet::Session)),
        Some(other) => Err(rt_err(format!("flag set {other:?}: \"own\" or \"session\""))),
    }
}
