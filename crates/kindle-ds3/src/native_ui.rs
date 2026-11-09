//! The game's own UI, as opposed to Kindle's overlay.
//!
//! Message box: the game's generic "OK" dialog. Located with the AOB from the Grand Archives
//! CE table (credited there to Luke_Yui), unique in 1.15.2 at `+0x75E610`. Its argument is a
//! `va_list` holding one UTF-16 string: the game formats it into a fixed template (`%s`) in a
//! 0x1000-character buffer, so the text is copied immediately. The function quietly does
//! nothing while some menus are open (it checks the menu state first).

use std::sync::OnceLock;

use crate::patterns;

const MESSAGE_PATTERN: &str = "40 57 48 83 ec ?? 48 c7 44 24 ?? fe ff ff ff 48 89 5c 24 ?? 48 8b f9 48 8b 05 ?? ?? ?? ?? 83 b8";
const MAX_CHARS: usize = 1000;

static MESSAGE: OnceLock<Option<usize>> = OnceLock::new();

/// Shows `text` in the game's own message dialog.
pub fn message(text: &str) -> Result<(), String> {
    crate::game::player_chr_addr()?;
    let f = MESSAGE.get_or_init(|| patterns::find_unique("message box", MESSAGE_PATTERN)).ok_or("the game's message box is unavailable on this game build")?;
    let mut wide: Vec<u16> = text.encode_utf16().take(MAX_CHARS).collect();
    wide.push(0);
    let args: [*const u16; 1] = [wide.as_ptr()];
    let show: unsafe extern "C" fn(*const [*const u16; 1]) = unsafe { std::mem::transmute(f) };
    unsafe { show(&args) };
    Ok(())
}

/// The game's place-name text Kindle shows its own banner text with (FMG "place_name").
const BANNER_TEXT_ID: u32 = 9_900_001;

/// The game's own banner across the screen with `text` (the area welcome message, event command
/// 2007[11], as "Firelink Shrine" shows on arriving), through a place name of Kindle's holding the
/// text (verified 2026-10-11: the game showed Kindle's text in its banner). Its font has no colon.
pub fn place_banner(owner: &str, text: &str) -> Result<(), String> {
    let cats = crate::text_override::category_ids("place_name")?;
    crate::text_override::set_or_add(&cats, BANNER_TEXT_ID, text, owner)?;
    let args = (BANNER_TEXT_ID as i32).to_le_bytes().to_vec();
    crate::emevd::run(crate::emevd::Command { id: 0, owner: owner.into(), bank: 2007, index: 11, args })
}

/// The dialog the game's menu system has open (NewMenuSystem +0xA0; null with none): what its
/// own check for an open menu reads (CheckSpecificPersonMenuIsOpen(-1), +0xEFA160 -> +0xBEA840).
pub(crate) const NEW_MENU_SYSTEM: &str = "NewMenuSystem";
pub(crate) const OPEN_DIALOG_AT: usize = 0xA0;

/// Whether a dialog of the game's menu system is open (the Esc menu, a talk's or a bonfire's menu,
/// the level up screen...), as the game's own check sees it.
pub fn menu_open() -> bool {
    crate::ds3::singleton_instance(NEW_MENU_SYSTEM).and_then(|m| crate::memory::read_u64(m + OPEN_DIALOG_AT)).is_some_and(|d| d != 0)
}

/// kindle.game: whether one of the game's menus is open (catalogue.rs).
pub static MENU_OPEN: crate::catalogue::Field = crate::catalogue::Field {
    name: "ui.menu_open",
    doc: "Whether one of the game's menus is open (the Esc menu, a bonfire's or a talk's menu, the level up screen...), as the game's own check sees it.",
    kind: crate::catalogue::Kind::Bool,
    per_character: false,
    get: |_| Ok(crate::catalogue::Value::Bool(menu_open())),
    set: None,
};

/// MenuMan: the action prompt shown now (its text id, -1 for none), the Esc menu's request (2
/// while it's open) and request 0x3F (+0x34 + 0x3F * 4: 1 while an item popup is up). Verified
/// 2026-10-11 with screenshots: "Read message" 50000210, "Rest at bonfire" 10010110, none -1 (also
/// while the Esc menu is open); the request 0 closed, 2 open.
pub(crate) const MENU_MAN: &str = "MenuMan";
pub(crate) const ACTION_PROMPT_AT: usize = 0x82C;
pub(crate) const ESC_MENU_AT: usize = 0x54;
const ITEM_POPUP_AT: usize = 0x130;

/// Whether the Esc menu (the game's main in-game menu) is open.
pub fn esc_menu_open() -> bool {
    menu_man_i32(ESC_MENU_AT).is_ok_and(|r| r != 0)
}

fn menu_man_i32(at: usize) -> Result<i64, String> {
    let m = crate::ds3::singleton_instance(MENU_MAN).ok_or("the game's menus aren't there yet")?;
    crate::memory::read::<i32>(m + at).map(i64::from).ok_or_else(|| "the game's menus are unreadable".into())
}

/// kindle.game: the game's action prompt (catalogue.rs).
pub static ACTION_PROMPT: crate::catalogue::Field = crate::catalogue::Field {
    name: "ui.action_prompt",
    doc: "The action prompt the game shows now (\"Rest at bonfire\", \"Pick up\"...), as its text id (menu text), or -1 for none (also while the Esc menu is open).",
    kind: crate::catalogue::Kind::Int { min: i32::MIN as i64, max: i32::MAX as i64 },
    per_character: false,
    get: |_| menu_man_i32(ACTION_PROMPT_AT).map(crate::catalogue::Value::Int),
    set: None,
};

/// kindle.game: the Esc menu (catalogue.rs).
pub static ESC_MENU_OPEN: crate::catalogue::Field = crate::catalogue::Field {
    name: "ui.esc_menu_open",
    doc: "Whether the Esc menu (the game's main in-game menu) is open.",
    kind: crate::catalogue::Kind::Bool,
    per_character: false,
    get: |_| menu_man_i32(ESC_MENU_AT).map(|r| crate::catalogue::Value::Bool(r != 0)),
    set: None,
};

/// MenuMan's request 0x87 (+0x34 + 0x87 * 4): Quit Game. The pause menu's System tab sets it to 1
/// on its Quit Game dialog's yes (text 16500, FrpgMenuDlgSystem +0x754E60), and so does the event
/// command 2003[60] (+0x755320, nothing else in it); the world's step reads it each frame
/// (+0x8F5D20, MoveMapStep's end-of-world check): the screen fades, the character is saved, the
/// world ends and the title opens at its main menu, and the step clears it.
const QUIT_GAME_AT: usize = 0x34 + 0x87 * 4;

/// kindle.game: the pause menu's Quit Game (catalogue.rs).
pub static QUIT_TO_TITLE: crate::catalogue::Function = crate::catalogue::Function {
    name: "world.quit_to_title",
    doc: "Quits to the title's main menu as the pause menu's Quit Game does: the screen fades, the character is saved and the world ends. Whether it was asked (false out of the world).",
    args: &[],
    call: |_| quit_to_title().map(|asked| Some(crate::catalogue::Value::Bool(asked))),
};

fn quit_to_title() -> Result<bool, String> {
    if !crate::game::player_loaded() {
        return Ok(false);
    }
    let m = crate::ds3::singleton_instance(MENU_MAN).ok_or("the game's menus aren't there yet")?;
    if crate::memory::read::<i32>(m + QUIT_GAME_AT) != Some(1) {
        unsafe { std::ptr::write_volatile((m + QUIT_GAME_AT) as *mut i32, 1) };
        crate::info!("ui: quit to the title");
    }
    Ok(true)
}

/// kindle.game: an item popup (catalogue.rs).
pub static ITEM_POPUP: crate::catalogue::Field = crate::catalogue::Field {
    name: "ui.item_popup",
    doc: "Whether an item popup (an item received, its OK shown as an action prompt) is up.",
    kind: crate::catalogue::Kind::Bool,
    per_character: false,
    get: |_| menu_man_i32(ITEM_POPUP_AT).map(|r| crate::catalogue::Value::Bool(r != 0)),
    set: None,
};
