//! Natives for kindle.net: a mod's messages to the same mod on the other games of a co-op session
//! (p2p MOD frames, over Kindle's own connection). Members are named by their ids as text
//! ("0x..."), as kindle.net.members gives them; a message is a value as kindle.store keeps them
//! (booleans, numbers, strings, tables), at most p2p::MOD_MAX_BYTES as JSON. A mod reaches only
//! the session's games, and on them only itself.

use super::prelude::*;

/// "host", "all" or a member's id ("0x...").
fn target(to: &str) -> mlua::Result<u64> {
    match to {
        "host" => Ok(0),
        "all" => Ok(crate::p2p::MOD_ALL),
        id => id
            .strip_prefix("0x")
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .ok_or_else(|| rt_err(format!("net: send to \"host\", \"all\" or a member's id, not {id:?}"))),
    }
}

fn channel_ok(channel: &str) -> mlua::Result<()> {
    if channel.is_empty() || channel.len() > 64 {
        return Err(rt_err("net: a channel's name is 1 to 64 bytes"));
    }
    Ok(())
}

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "net_send",
        lua.create_function(|lua, (to, channel, value): (String, String, LuaValue)| {
            channel_ok(&channel)?;
            let to = target(&to)?;
            let data = match crate::store::to_json(&value).map_err(rt_err)? {
                Some(v) => serde_json::to_vec(&v).map_err(|e| rt_err(e.to_string()))?,
                None => b"null".to_vec(),
            };
            let id = ctx(lua)?.id.clone();
            crate::p2p::send_mod(&id, &channel, to, &data).map_err(|e| rt_err(format!("net.send: {e}")))
        })?,
    )?;
    n.set(
        "net_lobby_send",
        lua.create_function(|lua, (to, channel, value): (String, String, LuaValue)| {
            channel_ok(&channel)?;
            let to = target(&to)?;
            let data = match crate::store::to_json(&value).map_err(rt_err)? {
                Some(v) => serde_json::to_vec(&v).map_err(|e| rt_err(e.to_string()))?,
                None => b"null".to_vec(),
            };
            let id = ctx(lua)?.id.clone();
            crate::lobby::send_mod(&id, &channel, to, &data).map_err(|e| rt_err(format!("net.lobby_send: {e}")))
        })?,
    )?;
    n.set(
        "net_set_welcome",
        lua.create_function(|lua, (channel, value): (String, LuaValue)| {
            channel_ok(&channel)?;
            let data = match crate::store::to_json(&value).map_err(rt_err)? {
                Some(v) => Some(serde_json::to_vec(&v).map_err(|e| rt_err(e.to_string()))?),
                None => None,
            };
            let id = ctx(lua)?.id.clone();
            crate::p2p::set_welcome(&id, &channel, data).map_err(|e| rt_err(format!("net.set_welcome: {e}")))
        })?,
    )?;
    n.set(
        "net_welcome",
        lua.create_function(|lua, channel: String| {
            channel_ok(&channel)?;
            let id = ctx(lua)?.id.clone();
            let Some(data) = crate::p2p::welcome(&id, &channel) else { return Ok(LuaValue::Nil) };
            let value: serde_json::Value = serde_json::from_slice(&data).map_err(|e| rt_err(format!("net.welcome: {e}")))?;
            crate::store::to_lua(lua, &value)
        })?,
    )?;
    n.set(
        "net_require_dlc",
        lua.create_function(|lua, names: Vec<String>| {
            let mut bits = 0u8;
            for name in &names {
                bits |= match name.as_str() {
                    "ashes_of_ariandel" => 1,
                    "ringed_city" => 2,
                    other => return Err(rt_err(format!("net.require_dlc: no DLC {other:?} (\"ashes_of_ariandel\", \"ringed_city\")"))),
                };
            }
            let id = ctx(lua)?.id.clone();
            crate::p2p::require_dlc(&id, bits);
            Ok(())
        })?,
    )?;
    n.set(
        "net_on",
        lua.create_function(|lua, (channel, f): (String, Function)| {
            channel_ok(&channel)?;
            let mut c = ctx(lua)?;
            let h = c.handlers.handle();
            c.handlers.net.push((h, channel, f));
            Ok(h)
        })?,
    )?;
    n.set(
        "net_role",
        lua.create_function(|_, ()| {
            Ok(match crate::p2p::hosting() {
                Some(true) => Some("host"),
                Some(false) => Some("player"),
                None => None,
            })
        })?,
    )?;
    n.set("net_me", lua.create_function(|_, ()| Ok(crate::p2p::me().map(|id| format!("{id:#x}"))))?)?;
    n.set("net_host", lua.create_function(|_, ()| Ok(crate::p2p::host_id().map(|id| format!("{id:#x}"))))?)?;
    n.set(
        "net_player",
        lua.create_function(|lua, id: String| {
            let steam = match target(&id)? {
                0 => crate::p2p::host_id().ok_or_else(|| rt_err("net.player: no session"))?,
                crate::p2p::MOD_ALL => return Err(rt_err("net.player: give \"host\" or a member's id")),
                s => s,
            };
            let Some((chr, name)) = crate::session_players::member_character(steam) else { return Ok(None) };
            let Some(handle) = crate::memory::read::<u32>(chr + 0x08) else { return Ok(None) };
            let t = lua.create_table()?;
            t.set("handle", handle)?;
            t.set("name", name)?;
            Ok(Some(t))
        })?,
    )?;
    n.set("net_host_session", lua.create_function(|_, ()| crate::session::host().map_err(|e| rt_err(format!("net.host_session: {e}"))))?)?;
    n.set("net_state", lua.create_function(|_, ()| crate::session::state().map_err(rt_err))?)?;
    n.set("net_code", lua.create_function(|_, ()| Ok((crate::session::code(), crate::p2p::code_is_local())))?)?;
    n.set(
        "net_pings",
        lua.create_function(|lua, ()| {
            let out = lua.create_table()?;
            for (i, (name, ms)) in crate::session_hud::ping_rows().into_iter().enumerate() {
                let row = lua.create_table()?;
                row.set("name", name)?;
                row.set("ms", ms)?;
                out.set(i + 1, row)?;
            }
            Ok(out)
        })?,
    )?;
    n.set(
        "net_leave",
        lua.create_function(|_, for_now: Option<bool>| crate::session::leave(!for_now.unwrap_or(false)).map_err(|e| rt_err(format!("net.leave: {e}"))))?,
    )?;
    n.set(
        "net_join",
        lua.create_function(|lua, (code, f): (String, Function)| {
            let mut c = ctx(lua)?;
            let h = c.handlers.handle();
            crate::session::join_later(&code, &c.id, h).map_err(|e| rt_err(format!("net.join: {e}")))?;
            c.handlers.joins.push((h, f));
            Ok(())
        })?,
    )?;
    n.set("net_open_lobby", lua.create_function(|_, ()| crate::lobby::open().map_err(|e| rt_err(format!("net.open_lobby: {e}"))))?)?;
    n.set(
        "net_join_lobby",
        lua.create_function(|lua, (code, f): (String, Function)| {
            let mut c = ctx(lua)?;
            let h = c.handlers.handle();
            crate::lobby::join_later(&code, &c.id, h).map_err(|e| rt_err(format!("net.join_lobby: {e}")))?;
            c.handlers.joins.push((h, f));
            Ok(())
        })?,
    )?;
    n.set(
        "net_leave_lobby",
        lua.create_function(|_, ()| {
            crate::lobby::leave();
            Ok(())
        })?,
    )?;
    n.set(
        "net_lobby",
        lua.create_function(|lua, ()| {
            let Some(hosting) = crate::lobby::hosting() else { return Ok(None) };
            let t = lua.create_table()?;
            t.set("role", if hosting { "host" } else { "member" })?;
            t.set("code", crate::lobby::code())?;
            if hosting {
                t.set("world", crate::party_saves::host_world())?;
            }
            if let Some((me, host)) = crate::lobby::ids() {
                t.set("me", format!("{me:#x}"))?;
                t.set("host", format!("{host:#x}"))?;
            }
            let members = lua.create_table()?;
            for (i, m) in crate::lobby::members().into_iter().enumerate() {
                let row = lua.create_table()?;
                row.set("id", format!("{:#x}", m.id))?;
                row.set("name", m.name)?;
                row.set("place", m.place.name())?;
                members.set(i + 1, row)?;
            }
            t.set("members", members)?;
            Ok(Some(t))
        })?,
    )?;
    n.set(
        "net_send_party_save",
        lua.create_function(|_, member: String| {
            let id = member
                .strip_prefix("0x")
                .and_then(|h| u64::from_str_radix(h, 16).ok())
                .ok_or_else(|| rt_err(format!("net.send_party_save: a member's id, not {member:?}")))?;
            crate::lobby::send_party_save(id).map_err(|e| rt_err(format!("net.send_party_save: {e}")))
        })?,
    )?;
    n.set("net_members", lua.create_function(|_, ()| Ok(crate::p2p::members().map(|m| m.iter().map(|id| format!("{id:#x}")).collect::<Vec<_>>())))?)?;
    Ok(())
}
