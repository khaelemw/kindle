//! Party saves: a lobby member's character lives with the host's world (the design, 2026-10-13:
//! "think of it as a Minecraft world": the host owns the world; a member's character, its items,
//! progression and stats, is kept with that world, and can't be played without the host). One
//! character per member per host world (2026-10-18).
//!
//! The game names its save file from a prefix it keeps in its save format, "DS3" (DS30000.sl2),
//! and reads it only at "PRESS ANY BUTTON" (the profile: the Load Game list) and as a character
//! loads; its writes go to the same file, each entry in place. A lobby member's game uses the
//! prefix "KP3" instead: its file KP30000.sl2 (next to its own) comes from the host (lobby.rs:
//! the host's mod sends it), or the first time there is none, so the game's main menu offers New
//! Game: the character made there is the member's in that world. Every write to it goes back to
//! the host, which keeps it with its world. The member's own DS30000.sl2 is never written while
//! the party save is in use (verified 2026-10-13 with a game alone: the copy loaded, its writes
//! went to KP30000.sl2, DS30000.sl2's checksum unchanged). What the player is told is the mod's:
//! the events below.
//!
//! The main menu keeps the profile it read at "PRESS ANY BUTTON", and Back doesn't return there,
//! so the prefix is switched at the main menu together with the title's own return to "PRESS ANY
//! BUTTON" (what Quit Game does: the title step 1, its re-init); the next button reads the party
//! save's profile. A member that leaves the party in its world (the co-op mod then quits it to the
//! title, world.quit_to_title) is at "PRESS ANY BUTTON" already: its own save is switched back there.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::lock;
use crate::{memory, patterns};

/// The game's own prefix, and a party save's (the same length: the game keeps it in place).
pub const OWN_PREFIX: &str = "DS3";
pub const PARTY_PREFIX: &str = "KP3";

// ── the save format's prefix ─────────────────────────────────────────────────────────────────
//
// FrpgSaveLoadMan is a static object in the exe's .data (its vtable found by RTTI); +8 its
// SLContentFormat, whose prefix is a std::wstring at +8 (characters inline while it's short,
// its length at +0x18, its capacity at +0x20). The game builds each file name from it as it
// opens the file.

fn prefix_string() -> Option<usize> {
    // Found once it's there: the object is in the exe but the game builds it (its vtable) as it
    // starts, and a file can be opened before that (seen at 0.66 s on a test game), so a miss
    // isn't kept.
    static AT: AtomicUsize = AtomicUsize::new(0);
    static WARNED: AtomicBool = AtomicBool::new(false);
    let mut man = AT.load(Ordering::Relaxed);
    if man == 0 {
        let vt = patterns::find_vtable("FrpgSaveLoadMan@NS_SPRJ")?;
        let (base, _) = memory::game_module()?;
        let (rva, len) = memory::section(".data")?;
        let data = memory::read_bytes(base + rva, len)?;
        let found: Vec<usize> =
            data.as_chunks::<8>().0.iter().enumerate().filter(|(_, q)| u64::from_le_bytes(**q) as usize == vt).map(|(i, _)| base + rva + i * 8).collect();
        match found[..] {
            [at] => {
                man = at;
                AT.store(at, Ordering::Relaxed);
                if WARNED.load(Ordering::Relaxed) {
                    crate::info!("party saves: the game's save manager is there now");
                }
            }
            _ => {
                if !WARNED.swap(true, Ordering::Relaxed) {
                    crate::warn!("party saves: the game's save manager wasn't found ({} matches)", found.len());
                }
                return None;
            }
        }
    }
    let format = memory::read_u64(man + 8).filter(|&p| p != 0)? as usize;
    Some(format + 8)
}

/// The prefix the game names its save file with now.
pub fn prefix() -> Option<String> {
    let s = prefix_string()?;
    let len = memory::read::<u64>(s + 0x10)? as usize;
    let cap = memory::read::<u64>(s + 0x18)? as usize;
    if len > 7 || cap > 7 {
        return None;
    }
    let chars: Vec<u16> = (0..len).map(|i| memory::read::<u16>(s + i * 2).unwrap_or(0)).collect();
    Some(String::from_utf16_lossy(&chars))
}

/// Sets the prefix (one of the same length as the game's, kept in place); whether it did.
fn set_prefix(to: &str) -> bool {
    let Some(s) = prefix_string() else { return false };
    if prefix().is_none_or(|p| p.len() != to.len()) {
        crate::warn!("party saves: the game's save name isn't as expected; not switched");
        return false;
    }
    for (i, c) in to.encode_utf16().enumerate() {
        unsafe { std::ptr::write_volatile((s + i * 2) as *mut u16, c) };
    }
    CHARACTER_PLAYED.store(false, Ordering::Relaxed);
    crate::info!("party saves: the game's save is {to}0000.sl2 now");
    true
}

// ── the title ────────────────────────────────────────────────────────────────────────────────
//
// NS_SPRJ::TitleStep: a step index at +0x10 into its table of steps. 9 is the whole title
// ("PRESS ANY BUTTON", the profile's read, the notices, the main menu and its lists), 4 a
// character picked, 5 the game running; Quit Game runs 1 (the title's re-init), then 9 again.
// Setting 1 at the main menu does the same (verified 2026-10-14: back to "PRESS ANY BUTTON", the
// next button read the profile again). The object comes from the title's step function (9),
// which the game calls with it each frame of the title. The main menu itself: from when the game
// builds its commands (front_menu's hook on the title menu) while the title stays at 9 out of the
// world. (On the loading screen after Continue the step is 4 or 5: never switched there; doing so
// gave a blank character, seen 2026-10-14.)

const TITLE_STEP_FN: &str =
    "40 57 48 83 ec 50 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 68 48 89 74 24 70 0f 29 74 24 40 0f 28 f1 48 8b d9 e8 ?? ?? ?? ?? c6 44 24 60 01";
const STEP: usize = 0x10;
const AT_TITLE: i32 = 9;
const REINIT: i32 = 1;
static TITLE: AtomicUsize = AtomicUsize::new(0);
/// The main menu is up (its commands were built and the title hasn't left step 9 since).
static MENU_UP: AtomicBool = AtomicBool::new(false);

fn title_hook(reg: *mut ilhook::x64::Registers) {
    static VTABLE: OnceLock<Option<usize>> = OnceLock::new();
    let this = unsafe { (*reg).rcx } as usize;
    let Some(vt) = *VTABLE.get_or_init(|| patterns::find_vtable("TitleStep@NS_SPRJ")) else { return };
    if memory::read_u64(this).is_some_and(|v| v as usize == vt) {
        TITLE.store(this, Ordering::Relaxed);
    }
}

fn title_step() -> Option<i32> {
    let t = TITLE.load(Ordering::Relaxed);
    if t == 0 { None } else { memory::read::<i32>(t + STEP) }
}

/// Whether the game shows its main menu (Continue, Load Game, ...).
pub fn at_main_menu() -> bool {
    MENU_UP.load(Ordering::Relaxed)
}

/// Where the title is: 0 not at the title (in the world, or loading), 1 the title before its main
/// menu ("PRESS ANY BUTTON", the profile's read, the notices), 2 the main menu.
fn title_screen() -> i64 {
    if crate::game::in_world() || title_step() != Some(AT_TITLE) {
        0
    } else if at_main_menu() {
        2
    } else {
        1
    }
}

/// kindle.game: where the title is (catalogue.rs).
pub static TITLE_SCREEN: crate::catalogue::Field = crate::catalogue::Field {
    name: "title.screen",
    doc: "Where the game's title is: 0 not at the title (in the world, or loading), 1 the title before its main menu (\"PRESS ANY BUTTON\", the notices), 2 the main menu (Continue or New Game first, Load Game, ...).",
    kind: crate::catalogue::Kind::Int { min: 0, max: 2 },
    per_character: false,
    get: |_| Ok(crate::catalogue::Value::Int(title_screen())),
    set: None,
};

/// kindle.game: the main menu back to "PRESS ANY BUTTON" (catalogue.rs).
pub static TITLE_BACK: crate::catalogue::Function = crate::catalogue::Function {
    name: "title.back_to_press_start",
    doc: "From the main menu back to \"PRESS ANY BUTTON\", as Quit Game goes: the next button reads the save again and the main menu is built anew, its first entry chosen (Continue, or New Game with no character). Whether it went (false away from the main menu).",
    args: &[],
    call: |_| Ok(Some(crate::catalogue::Value::Bool(back_to_press_start()))),
};

/// From the main menu back to "PRESS ANY BUTTON", as Quit Game goes; whether it did.
fn back_to_press_start() -> bool {
    if !at_main_menu() {
        return false;
    }
    let t = TITLE.load(Ordering::Relaxed);
    unsafe { std::ptr::write_volatile((t + STEP) as *mut i32, REINIT) };
    MENU_UP.store(false, Ordering::Relaxed);
    true
}

/// Each frame: the main menu is up from its commands' building while the title stays at step 9.
fn menu_frame() {
    if crate::front_menu::TITLE_MENU_BUILT.swap(false, Ordering::Relaxed) && title_step() == Some(AT_TITLE) {
        if !MENU_UP.swap(true, Ordering::Relaxed) {
            crate::debug!("party saves: the main menu is up");
        }
    } else if MENU_UP.load(Ordering::Relaxed) && (title_step() != Some(AT_TITLE) || crate::game::in_world()) {
        MENU_UP.store(false, Ordering::Relaxed);
        crate::debug!("party saves: the main menu is gone");
    }
}

// ── the files ────────────────────────────────────────────────────────────────────────────────
//
// The game's own file opens (its imports: CreateFileW, WriteFile, CloseHandle) tell where its
// saves are (the folder of the last *.sl2 it opened) and when it has finished writing the party
// save (the handle it wrote to is closed).

static CREATE: AtomicUsize = AtomicUsize::new(0);
static WRITE: AtomicUsize = AtomicUsize::new(0);
static CLOSE: AtomicUsize = AtomicUsize::new(0);
static SAVE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Handles of the party save the game opened, and whether each was written.
static PARTY_HANDLES: Mutex<Vec<(isize, bool)>> = Mutex::new(Vec::new());
/// The game finished writing the party save since the last look.
static WRITTEN: AtomicBool = AtomicBool::new(false);

fn file_name(prefix: &str) -> String {
    format!("{prefix}0000.sl2")
}

unsafe extern "system" fn create_file(name: *const u16, access: u32, share: u32, sa: usize, disp: u32, flags: u32, template: isize) -> isize {
    let real: unsafe extern "system" fn(*const u16, u32, u32, usize, u32, u32, isize) -> isize = unsafe { std::mem::transmute(CREATE.load(Ordering::Relaxed)) };
    let h = unsafe { real(name, access, share, sa, disp, flags, template) };
    if !name.is_null() && h != -1 {
        let mut n = 0;
        while n < 1024 && unsafe { *name.add(n) } != 0 {
            n += 1;
        }
        let path = PathBuf::from(String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(name, n) }));
        if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sl2")) {
            if let Some(dir) = path.parent() {
                *lock(&SAVE_DIR) = Some(dir.to_path_buf());
            }
            if path.file_name().is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(&file_name(PARTY_PREFIX))) {
                lock(&PARTY_HANDLES).push((h, false));
            }
        }
    }
    h
}

unsafe extern "system" fn write_file(h: isize, buf: usize, len: u32, done: *mut u32, overlapped: usize) -> i32 {
    let real: unsafe extern "system" fn(isize, usize, u32, *mut u32, usize) -> i32 = unsafe { std::mem::transmute(WRITE.load(Ordering::Relaxed)) };
    let r = unsafe { real(h, buf, len, done, overlapped) };
    if let Some(e) = lock(&PARTY_HANDLES).iter_mut().find(|e| e.0 == h) {
        e.1 = true;
    }
    r
}

unsafe extern "system" fn close_handle(h: isize) -> i32 {
    let real: unsafe extern "system" fn(isize) -> i32 = unsafe { std::mem::transmute(CLOSE.load(Ordering::Relaxed)) };
    let r = unsafe { real(h) };
    let mut handles = lock(&PARTY_HANDLES);
    if let Some(i) = handles.iter().position(|e| e.0 == h)
        && handles.remove(i).1
    {
        WRITTEN.store(true, Ordering::Relaxed);
    }
    r
}

static TITLE_STEP_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "party saves",
    name: "title step",
    pattern: TITLE_STEP_FN,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::JmpBack(title_hook),
    without: "party saves are off",
};

/// The hooks (once, at startup: before the game reads its profile).
pub fn install() {
    let imports = memory::game_imports("KERNEL32.dll");
    for (name, store, ours) in [
        ("CreateFileW", &CREATE, create_file as *const () as usize),
        ("WriteFile", &WRITE, write_file as *const () as usize),
        ("CloseHandle", &CLOSE, close_handle as *const () as usize),
    ] {
        let Some(&slot) = imports.get(name) else {
            crate::warn!("party saves: the game's {name} wasn't found; party saves are off");
            return;
        };
        let Some(real) = memory::read_u64(slot).filter(|&f| f != 0) else { return };
        store.store(real as usize, Ordering::Relaxed);
        if !unsafe { crate::hooks::write_ptr(slot, ours) } {
            crate::warn!("party saves: couldn't take the game's {name}; party saves are off");
            return;
        }
    }
    TITLE_STEP_SITE.install();
}

fn save_path(prefix: &str) -> Option<PathBuf> {
    lock(&SAVE_DIR).as_ref().map(|d| d.join(file_name(prefix)))
}

// ── a member's party save ────────────────────────────────────────────────────────────────────

enum Due {
    /// The host's copy of this member's party save.
    File(Vec<u8>),
    /// The host has none yet: the first time, no file (the game's New Game makes the character).
    New,
}

/// What the host sent, until it's put in place.
static DUE: Mutex<Option<Due>> = Mutex::new(None);
/// Back to the member's own save at the next main menu (it left the party in the world).
static OWN_DUE: AtomicBool = AtomicBool::new(false);
/// It left the party playing its party character: its own save is back as soon as its game is at
/// "PRESS ANY BUTTON" (where Quit Game goes: the title's re-init), before the next button reads the
/// profile, rather than at the main menu after it (seen 2026-10-19: a member quitting as it left
/// got its party character's main menu first, then "PRESS ANY BUTTON" again).
static OWN_AT_PRESS_START: AtomicBool = AtomicBool::new(false);
/// "Quit to the title" said for this party save.
static TOLD_IN_WORLD: AtomicBool = AtomicBool::new(false);

/// A member: the host's copy of its party save came (None: the host has none yet).
pub fn arrived(file: Option<Vec<u8>>) {
    *lock(&DUE) = Some(file.map_or(Due::New, Due::File));
    TOLD_IN_WORLD.store(false, Ordering::Relaxed);
}

/// Whether this game plays its party save now.
pub fn in_use() -> bool {
    prefix().as_deref() == Some(PARTY_PREFIX)
}

/// A member leaves the party: its own save again (at once at the main menu, else at the next).
/// Playing its party character then, nothing more of it reaches the host: it's told.
pub fn left() {
    *lock(&DUE) = None;
    if in_use() {
        OWN_DUE.store(true, Ordering::Relaxed);
        if crate::game::player_loaded() {
            OWN_AT_PRESS_START.store(true, Ordering::Relaxed);
            crate::catalogue::emit(&LEFT_PLAYING, Vec::new());
        }
    }
}

/// kindle.game: the party save's events (catalogue.rs), for the mod to tell the player.
pub static READY: crate::catalogue::Event = crate::catalogue::Event::new(
    "saves.party_save_ready",
    "This game plays its party save for the lobby host's world now (back at PRESS ANY BUTTON): has_character true, its character there (the main menu's Continue loads it); false, none yet (the main menu offers New Game: the character made is this player's in that world).",
    &["has_character"],
);
pub static WAITING: crate::catalogue::Event = crate::catalogue::Event::new(
    "saves.party_save_waiting",
    "This game's party save for the lobby host's world came while it plays a character: it's put in place at the main menu (quit to the title).",
    &[],
);
pub static LEFT_PLAYING: crate::catalogue::Event = crate::catalogue::Event::new(
    "saves.party_save_left",
    "This game left the lobby while playing its party character: what it does from now on isn't kept with the host's world; its own saves come back at the title (at \"PRESS ANY BUTTON\" after a quit, else at the main menu).",
    &[],
);
pub static OWN_BACK: crate::catalogue::Event = crate::catalogue::Event::new(
    "saves.own_saves_back",
    "This game plays its own saves again (back at PRESS ANY BUTTON) after leaving a lobby; the party save it had went to kindle's backups folder (the host keeps it).",
    &[],
);

/// The opening (the coffin, m40 14000100: a cutscene as a character first stands in the Cemetery
/// of Ash, in its own world) counts as watched only once it finishes. A party member loads its
/// character at home and joins the host a second later: the join cut it off every time, so it
/// started again at every join, and can't be skipped in a session (seen in co-op play, 2026-10-14).
/// A party character at home has it marked watched, as it would be after its first time.
const OPENING_WATCHED: u32 = 14000100;

fn opening_frame() {
    // (In the world first: the save manager is looked for while it isn't found yet.)
    if !crate::game::in_world() || !in_use() {
        return;
    }
    let Some(flags) = crate::ds3::EVENT_FLAG_MAN.instance() else { return };
    if crate::memory::read::<u32>(flags + 0x220) != Some(0) || crate::ds3::get_flag(OPENING_WATCHED) != Ok(false) {
        return;
    }
    if crate::ds3::set_flag(OPENING_WATCHED, true).is_ok() {
        crate::info!("party saves: this party character's opening is marked watched");
    }
}

/// Each frame (the game's thread, in a party or not): a party save due is put in place at the
/// main menu, and a member's own save is back there after it left.
pub fn frame() {
    menu_frame();
    if crate::game::player_loaded() && !CHARACTER_PLAYED.load(Ordering::Relaxed) && in_use() {
        CHARACTER_PLAYED.store(true, Ordering::Relaxed);
    }
    opening_frame();
    remember_continue();
    // (At "PRESS ANY BUTTON" once the quit's save is written: no party save open.)
    let at_press_start = OWN_AT_PRESS_START.load(Ordering::Relaxed) && title_screen() == 1 && lock(&PARTY_HANDLES).is_empty();
    if OWN_DUE.load(Ordering::Relaxed) && (at_main_menu() || at_press_start) {
        OWN_DUE.store(false, Ordering::Relaxed);
        OWN_AT_PRESS_START.store(false, Ordering::Relaxed);
        if set_prefix(OWN_PREFIX) {
            if at_main_menu() {
                back_to_press_start();
            }
            // The party save is the host's: none is left here to play without it. It goes to
            // kindle\backups (where the game doesn't look), a copy of the member's own in case the
            // host's is ever lost.
            if let Some(path) = save_path(PARTY_PREFIX) {
                let to = std::env::current_exe()
                    .ok()
                    .and_then(|e| Some(e.parent()?.join("kindle").join("backups").join(format!("party-{}.sl2", crate::saves::timestamp()))));
                let moved = to.as_ref().is_some_and(|to| to.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && move_file(&path, to).is_ok());
                match (moved, to) {
                    (true, Some(to)) => crate::info!("party saves: {} moved to {} (the host keeps it)", path.display(), to.display()),
                    _ => match std::fs::remove_file(&path) {
                        Ok(()) => crate::info!("party saves: {} removed (the host keeps it)", path.display()),
                        Err(e) => crate::warn!("party saves: couldn't remove {} ({e})", path.display()),
                    },
                }
            }
            crate::catalogue::emit(&OWN_BACK, Vec::new());
        }
    }
    if lock(&DUE).is_some() {
        if at_main_menu() {
            if let Some(due) = lock(&DUE).take() {
                put_in_place(due);
            }
        } else if crate::game::player_loaded() && !in_use() && !TOLD_IN_WORLD.swap(true, Ordering::Relaxed) {
            crate::catalogue::emit(&WAITING, Vec::new());
        }
    }
}

/// A character was played from the party save in use (set as one loads, cleared as the save
/// changes): only then does what the game writes go to the host. The first time, the game makes
/// the file as soon as it reads its profile at the title, with no character in it yet; kept by the
/// host, that empty file came back as the member's "character" the next time (seen with two games on one PC,
/// 2026-10-18).
static CHARACTER_PLAYED: AtomicBool = AtomicBool::new(false);

/// Whether the game has finished writing the party save since the last call, with a character
/// played from it (it goes to the host).
pub fn take_written() -> bool {
    WRITTEN.swap(false, Ordering::Relaxed) && in_use() && CHARACTER_PLAYED.load(Ordering::Relaxed)
}

/// The party save as the game last wrote it.
pub fn read() -> Option<Vec<u8>> {
    let path = save_path(PARTY_PREFIX)?;
    std::fs::read(&path).map_err(|e| crate::warn!("party saves: couldn't read {} ({e})", path.display())).ok()
}

/// Moves a file, across drives too (the game's save folder and its own folder, where kindle's
/// backups are, are often on different drives: a rename fails there).
fn move_file(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// A party save left here goes to kindle's backups folder (where the game doesn't look): the host
/// keeps its own copy.
fn set_aside(path: &std::path::Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let exe = std::env::current_exe()?;
    let dir = exe.parent().ok_or_else(|| std::io::Error::other("no folder"))?.join("kindle").join("backups");
    std::fs::create_dir_all(&dir)?;
    let to = dir.join(format!("party-{}.sl2", crate::saves::timestamp()));
    move_file(path, &to)?;
    crate::info!("party saves: {} moved to {}", path.display(), to.display());
    Ok(())
}

/// Writes the party save next to the member's own, switches the game to it, and has the title
/// read it.
fn put_in_place(due: Due) {
    let Some(party) = save_path(PARTY_PREFIX) else {
        crate::warn!("party saves: the game's save folder isn't known yet");
        return;
    };
    let written = match &due {
        Due::File(bytes) => std::fs::write(&party, bytes),
        // None yet: no file (one left from another world is set aside), so the game's main menu
        // offers New Game and makes the file with the character.
        Due::New => set_aside(&party),
    };
    if let Err(e) = written {
        crate::warn!("party saves: couldn't write {} ({e})", party.display());
        return;
    }
    if set_prefix(PARTY_PREFIX) && back_to_press_start() {
        let has_character = matches!(due, Due::File(_));
        crate::info!("party saves: {} in place ({})", party.display(), if has_character { "the host's copy" } else { "none yet: a new character" });
        crate::catalogue::emit(&READY, vec![crate::catalogue::Value::Bool(has_character)]);
    }
}

// ── the host's copies ────────────────────────────────────────────────────────────────────────
//
// Kept in kindle\parties\<the host's character>\<member id>.sl2, the last one before as .bak,
// and a copy every HISTORY_EVERY in history\<member id>\<date time>.sl2 (the newest HISTORY_KEEP):
// a member's character lives only here once its party ends.
const HISTORY_EVERY: std::time::Duration = std::time::Duration::from_secs(15 * 60);
const HISTORY_KEEP: usize = 24;

fn parties_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("kindle").join("parties"))
}

/// The host's world, for its members' saves: its character's name and save slot (with a
/// character of its own in its world).
pub fn world_key() -> Option<String> {
    if !crate::game::player_loaded() || in_use() {
        return None;
    }
    let slot = crate::bonfire::save_slot().ok()?;
    let name = crate::game::player_game_data().ok()?.player_info().name();
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { '_' }).collect();
    Some(format!("{}_{slot}", clean.trim()))
}

// ── the world Continue loads ─────────────────────────────────────────────────────────────────
//
// The title's Continue loads the character last played, so the world a host's Continue will load
// is the last one it was in with a character of its own: kept in kindle\parties\continue.txt as
// its characters load (across restarts), for its members' party saves to go out before it loads.

/// The world key last written (not to write it every frame).
static CONTINUE_SEEN: Mutex<Option<String>> = Mutex::new(None);

fn continue_path() -> Option<PathBuf> {
    Some(parties_dir()?.join("continue.txt"))
}

fn remember_continue() {
    let Some(world) = world_key() else { return };
    let mut seen = lock(&CONTINUE_SEEN);
    if seen.as_deref() == Some(world.as_str()) {
        return;
    }
    *seen = Some(world.clone());
    let Some(path) = continue_path() else { return };
    let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::write(&path, &world));
    if let Err(e) = written {
        crate::warn!("party saves: couldn't note the world Continue loads ({e})");
    }
}

/// The world this game's title Continue will load (its own characters; at the title), as far as
/// Kindle has seen: the last one played. None if no character of its own has been seen in a world.
pub fn continue_world() -> Option<String> {
    let world = std::fs::read_to_string(continue_path()?).ok()?;
    let world = world.trim();
    (!world.is_empty()).then(|| world.to_string())
}

/// The world a host's party plays in: the one its character is in, or at the title the one its
/// Continue will load.
pub fn host_world() -> Option<String> {
    if crate::game::player_loaded() { world_key() } else { continue_world() }
}

fn copy_path(world: &str, member: u64) -> Option<PathBuf> {
    Some(parties_dir()?.join(world).join(format!("{member:016x}.sl2")))
}

/// The host: its copy of a member's party save for this world, if it has one.
pub fn copy_of(world: &str, member: u64) -> Option<Vec<u8>> {
    std::fs::read(copy_path(world, member)?).ok()
}

/// The host: a member's party save came back; kept (the one before as .bak).
pub fn keep(world: &str, member: u64, bytes: &[u8]) {
    let Some(path) = copy_path(world, member) else { return };
    let kept = (|| {
        std::fs::create_dir_all(path.parent().ok_or("no folder")?).map_err(|e| e.to_string())?;
        if path.exists() {
            std::fs::copy(&path, path.with_extension("sl2.bak")).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("sl2.new");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    })();
    match kept {
        Ok(()) => crate::debug!("party saves: kept {member:#x}'s save for {world} ({} bytes)", bytes.len()),
        Err(e) => crate::warn!("party saves: couldn't keep {member:#x}'s save ({e})"),
    }
    if let Err(e) = history(&path, member, bytes) {
        crate::warn!("party saves: couldn't keep a dated copy of {member:#x}'s save ({e})");
    }
}

/// The host: a dated copy of a member's save, at most every HISTORY_EVERY, the newest HISTORY_KEEP.
fn history(path: &std::path::Path, member: u64, bytes: &[u8]) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?.join("history").join(format!("{member:016x}"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut copies: Vec<PathBuf> =
        std::fs::read_dir(&dir).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "sl2")).collect();
    copies.sort();
    let recent = copies.last().and_then(|p| std::fs::metadata(p).ok()?.modified().ok()?.elapsed().ok()).is_some_and(|age| age < HISTORY_EVERY);
    if recent {
        return Ok(());
    }
    let to = dir.join(format!("{}.sl2", crate::saves::timestamp()));
    std::fs::write(&to, bytes).map_err(|e| e.to_string())?;
    copies.push(to);
    while copies.len() > HISTORY_KEEP {
        let old = copies.remove(0);
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}
