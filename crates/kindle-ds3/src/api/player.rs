//! Natives for kindle.player.

use super::prelude::*;

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "player_camera",
        lua.create_function(|lua, ()| {
            let c = game::camera().map_err(rt_err)?;
            let t = lua.create_table()?;
            for (k, v) in [("x", c.pos[0]), ("y", c.pos[1]), ("z", c.pos[2])] {
                t.set(k, v)?;
            }
            for (k, v) in [("dx", c.forward[0]), ("dy", c.forward[1]), ("dz", c.forward[2])] {
                t.set(k, v)?;
            }
            // The same convention as world.facing: a character with this facing looks the
            // way the camera does (horizontally).
            t.set("yaw", (-c.forward[0]).atan2(-c.forward[2]))?;
            t.set("fov", c.fov)?;
            Ok(t)
        })?,
    )?;
    n.set(
        "player_lock_target",
        lua.create_function(|lua, ()| {
            let Some(handle) = chars::lock_target() else { return Ok(LuaValue::Nil) };
            let all = chars::all_with_players().map_err(rt_err)?;
            let here = all.iter().find(|c| c.is_player).and_then(|p| p.pos);
            match all.iter().find(|c| c.handle == handle) {
                Some(c) => Ok(LuaValue::Table(character_table(lua, c, here)?)),
                None => Ok(LuaValue::Nil),
            }
        })?,
    )?;
    n.set("player_set_lock_target", lua.create_function(|_, handle: Option<u32>| chars::set_lock_target(handle).map_err(rt_err))?)?;
    n.set(
        "player_set_camera",
        lua.create_function(|lua, cam: Option<Table>| {
            let id = ctx(lua)?.id.clone();
            let Some(t) = cam else {
                return Ok(crate::camera::release(&id));
            };
            let num = |t: &Table, k: &str| t.get::<Option<f32>>(k).map_err(|_| rt_err(format!("player.set_camera: {k} must be a number")));
            let (Some(x), Some(y), Some(z)) = (num(&t, "x")?, num(&t, "y")?, num(&t, "z")?) else {
                return Err(rt_err("player.set_camera: it needs x, y and z (nil lets the camera go)"));
            };
            let forward = if let Some(at) = t.get::<Option<Table>>("look_at").map_err(|_| rt_err("player.set_camera: look_at must be {x, y, z}"))? {
                let (Some(ax), Some(ay), Some(az)) = (num(&at, "x")?, num(&at, "y")?, num(&at, "z")?) else {
                    return Err(rt_err("player.set_camera: look_at needs x, y and z"));
                };
                [ax - x, ay - y, az - z]
            } else {
                match (num(&t, "dx")?, num(&t, "dy")?, num(&t, "dz")?) {
                    (Some(dx), Some(dy), Some(dz)) => [dx, dy, dz],
                    _ => return Err(rt_err("player.set_camera: it needs dx, dy and dz (where it looks) or look_at")),
                }
            };
            crate::camera::hold(&id, [x, y, z], forward, num(&t, "fov")?).map_err(|e| rt_err(format!("player.set_camera: {e}")))?;
            Ok(true)
        })?,
    )?;
    n.set("player_loaded", lua.create_function(|_, ()| Ok(game::in_world()))?)?;
    n.set("player_name", lua.create_function(|_, ()| Ok(game::player_game_data().map_err(rt_err)?.player_info().name()))?)?;
    n.set(
        "player_stats",
        lua.create_function(|lua, ()| {
            let p = game::player_game_data().map_err(rt_err)?.player_info();
            let t = lua.create_table()?;
            for (k, v) in [
                ("vigor", p.vigor),
                ("attunement", p.attunement),
                ("endurance", p.endurance),
                ("vitality", p.vitality),
                ("strength", p.strength),
                ("dexterity", p.dexterity),
                ("intelligence", p.intelligence),
                ("faith", p.faith),
                ("luck", p.luck),
            ] {
                t.set(k, v)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "player_get",
        lua.create_function(|_, stat: String| {
            let d = game::chr_data().map_err(rt_err)?;
            Ok(match stat.as_str() {
                "hp" => d.hp,
                "max_hp" => d.max_hp,
                "fp" => d.fp,
                "max_fp" => d.max_fp,
                "stamina" => d.stamina,
                "max_stamina" => d.max_stamina,
                _ => return Err(rt_err(format!("unknown stat {stat:?}"))),
            })
        })?,
    )?;
    n.set(
        "player_set",
        lua.create_function(|_, (stat, value): (String, i64)| {
            let d = game::chr_data().map_err(rt_err)?;
            let clamp = |max: i32| value.clamp(0, max as i64) as i32;
            match stat.as_str() {
                "hp" => d.hp = clamp(d.max_hp),
                "fp" => d.fp = clamp(d.max_fp),
                "stamina" => d.stamina = clamp(d.max_stamina),
                _ => return Err(rt_err(format!("stat {stat:?} cannot be set"))),
            }
            Ok(())
        })?,
    )?;
    n.set("player_apply_effect", lua.create_function(|_, id: i64| game::player_apply_effect(id).map_err(rt_err))?)?;
    n.set("player_remove_effect", lua.create_function(|_, id: i64| game::player_remove_effect(id).map_err(rt_err))?)?;
    n.set(
        "player_effects",
        lua.create_function(|lua, ()| {
            let list = lua.create_table()?;
            for (i, a) in game::player_effects().map_err(rt_err)?.into_iter().enumerate() {
                let e = lua.create_table()?;
                e.set("id", a.id)?;
                e.set("time_left", a.time_left)?;
                e.set("duration", a.duration)?;
                list.set(i + 1, e)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "player_set_attribute",
        lua.create_function(|_, (name, value, keep_level): (String, i64, Option<bool>)| {
            game::set_attribute(&name, value, keep_level.unwrap_or(false)).map_err(rt_err)
        })?,
    )?;
    n.set("player_set_soul_level", lua.create_function(|_, v: i64| game::set_soul_level(v).map_err(rt_err))?)?;
    n.set(
        "player_equipment",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            for (slot, item) in game::equipment().map_err(rt_err)? {
                t.set(slot, item)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "player_equip",
        lua.create_function(|_, (slot, item): (String, mlua::Value)| {
            let target = match item {
                mlua::Value::Nil => equip::Target::Empty,
                mlua::Value::Integer(i) => equip::Target::Item(i),
                mlua::Value::String(s) => equip::Target::Name(s.to_str()?.to_string()),
                _ => return Err(rt_err("item must be an item id or name")),
            };
            equip::equip(&slot, target).map_err(rt_err)
        })?,
    )?;
    n.set("player_last_bonfire", lua.create_function(|_, ()| bonfire::last_rested().map_err(rt_err))?)?;
    n.set(
        "player_warp",
        lua.create_function(|_, (target, force): (mlua::Value, Option<bool>)| {
            let key = match target {
                mlua::Value::Integer(i) => bonfire::BonfireKey::Id(i),
                mlua::Value::Number(n) => bonfire::BonfireKey::Id(n as i64),
                mlua::Value::String(s) => bonfire::BonfireKey::Name(s.to_str()?.to_string()),
                mlua::Value::Table(t) => bonfire::BonfireKey::Id(t.get::<i64>("entity_id")?),
                _ => return Err(rt_err("warp target must be a bonfire table, name or id")),
            };
            let b = bonfire::find(bonfire::all().map_err(rt_err)?, &key).map_err(rt_err)?;
            bonfire::warp(&b, force.unwrap_or(false)).map_err(rt_err)
        })?,
    )?;
    n.set("player_spells", lua.create_function(|_, ()| game::spells().map_err(rt_err))?)?;
    n.set("player_ng_cycle", lua.create_function(|_, ()| game::ng_cycle().map_err(rt_err))?)?;
    n.set("player_deaths", lua.create_function(|_, ()| game::deaths().map_err(rt_err))?)?;
    n.set("player_play_time", lua.create_function(|_, ()| game::play_time().map_err(rt_err))?)?;
    n.set("player_souls", lua.create_function(|_, ()| game::pgd_read_i32(0x74).map_err(rt_err))?)?;
    n.set("player_set_souls", lua.create_function(|_, v: i64| game::set_souls(v).map_err(rt_err))?)?;
    n.set("player_soul_level", lua.create_function(|_, ()| game::pgd_read_i32(0x70).map_err(rt_err))?)?;
    n.set("player_total_souls", lua.create_function(|_, ()| game::pgd_read_i32(0x78).map_err(rt_err))?)?;
    n.set("player_embered", lua.create_function(|_, ()| game::embered().map_err(rt_err))?)?;
    n.set("player_set_embered", lua.create_function(|_, on: bool| game::set_embered(on).map_err(rt_err))?)?;
    n.set(
        "player_estus",
        lua.create_function(|lua, ()| {
            let e = game::estus().map_err(rt_err)?;
            let t = lua.create_table()?;
            t.set("hp", e.hp)?;
            t.set("hp_max", e.hp_max)?;
            t.set("fp", e.fp)?;
            t.set("fp_max", e.fp_max)?;
            Ok(t)
        })?,
    )?;
    n.set("player_set_estus", lua.create_function(|_, (hp, fp): (Option<i64>, Option<i64>)| game::set_estus(hp, fp).map_err(rt_err))?)?;
    n.set(
        "player_position",
        lua.create_function(|_, ()| {
            let p = player_pos().ok_or_else(|| rt_err("no character is loaded"))?;
            Ok((p[0], p[1], p[2]))
        })?,
    )?;
    Ok(())
}
