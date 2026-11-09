//! Natives for kindle.input: what the player presses, and keeping it from the game (input_block.rs
//! reads and filters the game's keyboard and controller reads). A mod that blocks the player's
//! actions is the one that gives them back; its unload does too.

use std::sync::Mutex;

use super::prelude::*;
use crate::input_block::{self, KEYS_LEFT_RIGHT, PAD_LEFT, PAD_RAW, PAD_RIGHT};

/// The mod whose block is on.
static BLOCKER: Mutex<Option<String>> = Mutex::new(None);

fn blocker() -> std::sync::MutexGuard<'static, Option<String>> {
    BLOCKER.lock().unwrap_or_else(|e| e.into_inner())
}

/// A mod unloads: its block is lifted.
pub(crate) fn release(owner: &str) {
    let mut b = blocker();
    if b.as_deref() == Some(owner) {
        *b = None;
        input_block::set_actions_blocked(false);
    }
}

pub(super) fn register(lua: &Lua, n: &Table) -> mlua::Result<()> {
    n.set(
        "input_block_actions",
        lua.create_function(|lua, on: bool| {
            let id = ctx(lua)?.id.clone();
            let mut b = blocker();
            if let Some(other) = b.as_deref().filter(|o| *o != id) {
                return Err(rt_err(format!("input.block_actions: {other} holds the block")));
            }
            *b = on.then_some(id);
            input_block::set_actions_blocked(on);
            Ok(())
        })?,
    )?;
    n.set(
        "input_block_interact",
        lua.create_function(|lua, on: bool| {
            let id = ctx(lua)?.id.clone();
            if blocker().as_deref() != Some(id.as_str()) {
                return Err(rt_err("input.block_interact: only while this mod blocks the player's actions"));
            }
            input_block::set_interact_blocked(on);
            Ok(())
        })?,
    )?;
    n.set(
        "input_held",
        lua.create_function(|_, name: String| {
            let lr = PAD_RAW.load(Ordering::Relaxed) | KEYS_LEFT_RIGHT.load(Ordering::Relaxed);
            Ok(match name.as_str() {
                "left" => lr & PAD_LEFT != 0,
                "right" => lr & PAD_RIGHT != 0,
                "action" => input_block::action_held(),
                _ => return Err(rt_err(format!("input.held: \"left\", \"right\" or \"action\", not {name:?}"))),
            })
        })?,
    )?;
    n.set(
        "input_mouse",
        lua.create_function(|lua, ()| {
            let Some([x, y]) = input_block::mouse_at() else { return Ok(mlua::Value::Nil) };
            let t = lua.create_table()?;
            t.set("x", x.round() as f64)?;
            t.set("y", y.round() as f64)?;
            Ok(mlua::Value::Table(t))
        })?,
    )?;
    n.set("input_using_pad", lua.create_function(|_, ()| Ok(input_block::using_pad()))?)?;
    n.set(
        "input_down",
        lua.create_function(|_, name: String| {
            let input = input_block::input_named(&name)
                .ok_or_else(|| rt_err(format!("input.down: no key or button {name:?} (names: {})", input_block::input_names().join(", "))))?;
            Ok(input_block::input_down(input))
        })?,
    )?;
    n.set("input_press", lua.create_function(|_, (key, seconds): (String, Option<f64>)| input_block::press(&key, seconds.unwrap_or(0.1)).map_err(rt_err))?)?;
    Ok(())
}
