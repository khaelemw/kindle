//! Test rig builds only (`--features rig`, never in a release): what running two copies of the
//! game on one PC, driven by scripts, needs. Each switch is off unless the rig's launch sets its
//! environment variable, so a rig build plays as a release does otherwise.

use mlua::{Lua, Table};

/// Whether the rig's launch turned a switch on (an environment variable set).
pub fn switch(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

/// Every rig game is told it's in front (KINDLE_TEST_FOCUS), so each reads its input while the
/// other is.
pub fn focus_spoofed() -> bool {
    switch("KINDLE_TEST_FOCUS")
}

/// A DirectInput read's result on a rig game: a controller (anything but the keyboard's 256 bytes
/// and the mouse's 16 or 20) is gone (DIERR_INPUTLOST). A rig game is always "in front", so it read
/// every controller all the time, and a PS4 pad on USB (its triggers rest at 0, which the game
/// takes as the camera stick held over) spun both cameras nonstop; the rig's own presses go
/// through XInput.
pub fn direct_input_read(size: u64, result: usize) -> usize {
    if focus_spoofed() && ![256, 16, 20].contains(&size) { 0x8007_001E } else { result }
}

/// At startup: a second copy of the game runs (KINDLE_SECOND_GAME), its check for one already
/// running (GetLastError() == ERROR_ALREADY_EXISTS after its mutex) passed over.
pub fn at_start() {
    if switch("KINDLE_SECOND_GAME")
        && let Some(at) = crate::patterns::find_unique("second game check", "ff 15 ?? ?? ?? ?? 3d b7 00 00 00 75 4e 48 89 5c 24 70")
    {
        unsafe { crate::hooks::write_bytes(at + 11, &[0xEB]) };
        crate::info!("rig: a second copy of the game may run");
    }
}

/// `kindle.rig` in the console's and developer mods' Lua states.
pub fn install(lua: &Lua) -> mlua::Result<()> {
    let kindle: Table = lua.globals().get("kindle")?;
    let rig = lua.create_table()?;
    // A flag set as the game's own code sets one (reported by flags.changed, which leaves
    // kindle.flags.set out): a scenario stands in for a summon sign's or a lever's set.
    rig.set(
        "set_flag_as_game",
        lua.create_function(|_, (flag, on): (i64, bool)| {
            let id = crate::ds3::check_flag(flag).map_err(mlua::Error::runtime)?;
            crate::ds3::set_flag_as_game(id, on).map_err(mlua::Error::runtime)
        })?,
    )?;
    // A MenuMan request (+0x34 + id * 4) set to a value, as the game's code sets them.
    rig.set(
        "menu_request",
        lua.create_function(|_, (id, value): (usize, i32)| {
            let m = crate::ds3::singleton_instance(crate::native_ui::MENU_MAN).ok_or_else(|| mlua::Error::runtime("no MenuMan"))?;
            let at = m + 0x34 + id.min(0x400) * 4;
            let before = crate::memory::read::<i32>(at).ok_or_else(|| mlua::Error::runtime("unreadable"))?;
            unsafe { std::ptr::write_volatile(at as *mut i32, value) };
            Ok(before)
        })?,
    )?;
    kindle.set("rig", rig)
}
