//! Elden Ring's own on-screen messages for mods (kindle.ui): a mod's text shown by the game's message
//! commands (bank 2007), as its event scripts show theirs; and Kindle's own, drawn by the overlay
//! (overlay.rs): banners, toasts, a prompt and a status line, as Kindle for Dark Souls III's.
//!
//! Those commands show an entry of EventTextForMap (FMG 34, text.rs's "event_text_map") by id: Kindle
//! adds the mod's text there under an id of its own (above the game's, which end at 20,208,036 in 2.7.0.0) and
//! runs the command with it. Ids are taken in turn from a few, so a message still on screen keeps its
//! text while the next is shown. The text goes in first: an added entry lands on the game's thread at
//! Kindle's next frame (text::tick), before that frame's event commands run.

use std::sync::atomic::{AtomicU32, Ordering};

use kindle_core::ui_bridge::{self, NoticeKind};

use crate::{emevd, text};

/// Kindle's ids in EventTextForMap, and how many it takes in turn.
const FIRST_ID: u32 = 2_000_000_000;
const IDS: u32 = 16;

static NEXT: AtomicU32 = AtomicU32::new(0);

/// The game's ways of showing a message: (bank, index, argument format after the text id, its values).
fn command(kind: &str) -> Result<(i32, i32, &'static str, &'static [f64]), String> {
    Ok(match kind {
        // A line in the middle of the screen that fades ("Somewhere, a heavy door has opened").
        "blinking" => (2007, 4, "", &[]),
        // A line across the whole screen, the game paused under it until dismissed.
        "full_screen" => (2007, 9, "", &[]),
        // A status line, as a pad prompt shows one (pad state 0).
        "status" => (2007, 3, "B", &[0.0]),
        // The name of a place, as entering one shows it.
        "area" => (2007, 14, "", &[]),
        "subarea" => (2007, 13, "", &[]),
        k => return Err(format!("no message kind {k:?} (blinking, full_screen, status, area, subarea)")),
    })
}

/// The command that shows `message` as the game's message of this kind, its text added first.
pub fn message(kind: &str, message: &str, owner: &str) -> Result<emevd::Command, String> {
    let (bank, index, format, rest) = command(kind)?;
    let id = FIRST_ID + NEXT.fetch_add(1, Ordering::Relaxed) % IDS;
    text::set_or_add(&text::category("event_text_map")?, id, message, owner)?;
    let mut values = vec![id as f64];
    values.extend_from_slice(rest);
    let args = kindle_core::emevd::pack(&format!("i{format}"), &values)?;
    Ok(emevd::Command { id: emevd::next_id(), owner: owner.into(), bank, index, args, prepare: None })
}

/// The natives for kindle.ui.
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::{ctx, rt_err};
    n.set(
        "ui_message",
        lua.create_function(|lua, (kind, text, reply): (String, String, Option<mlua::Function>)| {
            let owner = ctx(lua)?.id.clone();
            let cmd = message(&kind, &text, &owner).map_err(rt_err)?;
            let id = cmd.id;
            emevd::run(cmd).map_err(rt_err)?;
            if let Some(f) = reply {
                ctx(lua)?.handlers.replies.push((id, f));
            }
            Ok(id)
        })?,
    )?;
    n.set(
        "ui_notice",
        lua.create_function(|lua, (kind, msg, seconds, color): (String, String, f32, String)| {
            let kind = if kind == "banner" { NoticeKind::Banner } else { NoticeKind::Toast };
            let seconds = if seconds.is_finite() { seconds.clamp(0.5, 60.0) } else { 3.0 };
            let color = kindle_core::mods::parse_color(&color)?;
            if !ui_bridge::RENDERER_ACTIVE.load(Ordering::Relaxed) {
                // No overlay: at least the log has it.
                let id = ctx(lua)?.id.clone();
                kindle_core::log::write(
                    kindle_core::log::Level::Info,
                    &id,
                    &format!("[{}] {msg}", if kind == NoticeKind::Banner { "banner" } else { "toast" }),
                );
            }
            ui_bridge::push_notice(kind, msg, color, seconds);
            Ok(())
        })?,
    )?;
    n.set(
        "ui_status",
        lua.create_function(|lua, (text, progress): (Option<String>, Option<f32>)| {
            let status = text.map(|t| (t, progress.filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0))));
            ui_bridge::set_status(&ctx(lua)?.id, status);
            Ok(())
        })?,
    )?;
    n.set(
        "ui_prompt",
        lua.create_function(|lua, text: Option<String>| {
            ui_bridge::set_prompt(&ctx(lua)?.id, text);
            Ok(())
        })?,
    )?;
    Ok(())
}
