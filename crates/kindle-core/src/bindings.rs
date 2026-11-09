//! What Kindle's mod runtime asks of the game it runs in. Each game's runtime (kindle.dll for Dark Souls III)
//! sets this once as it starts, before any mod's Lua state is made: its half of the Lua API (its natives, and
//! the prelude that wraps them into its modules), who the loaded character is, what to undo in the game
//! when a mod unloads, its catalogue and event names, and the console's help.
//!
//! The rest of a game's part is its own code calling Kindle's: its frame (the order a frame's work runs in)
//! and its hooks' dispatch to the mods, through runtime.rs.

use std::sync::OnceLock;

use mlua::{Lua, Table};

use crate::catalogue::Catalogue;
use crate::mods::ModCtx;
use crate::params::{AddRows, ParamCap};

pub struct Bindings {
    /// The game's id, as a mod's `games` in mod.toml names it (config::GAMES): the runtime leaves out the
    /// mods for other games.
    pub game: &'static str,
    /// The game's exe as Kindle's log names it, before an offset in it ("DarkSoulsIII.exe+0x...").
    pub exe: &'static str,
    /// The game's natives, added to the `native` table each Lua state gets (after Kindle's own).
    pub natives: fn(&Lua, &Table) -> mlua::Result<()>,
    /// The Lua side of the game's API: run at the end of Kindle's prelude (kindle-core's prelude.lua, the
    /// sandbox and Kindle's own API) with the native table, the modules the mod may use, the mod's info, the
    /// `kindle` table so far and Kindle's argument check (`check(ok, message)`); it adds the game's modules.
    pub prelude: &'static str,
    /// Run in a state with raw access, after the prelude (the test rig's kindle.rig).
    pub raw_state: fn(&Lua) -> mlua::Result<()>,
    /// The loaded character's save slot (None with no character in the world), for a mod's character store.
    pub character_slot: fn() -> Result<Option<i32>, String>,
    /// The loaded character's name (a character store remembers whose it is).
    pub character_name: fn() -> Result<String, String>,
    /// A character's address from its handle, looked up in the game's lists now (kindle.game's
    /// fields of one character).
    pub character: fn(i64) -> Result<usize, String>,
    /// A mod is unloading: what to undo before its unload handlers run (with its context, if it has one).
    pub unloading: fn(&str, Option<&ModCtx>),
    /// A mod has unloaded: what to undo in the game (param edits, event scripts, map changes, menus...).
    pub unloaded: fn(&mut ModCtx),
    /// The game's param tables now (None until the game has loaded them), for params.rs's registry.
    pub param_caps: fn() -> Option<Vec<ParamCap>>,
    /// The paramdefs the game's DLL embeds (file name, XML), by which params.rs reads its tables.
    pub paramdefs: &'static [(&'static str, &'static str)],
    /// Adds rows to a param table: params.add_rows.
    pub add_param_rows: AddRows,
    /// The game's catalogue (kindle.game).
    pub catalogue: &'static Catalogue,
    /// Events the game's parts raise, besides the runtime's own (events::CORE).
    pub events: &'static [&'static [&'static str]],
    /// The console's `:help`.
    pub console_help: &'static str,
    /// Lua run once in the console's state: its shorthand globals.
    pub console_globals: &'static str,
}

static BINDINGS: OnceLock<&'static Bindings> = OnceLock::new();

/// Sets the game's bindings, once (later calls change nothing).
pub fn set(bindings: &'static Bindings) {
    let _ = BINDINGS.set(bindings);
}

/// The game's bindings (None until the game's runtime sets them).
pub fn get() -> Option<&'static Bindings> {
    BINDINGS.get().copied()
}

/// The game's exe as Kindle's log names it ("the game" before the game's runtime sets its bindings).
pub fn exe() -> &'static str {
    get().map_or("the game", |b| b.exe)
}
