//! How Dark Souls III's things look to mods: the tables for characters and bonfires, item names as the
//! player sees them (with mods' renames), HKS values. Used by the natives (api/) and the hooks' dispatch
//! (frame.rs).

use mlua::{Lua, Table};

use crate::mods::rt_err;
use crate::{bonfire, chars, game, text, text_override};

/// The ChrIns an HKS request is for (character handle or entity id; nil = the player).
pub fn hks_owner(key: Option<i64>) -> mlua::Result<usize> {
    match key {
        Some(k) => Ok(chars::find(k).map_err(rt_err)?.addr),
        None => game::player_chr_addr().map_err(rt_err),
    }
}

/// A bonfire as mods see it (kindle.bonfire).
pub fn bonfire_table(lua: &Lua, b: bonfire::Bonfire, last_rested: Option<i32>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("id", b.row)?;
    t.set("entity_id", b.entity_id)?;
    t.set("name", b.name)?;
    t.set("lit", b.lit)?;
    t.set("flag", b.flag)?;
    t.set("warpable", b.warpable)?;
    t.set("dlc", b.dlc)?;
    t.set("last_rested", last_rested == Some(b.entity_id))?;
    Ok(t)
}

/// An HKS value as a Lua value.
pub fn hks_to_lua(lua: &Lua, v: &crate::hks::HksValue) -> mlua::Result<mlua::Value> {
    use crate::hks::HksValue;
    Ok(match v {
        HksValue::Bool(b) => mlua::Value::Boolean(*b),
        HksValue::Number(n) => mlua::Value::Number(*n as f64),
        HksValue::Str(s) => mlua::Value::String(lua.create_string(s)?),
        _ => mlua::Value::Nil,
    })
}

/// An item's current display name: a mod's rename if any, else the game's text.
pub fn display_name(id: crate::ds3::ItemId) -> Option<String> {
    let kind = text::Kind::from_category(id.category());
    let pid = id.param_id();
    // Upgraded weapons (+N) share their base weapon's text entry.
    let base = if kind == text::Kind::Weapon { pid - pid % 100 } else { pid };
    if let Some(n) = text_override::item_name_override(kind, base) {
        return Some(if base != pid { format!("{n} +{}", pid - base) } else { n });
    }
    text::item_names().ok()?.name(kind, pid)
}

/// Item id for a display name (names given by mods via items.set_name count too).
pub fn item_by_name(name: &str) -> Result<i64, String> {
    let names = text::item_names()?;
    let (k, id) = match text_override::find_renamed_item(name, None) {
        Some(found) => found,
        None => names.lookup(name, None)?,
    };
    game::make_item(k.category(), id as i64)
}

/// A name as a Lua identifier: lowercase, runs of anything else become one '_'
/// ("Firelink Shrine" -> "firelink_shrine", "Curse-rotted Greatwood" -> "curse_rotted_greatwood").
pub fn snake_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if c == '\'' {
            continue;
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

/// Bonfires' lit flags by name (snake_case), the first bonfire of each name.
pub fn bonfire_flags() -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = Vec::new();
    for b in bonfire::all().unwrap_or_default() {
        if let Some(name) = b.name
            && b.flag > 0
        {
            let key = snake_name(&name);
            if !key.is_empty() && !out.iter().any(|(k, _)| *k == key) {
                out.push((key, b.flag));
            }
        }
    }
    out
}

pub fn character_table(lua: &Lua, c: &chars::Character, from: Option<[f32; 3]>) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("handle", c.handle)?;
    t.set("entity_id", c.entity_id)?;
    t.set("npc_id", c.npc_param_id)?;
    t.set("name", chars::name(c.npc_param_id))?;
    t.set("model", c.model.as_str())?;
    t.set("hp", c.hp)?;
    t.set("max_hp", c.max_hp)?;
    t.set("alive", c.hp > 0)?;
    t.set("is_player", c.is_player)?;
    t.set("other_player", c.other_player)?;
    t.set("team", c.team)?;
    t.set("kind", c.kind())?;
    t.set("address", c.addr as i64)?; // for the debug module
    if let Some([x, y, z]) = c.pos {
        t.set("x", x)?;
        t.set("y", y)?;
        t.set("z", z)?;
        if let Some(f) = from {
            t.set("distance", chars::distance(f, [x, y, z]))?;
        }
    }
    Ok(t)
}

pub fn player_pos() -> Option<[f32; 3]> {
    // The player alone: every character (chars::all) was hundreds of reads, per call.
    let addr = crate::ds3::main_player()?.addr();
    chars::physics_position(addr).or_else(|| chars::position(addr))
}

pub fn item_text_target(item: i64, field: &str) -> mlua::Result<(Vec<u32>, u32)> {
    let id = game::item_id(item).map_err(rt_err)?;
    let kind = text::Kind::from_category(id.category());
    let pid = id.param_id();
    let base = if kind == text::Kind::Weapon { pid - pid % 100 } else { pid };
    Ok((text_override::item_categories(kind, field).map_err(rt_err)?, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_names() {
        assert_eq!(snake_name("Firelink Shrine"), "firelink_shrine");
        assert_eq!(snake_name("Curse-rotted Greatwood"), "curse_rotted_greatwood");
        assert_eq!(snake_name("Aldrich, Devourer of Gods"), "aldrich_devourer_of_gods");
        assert_eq!(snake_name("Champion's Gravetender"), "champions_gravetender");
    }
}
