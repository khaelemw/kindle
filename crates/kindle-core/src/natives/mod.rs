//! The `native` table each mod's Lua state gets: the Rust side of the `kindle` API. Kindle's own
//! natives are here (logging, events, timers, the store, mods calling each other, the catalogue,
//! changes to the game's event files);
//! the game's runtime adds its own (bindings.rs), and its prelude wraps them all into the public
//! modules and checks permissions. One file per area.

mod core;
mod event_files;
pub mod game;
mod interop;
mod params;
mod store;

use mlua::{Lua, Table};

/// Everything the natives use, in one place.
mod prelude {
    #![allow(unused_imports)]
    pub(super) use std::sync::atomic::Ordering;
    pub(super) use std::time::{Duration, Instant};

    pub(super) use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};

    pub(super) use crate::log::{self, Level};
    pub(super) use crate::mods::*;
    pub(super) use crate::ui_bridge::{self, NoticeKind};
}

/// The `native` table for one Lua state: Kindle's natives, then the game's.
pub fn build(lua: &Lua) -> mlua::Result<Table> {
    let n = lua.create_table()?;
    core::register(lua, &n)?;
    interop::register(lua, &n)?;
    store::register(lua, &n)?;
    event_files::register(lua, &n)?;
    game::register(lua, &n)?;
    params::register(lua, &n)?;
    if let Some(b) = crate::bindings::get() {
        (b.natives)(lua, &n)?;
    }
    Ok(n)
}
