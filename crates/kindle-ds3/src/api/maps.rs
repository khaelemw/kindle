//! Natives for kindle.maps: the game's map files' parts, and mods' changes to them.

use super::prelude::*;

const KINDS: [(i32, &str); 8] =
    [(0, "map_piece"), (1, "object"), (2, "enemy"), (4, "player"), (5, "collision"), (9, "dummy_object"), (10, "dummy_enemy"), (11, "connect_collision")];

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "maps_parts",
        lua.create_function(|lua, map: String| {
            let Some(parts) = crate::map_edits::parts(&map).map_err(rt_err)? else { return Ok(None) };
            let list = lua.create_table_with_capacity(parts.len(), 0)?;
            for p in parts.iter() {
                let t = lua.create_table()?;
                t.set("name", p.name.as_str())?;
                t.set("type", p.kind)?;
                if let Some((_, kind)) = KINDS.iter().find(|(k, _)| *k == p.kind) {
                    t.set("kind", *kind)?;
                }
                t.set("model", p.model.as_str())?;
                t.set("position", lua.create_sequence_from(p.position.iter().map(|&v| v as f64))?)?;
                t.set("rotation", lua.create_sequence_from(p.rotation.iter().map(|&v| v as f64))?)?;
                if p.entity > 0 {
                    t.set("entity_id", p.entity)?;
                }
                if !p.groups.is_empty() {
                    t.set("entity_groups", p.groups.clone())?;
                }
                if let Some([ai, npc, talk]) = p.enemy {
                    t.set("think_param", ai)?;
                    t.set("npc_param", npc)?;
                    t.set("talk", talk)?;
                }
                list.push(t)?;
            }
            Ok(Some(list))
        })?,
    )?;
    n.set(
        "maps_edit",
        lua.create_function(|lua, (map, edits): (String, Vec<Table>)| {
            let mut list = Vec::new();
            for (i, e) in edits.iter().enumerate() {
                list.push(edit(e).map_err(|err| rt_err(format!("maps.edit: change {}: {err}", i + 1)))?);
            }
            let owner = ctx(lua)?.id.clone();
            crate::map_edits::set(&owner, &map, list).map_err(rt_err)
        })?,
    )?;
    n.set("maps_preload", lua.create_function(|_, maps: Vec<String>| crate::map_edits::preload(maps).map_err(rt_err))?)?;
    n.set(
        "maps_reset",
        lua.create_function(|lua, map: Option<String>| {
            let owner = ctx(lua)?.id.clone();
            Ok(crate::map_edits::reset(&owner, map.as_deref()))
        })?,
    )?;
    Ok(())
}

/// A part number (1 = the first) as an index.
fn index(n: Option<i64>, what: &str) -> Result<usize, String> {
    match n {
        Some(n) if n >= 1 => Ok(n as usize - 1),
        _ => Err(format!("{what} must be a part number (1 or more)")),
    }
}

/// One change of maps.edit: {replace_enemy = n, from = {map =, part =}, keep_ids = bool} or
/// {replace_enemy = n, model = "cNNNN", npc_param =, think_param =, ai = "name", keep_ids = bool}.
fn edit(t: &Table) -> Result<crate::map_edits::Edit, String> {
    let get_int = |t: &Table, k: &str| t.get::<Option<i64>>(k).map_err(|e| e.to_string());
    if let Some(part) = get_int(t, "replace_enemy")? {
        if let Some(model) = t.get::<Option<String>>("model").map_err(|e| e.to_string())? {
            let small =
                |k: &str| -> Result<Option<i32>, String> { get_int(t, k)?.map(|v| i32::try_from(v).map_err(|_| format!("{k} is too big"))).transpose() };
            return Ok(crate::map_edits::Edit::SetEnemy {
                part: index(Some(part), "replace_enemy")?,
                model,
                npc: small("npc_param")?,
                think: small("think_param")?,
                ai: t.get::<Option<String>>("ai").map_err(|e| e.to_string())?,
                keep_ids: t.get::<Option<bool>>("keep_ids").map_err(|e| e.to_string())?.unwrap_or(false),
            });
        }
        let from: Table = t.get("from").map_err(|_| "replace_enemy needs from = {map =, part =}".to_string())?;
        let from_map: String = from.get("map").map_err(|_| "from.map must be a map's name".to_string())?;
        return Ok(crate::map_edits::Edit::ReplaceEnemy {
            part: index(Some(part), "replace_enemy")?,
            from_part: index(get_int(&from, "part")?, "from.part")?,
            from_map,
            keep_ids: t.get::<Option<bool>>("keep_ids").map_err(|e| e.to_string())?.unwrap_or(false),
        });
    }
    Err("unknown change (replace_enemy)".into())
}
