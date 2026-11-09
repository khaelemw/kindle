//! Natives for kindle.params (each game's prelude makes the module of them): its tables through
//! params.rs's registry, new rows the game's way (its bindings' `add_param_rows`).

use super::prelude::*;
use crate::params;

/// Adds rows the game's way, as the mod's own work kept off its time limit (a table's rebuild).
/// Fields may then name the new rows, above their limits too (params.rs's `write_checked`).
fn add_rows(lua: &Lua, table: &str, rows: &[(u64, u64)]) -> mlua::Result<()> {
    let owner = ctx(lua)?.id.clone();
    let add = crate::bindings::get().ok_or_else(|| rt_err("no game's bindings are set"))?.add_param_rows;
    kindles_own(lua, || add(&owner, table, rows)).map_err(rt_err)?;
    params::note_added(rows.iter().map(|&(id, _)| id));
    Ok(())
}

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "param_tables",
        lua.create_function(|lua, ()| {
            let reg = params::registry().map_err(rt_err)?;
            lua.create_sequence_from(reg.tables().filter(|t| t.valid).map(|t| t.name.clone()))
        })?,
    )?;
    n.set(
        "param_fields",
        lua.create_function(|lua, table: String| {
            let reg = params::registry().map_err(rt_err)?;
            let t = reg.table(&table).map_err(rt_err)?;
            let list = lua.create_table()?;
            for (i, f) in t.def.fields.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", f.name.as_str())?;
                e.set("type", f.ty.name())?;
                e.set("offset", f.offset)?;
                if f.count > 1 {
                    e.set("count", f.count)?;
                }
                if let Some((bit, width)) = f.bits {
                    e.set("bit", bit)?;
                    e.set("bits", width)?;
                }
                list.set(i + 1, e)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "param_rows",
        lua.create_function(|lua, table: String| {
            let reg = params::registry().map_err(rt_err)?;
            let t = reg.table(&table).map_err(rt_err)?;
            lua.create_sequence_from(t.row_ids.iter().copied())
        })?,
    )?;
    n.set(
        "param_has",
        lua.create_function(|_, (table, row): (String, u64)| {
            let reg = params::registry().map_err(rt_err)?;
            Ok(reg.table(&table).map_err(rt_err)?.has_row(row))
        })?,
    )?;
    n.set("param_add_row", lua.create_function(|lua, (table, id, copy_from): (String, u64, u64)| add_rows(lua, &table, &[(id, copy_from)]))?)?;
    n.set(
        "param_add_rows",
        lua.create_function(|lua, (table, ids, copies): (String, Vec<u64>, Vec<u64>)| {
            let rows: Vec<(u64, u64)> = ids.into_iter().zip(copies).collect();
            add_rows(lua, &table, &rows)
        })?,
    )?;
    n.set(
        "param_read",
        lua.create_function(|lua, (table, row, field): (String, u64, String)| {
            let reg = params::registry().map_err(rt_err)?;
            let v = reg.table(&table).map_err(rt_err)?.read(row, &field).map_err(rt_err)?;
            from_param_value(lua, v)
        })?,
    )?;
    n.set(
        "param_copy",
        lua.create_function(|lua, (table, pairs, keep): (String, Vec<Vec<u64>>, Option<Vec<String>>)| {
            let pairs: Vec<(u64, u64)> = pairs
                .iter()
                .map(|p| match p[..] {
                    [from, to] => Ok((from, to)),
                    _ => Err(rt_err("params.copy: each pair is {from, to}")),
                })
                .collect::<mlua::Result<_>>()?;
            let reg = params::registry().map_err(rt_err)?;
            let t = reg.table(&table).map_err(rt_err)?;
            let keep = keep.unwrap_or_default();
            for k in &keep {
                t.field(k).map_err(rt_err)?;
            }
            // Every source as it is before any copy, so a row both copied from and to gives
            // what it held.
            let sources: Vec<Vec<u8>> = pairs.iter().map(|&(from, _)| t.row_data(from)).collect::<Result<_, _>>().map_err(rt_err)?;
            let names: Vec<String> = t.def.fields.iter().map(|f| f.name.clone()).filter(|n| !keep.contains(n)).collect();
            let keep: Vec<&str> = keep.iter().map(String::as_str).collect();
            for (&(_, to), bytes) in pairs.iter().zip(&sources) {
                // Each field's value as it was, so the copy is reverted like any write.
                let old: Vec<(String, kindle_paramdef::Value)> = names.iter().filter_map(|n| Some((n.clone(), t.read(to, n).ok()?))).collect();
                t.write_row(to, bytes, &keep).map_err(rt_err)?;
                let mut c = ctx(lua)?;
                for (field, value) in old {
                    c.journal.entry((t.name.clone(), to, field)).or_insert(value);
                }
            }
            Ok(())
        })?,
    )?;
    n.set(
        "param_reset",
        lua.create_function(|lua, (table, row): (Option<String>, Option<u64>)| {
            let reg = params::registry().map_err(rt_err)?;
            let mut c = ctx(lua)?;
            let mine: Vec<(String, u64, String)> = c
                .journal
                .keys()
                .filter(|(t, r, _)| table.as_ref().is_none_or(|want| want.eq_ignore_ascii_case(t)) && row.is_none_or(|want| want == *r))
                .cloned()
                .collect();
            let mut reverted = 0;
            for key in mine {
                if let Some(original) = c.journal.remove(&key)
                    && let Ok(t) = reg.table(&key.0)
                    && t.write(key.1, &key.2, &original).is_ok()
                {
                    reverted += 1;
                }
            }
            Ok(reverted)
        })?,
    )?;
    n.set(
        "param_write",
        lua.create_function(|lua, (table, row, field, value): (String, u64, String, LuaValue)| {
            let value = to_param_value(&value)?;
            let reg = params::registry().map_err(rt_err)?;
            let t = reg.table(&table).map_err(rt_err)?;
            let field_name = t.field(&field).map_err(rt_err)?.name.clone();
            let old = t.write_checked(row, &field_name, &value).map_err(rt_err)?;
            ctx(lua)?.journal.entry((t.name.clone(), row, field_name)).or_insert(old);
            Ok(())
        })?,
    )?;
    Ok(())
}
