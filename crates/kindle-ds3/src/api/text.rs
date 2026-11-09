//! Natives for kindle.text.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "text_get",
        lua.create_function(|_, (cat, id): (String, u32)| {
            let cats = text_override::category_ids(&cat).map_err(rt_err)?;
            Ok(text_override::get_any(&cats, id))
        })?,
    )?;
    n.set(
        "text_set",
        lua.create_function(|lua, (cat, id, value): (String, u32, String)| {
            let cats = text_override::category_ids(&cat).map_err(rt_err)?;
            let owner = ctx(lua)?.id.clone();
            match text_override::set_or_add(&cats, id, &value, &owner).map_err(rt_err)? {
                0 => Err(rt_err(format!("there is no {cat} text with id {id} to replace"))),
                n => Ok(n),
            }
        })?,
    )?;
    n.set(
        "text_reset",
        lua.create_function(|_, (cat, id): (String, u32)| {
            let cats = text_override::category_ids(&cat).map_err(rt_err)?;
            Ok(text_override::reset(&cats, id))
        })?,
    )?;
    n.set("text_categories", lua.create_function(|lua, ()| lua.create_sequence_from(text_override::CATEGORIES.iter().map(|(n, _)| *n)))?)?;
    n.set(
        "text_item_get",
        lua.create_function(|_, (item, field): (i64, String)| {
            let (cats, id) = item_text_target(item, &field)?;
            Ok(text_override::get_any(&cats, id))
        })?,
    )?;
    n.set(
        "text_item_set",
        lua.create_function(|lua, (item, field, value): (i64, String, String)| {
            let (cats, id) = item_text_target(item, &field)?;
            let owner = ctx(lua)?.id.clone();
            match text_override::set_or_add(&cats, id, &value, &owner).map_err(rt_err)? {
                0 => Err(rt_err(format!("this item has no {field} text to replace"))),
                n => Ok(n),
            }
        })?,
    )?;
    n.set(
        "text_item_reset",
        lua.create_function(|_, (item, field): (i64, String)| {
            let (cats, id) = item_text_target(item, &field)?;
            Ok(text_override::reset(&cats, id))
        })?,
    )?;
    Ok(())
}
