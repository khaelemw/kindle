//! A "Mods" entry in the game's own title menu and pause menu, which opens the native Mods
//! screen (native_mods.rs), or Kindle's own Mods screen (mods_screen.rs) if that can't open.
//! The pause menu's movie is edited for its icon (menu_movies.rs); nothing
//! else of the game's menus changes.
//!
//! Both menus are `AppMenu` command lists (DS3 1.15.2; names from RTTI). Each entry is added
//! with `AddCommand(list, label*, functor*, flags)` (`+0xA4BAA0`, and a byte-identical twin).
//! A label (0x70 bytes) is two parts, the name and the help line, each
//! `{const wchar_t* text, string, bool}`; `+0x7B7830(part, text)` builds one from a literal and
//! keeps the pointer. The functor is an MSVC `std::function`: storage `{vtable, captures}` with
//! the impl pointer at +0x18; vtable slot 2 is `call(this, job* out, ctx) -> out`, which writes
//! the menu job to run next. Writing null runs nothing and leaves the menu open, as the game's
//! own greyed-out entries do.
//!
//! - Pause menu (`NewIngameTopMenuWindow`): a 6 x 2 grid. The top row has the 5 usual entries
//!   and a sixth, the developers' "Debug" entry, added only when `NewMenuSystem +0x3083` is
//!   set (the game sets it once, when the menu system is created). Kindle sets that byte,
//!   rewrites the entry's text literals and points its functor's call slot at Kindle's. The
//!   game's movie has no icon for that cell and sets it outside the panel; menu_movies.rs gives
//!   it the unused skull from the menu texture and re-centres the row.
//! - Title menu (`TitleTopDialog`): Kindle hooks AddCommand and adds "Mods" right after the
//!   game adds "System", with the same functor type as the pause entry.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::text::wide;
use crate::{hooks, memory, patterns, ui_bridge};

/// Set at startup from kindle.toml (overlay enabled and `game_menus`).
pub static ENABLED: AtomicBool = AtomicBool::new(false);

/// Pause-menu builder where the "Debug" entry is added: `call` the menu system getter (+0),
/// test the flag, the entry's functor vtable (`lea` at +0x31), its label builder (`call` at
/// +0x48).
const PAUSE_DEBUG_SITE: &str = "e8 ?? ?? ?? ?? 80 b8 83 30 00 00 00 0f 84 ?? ?? ?? ?? 48 8d 45 b8 48 89 44 24 30 \
    48 8d 45 b8 48 89 44 24 58 48 8d 45 b8 48 89 44 24 48 4c 89 6d b8 48 8d 05 ?? ?? ?? ?? 48 89 45 b8 \
    48 8d 45 b8 48 89 45 d0 48 8d 4d 20 e8";
const DEBUG_VTABLE_AT: usize = 0x31;
const DEBUG_LABEL_CALL_AT: usize = 0x48;
/// NewMenuSystem: "add the Debug entry to the pause menu".
const DEBUG_ENTRY_FLAG: usize = 0x3083;
/// In the Debug label builder: `lea rdx, [name]` and `lea rdx, [help]`.
const DEBUG_NAME_LEA_AT: usize = 0x1E;
const DEBUG_HELP_LEA_AT: usize = 0x2F;

/// One title-menu entry in the title builder (the same code for each entry): its functor
/// vtable (`lea` at +0), label builder (`call` at +0x22) and AddCommand (`call` at +0x4E).
const TITLE_ENTRY_SITE: &str = "48 8d 05 ?? ?? ?? ?? 48 89 44 24 38 48 89 74 24 40 48 8d 44 24 38 48 89 44 24 50 \
    48 8d 8d 70 09 00 00 e8 ?? ?? ?? ?? 48 8b d8 45 33 c0 33 d2 48 8d 4c 24 30 e8 ?? ?? ?? ?? 90 4c 8b 4c 24 30 \
    4c 8d 44 24 38 48 8b d3 48 8d 8d 50 0a 00 00 e8";
const TITLE_LABEL_CALL_AT: usize = 0x22;
const TITLE_ADD_CALL_AT: usize = 0x4E;
/// The "System" entry's label builder, by its text id (401304), 0x1E bytes into it.
const SYSTEM_LABEL_BODY: &str = "ba 98 1f 06 00 e8 ?? ?? ?? ?? 90 48 8d 4b 38 ba 98 1f 06 00 e8";
const SYSTEM_LABEL_BODY_AT: usize = 0x1E;
/// The label constructor's call to the wide string constructor.
pub(crate) const LABEL_STRING_CTOR_AT: usize = 0x32;
/// Label part from a text literal (it constructs a wide string with the `call` at +0x32).
pub(crate) const LABEL_FROM_TEXT: &str = "48 89 4c 24 08 57 48 83 ec 30 48 c7 44 24 28 fe ff ff ff 48 89 5c 24 50 48 8b f9 \
    c7 44 24 20 00 00 00 00 48 89 11 48 8d 59 08 48 89 5c 24 48 48 8b cb e8 ?? ?? ?? ?? 90 c6 43 28 01";

const NAME: &str = "Mods";
const HELP: &str = "Kindle mods";
const LABEL_SIZE: usize = 0x70;
const LABEL_HELP_AT: usize = 0x38;
const FUNCTOR_CALL_SLOT: usize = 0x10;
const FUNCTOR_DELETE_SLOT: usize = 0x20;
const FUNCTOR_IMPL_AT: usize = 0x18;

struct Game {
    /// Static holding the NewMenuSystem instance.
    menu_system: usize,
    debug_vtable: usize,
    debug_label: usize,
    label_from_text: usize,
}

static GAME: OnceLock<Option<Game>> = OnceLock::new();
static TITLE_SYSTEM_VTABLE: OnceLock<usize> = OnceLock::new();
static INSTALLED: OnceLock<bool> = OnceLock::new();
static FLAG_SET: AtomicBool = AtomicBool::new(false);
static RELABELED: AtomicBool = AtomicBool::new(false);

fn game() -> Option<&'static Game> {
    GAME.get_or_init(|| {
        let site = patterns::find_unique("pause menu Debug entry", PAUSE_DEBUG_SITE)?;
        let getter = patterns::call_target(site)?;
        // The getter is `mov rax, [rip+disp]; ret`.
        if memory::read_bytes(getter, 3)? != [0x48, 0x8b, 0x05] || memory::read::<u8>(getter + 7)? != 0xC3 {
            crate::warn!("game menus: the menu system getter looks different; not adding Mods entries");
            return None;
        }
        Some(Game {
            menu_system: patterns::lea_target(getter)?,
            debug_vtable: patterns::lea_target(site + DEBUG_VTABLE_AT)?,
            debug_label: patterns::call_target(site + DEBUG_LABEL_CALL_AT)?,
            label_from_text: patterns::find_unique("menu label from text", LABEL_FROM_TEXT)?,
        })
    })
    .as_ref()
}

/// The game's "label part from a text literal" function.
pub(crate) fn label_from_text() -> Option<usize> {
    game().map(|g| g.label_from_text)
}

/// The vtable of the pause menu entry's functor type: a lambda with no captures, so a functor
/// of Kindle's can use a copy of it (copy and destroy are trivial) with its own call slot.
pub(crate) fn plain_functor_vtable() -> Option<usize> {
    game().map(|g| g.debug_vtable)
}

/// Called every frame (game thread): installs once, then sets the pause flag and the labels
/// as soon as the game's data allows.
pub fn tick() {
    if !ENABLED.load(Ordering::Relaxed) || (FLAG_SET.load(Ordering::Relaxed) && RELABELED.load(Ordering::Relaxed)) {
        return;
    }
    if !*INSTALLED.get_or_init(install) {
        ENABLED.store(false, Ordering::Relaxed);
        return;
    }
    let Some(g) = game() else { return };
    if !FLAG_SET.load(Ordering::Relaxed)
        && let Some(menus) = memory::read_u64(g.menu_system).filter(|&p| p != 0)
    {
        // Read once, when the pause menu is first built.
        unsafe { std::ptr::write_volatile((menus as usize + DEBUG_ENTRY_FLAG) as *mut u8, 1) };
        FLAG_SET.store(true, Ordering::Relaxed);
    }
    if !RELABELED.load(Ordering::Relaxed) && relabel_pause_entry(g) {
        RELABELED.store(true, Ordering::Relaxed);
    }
}

fn install() -> bool {
    let Some(g) = game() else { return false };
    let picked = mods_picked as *const () as usize;
    if !unsafe { hooks::write_ptr(g.debug_vtable + FUNCTOR_CALL_SLOT, picked) } {
        crate::warn!("game menus: could not take over the pause menu entry");
        return false;
    }
    if !hook_title_menu() {
        crate::warn!("game menus: Mods is in the pause menu only");
    }
    crate::info!("game menus: Mods entry added");
    true
}

/// The pause entry's name and help text are literals in the game's data; overwrite them in
/// place (the new text is shorter). False while the label builder's code isn't readable yet.
fn relabel_pause_entry(g: &Game) -> bool {
    let f = g.debug_label;
    let lea_rdx = [0x48, 0x8d, 0x15];
    let looks_right = |at: usize| memory::read_bytes(f + at, 3).is_some_and(|b| b == lea_rdx);
    if !looks_right(DEBUG_NAME_LEA_AT) || !looks_right(DEBUG_HELP_LEA_AT) {
        return false;
    }
    let (Some(name), Some(help)) = (patterns::lea_target(f + DEBUG_NAME_LEA_AT), patterns::lea_target(f + DEBUG_HELP_LEA_AT)) else {
        return false;
    };
    for (at, text) in [(name, NAME), (help, HELP)] {
        let Some(old) = memory::read_wstr(at, 64) else { return true };
        let new: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        if new.len() > old.encode_utf16().count() + 1 {
            crate::warn!("game menus: no room for {text:?} in the pause menu entry");
            continue;
        }
        let bytes: Vec<u8> = new.iter().flat_map(|u| u.to_le_bytes()).collect();
        unsafe { hooks::write_bytes(at, &bytes) };
    }
    true
}

type AddCommandFn = unsafe extern "C" fn(usize, usize, usize, u64) -> usize;
type LabelFromTextFn = unsafe extern "C" fn(usize, *const u16) -> usize;
type DeleteFn = unsafe extern "C" fn(usize, bool);

/// The title's main menu was just built (its commands; party_saves.rs: the main menu is up).
pub static TITLE_MENU_BUILT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static TITLE_ADD_COMMAND_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "game menus",
    name: "title menu add command",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(add_command_hook),
    without: "the title menu has no Mods",
};

fn hook_title_menu() -> bool {
    let Some(system_label) = patterns::find_unique("title menu System label", SYSTEM_LABEL_BODY).map(|a| a - SYSTEM_LABEL_BODY_AT) else {
        return false;
    };
    let Some(site) =
        patterns::find_where("title menu System entry", TITLE_ENTRY_SITE, |a| patterns::call_target(a + TITLE_LABEL_CALL_AT) == Some(system_label))
    else {
        return false;
    };
    let (Some(vtable), Some(add)) = (patterns::lea_target(site), patterns::call_target(site + TITLE_ADD_CALL_AT)) else {
        return false;
    };
    let _ = TITLE_SYSTEM_VTABLE.set(vtable);
    match TITLE_ADD_COMMAND_SITE.hook_at(add) {
        Ok(h) => {
            std::mem::forget(h); // for the life of the process
            true
        }
        Err(_) => false,
    }
}

/// The vtable of the functor a command is being added with.
fn functor_vtable(functor: usize) -> Option<usize> {
    let imp = memory::read_u64(functor + FUNCTOR_IMPL_AT)? as usize;
    memory::read_u64(imp).map(|v| v as usize)
}

fn add_command_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (list, label, functor, flags) = (r.rcx as usize, r.rdx as usize, r.r8 as usize, r.r9);
    let add: AddCommandFn = unsafe { std::mem::transmute(original) };
    // AddCommand moves the functor out, so look at it first.
    let after_system = TITLE_SYSTEM_VTABLE.get().is_some_and(|&vt| functor_vtable(functor) == Some(vt));
    let result = unsafe { add(list, label, functor, flags) };
    if after_system {
        TITLE_MENU_BUILT.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = std::panic::catch_unwind(|| {
            static TEXT: OnceLock<(&'static [u16], &'static [u16])> = OnceLock::new();
            let (name, help) = *TEXT.get_or_init(|| (wide(NAME), wide(HELP)));
            if let Some(g) = game() {
                add_entry(add, list, name, help, g.debug_vtable);
            }
            add_title_entries(add, list);
        });
    }
    result
}

// ── mods' title entries ──────────────────────────────────────────────────────────────────────
//
// A mod's entry on the title's main menu, after Mods (kindle.ui.title_entry), opening the Mods
// screen with only its tab (native_mods::open_mod_screen), or running the mod's own function at
// the next frame (a screen of its own, kindle.ui.screen, over the menu). Each place has a functor
// type of its own (char_make::functor_vtable), whose call knows its place.

const TITLE_ENTRIES: usize = 2;
const TITLE_LABEL_MAX: usize = 24;
const TITLE_HELP_MAX: usize = 120;

struct TitleEntry {
    owner: String,
    label: &'static [u16],
    help: &'static [u16],
    /// The mod runs its own function when it's picked (not its Mods tab).
    own: bool,
}

static TITLE_ENTRY_LIST: Mutex<Vec<TitleEntry>> = Mutex::new(Vec::new());
/// Title entries picked whose mod runs its own function: their owners, for the next frame.
static TITLE_PICKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The mods whose own title entry was picked since the last frame (frame.rs runs their function).
pub fn take_title_picks() -> Vec<String> {
    std::mem::take(&mut *TITLE_PICKED.lock().unwrap_or_else(|e| e.into_inner()))
}
static TITLE_VTABLES: [OnceLock<Option<usize>>; TITLE_ENTRIES] = [OnceLock::new(), OnceLock::new()];
const TITLE_CALLS: [crate::char_make::CallFn; TITLE_ENTRIES] = [title_entry_picked::<0>, title_entry_picked::<1>];

fn title_entries() -> std::sync::MutexGuard<'static, Vec<TitleEntry>> {
    TITLE_ENTRY_LIST.lock().unwrap_or_else(|e| e.into_inner())
}

/// A mod's title entry, replacing its earlier one (shown the next time the main menu is built).
pub fn set_title_entry(owner: &str, label: &str, help: &str, own: bool) -> Result<(), String> {
    if label.is_empty() || label.chars().count() > TITLE_LABEL_MAX || help.chars().count() > TITLE_HELP_MAX {
        return Err(format!("a title entry's label is 1 to {TITLE_LABEL_MAX} characters, its help at most {TITLE_HELP_MAX}"));
    }
    let mut entries = title_entries();
    if let Some(e) = entries.iter_mut().find(|e| e.owner == owner) {
        (e.label, e.help, e.own) = (wide(label), wide(help), own);
        return Ok(());
    }
    if entries.len() >= TITLE_ENTRIES {
        let others: Vec<&str> = entries.iter().map(|e| e.owner.as_str()).collect();
        return Err(format!("the title menu has room for {TITLE_ENTRIES} mods' entries, taken by {}", others.join(", ")));
    }
    crate::info!("game menus: {owner}'s title entry {label:?}");
    entries.push(TitleEntry { owner: owner.to_string(), label: wide(label), help: wide(help), own });
    Ok(())
}

/// A mod unloads (or takes its entry away).
pub fn remove_title_entry(owner: &str) {
    title_entries().retain(|e| e.owner != owner);
}

fn add_title_entries(add: AddCommandFn, list: usize) {
    let texts: Vec<(&'static [u16], &'static [u16])> = title_entries().iter().map(|e| (e.label, e.help)).collect();
    for (place, (name, help)) in texts.into_iter().enumerate().take(TITLE_ENTRIES) {
        let Some(vtable) = *TITLE_VTABLES[place].get_or_init(|| crate::char_make::functor_vtable(TITLE_CALLS[place])) else { return };
        add_entry(add, list, name, help, vtable);
    }
}

/// A title entry's call: the mod's screen's job, else none (the menu stays); a mod with a
/// function of its own has it run at the next frame, the menu staying.
extern "C" fn title_entry_picked<const PLACE: usize>(_this: usize, out: *mut usize, ctx: usize) -> *mut usize {
    let entry = title_entries().get(PLACE).map(|e| (e.owner.clone(), e.own));
    match entry {
        Some((owner, true)) => {
            // The menu calls an entry's functor twice for one pick (seen 2026-11-05, both in the
            // same frame): one run of the mod's function per pick, or two screens opened and
            // closing one showed the other.
            let mut picked = TITLE_PICKED.lock().unwrap_or_else(|e| e.into_inner());
            if !picked.contains(&owner) {
                picked.push(owner);
            }
        }
        Some((owner, false)) => {
            if let Some(job) = crate::native_mods::open_mod_screen(out, ctx, &owner) {
                return job;
            }
        }
        None => {}
    }
    if !out.is_null() {
        unsafe { *out = 0 };
    }
    out
}

/// Adds an entry to the command list being built (title menu, after System), its functor of the
/// type `vtable` (no captures).
fn add_entry(add: AddCommandFn, list: usize, name: &'static [u16], help: &'static [u16], vtable: usize) {
    let Some(g) = game() else { return };

    #[repr(C, align(16))]
    struct Label([u8; LABEL_SIZE]);
    let mut label = Label([0; LABEL_SIZE]);
    let at = label.0.as_mut_ptr() as usize;
    let from_text: LabelFromTextFn = unsafe { std::mem::transmute(g.label_from_text) };
    unsafe {
        from_text(at, name.as_ptr());
        from_text(at + LABEL_HELP_AT, help.as_ptr());
    }

    // The pause entry's functor type (no captures); its call slot is Kindle's.
    #[repr(C)]
    struct Functor {
        vtable: usize,
        captures: [usize; 2],
        imp: usize,
    }
    let mut functor = Functor { vtable, captures: [0; 2], imp: 0 };
    functor.imp = &raw const functor as usize;
    let storage = &raw const functor as usize;
    unsafe { add(list, at, storage, 0) };
    // AddCommand moves the functor out (clearing `imp`); if it didn't, destroy ours in place
    // as the game does. The label parts only point at Kindle's text and own nothing.
    let imp = unsafe { std::ptr::read_volatile(&raw const functor.imp) };
    if imp != 0
        && let Some(delete) = memory::read_u64(vtable + FUNCTOR_DELETE_SLOT)
    {
        let delete: DeleteFn = unsafe { std::mem::transmute(delete as usize) };
        unsafe { delete(imp, imp != storage) };
    }
}

/// The entry's functor call: the native Mods screen's job, else open Kindle's Mods screen and
/// run no menu job.
extern "C" fn mods_picked(_this: usize, out: *mut usize, ctx: usize) -> *mut usize {
    if let Some(job) = crate::native_mods::open(out, ctx) {
        return job;
    }
    if !out.is_null() {
        unsafe { *out = 0 };
    }
    ui_bridge::OPEN_MODS_SCREEN.store(true, Ordering::Relaxed);
    out
}
