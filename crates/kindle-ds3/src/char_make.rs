//! The character creation screen (`CharMakeTopDialog`): mods' rows under the game's own
//! (`kindle.ui.character_rows`): a row opens a page of switches (the System screen with the mod's
//! tabs, native_mods.rs) or the game's text box, for the character being made. Mods read what
//! was chosen (`kindle.ui.character_choices`) when they see the new character.
//!
//! The screen's left list (Name, Gender, Age, Class, Burial Gift, Face Presets, Build,
//! Appearance) is built by chaining `AddRow(list, label*, functor*, style) -> list` (1.15.2
//! +0xADE0F0), one call per row in screen order; then the list's +0xF80 flag is set and its
//! button, Finalize creation, is added with the menus' AddCommand. A label is the same 0x70-byte
//! name + help pair as the title menu's (front_menu.rs); the style an 8-byte value (all rows pass
//! 0). Kindle hooks AddRow and, right after the game adds Appearance (its label's text is the
//! game's menu text 402016), adds the mods' rows. The list has as many places as its movie has
//! row items, so menu_movies.rs adds ROWS places (the game reads the movie before mods load, so
//! the count is fixed); without them the list would scroll and its button run one of Kindle's
//! rows, so the rows are added only when the game got that edit. A place no mod's row takes gets
//! a blank row that does nothing.
//!
//! A row's functor is an MSVC `std::function` (impl pointer at +0x18). AddRow clones it through
//! its vtable's copy slot, and a clone takes whatever vtable copy writes, so each of Kindle's rows
//! has a functor type of its own: a copy of a captureless lambda's vtable (the pause menu
//! entry's) with Kindle's copy, move, call and destroy (with the game's copy, the clone was the
//! pause entry's again and opened the Mods screen).
//!
//! The text box is the one the Name row opens (`menu/02_990_TextInput`): its job is made by
//! `make(out, menu base, settings*, on_done functor, kind*)`, the settings starting with the
//! text it starts with, `on_done` a functor calling a plain function with the text typed
//! (NUL-terminated; called on OK), `kind` the Name box's static description (16 characters);
//! the job comes out of the result through a second call. Kindle's `on_done` is the game's own
//! functor type for a plain function (the Name row's), holding Kindle's function. The box starts
//! with the text as typed, and OK shows it in a toast (a row's help line can't change: AddRow
//! copies the label's text).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::text::wide_kept;
use crate::{memory, patterns};

/// AddRow's start.
const ADD_ROW: &str = "4c 89 4c 24 20 53 56 57 48 81 ec 50 01 00 00 48 c7 44 24 28 fe ff ff ff 48 8b 05 ?? ?? ?? ?? \
    48 33 c4 48 89 84 24 40 01 00 00 49 8b d8 48 8b f9 48 89 5c 24 20";
/// The Name row's job, where it opens the text box: its functor (two vtable writes, then the
/// function it calls), the kind, the job's make and the call taking the job out.
const NAME_BOX: &str = "48 8d 05 ?? ?? ?? ?? 48 89 45 bf 48 8d 05 ?? ?? ?? ?? 48 89 45 bf 48 8d 05 ?? ?? ?? ?? 48 89 45 c7 \
    48 8d 45 bf 48 89 45 d7 48 8d 05 ?? ?? ?? ?? 48 89 44 24 20 4c 8d 4d bf 4c 8d 45 07 48 8b d3 48 8d 4d b7 \
    e8 ?? ?? ?? ?? 90 48 8d 55 af 48 8b c8 e8";
const NAME_BOX_VTABLE_AT: usize = 11;
const NAME_BOX_KIND_AT: usize = 41;
const NAME_BOX_MAKE_AT: usize = 68;
const NAME_BOX_TAKE_AT: usize = 81;
/// The menu text the Appearance row shows (FMG 200).
const MENU_TEXT: u32 = 200;
const APPEARANCE: u32 = 402016;

/// Places for mods' rows, after Appearance (menu_movies.rs adds them).
pub const ROWS: usize = 2;
const LABEL_MAX: usize = 32;
const HELP_MAX: usize = 120;
const KEY_MAX: usize = 32;
const TEXT_MAX: usize = 64;
const LABEL_SIZE: usize = 0x70;
const LABEL_HELP_AT: usize = 0x38;
const FUNCTOR_COPY_SLOT: usize = 0;
const FUNCTOR_MOVE_SLOT: usize = 1;
const FUNCTOR_CALL_SLOT: usize = 2;
const FUNCTOR_DELETE_SLOT: usize = 4;
/// Slots copied from the game's functor vtable for Kindle's.
const VTABLE_SLOTS: usize = 8;
/// A menu functor's call gets the dialog; jobs want this base.
const MENU_BASE_AT: usize = 0x48;
/// The game's refcounted objects (jobs): the count at +8, destroyed through vtable slot 0.
const REFCOUNT_AT: usize = 8;

type AddRowFn = unsafe extern "C" fn(usize, usize, usize, u64) -> usize;
type LabelFromTextFn = unsafe extern "C" fn(usize, *const u16) -> usize;
pub(crate) type CallFn = extern "C" fn(usize, *mut usize, usize) -> *mut usize;
type MakeFn = unsafe extern "C" fn(*mut usize, usize, *const BoxSettings, *mut Functor, usize) -> usize;
type TakeFn = unsafe extern "C" fn(usize, *mut usize) -> usize;

static INSTALLED: OnceLock<bool> = OnceLock::new();
/// Kindle's functor vtables, one per row.
static VTABLES: [AtomicUsize; ROWS] = [AtomicUsize::new(0), AtomicUsize::new(0)];
/// The row whose text box is open.
static TEXT_ROW: AtomicUsize = AtomicUsize::new(0);

/// A mod's row on the creation screen.
pub struct ModRow {
    pub owner: String,
    pub label: String,
    pub help: String,
    pub kind: RowKind,
}

pub enum RowKind {
    /// A page of switches (the System screen with the mod's tabs).
    Page(Vec<PageTab>),
    /// The game's text box: its text under `key`; `empty` is what the toast says for no text.
    Text { key: String, empty: String },
}

pub struct PageTab {
    pub title: String,
    /// Its icon (menu_movies::TAB_ICONS).
    pub icon: Option<usize>,
    pub switches: Vec<Switch>,
}

pub struct Switch {
    pub key: String,
    pub label: String,
    pub help: String,
    pub default: bool,
}

/// What was chosen on a row for the character being made.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Switch(bool),
    Text(String),
}

static MOD_ROWS: Mutex<Vec<ModRow>> = Mutex::new(Vec::new());
/// Choices made since the mod last reset them: (owner, key, value); a key not here has its
/// default.
static CHOICES: Mutex<Vec<(String, String, Choice)>> = Mutex::new(Vec::new());

fn check_text(what: &str, text: &str, max: usize) -> Result<(), String> {
    if text.chars().count() > max {
        return Err(format!("{what} is {max} characters at most"));
    }
    Ok(())
}

/// Checks a mod's rows: labels, sizes, and each key once.
fn check_rows(rows: &[ModRow]) -> Result<(), String> {
    let mut keys: Vec<&String> = Vec::new();
    for r in rows {
        if r.label.is_empty() {
            return Err("a row needs a label".into());
        }
        check_text("a row's label", &r.label, LABEL_MAX)?;
        check_text("a row's help", &r.help, HELP_MAX)?;
        let row_keys: Vec<&String> = match &r.kind {
            RowKind::Page(tabs) => {
                if tabs.is_empty() || tabs.len() > crate::native_mods::PAGE_TABS {
                    return Err(format!("a page has 1 to {} tabs", crate::native_mods::PAGE_TABS));
                }
                for t in tabs {
                    check_text("a tab's title", &t.title, LABEL_MAX)?;
                    if t.switches.is_empty() || t.switches.len() > crate::native_mods::PAGE_ROWS {
                        return Err(format!("a tab has 1 to {} switches", crate::native_mods::PAGE_ROWS));
                    }
                    for w in &t.switches {
                        check_text("a switch's label", &w.label, LABEL_MAX)?;
                        check_text("a switch's help", &w.help, HELP_MAX)?;
                    }
                }
                tabs.iter().flat_map(|t| t.switches.iter().map(|w| &w.key)).collect()
            }
            RowKind::Text { key, empty } => {
                check_text("a text row's empty text", empty, LABEL_MAX)?;
                vec![key]
            }
        };
        for k in row_keys {
            if k.is_empty() || k.len() > KEY_MAX {
                return Err(format!("a key is 1 to {KEY_MAX} characters"));
            }
            if keys.contains(&k) {
                return Err(format!("key {k:?} is used twice"));
            }
            keys.push(k);
        }
    }
    Ok(())
}

/// Sets `owner`'s rows (replacing its earlier ones). All mods' rows share ROWS places.
pub fn set_rows(owner: &str, rows: Vec<ModRow>) -> Result<(), String> {
    check_rows(&rows)?;
    let mut all = MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner());
    let others: Vec<&str> = all.iter().filter(|r| r.owner != owner).map(|r| r.owner.as_str()).collect();
    if others.len() + rows.len() > ROWS {
        return Err(format!("the creation screen has room for {ROWS} mods' rows; {} taken by {}", others.len(), others.join(", ")));
    }
    all.retain(|r| r.owner != owner);
    crate::info!("character creation: {owner}'s rows {:?}", rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>());
    all.extend(rows.into_iter().map(|r| ModRow { owner: owner.to_string(), ..r }));
    Ok(())
}

/// A mod unloads: its rows and choices go.
pub fn remove_rows(owner: &str) {
    MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner()).retain(|r| r.owner != owner);
    reset_choices(owner);
}

/// What `owner`'s rows hold now: every key with its choice, else its default.
pub fn choices(owner: &str) -> Vec<(String, Choice)> {
    let set = CHOICES.lock().unwrap_or_else(|e| e.into_inner());
    let chosen = |key: &str| set.iter().find(|(o, k, _)| o == owner && k == key).map(|(_, _, c)| c.clone());
    let mut out = Vec::new();
    for r in MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|r| r.owner == owner) {
        match &r.kind {
            RowKind::Page(tabs) => {
                for w in tabs.iter().flat_map(|t| &t.switches) {
                    out.push((w.key.clone(), chosen(&w.key).unwrap_or(Choice::Switch(w.default))));
                }
            }
            RowKind::Text { key, .. } => out.push((key.clone(), chosen(key).unwrap_or(Choice::Text(String::new())))),
        }
    }
    out
}

/// Back to the defaults (the mod took the choices for a character).
pub fn reset_choices(owner: &str) {
    CHOICES.lock().unwrap_or_else(|e| e.into_inner()).retain(|(o, _, _)| o != owner);
}

fn set_choice(owner: &str, key: &str, value: Choice) {
    let mut set = CHOICES.lock().unwrap_or_else(|e| e.into_inner());
    set.retain(|(o, k, _)| !(o == owner && k == key));
    set.push((owner.to_string(), key.to_string(), value));
}

/// A switch's value now (the page's rows show it).
pub fn switch(owner: &str, key: &str) -> bool {
    choices(owner).into_iter().any(|(k, c)| k == key && c == Choice::Switch(true))
}

pub fn set_switch(owner: &str, key: &str, on: bool) {
    set_choice(owner, key, Choice::Switch(on));
}

struct NameBox {
    vtable: usize,
    kind: usize,
    make: usize,
    take: usize,
}

static ADD_ROW_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "character creation",
    name: "character creation AddRow",
    pattern: ADD_ROW,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(add_row_hook),
    without: "the creation screen has no Seed row",
};

/// Installs the hook (once; from the frame task, long before the screen can open).
pub fn install() -> bool {
    *INSTALLED.get_or_init(|| {
        let Some(found) = ADD_ROW_SITE.find() else { return false };
        // The Seed row needs it; without it the row stays and does nothing.
        let _ = name_box();
        ADD_ROW_SITE.hook_all(&found)
    })
}

fn name_box() -> Option<&'static NameBox> {
    static FOUND: OnceLock<Option<NameBox>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let at = patterns::find_unique("character creation text box", NAME_BOX)?;
            Some(NameBox {
                vtable: patterns::lea_target(at + NAME_BOX_VTABLE_AT)?,
                kind: patterns::lea_target(at + NAME_BOX_KIND_AT)?,
                make: patterns::call_target(at + NAME_BOX_MAKE_AT)?,
                take: patterns::call_target(at + NAME_BOX_TAKE_AT)?,
            })
        })
        .as_ref()
}

/// The text a label's name part points at.
fn label_text(label: usize) -> Option<String> {
    memory::read_u64(label).and_then(|p| memory::read_wstr(p as usize, 64))
}

fn add_row_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (list, label, functor, style) = (r.rcx as usize, r.rdx as usize, r.r8 as usize, r.r9);
    let add: AddRowFn = unsafe { std::mem::transmute(original) };
    let appearance = std::panic::catch_unwind(|| Some(label_text(label)? == crate::text_override::get(MENU_TEXT, APPEARANCE)?));
    let mut result = unsafe { add(list, label, functor, style) };
    // Without the movie's places for them, the list would scroll and its button run a row's.
    if appearance.ok().flatten() == Some(true) && crate::menu_movies::char_make_places_added() {
        result = std::panic::catch_unwind(|| add_rows(add, result, style)).unwrap_or(result);
    }
    result
}

/// The vtable of Kindle's row `row`, whose call slot is `call`.
fn vtable(row: usize, call: CallFn) -> Option<usize> {
    let vt = VTABLES[row].load(Ordering::Acquire);
    if vt != 0 {
        return Some(vt);
    }
    let vt = functor_vtable(call)?;
    VTABLES[row].store(vt, Ordering::Release);
    Some(vt)
}

/// A new functor type of Kindle's for the game's menus (a menu entry, a row), whose call is
/// `call`: a copy of the game's captureless lambda vtable with Kindle's copy, move and destroy
/// (a clone takes whatever vtable copy writes). Lives for the rest of the process.
pub(crate) fn functor_vtable(call: CallFn) -> Option<usize> {
    let plain = crate::front_menu::plain_functor_vtable()?;
    let mut slots: Vec<usize> = (0..VTABLE_SLOTS).map(|i| memory::read_u64(plain + 8 * i).unwrap_or(0) as usize).collect();
    slots[FUNCTOR_COPY_SLOT] = functor_copy as *const () as usize;
    slots[FUNCTOR_MOVE_SLOT] = functor_copy as *const () as usize;
    slots[FUNCTOR_CALL_SLOT] = call as usize;
    slots[FUNCTOR_DELETE_SLOT] = functor_delete as *const () as usize;
    Some(Box::leak(slots.into_boxed_slice()).as_ptr() as usize)
}

/// The calls of Kindle's rows, by place.
const CALLS: [CallFn; ROWS] = [row_picked::<0>, row_picked::<1>];

/// Adds the mods' rows to the list (after Appearance), a blank one in each place left; returns
/// the list.
fn add_rows(add: AddRowFn, mut list: usize, style: u64) -> usize {
    let Some(from_text) = crate::front_menu::label_from_text() else { return list };
    let texts: Vec<(&'static [u16], &'static [u16])> = {
        let rows = MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner());
        (0..ROWS).map(|i| rows.get(i).map_or((wide_kept(""), wide_kept("")), |r| (wide_kept(&r.label), wide_kept(&r.help)))).collect()
    };
    for (row, (name, help)) in texts.into_iter().enumerate() {
        let Some(vtable) = vtable(row, CALLS[row]) else { return list };
        #[repr(C, align(16))]
        struct Label([u8; LABEL_SIZE]);
        let mut label = Label([0; LABEL_SIZE]);
        let at = label.0.as_mut_ptr() as usize;
        let from_text: LabelFromTextFn = unsafe { std::mem::transmute(from_text) };
        unsafe {
            from_text(at, name.as_ptr());
            from_text(at + LABEL_HELP_AT, help.as_ptr());
        }
        // Kindle's functor, in place: AddRow clones it; ours needs no destroying (nothing to
        // free), nor does the label (it owns nothing).
        let mut functor = Functor { vtable, storage: [0; 2], imp: 0 };
        functor.imp = &raw const functor as usize;
        list = unsafe { add(list, at, &raw const functor as usize, style) };
    }
    list
}

/// A `std::function` as the game lays it out: the impl in place ({vtable, capture}), then the
/// impl pointer.
#[repr(C)]
struct Functor {
    vtable: usize,
    storage: [usize; 2],
    imp: usize,
}

/// Kindle's row functors: just their vtable (no captures), built in place.
#[repr(C)]
struct Impl {
    vtable: usize,
}

/// Copy (and move): builds a copy of the functor `this` at `at` (the std::function's storage).
extern "C" fn functor_copy(this: *const Impl, at: *mut Impl) -> *mut Impl {
    if !at.is_null() && !this.is_null() {
        unsafe { at.write(Impl { vtable: (*this).vtable }) };
    }
    at
}

/// Destroy: nothing to free (it lives in the std::function's storage).
extern "C" fn functor_delete(_this: usize, _free: bool) {}

/// No job: the screen stays as it is.
fn no_job(out: *mut usize) -> *mut usize {
    if !out.is_null() {
        unsafe { *out = 0 };
    }
    out
}

/// The game calls a row's call twice for one press: true for the second.
fn repeat(last: &Mutex<Option<std::time::Instant>>) -> bool {
    let mut last = last.lock().unwrap_or_else(|e| e.into_inner());
    let repeat = last.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(500));
    if !repeat {
        *last = Some(std::time::Instant::now());
    }
    repeat
}

/// Row ROW was picked: its page's or its text box's job (a blank place: none).
extern "C" fn row_picked<const ROW: usize>(_this: usize, out: *mut usize, ctx: usize) -> *mut usize {
    static LAST: [Mutex<Option<std::time::Instant>>; ROWS] = [Mutex::new(None), Mutex::new(None)];
    if repeat(&LAST[ROW]) || ctx == 0 {
        return no_job(out);
    }
    let picked = std::panic::catch_unwind(|| {
        let rows = MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner());
        let row = rows.get(ROW)?;
        match &row.kind {
            RowKind::Page(tabs) => {
                let page: Vec<crate::native_mods::PageTabSpec> = tabs
                    .iter()
                    .map(|t| crate::native_mods::PageTabSpec {
                        title: t.title.clone(),
                        icon: t.icon,
                        switches: t.switches.iter().map(|w| (w.key.clone(), w.label.clone(), w.help.clone(), w.default)).collect(),
                    })
                    .collect();
                let owner = row.owner.clone();
                drop(rows);
                crate::native_mods::open_page(out, ctx, &owner, page)
            }
            RowKind::Text { .. } => {
                drop(rows);
                TEXT_ROW.store(ROW, Ordering::Relaxed);
                open_text_box(out, ctx, ROW)
            }
        }
    });
    picked.ok().flatten().unwrap_or_else(|| no_job(out))
}

/// The settings the text box starts from: the text, or (null) the string after it.
#[repr(C)]
struct BoxSettings {
    text: *const u16,
    /// An empty MSVC wide string (inline buffer, size 0, capacity 7).
    string: [usize; 4],
}

/// The text box for text row `row`, starting with its text as typed.
fn open_text_box(out: *mut usize, ctx: usize, row: usize) -> Option<*mut usize> {
    let b = name_box()?;
    let (owner, key) = text_row(row)?;
    let current = match choices(&owner).into_iter().find(|(k, _)| *k == key) {
        Some((_, Choice::Text(t))) => t,
        _ => String::new(),
    };
    static START: Mutex<Vec<u16>> = Mutex::new(Vec::new());
    let mut start = START.lock().unwrap_or_else(|e| e.into_inner());
    *start = current.encode_utf16().chain(std::iter::once(0)).collect();
    let settings = BoxSettings { text: start.as_ptr(), string: [0, 0, 0, 7] };
    // The game's plain-function functor holding Kindle's function; make takes it over.
    let mut on_done = Functor { vtable: b.vtable, storage: [text_typed as *const () as usize, 0], imp: 0 };
    on_done.imp = &raw const on_done as usize;
    let make: MakeFn = unsafe { std::mem::transmute(b.make) };
    let take: TakeFn = unsafe { std::mem::transmute(b.take) };
    let mut made = 0usize;
    let mut job = 0usize;
    unsafe {
        let r = make(&mut made, ctx + MENU_BASE_AT, &settings, &mut on_done, b.kind);
        take(r, &mut job);
        *out = job;
    }
    release(made);
    Some(out)
}

/// Drops one reference to a game object (a job).
fn release(obj: usize) {
    if obj == 0 {
        return;
    }
    let count = unsafe { &*((obj + REFCOUNT_AT) as *const std::sync::atomic::AtomicI32) };
    if count.fetch_sub(1, Ordering::AcqRel) == 1
        && let Some(vt) = memory::read_u64(obj)
        && let Some(destroy) = memory::read_u64(vt as usize)
    {
        let destroy: extern "C" fn(usize) = unsafe { std::mem::transmute(destroy as usize) };
        destroy(obj);
    }
}

/// Text row `row`'s owner and key.
fn text_row(row: usize) -> Option<(String, String)> {
    let rows = MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner());
    let r = rows.get(row)?;
    match &r.kind {
        RowKind::Text { key, .. } => Some((r.owner.clone(), key.clone())),
        RowKind::Page(_) => None,
    }
}

/// The text box's OK: the text row's text for the character being made, shown in a toast.
extern "C" fn text_typed(text: *const u16) {
    let _ = std::panic::catch_unwind(|| {
        let typed = memory::read_wstr(text as usize, 64).unwrap_or_default();
        let typed: String = typed.trim().chars().take(TEXT_MAX).collect();
        let row = TEXT_ROW.load(Ordering::Relaxed);
        let Some((owner, key)) = text_row(row) else { return };
        set_choice(&owner, &key, Choice::Text(typed.clone()));
        let (label, empty) = match MOD_ROWS.lock().unwrap_or_else(|e| e.into_inner()).get(row) {
            Some(ModRow { label, kind: RowKind::Text { empty, .. }, .. }) => (label.clone(), empty.clone()),
            _ => return,
        };
        let text = if typed.is_empty() { format!("{label}: {empty}") } else { format!("{label}: {typed}") };
        crate::info!("character creation: [{owner}] {text}");
        crate::ui_bridge::push_notice(crate::ui_bridge::NoticeKind::Toast, text, TEXT_COLOR, 5.0);
    });
}

/// The toast's colour (the mods' "gold").
const TEXT_COLOR: [f32; 4] = [1.0, 0.82, 0.5, 1.0];

#[cfg(test)]
mod tests {
    use super::*;

    fn page(keys: &[&str]) -> ModRow {
        let switches = keys.iter().map(|k| Switch { key: k.to_string(), label: k.to_string(), help: String::new(), default: true }).collect();
        ModRow {
            owner: String::new(),
            label: "Page".into(),
            help: String::new(),
            kind: RowKind::Page(vec![PageTab { title: "T".into(), icon: None, switches }]),
        }
    }

    #[test]
    fn rows_and_choices() {
        let text =
            ModRow { owner: String::new(), label: "Seed".into(), help: String::new(), kind: RowKind::Text { key: "seed".into(), empty: "random".into() } };
        set_rows("rowmod", vec![page(&["a", "b"]), text]).unwrap();
        let want = vec![("a".to_string(), Choice::Switch(true)), ("b".to_string(), Choice::Switch(true)), ("seed".to_string(), Choice::Text(String::new()))];
        assert_eq!(choices("rowmod"), want);
        set_switch("rowmod", "b", false);
        assert!(!switch("rowmod", "b"));
        assert!(set_rows("other", vec![page(&["x"])]).is_err(), "both places taken");
        reset_choices("rowmod");
        assert!(switch("rowmod", "b"));
        assert!(set_rows("rowmod", vec![page(&["a", "a"])]).is_err(), "a key twice");
        remove_rows("rowmod");
        assert!(choices("rowmod").is_empty());
        set_rows("other", vec![page(&["x"])]).unwrap();
        remove_rows("other");
    }
}
