//! The other players of a Kindle session on the HUD: their pings (rows and by their name tags, the
//! overlay draws them), and the hook point ui.hidden_name_tags (no name tag over a character a
//! mod hid).

use crate::catalogue::HookPoint;
use crate::hook_sites::{Kind, Site};
use crate::memory;
use crate::native_ui::{ESC_MENU_AT, MENU_MAN};
use crate::session_players::{character_of_slot, member_slot, members};

/// In a session with other players.
fn together() -> bool {
    crate::p2p::members().is_some_and(|m| m.len() > 1)
}

/// The other players of the session as the HUD lists them (name, ping in ms), made on the
/// game's thread every PING_ROWS_EVERY (the overlay draws them).
static PING_ROWS: std::sync::Mutex<Vec<(String, u32)>> = std::sync::Mutex::new(Vec::new());
static PING_ROWS_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const PING_ROWS_EVERY: std::time::Duration = std::time::Duration::from_secs(1);

/// Each other player's ping by its character's handle (made with the rows), and where the game
/// draws its overhead tag now with that ping (the overlay puts the signal there).
static HANDLE_PINGS: std::sync::Mutex<Vec<(u32, u32)>> = std::sync::Mutex::new(Vec::new());
static TAG_PINGS: std::sync::Mutex<Vec<([f32; 2], u32)>> = std::sync::Mutex::new(Vec::new());
/// The overhead tags (name, HP bar) the HUD builds each frame (Hud_PartnerTags_Build +0x6C62E0):
/// MenuMan +0xD08, 7 records of 0x118 bytes (the accessor +0x761620): +0 and +4 where it's drawn,
/// in the HUD's 1920 x 1080 (its on-screen check, +0x6C74D0, tests those bounds), +0xC 1 while
/// it's shown (on screen, not hidden: a player who is out isn't, see name_tag_hook), +0x8C the
/// character's handle (-1 for none).
const TAGS_AT: usize = 0xD08;
const TAG_SIZE: usize = 0x118;
const TAGS: usize = 7;
const TAG_SHOWN_AT: usize = 0xC;
const TAG_HANDLE_AT: usize = 0x8C;

/// The other players' tags as the HUD shows them now, each with that player's ping: (where in the
/// HUD's 1920 x 1080, ms). Empty while a game menu is open.
pub fn tag_pings() -> Vec<([f32; 2], u32)> {
    lock(&TAG_PINGS).clone()
}

/// Each frame: where the other players' tags are, for their pings.
fn tag_pings_frame() {
    let pings = lock(&HANDLE_PINGS).clone();
    let mut out = Vec::new();
    if !pings.is_empty()
        && !MENU_UP.load(std::sync::atomic::Ordering::Relaxed)
        && let Some(menu) = crate::ds3::singleton_instance(MENU_MAN)
    {
        for i in 0..TAGS {
            let tag = menu + TAGS_AT + i * TAG_SIZE;
            if memory::read::<u8>(tag + TAG_SHOWN_AT).unwrap_or(0) == 0 {
                continue;
            }
            let Some(handle) = memory::read::<u32>(tag + TAG_HANDLE_AT) else { continue };
            let (Some(x), Some(y)) = (memory::read::<f32>(tag), memory::read::<f32>(tag + 4)) else { continue };
            if let Some(&(_, ms)) = pings.iter().find(|p| p.0 == handle)
                && x.is_finite()
                && y.is_finite()
            {
                out.push(([x, y], ms));
            }
        }
    }
    *lock(&TAG_PINGS) = out;
}

use crate::lock;
use crate::native_ui::{NEW_MENU_SYSTEM, OPEN_DIALOG_AT};

/// One of the game's menus is open (the Esc menu, a talk's or another dialog, or the bonfire's:
/// the player sits at one): the HUD's ping rows are hidden.
static MENU_UP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The other players' names and pings (for the overlay and the Co-op tab).
pub fn ping_rows() -> Vec<(String, u32)> {
    lock(&PING_ROWS).clone()
}

/// Each frame: the other players' pings and where their tags are.
pub(crate) fn frame() {
    if !together() {
        lock(&PING_ROWS).clear();
        lock(&HANDLE_PINGS).clear();
        lock(&TAG_PINGS).clear();
        return;
    }
    ping_rows_frame();
    tag_pings_frame();
}

/// Whether the HUD's ping rows are hidden now (a game menu is open).
pub fn ping_rows_hidden() -> bool {
    MENU_UP.load(std::sync::atomic::Ordering::Relaxed)
}

fn ping_rows_frame() {
    let esc = crate::ds3::singleton_instance(MENU_MAN).and_then(|m| memory::read::<i32>(m + ESC_MENU_AT)).is_some_and(|r| r != 0);
    let dialog = crate::ds3::singleton_instance(NEW_MENU_SYSTEM).and_then(|m| memory::read_u64(m + OPEN_DIALOG_AT)).is_some_and(|d| d != 0);
    let resting = crate::game::player_chr_addr().ok().and_then(crate::chars::animation_name).is_some_and(|a| a.contains("Bonfire"));
    MENU_UP.store(esc || dialog || resting, std::sync::atomic::Ordering::Relaxed);
    {
        let mut at = lock(&PING_ROWS_AT);
        if at.is_some_and(|t| t.elapsed() < PING_ROWS_EVERY) {
            return;
        }
        *at = Some(std::time::Instant::now());
    }
    let mut rows = Vec::new();
    let mut handles = Vec::new();
    if crate::p2p::hosting().is_some() && crate::game::in_world() {
        let me = crate::p2p::me();
        let pings = crate::p2p::pings();
        let ping = |id: u64| pings.iter().find(|p| p.0 == id).map(|p| p.1);
        let all = members();
        let host = all.iter().find(|m| m.host).map(|m| m.steam);
        for m in all.iter().filter(|m| Some(m.steam) != me) {
            // A player: the host's ping is its own to the host; another player's adds its own.
            let ms = match (crate::p2p::hosting(), host) {
                (Some(false), Some(h)) if m.steam != h => ping(m.steam).zip(ping(h)).map(|(a, b)| a + b),
                _ => ping(m.steam),
            };
            let Some(ms) = ms else { continue };
            let chr = member_slot(m.addr + 0x68).and_then(character_of_slot);
            if let Some(handle) = chr.and_then(|c| memory::read::<u32>(c + 8)) {
                handles.push((handle, ms));
            }
            let name =
                chr.and_then(|c| unsafe { crate::ds3::PlayerIns::at(c) }.game_data().map(|d| d.player_info().name())).unwrap_or_else(|| "Player".to_string());
            rows.push((name, ms));
        }
    }
    *lock(&PING_ROWS) = rows;
    *lock(&HANDLE_PINGS) = handles;
}

/// The tags over the other players (their name, "Phantom" or "Summoner", and a health bar): each
/// frame the HUD (+0x6C62E0) goes through the game's characters, looks each one up by its handle
/// and gives none to one that isn't found (or is dead, or leaving). A character not drawn
/// (world.set_visible false: ChrIns +0x1A09 bit 5 off) still had its tag, floating where it is,
/// with an empty health bar; the match is that lookup, the hook after it (at +8) makes such a
/// character not found.
const NAME_TAGS: &str =
    "8b 55 80 e8 ?? ?? ?? ?? 4c 8b f8 48 85 c0 0f 84 ?? ?? ?? ?? 48 8b 10 48 8b c8 ff 92 a8 01 00 00 84 c0 0f 84 ?? ?? ?? ?? ba 80 25 00 00";

fn name_tag_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &mut *reg };
    if r.rax == 0 || HIDDEN_NAME_TAGS.switch() != 1 {
        return;
    }
    if crate::chars::drawn(r.rax as usize) == Some(false) {
        r.rax = 0;
    }
}

static NAME_TAGS_SITE: Site = Site {
    area: "ui",
    name: "name tags",
    pattern: NAME_TAGS,
    offset: 8,
    matches: 1,
    kind: Kind::JmpBack(name_tag_hook),
    without: "a hidden character keeps its name tag",
};

/// kindle.game: ui.hidden_name_tags (catalogue.rs).
pub static HIDDEN_NAME_TAGS: HookPoint = HookPoint::hooked(
    "ui.hidden_name_tags",
    "The HUD's name tags over other players: a character not drawn (world.set_visible false) keeps its tag, floating with an empty health bar: \"game\", or \"hide\" (no tag over it). Hot: a switch.",
    &["game", "hide"],
    || NAME_TAGS_SITE.install().map(|_| ()).ok_or_else(|| NAME_TAGS_SITE.without.to_string()),
);
