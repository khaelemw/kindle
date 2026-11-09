//! Natives for kindle.world: other characters, bonfires.

use super::prelude::*;

/// A spawned character as world.character returns it.
fn spawned(lua: &Lua, addr: usize) -> mlua::Result<LuaValue> {
    let all = chars::all().map_err(rt_err)?;
    let here = all.iter().find(|c| c.is_player).and_then(|p| p.pos);
    match all.iter().find(|c| c.addr == addr) {
        Some(c) => Ok(LuaValue::Table(character_table(lua, c, here)?)),
        None => Ok(LuaValue::Nil),
    }
}

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set("world_load_ai", lua.create_function(|_, area: String| crate::spawn::load_ai(&area).map_err(rt_err))?)?;
    n.set(
        "world_spawn",
        lua.create_function(|lua, opts: Table| {
            let all = chars::all().map_err(rt_err)?;
            let player = all.iter().find(|c| c.is_player).ok_or_else(|| rt_err("no character is loaded"))?;
            // A named enemy fills in the ids and its home area; explicit ones still win.
            let enemy = match opts.get::<Option<String>>("enemy")? {
                Some(name) => Some(
                    crate::enemies::find(&name)
                        .ok_or_else(|| rt_err(format!("no enemy named {name:?} (see kindle.enemy in the stubs, or the overlay's Find tab)")))?,
                ),
                None => None,
            };
            let model: Option<u32> = match opts.get::<LuaValue>("model")? {
                LuaValue::Nil => enemy.map(|e| e.model),
                LuaValue::Integer(n) => Some(n as u32),
                LuaValue::Number(n) => Some(n as u32),
                LuaValue::String(s) => {
                    let s = s.to_str()?.to_string();
                    let digits = s.trim_start_matches(['c', 'C']);
                    Some(digits.parse().map_err(|_| rt_err(format!("model {s:?}: use a number (1100) or \"c1100\"")))?)
                }
                other => return Err(rt_err(format!("model: expected a number or string, got {}", other.type_name()))),
            };
            let mut npc_param: Option<i32> = opts.get::<Option<i32>>("npc_param")?.or(enemy.map(|e| e.npc_param));
            // Fill the missing one from a loaded character of the same kind.
            let model = match (model, npc_param) {
                (Some(m), _) => m,
                (None, Some(p)) => {
                    let c = all
                        .iter()
                        .find(|c| c.npc_param_id == p)
                        .ok_or_else(|| rt_err(format!("no loaded character uses NpcParam {p}; give a model or an enemy name")))?;
                    let base = crate::spawn::base_model(&c.model);
                    base.trim_start_matches('c').parse().map_err(|_| rt_err(format!("unexpected model {:?}", c.model)))?
                }
                (None, None) => return Err(rt_err("world.spawn needs an enemy name, a model or an npc_param")),
            };
            if npc_param.is_none() {
                let name = crate::spawn::model_name(model);
                npc_param = all.iter().find(|c| crate::spawn::base_model(&c.model) == name).map(|c| c.npc_param_id);
            }
            let npc_param = npc_param.ok_or_else(|| rt_err("world.spawn needs an npc_param for that model"))?;
            let think_param: i32 =
                opts.get::<Option<i32>>("think_param")?.or(enemy.filter(|e| e.npc_param == npc_param).map(|e| e.think_param)).unwrap_or(npc_param);
            let area: Option<String> = opts.get::<Option<String>>("area")?.or(enemy.map(|e| e.area.to_string()));
            let ai: bool = opts.get::<Option<bool>>("ai_enabled")?.unwrap_or(true);
            let here = player.pos.ok_or_else(|| rt_err("the player's position is unavailable"))?;
            let facing = chars::facing(player.addr).unwrap_or(0.0);
            let distance: f32 = opts.get::<Option<f32>>("distance")?.unwrap_or(3.0);
            let pos = match (opts.get::<Option<f32>>("x")?, opts.get::<Option<f32>>("y")?, opts.get::<Option<f32>>("z")?) {
                (Some(x), Some(y), Some(z)) => [x, y, z],
                // The character's facing points along (-sin, -cos): checked in game.
                (None, None, None) => [here[0] - facing.sin() * distance, here[1], here[2] - facing.cos() * distance],
                _ => return Err(rt_err("world.spawn: give all of x, y and z, or none")),
            };
            // Facing the player unless told otherwise.
            let angle: f32 = opts.get::<Option<f32>>("angle")?.unwrap_or(facing + std::f32::consts::PI);
            let spawn = crate::spawn::Spawn { model, npc_param, think_param, pos, angle, area, ai };
            // The character now, or a token to poll while its area's AI loads.
            match crate::spawn::begin(spawn).map_err(rt_err)? {
                crate::spawn::Started::Now(addr) => Ok((spawned(lua, addr)?, None)),
                crate::spawn::Started::Pending(token) => Ok((LuaValue::Nil, Some(token))),
            }
        })?,
    )?;
    n.set(
        "world_spawn_poll",
        lua.create_function(|lua, token: u32| match crate::spawn::poll(token).map_err(rt_err)? {
            Some(addr) => spawned(lua, addr),
            None => Ok(LuaValue::Nil),
        })?,
    )?;
    n.set(
        "world_bonfires",
        lua.create_function(|lua, ()| {
            let last = bonfire::last_rested().ok().flatten();
            let list = lua.create_table()?;
            for b in bonfire::all().map_err(rt_err)? {
                list.push(bonfire_table(lua, b, last)?)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "chr_teleport",
        lua.create_function(|_, (key, x, y, z): (Option<i64>, f32, f32, f32)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            chars::teleport(addr, [x, y, z]).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_facing",
        lua.create_function(|_, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            Ok(chars::facing(addr))
        })?,
    )?;
    n.set(
        "chr_set_facing",
        lua.create_function(|_, (key, radians): (Option<i64>, f32)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            chars::set_facing(addr, radians).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_flag",
        lua.create_function(|_, (key, name): (Option<i64>, String)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            let (at, bit) = chars::flag_location(addr, &name).map_err(rt_err)?;
            Ok(chars::flag(at, bit))
        })?,
    )?;
    n.set("chr_clear_aggro", lua.create_function(|_, key: i64| chars::clear_aggro(chars::find(key).map_err(rt_err)?.addr).map_err(rt_err))?)?;
    n.set(
        "chr_set_ai",
        lua.create_function(|lua, (key, on): (i64, bool)| {
            let owner = ctx(lua)?.id.clone();
            let c = chars::find(key).map_err(rt_err)?;
            chars::set_ai(&c, on, &owner).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_set_flag",
        lua.create_function(|_, (key, name, on): (Option<i64>, String, bool)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            let (at, bit) = chars::flag_location(addr, &name).map_err(rt_err)?;
            if name == "draw_model" {
                chars::mod_set_visible(addr, on);
            }
            chars::set_flag(at, bit, on).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_status",
        lua.create_function(|lua, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            let t = lua.create_table()?;
            for (name, (value, max)) in chars::STATUSES.iter().zip(chars::status(addr).map_err(rt_err)?) {
                let s = lua.create_table()?;
                s.set("value", value)?;
                s.set("max", max)?;
                t.set(*name, s)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "chr_poise",
        lua.create_function(|_, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            chars::poise(addr).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_animation",
        lua.create_function(|_, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            Ok(chars::animation_name(addr))
        })?,
    )?;
    n.set(
        "chr_speed",
        lua.create_function(|_, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            Ok(chars::animation_speed(addr))
        })?,
    )?;
    n.set(
        "chr_set_speed",
        lua.create_function(|_, (key, speed): (Option<i64>, f32)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            chars::set_animation_speed(addr, speed).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_set_team",
        lua.create_function(|_, (key, team): (Option<i64>, u8)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            chars::set_team(addr, team).map_err(rt_err)
        })?,
    )?;
    n.set(
        "chr_animations",
        lua.create_function(|lua, key: Option<i64>| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            let t = lua.create_table()?;
            for (id, name) in anim::names(addr).map_err(rt_err)? {
                t.set(id, name)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "chr_play_animation",
        lua.create_function(|_, (key, anim): (Option<i64>, mlua::Value)| {
            let addr = match key {
                Some(k) => chars::find(k).map_err(rt_err)?.addr,
                None => game::player_chr_addr().map_err(rt_err)?,
            };
            let id = match anim {
                mlua::Value::Integer(i) => i as i32,
                mlua::Value::String(s) => anim::id_of(addr, &s.to_str()?).map_err(rt_err)?,
                _ => return Err(rt_err("animation must be an event name or id")),
            };
            anim::play(addr, id).map_err(rt_err)
        })?,
    )?;
    n.set(
        "world_objects",
        lua.create_function(|lua, (radius, map, entity_id): (Option<f32>, Option<String>, Option<i32>)| {
            let me = chars::all_with_players().map_err(rt_err)?.iter().find(|c| c.is_player).and_then(|c| c.pos);
            let mut picked: Vec<(f32, objects::Object)> = objects::all()
                .map_err(rt_err)?
                .into_iter()
                .filter(|o| map.as_deref().is_none_or(|m| m == o.map))
                .filter(|o| entity_id.is_none_or(|e| o.entity_id == Some(e)))
                .map(|o| (me.map_or(f32::INFINITY, |p| chars::distance(p, o.pos)), o))
                .filter(|(d, _)| radius.is_none_or(|r| *d <= r))
                .collect();
            picked.sort_by(|a, b| a.0.total_cmp(&b.0));
            let list = lua.create_table()?;
            for (i, (d, o)) in picked.into_iter().enumerate() {
                let t = lua.create_table()?;
                t.set("handle", o.handle)?;
                t.set("entity_id", o.entity_id)?;
                t.set("map", o.map)?;
                t.set("x", o.pos[0])?;
                t.set("y", o.pos[1])?;
                t.set("z", o.pos[2])?;
                t.set("destroyed", o.destroyed)?;
                if d.is_finite() {
                    t.set("distance", d)?;
                }
                list.set(i + 1, t)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "world_characters",
        lua.create_function(|lua, (radius, alive_only, include_player, include_helpers, kind): (Option<f32>, bool, bool, bool, Option<String>)| {
            let all = chars::all_with_players().map_err(rt_err)?;
            let me = all.iter().find(|c| c.is_player).and_then(|c| c.pos);
            let mut picked: Vec<(f32, &chars::Character)> = all
                .iter()
                .filter(|c| include_player || !(c.is_player || c.other_player))
                .filter(|c| !alive_only || c.hp > 0)
                .filter(|c| include_helpers || c.kind() != "helper")
                .filter(|c| kind.as_deref().is_none_or(|k| k == c.kind()))
                .filter_map(|c| {
                    let d = match (me, c.pos) {
                        (Some(a), Some(b)) => chars::distance(a, b),
                        _ => f32::INFINITY,
                    };
                    match radius {
                        Some(r) if d > r => None,
                        _ => Some((d, c)),
                    }
                })
                .collect();
            picked.sort_by(|a, b| a.0.total_cmp(&b.0));
            let list = lua.create_table()?;
            for (i, (_, c)) in picked.into_iter().enumerate() {
                list.set(i + 1, character_table(lua, c, me)?)?;
            }
            Ok(list)
        })?,
    )?;
    n.set(
        "world_players",
        lua.create_function(|lua, ()| {
            let list = chars::players();
            let me = list.iter().find(|c| c.is_player).and_then(|c| c.pos);
            let out = lua.create_table()?;
            for (i, c) in list.iter().enumerate() {
                out.set(i + 1, character_table(lua, c, me)?)?;
            }
            Ok(out)
        })?,
    )?;
    n.set(
        "world_character",
        lua.create_function(|lua, key: i64| match chars::find(key) {
            Ok(c) => Ok(Some(character_table(lua, &c, player_pos())?)),
            Err(_) => Ok(None),
        })?,
    )?;
    n.set(
        "world_set_hp",
        lua.create_function(|_, (key, hp): (i64, i64)| {
            let c = chars::find(key).map_err(rt_err)?;
            chars::set_hp(c.addr, hp).map_err(rt_err)
        })?,
    )?;
    n.set(
        "world_apply_effect",
        lua.create_function(|_, (key, id): (i64, i64)| {
            let id = game::checked_effect_id(id).map_err(rt_err)?;
            let c = chars::find(key).map_err(rt_err)?;
            speffect::apply(c.addr, id).map_err(rt_err)
        })?,
    )?;
    n.set(
        "world_remove_effect",
        lua.create_function(|_, (key, id): (i64, i32)| {
            let c = chars::find(key).map_err(rt_err)?;
            speffect::expire(c.addr, id).map_err(rt_err)
        })?,
    )?;
    n.set(
        "world_effects",
        lua.create_function(|lua, key: i64| {
            let c = chars::find(key).map_err(rt_err)?;
            let list = lua.create_table()?;
            for (i, a) in speffect::active(c.addr).map_err(rt_err)?.into_iter().enumerate() {
                let e = lua.create_table()?;
                e.set("id", a.id)?;
                e.set("time_left", a.time_left)?;
                e.set("duration", a.duration)?;
                list.set(i + 1, e)?;
            }
            Ok(list)
        })?,
    )?;
    Ok(())
}
