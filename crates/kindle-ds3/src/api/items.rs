//! Natives for kindle.items.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "item_make",
        lua.create_function(|_, (cat, id): (String, i64)| {
            let category = match cat.as_str() {
                "weapon" => ItemCategory::Weapon,
                "armor" => ItemCategory::Protector,
                "ring" => ItemCategory::Accessory,
                "goods" => ItemCategory::Goods,
                _ => return Err(rt_err(format!("unknown item category {cat:?}"))),
            };
            game::make_item(category, id).map_err(rt_err)
        })?,
    )?;
    n.set(
        "item_give",
        lua.create_function(|_, (item, qty, popup): (i64, u32, bool)| game::give_item(game::item_id(item).map_err(rt_err)?, qty, popup).map_err(rt_err))?,
    )?;
    n.set("item_remove", lua.create_function(|_, (item, qty): (i64, u32)| game::remove_item(game::item_id(item).map_err(rt_err)?, qty).map_err(rt_err))?)?;
    n.set("item_count", lua.create_function(|_, item: i64| game::item_count(game::item_id(item).map_err(rt_err)?).map_err(rt_err))?)?;
    n.set(
        "item_info",
        lua.create_function(|_, item: i64| {
            let id = game::item_id(item).map_err(rt_err)?;
            let category = game::category_name(id);
            // Its icon, for pictures (a layer's image, a screen row's icon): a weapon's row is
            // its base's (+0 of the infusion), armour has a man's and a woman's (the man's).
            let (table, field, row) = match category {
                "weapon" => ("EquipParamWeapon", "iconId", id.param_id() - id.param_id() % 100),
                "armor" => ("EquipParamProtector", "iconIdM", id.param_id()),
                "ring" => ("EquipParamAccessory", "iconId", id.param_id()),
                _ => ("EquipParamGoods", "iconId", id.param_id()),
            };
            let icon = kindle_core::params::registry().ok().and_then(|reg| reg.table(table).ok()?.read(row as u64, field).ok()).and_then(|v| match v {
                kindle_paramdef::Value::Int(i) if i >= 0 => Some(i),
                _ => None,
            });
            Ok((category, id.param_id(), display_name(id), icon))
        })?,
    )?;
    n.set(
        "item_name",
        lua.create_function(|_, item: i64| {
            let id = game::item_id(item).map_err(rt_err)?;
            text::item_names().map_err(rt_err)?;
            Ok(display_name(id))
        })?,
    )?;
    n.set(
        "item_by_name",
        lua.create_function(|_, (name, kind): (String, Option<String>)| {
            let kind = match kind.as_deref() {
                None => None,
                Some(k) => Some(text::Kind::parse(k).ok_or_else(|| rt_err(format!("unknown item category {k:?}")))?),
            };
            let names = text::item_names().map_err(rt_err)?;
            // Names given by mods (items.set_name) count too.
            let (k, id) = match text_override::find_renamed_item(&name, kind) {
                Some(found) => found,
                None => names.lookup(&name, kind).map_err(rt_err)?,
            };
            game::make_item(k.category(), id as i64).map_err(rt_err)
        })?,
    )?;
    n.set(
        "item_find",
        lua.create_function(|lua, (query, limit): (String, Option<usize>)| {
            let names = text::item_names().map_err(rt_err)?;
            let list = lua.create_table()?;
            for (i, (k, id, name)) in names.find(&query, limit.unwrap_or(25).min(500)).into_iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", name)?;
                e.set("category", k.name())?;
                e.set("id", id)?;
                e.set("item", game::make_item(k.category(), id as i64).map_err(rt_err)?)?;
                list.set(i + 1, e)?;
            }
            Ok(list)
        })?,
    )?;
    Ok(())
}
