//! The native Mods screen: the game menus' Mods entry opens the game's own System screen
//! (`AppMenu::OptionSettingTopDialog`, movie `02_040_OptionSetting`) with Kindle's tabs and
//! rows instead of the game's: Kindle (Kindle's own settings), Mods (each mod on or off) and Mod
//! menus (a row per mod with buttons or settings, opening that mod's menu over the screen). A
//! mod's menu is another System screen: its buttons (kindle.ui.mod_tab) and its settings, those
//! the screen can show as switches, choices of two and sliders of whole numbers 0 to 255; the
//! others are rows that open Kindle's Mods screen (mods_screen.rs) at the mod, as the Kindle
//! tab's "All mod settings" opens it for all. Changes apply as they are made; the screen's
//! Defaults button restores the defaults. Mods' screens (kindle.ui.screen) that fit open on it too (below). If anything here
//! isn't found, the entry opens Kindle's Mods screen instead.
//!
//! How the System screen works (DS3 1.15.2, names from RTTI, verified in game 2026-10-04):
//! - `open(job* out, ctx)` makes the menu job that opens it, one for in game and one for the
//!   title (the pause and title menus' System entries call them). A command's call slot gets
//!   the menu window; the job wants its base at +0x48 (passing the window itself trips the
//!   game's reference count check and it exits).
//! - The job's factory allocates the dialog (0x14E8 bytes) and constructs it. The constructor
//!   adds the tabs with `add_tab(list, entry)`: an entry (0x78 bytes) starts with its label
//!   (the text pointer at +0) and has its window index at +0x38. The list (dialog +0x1010)
//!   holds 8 at most, as does the window cache after it, which is why the game's own screen
//!   can't take a ninth tab; Kindle's screen is a second copy of it instead.
//! - `make_window(windows, index)` (windows = dialog +0x13E0) makes a tab's window from
//!   `WindowList.<name>` in the movie and runs the tab's builder, `builder(window, settings)`;
//!   the first tab's window is made inside the constructor. Windows 0-2 and 4 (Game Options,
//!   Camera, Sound and display, Network) are plain rows; Kindle uses those. A window applies
//!   the game's settings it holds when the screen closes; Kindle's rows don't touch them.
//!   The Network window also holds a notice (`UgcRestricted`, system text 100020) that its
//!   builder hides or shows: `find(window +0x1010, out, name)` gives a handle to a named part
//!   of the window's movie clip, `set_visible(handle, bool)`, then `release(out +0x28)`.
//! - A builder adds rows: `row_list(window, label, u8* value, choices, u8* default, 1)` for a
//!   list of choices (the game's On/Off list comes from `on_off(list)`: choices are 0x78 bytes
//!   {u8 value, label}, count at +0xF8, each destroyed with `destroy_choice`), and
//!   `row_slider(window, label, u8* value, range*, u8* default)` with range
//!   `{i32 min, i32 max, i32 step, u8 0}`: the slider holds the byte and shows it as it is.
//!   Rows copy labels and choices but keep their text pointers. Defaults copies each row's
//!   default byte into its value.
//! - `row_button(window, label, on_press*, greyed*, key_guide*)` adds a button row. The three
//!   are MSVC `std::function`s: 0x18 bytes of storage, then the callable's pointer (0 for
//!   none). A callable's vtable is copy, move, call, type, delete, destroy, target. `on_press`
//!   returns nothing; `greyed` returns a bool, asked every frame, and shows the row "Grayout"
//!   or "Normal" (the row's controller, +0xA8F5E0; it doesn't stop a press); `key_guide`
//!   returns a bool for the confirm prompt (verified 2026-11-09 in the decompile).
//! - A row opens a screen over its own as the game's do (+0xAA8E60): if the window's job
//!   runner (window +0x10) is idle (+0xA4C490), the screen's job is made with the window +0x48
//!   as its base and pushed onto the runner (+0xA4C560, which takes the job's reference).
//! - The dialog's deleting destructor (its vtable's second slot, +0xAB8100; the first,
//!   +0x927A20, releases the dialog through it) says when one closes,
//!   whether the player backed out of it or the game closed its menus.
//!
//! Kindle's rows edit bytes in Kindle's memory; `tick` turns changes into the same requests the
//! overlay sends, and after Kindle's Mods screen has been open the rows follow what changed
//! there.

use std::cell::UnsafeCell;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use kindle_core::screens::{self, RowKind};

use crate::config::{SettingKind, SettingValue};
use crate::menu_movies;
use crate::text::wide_kept;
use crate::ui_bridge::{self, UiRequest};
use crate::{front_menu, lock, patterns};

const OPEN_IN_GAME: &str = "48 89 4c 24 08 53 48 83 ec 30 48 c7 44 24 28 fe ff ff ff 48 8b d9 c7 44 24 20 00 00 00 00 41 b0 01";
const OPEN_FROM_TITLE: &str = "48 89 4c 24 08 53 48 83 ec 30 48 c7 44 24 28 fe ff ff ff 48 8b d9 c7 44 24 20 00 00 00 00 45 33 c0 e8 ?? ?? ?? ??";
const FACTORY: &str = "48 8b c4 56 57 41 56 48 81 ec 90 00 00 00 48 c7 40 80 fe ff ff ff 48 89 58 08 48 8b f2 4c 8b f1 33 db \
    89 5c 24 20 4c 8b 05 ?? ?? ?? ?? 4c 89 40 18 8d 53 08 b9 e8 14 00 00";
const ADD_TAB: &str = "40 57 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 48 48 8b fa 48 8b d9 48 8b 81 c8 03 00 00";
const MAKE_WINDOW: &str = "48 8b c4 55 57 41 54 41 56 41 57 48 8d 68 88 48 81 ec 50 01 00 00 48 c7 44 24 60 fe ff ff ff";
const ON_OFF: &str = "48 8b c4 55 48 8d 68 a1 48 81 ec 00 01 00 00 48 c7 45 8f fe ff ff ff 48 89 58 10 48 89 78 18 48 8b 05 \
    ?? ?? ?? ?? 48 33 c4 48 89 45 4f 48 8b d9 48 89 4d 87 33 ff 89 7c 24 28 48 89 b9 f8 00 00 00 c7 44 24 28 01 00 00 00 \
    ba 6c 9d 00 00";
/// The Game Options and Camera tab builders share their opening; each is told apart by what
/// follows.
const BUILDER_START: &str = "40 55 57 41 54 41 56 41 57 48 8d ac 24 20 ee ff ff b8 e0 12 00 00 e8 ?? ?? ?? ?? 48 2b e0 48 c7 44 24 \
    50 fe ff ff ff 48 89 9c 24 20 13 00 00 48 89 b4 24 28 13 00 00 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 85 d0 11 00 00 4c 8b \
    fa 4c 8b f1 48 8d 8d 60 11 00 00 e8 ?? ?? ?? ?? 90";
const GAME_OPTIONS_BUILDER: &str = "c6 44 24 30 17";
const CAMERA_BUILDER: &str = "48 8d 4c 24 58";
const SOUND_BUILDER: &str = "40 55 57 41 54 41 56 41 57 48 8d ac 24 10 df ff ff";
const NETWORK_BUILDER: &str = "40 55 56 57 41 54 41 55 41 56 41 57 48 8d ac 24 20 dd ff ff";
const ROW_BUTTON: &str = "40 55 53 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 e9 48 81 ec c8 00 00 00 48 c7 45 af fe ff ff ff";
/// In the Game Options builder: the calls to the row makers (their code has byte-identical
/// twins, so they are found through these calls).
const ROW_LIST_CALL_AT: usize = 0xB2;
const DESTROY_CHOICE_CALL_AT: usize = 0x1C9;
const ROW_SLIDER_CALL_AT: usize = 0x67B;
/// In the Network builder: the calls that hide its notice.
const FIND_PART_CALL_AT: usize = 0xA7C;
const SET_VISIBLE_CALL_AT: usize = 0xA8D;
const RELEASE_PART_CALL_AT: usize = 0xA9A;
/// A System screen row's press that opens a screen over its own (+0xAA8E60): its calls are the
/// window's job runner's idle check and push.
const OPEN_OVER: &str = "40 53 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 8b d9 48 8b 0d ?? ?? ?? ?? e8 ?? ?? ?? ?? 84 c0 0f 85 \
    ?? ?? ?? ?? 48 8b 0b 48 83 c1 10 e8";
const JOB_IDLE_CALL_AT: usize = 0x2D;
const JOB_PUSH_CALL_AT: usize = 0x77;
/// A menu window's job runner: the screens opened over the window run there.
const JOB_RUNNER_AT: usize = 0x10;
/// How the System screen's deleting destructor starts (checked before it's hooked).
const DIALOG_END: &str = "48 89 5c 24 08 57 48 83 ec 20 8b da 48 8b f9 e8";

/// A menu entry's call gets the window; the job wants this base.
const MENU_BASE_AT: usize = 0x48;
const WINDOWS_AT: usize = 0x13E0;
const TAB_WINDOW_AT: usize = 0x38;
/// A tab's icon: the frame (from 1) of the screen's tab icon sprites (the game's own icons'
/// order isn't the windows': Camera's is 6).
const TAB_ICON_AT: usize = 0x40;
const CHOICE_SIZE: usize = 0x78;
const CHOICE_COUNT_AT: usize = 0xF8;
/// A choice's label (and its help after it, as a row's), after its value byte.
const CHOICE_LABEL_AT: usize = 8;
const CHOICE_LIST_SIZE: usize = 0x100;
const LABEL_SIZE: usize = 0x70;
const LABEL_HELP_AT: usize = 0x38;
const WINDOW_CLIP_AT: usize = 0x1010;
const PART_RELEASE_AT: usize = 0x28;
/// Parts of the game's windows Kindle's tabs hide: (window index, part name).
const HIDDEN_PARTS: [(i32, &str); 1] = [(4, "UgcRestricted")];
/// The windows Kindle's tabs use, in order (Game Options, Camera, Sound and display, Network).
const WINDOWS: [i32; 4] = [0, 1, 2, 4];
/// The rows each of those windows holds: the game makes no more (seen on B 2026-11-09, 12 rows
/// on each: the 7th or 8th on isn't made, nor the window's Defaults button). A longer tab ends in
/// a More row that opens the rest over it.
const ROOM: [usize; 4] = [6, 7, 7, 6];
const MAX_ROWS: usize = 256;
/// Frames the rows follow Kindle's Mods screen after it closes (the runtime republishes the
/// mods every 30).
const EDITOR_FOLLOW_FRAMES: u32 = 60;

struct Game {
    open_in_game: usize,
    open_from_title: usize,
    factory: usize,
    add_tab: usize,
    make_window: usize,
    builders: [usize; 4],
    row_list: usize,
    row_slider: usize,
    on_off: usize,
    destroy_choice: usize,
    row_button: usize,
    find_part: usize,
    set_visible: usize,
    release_part: usize,
}

static GAME: OnceLock<Option<Game>> = OnceLock::new();

fn game() -> Option<&'static Game> {
    GAME.get_or_init(|| {
        let game_options = patterns::find_unique("Game Options tab builder", &format!("{BUILDER_START} {GAME_OPTIONS_BUILDER}"))?;
        let network = patterns::find_unique("Network tab builder", NETWORK_BUILDER)?;
        Some(Game {
            open_in_game: patterns::find_unique("System screen job (in game)", OPEN_IN_GAME)?,
            open_from_title: patterns::find_unique("System screen job (title)", OPEN_FROM_TITLE)?,
            factory: patterns::find_unique("System screen factory", FACTORY)?,
            add_tab: patterns::find_unique("System screen tab add", ADD_TAB)?,
            make_window: patterns::find_unique("System screen tab window", MAKE_WINDOW)?,
            builders: [
                game_options,
                patterns::find_unique("Camera tab builder", &format!("{BUILDER_START} {CAMERA_BUILDER}"))?,
                patterns::find_unique("Sound and display tab builder", SOUND_BUILDER)?,
                network,
            ],
            row_list: patterns::call_target(game_options + ROW_LIST_CALL_AT)?,
            row_slider: patterns::call_target(game_options + ROW_SLIDER_CALL_AT)?,
            on_off: patterns::find_unique("On/Off choices", ON_OFF)?,
            destroy_choice: patterns::call_target(game_options + DESTROY_CHOICE_CALL_AT)?,
            row_button: patterns::find_unique("System screen button row", ROW_BUTTON)?,
            find_part: patterns::call_target(network + FIND_PART_CALL_AT)?,
            set_visible: patterns::call_target(network + SET_VISIBLE_CALL_AT)?,
            release_part: patterns::call_target(network + RELEASE_PART_CALL_AT)?,
        })
    })
    .as_ref()
}

/// The window job runner's idle check and push, for screens opened over Kindle's.
struct Over {
    idle: usize,
    push: usize,
}

fn over() -> Option<&'static Over> {
    static OVER: OnceLock<Option<Over>> = OnceLock::new();
    OVER.get_or_init(|| {
        let at = patterns::find_unique("System screen row opening a screen", OPEN_OVER)?;
        Some(Over { idle: patterns::call_target(at + JOB_IDLE_CALL_AT)?, push: patterns::call_target(at + JOB_PUSH_CALL_AT)? })
    })
    .as_ref()
}

#[derive(Clone)]
enum Row {
    /// Kindle's developer overlay.
    Developer,
    Enabled {
        id: String,
    },
    /// A mod's on/off setting.
    Bool {
        id: String,
        name: String,
    },
    /// A mod's whole-number setting within 0 to 255: its byte is the value.
    Int {
        id: String,
        name: String,
        min: i64,
        max: i64,
    },
    /// A mod's setting of two choices: its byte is the choice's index.
    SettingChoice {
        id: String,
        name: String,
        choices: [String; 2],
    },
    /// A switch of a mod's creation screen page (char_make.rs): on/off.
    Choice {
        owner: String,
        key: String,
    },
    /// A switch of a mod's screen (kindle.ui.screen): the screen's id and the row's index.
    ScreenSwitch {
        screen: u64,
        row: usize,
    },
    /// A slider of a mod's screen, whole numbers 0 to 255: its byte is the value.
    ScreenSlider {
        screen: u64,
        row: usize,
        min: f64,
        max: f64,
        step: f64,
    },
    /// A choice of two of a mod's screen: its byte is the choice's index.
    ScreenChoice {
        screen: u64,
        row: usize,
        choices: [String; 2],
    },
}

impl Row {
    /// A two-choice row's choices (the game's list holds two at most).
    fn choices(&self) -> Option<&[String; 2]> {
        match self {
            Row::SettingChoice { choices, .. } | Row::ScreenChoice { choices, .. } => Some(choices),
            _ => None,
        }
    }
}

/// Two choices from a list of exactly two.
fn two(choices: &[String]) -> Option<[String; 2]> {
    match choices {
        [a, b] => Some([a.clone(), b.clone()]),
        _ => None,
    }
}

#[derive(Clone)]
struct RowSpec {
    label: &'static [u16],
    help: &'static [u16],
    row: Row,
    slot: usize,
}

struct Tab {
    label: &'static [u16],
    /// Its rows, in order.
    items: Vec<Item>,
    /// Kindle's icon for the tab (menu_movies::TAB_ICONS), else the window's own.
    icon: Option<usize>,
}

#[derive(Clone)]
enum Item {
    Row(RowSpec),
    Button(Button),
}

impl Tab {
    fn rows(&self) -> impl Iterator<Item = &RowSpec> {
        self.items.iter().filter_map(|i| match i {
            Item::Row(r) => Some(r),
            Item::Button(_) => None,
        })
    }
}

/// A button row, or a line of text (no press).
#[derive(Clone)]
struct Button {
    label: &'static [u16],
    help: &'static [u16],
    /// Its place in PRESS_SLOTS (what a press does, and whether it's greyed now); None for a line.
    slot: Option<usize>,
    /// A line shown greyed (a button is greyed by its slot's GREYED).
    greyed: bool,
}

/// An MSVC `std::function` as the game's menus take them.
#[repr(C)]
struct Function {
    storage: [usize; 3],
    callable: usize,
}

impl Function {
    fn of(callable: &'static Callable) -> Self {
        Self { storage: [0; 3], callable: callable as *const Callable as usize }
    }

    fn empty() -> Self {
        Self { storage: [0; 3], callable: 0 }
    }
}

/// A callable for the game's `std::function`s. Kindle's live for the life of the process, so
/// copies share them and deleting one does nothing.
#[repr(C)]
struct Callable {
    vtable: &'static CallableVtable,
}

#[repr(C)]
struct CallableVtable {
    copy: extern "C" fn(&'static Callable, usize) -> &'static Callable,
    move_to: extern "C" fn(&'static Callable, usize) -> &'static Callable,
    call: extern "C" fn(&'static Callable) -> u8,
    target_type: extern "C" fn(&'static Callable) -> usize,
    delete: extern "C" fn(&'static Callable, u8),
    destroy: extern "C" fn(&'static Callable, u8),
    target: extern "C" fn(&'static Callable) -> usize,
}

const fn callable(call: extern "C" fn(&'static Callable) -> u8) -> CallableVtable {
    CallableVtable { copy: shared, move_to: shared, call, target_type: nothing, delete: keep, destroy: keep, target: nothing }
}

extern "C" fn shared(this: &'static Callable, _to: usize) -> &'static Callable {
    this
}

extern "C" fn nothing(_: &'static Callable) -> usize {
    0
}

extern "C" fn keep(_: &'static Callable, _dealloc: u8) {}

extern "C" fn always(_: &'static Callable) -> u8 {
    1
}

extern "C" fn never(_: &'static Callable) -> u8 {
    0
}

static ALWAYS: Callable = Callable { vtable: &callable(always) };
static NEVER: Callable = Callable { vtable: &callable(never) };

/// A callable that knows its place among its kind (a button's press, its greyed check).
#[repr(C)]
struct SlotCallable {
    vtable: &'static CallableVtable,
    slot: usize,
}

impl SlotCallable {
    fn callable(&'static self) -> &'static Callable {
        // (A SlotCallable starts as a Callable does: its vtable.)
        unsafe { &*(self as *const SlotCallable as *const Callable) }
    }

    fn slot_of(this: &'static Callable) -> usize {
        unsafe { (*(this as *const Callable as *const SlotCallable)).slot }
    }
}

/// N callables of one kind, each knowing its place.
const fn slot_callables<const N: usize>(vtable: &'static CallableVtable) -> [SlotCallable; N] {
    let mut all = [const { SlotCallable { vtable: &PRESS_VTABLE, slot: 0 } }; N];
    let mut i = 0;
    while i < N {
        all[i].vtable = vtable;
        all[i].slot = i;
        i += 1;
    }
    all
}

/// The buttons of the System screens open (Kindle's, mods' menus, mods' screens) together.
const BUTTON_SLOTS: usize = 128;
static PRESS_VTABLE: CallableVtable = callable(button_pressed);
static GREYED_VTABLE: CallableVtable = callable(greyed_now);
static PRESS_CALLABLES: [SlotCallable; BUTTON_SLOTS] = slot_callables(&PRESS_VTABLE);
static GREYED_CALLABLES: [SlotCallable; BUTTON_SLOTS] = slot_callables(&GREYED_VTABLE);
/// Each button slot's greyed state, read by the game every frame.
static GREYED: [AtomicBool; BUTTON_SLOTS] = [const { AtomicBool::new(false) }; BUTTON_SLOTS];

/// What a button does when pressed.
#[derive(Clone, Debug, PartialEq)]
enum Press {
    /// A mod's button (kindle.ui.mod_tab): its press goes to the mod at Kindle's next frame.
    ModTab { owner: String, label: String, pastes: bool },
    /// A button of a mod's screen (kindle.ui.screen).
    Screen { id: u64, row: usize },
    /// Opens a mod's menu over this screen.
    ModMenu { owner: String },
    /// Opens Kindle's Mods screen over this one, at a mod's settings or at the first mod's.
    Settings { owner: Option<String> },
    /// Opens the rest of a tab's rows over this screen (a tab with more than its window holds).
    More { tab: usize },
}

#[derive(Clone)]
struct PressSlot {
    press: Press,
    /// The plan and tab the button is on.
    plan: usize,
    tab: usize,
}

/// The buttons' presses, by slot: each plan's after those of the plans under it.
static PRESS_SLOTS: Mutex<Vec<PressSlot>> = Mutex::new(Vec::new());

extern "C" fn greyed_now(this: &'static Callable) -> u8 {
    GREYED.get(SlotCallable::slot_of(this)).is_some_and(|g| g.load(Ordering::Relaxed)) as u8
}

extern "C" fn button_pressed(this: &'static Callable) -> u8 {
    let slot = SlotCallable::slot_of(this);
    // (The game's row presses whether greyed or not.)
    if GREYED.get(slot).is_none_or(|g| g.load(Ordering::Relaxed)) {
        return 0;
    }
    let Some(PressSlot { press, plan, tab }) = lock(&PRESS_SLOTS).get(slot).cloned() else { return 0 };
    if let Some(p) = lock(&PLANS).get_mut(plan) {
        p.pressed_tab = tab;
    }
    match press {
        Press::ModTab { owner, label, pastes } => {
            let pasted = pastes.then(|| crate::clipboard::text().map(|t| t.chars().take(PASTE_MAX).collect::<String>()).unwrap_or_default());
            lock(&PRESSES).push((owner, label, pasted));
        }
        Press::Screen { id, row } => screens::report(id, screens::ScreenEvent::Selected(row)),
        Press::ModMenu { owner } => {
            let version = mod_tab_version(&owner);
            let opened = open_over(plan, tab, |b| mod_menu(b, &owner).map(|(title, tabs)| (Some(title), tabs, None)));
            if opened {
                mark_menu(&owner, version);
            } else {
                open_settings(Some(owner));
            }
        }
        Press::Settings { owner } => open_settings(owner),
        Press::More { tab } => {
            let menu = lock(&PLANS).get(plan).and_then(|p| p.menu.clone());
            if open_over(plan, tab, |b| more(b, plan, tab))
                && let Some((owner, version)) = menu
            {
                mark_menu(&owner, version);
            }
        }
    }
    0
}

/// The rest of plan `level`'s tab `tab`, as a plan over it: its rows keep their bytes and slots
/// (those of the plan under it), and its buttons' presses are now this plan's.
fn more(b: &mut Builder, level: usize, tab: usize) -> Option<(Option<String>, Vec<Tab>, Option<screens::Screen>)> {
    let (title, label, icon, items) = {
        let plans = lock(&PLANS);
        let p = plans.get(level)?;
        let t = p.tabs.get(tab)?;
        (p.title.clone(), t.label, t.icon, p.more.get(tab)?.clone())
    };
    let mut slots = lock(&PRESS_SLOTS);
    for slot in items.iter().filter_map(|i| match i {
        Item::Button(Button { slot, .. }) => *slot,
        Item::Row(_) => None,
    }) {
        if let Some(s) = slots.get_mut(slot) {
            (s.plan, s.tab) = (b.level, 0);
        }
    }
    Some((title, vec![Tab { label, items, icon }], None))
}

/// Kindle's Mods screen over the System screen, at a mod's settings.
fn open_settings(owner: Option<String>) {
    *lock(&ui_bridge::MODS_SCREEN_AT) = owner;
    ui_bridge::OPEN_MODS_SCREEN.store(true, Ordering::Relaxed);
    EDITOR.store(EDITOR_ASKED, Ordering::Relaxed);
}

// ── mods' menus ────────────────────────────────────────────────────────────────────────────────
//
// A mod gives buttons for its menu (kindle.ui.mod_tab): the Mods screen's Mod menus tab opens
// the menu, those buttons and the mod's settings, over it (as does the mod's title menu entry,
// front_menu.rs). Presses go to the mod's functions at Kindle's next frame (take_presses); a
// paste button gives them the clipboard's text, read as the player pressed it (a mod reads the
// clipboard no other way).

/// A row of a mod's menu: a button, or a line of information (no press).
#[derive(Clone, PartialEq)]
pub struct ModTabButton {
    pub label: String,
    pub help: String,
    /// Its press gives the mod the clipboard's text.
    pub pastes: bool,
    pub info: bool,
}

/// A mod's menu buttons as the mod gave them.
struct ModTab {
    owner: String,
    title: String,
    buttons: Vec<ModTabButton>,
    /// Which set of buttons this is (each set_mod_tab makes a new one).
    version: u64,
}

static MOD_TABS: Mutex<Vec<ModTab>> = Mutex::new(Vec::new());
static MOD_TAB_VERSION: AtomicU64 = AtomicU64::new(0);
/// Presses waiting for Kindle's frame: (owner, button label, the clipboard's text for a paste).
static PRESSES: Mutex<Vec<(String, String, Option<String>)>> = Mutex::new(Vec::new());
const MOD_TAB_BUTTONS: usize = 16;
const TITLE_MAX: usize = 24;
const LABEL_MAX: usize = 48;
const HELP_MAX: usize = 300;
/// The clipboard's text given to a paste button, at most.
const PASTE_MAX: usize = 1024;
/// The Mod menus tab's rows, at most (a tab's).
const MENU_ROWS: usize = 24;

/// A mod's menu buttons (its title and rows), replacing its earlier ones.
pub fn set_mod_tab(owner: &str, title: &str, buttons: Vec<ModTabButton>) -> Result<(), String> {
    if title.is_empty() || title.chars().count() > TITLE_MAX {
        return Err(format!("a tab's title is 1 to {TITLE_MAX} characters"));
    }
    if buttons.is_empty() || buttons.len() > MOD_TAB_BUTTONS {
        return Err(format!("a tab has 1 to {MOD_TAB_BUTTONS} rows"));
    }
    for (i, b) in buttons.iter().enumerate() {
        if b.label.is_empty() || b.label.chars().count() > LABEL_MAX || b.help.chars().count() > HELP_MAX {
            return Err(format!("a row's label is 1 to {LABEL_MAX} characters, its help at most {HELP_MAX}"));
        }
        if !b.info && buttons[..i].iter().any(|other| !other.info && other.label == b.label) {
            return Err(format!("two buttons are labelled {:?}", b.label));
        }
    }
    let mut tabs = lock(&MOD_TABS);
    // (The same buttons again keep their version: an open menu showing them stays.)
    if tabs.iter().any(|t| t.owner == owner && t.title == title && t.buttons == buttons) {
        return Ok(());
    }
    tabs.retain(|t| t.owner != owner);
    crate::debug!("mods screen: {owner}'s menu {title:?} ({} rows)", buttons.len());
    let version = MOD_TAB_VERSION.fetch_add(1, Ordering::Relaxed) + 1;
    tabs.push(ModTab { owner: owner.to_string(), title: title.to_string(), buttons, version });
    Ok(())
}

/// The version of a mod's menu buttons, None without them.
fn mod_tab_version(owner: &str) -> Option<u64> {
    lock(&MOD_TABS).iter().find(|t| t.owner == owner).map(|t| t.version)
}

/// A mod unloads: its menu goes.
pub fn remove_mod_tab(owner: &str) {
    lock(&MOD_TABS).retain(|t| t.owner != owner);
    lock(&PRESSES).retain(|p| p.0 != owner);
}

/// The presses of mods' buttons since the last frame: (owner, the button's label, a paste's text).
pub fn take_presses() -> Vec<(String, String, Option<String>)> {
    std::mem::take(&mut *lock(&PRESSES))
}

fn has_menu(m: &ui_bridge::ModRow) -> bool {
    !m.settings.is_empty() || lock(&MOD_TABS).iter().any(|t| t.owner == m.id)
}

/// A mod's menu: its buttons (if it gave some) and its settings (if it has any), and its title.
fn mod_menu(b: &mut Builder, owner: &str) -> Option<(String, Vec<Tab>)> {
    let mods = ui_bridge::mods();
    let m = mods.iter().find(|m| m.id == owner);
    let buttons = lock(&MOD_TABS).iter().find(|t| t.owner == owner).map(|t| (t.title.clone(), t.buttons.clone()));
    let mut tabs = Vec::new();
    if let Some((title, buttons)) = buttons {
        b.tab = tabs.len();
        let items = buttons
            .iter()
            .map(|x| {
                let press = Press::ModTab { owner: owner.to_string(), label: x.label.clone(), pastes: x.pastes };
                let slot = if x.info { None } else { b.button(press, false) };
                Item::Button(Button { label: wide_kept(&x.label), help: wide_kept(&x.help), slot, greyed: false })
            })
            .collect();
        tabs.push(Tab { label: wide_kept(&title), items, icon: Some(menu_movies::MODS_TAB) });
    }
    if let Some(m) = m
        && !m.settings.is_empty()
    {
        b.tab = tabs.len();
        let items = m.settings.iter().take(PAGE_ROWS).map(|s| setting_row(b, &m.id, s)).collect();
        tabs.push(Tab { label: wide_kept("Settings"), items, icon: Some(menu_movies::KINDLE_TAB) });
    }
    if tabs.is_empty() {
        return None;
    }
    Some((m.map_or_else(|| owner.to_string(), |m| m.name.clone()), tabs))
}

/// Whether a whole-number setting's range fits the System screen's slider (its byte is the value).
fn byte_range(min: i64, max: i64) -> bool {
    (0..=255).contains(&min) && (0..=255).contains(&max) && min < max
}

/// A mod's setting as a row: a switch, a slider of whole numbers 0 to 255, or a row that opens
/// Kindle's Mods screen at the mod for a setting the System screen can't show.
fn setting_row(b: &mut Builder, id: &str, s: &ui_bridge::SettingRow) -> Item {
    let label = wide_kept(if s.label.is_empty() { &s.name } else { &s.label });
    let help = wide_kept(&s.description);
    match (&s.kind, &s.value) {
        (SettingKind::Bool, SettingValue::Bool(on)) => {
            let default = matches!(s.default, SettingValue::Bool(true));
            let slot = b.value(*on as u8, default as u8);
            Item::Row(RowSpec { label, help, row: Row::Bool { id: id.to_string(), name: s.name.clone() }, slot })
        }
        (SettingKind::Int { min, max }, SettingValue::Int(v)) if byte_range(*min, *max) => {
            let default = match s.default {
                SettingValue::Int(d) => d,
                _ => *v,
            };
            let slot = b.value((*v).clamp(*min, *max) as u8, default.clamp(*min, *max) as u8);
            Item::Row(RowSpec { label, help, row: Row::Int { id: id.to_string(), name: s.name.clone(), min: *min, max: *max }, slot })
        }
        (SettingKind::Choice(list), SettingValue::Choice(c)) if two(list).is_some() => {
            let index = |c: &str| list.iter().position(|x| x == c).unwrap_or(0) as u8;
            let default = match &s.default {
                SettingValue::Choice(d) => index(d),
                _ => index(c),
            };
            let slot = b.value(index(c), default);
            let choices = two(list).unwrap_or_default();
            Item::Row(RowSpec { label, help, row: Row::SettingChoice { id: id.to_string(), name: s.name.clone(), choices }, slot })
        }
        _ => {
            let help = match s.description.trim() {
                "" => "Change it in Kindle's settings screen.".to_string(),
                d => format!("{d} Change it in Kindle's settings screen."),
            };
            let slot = b.button(Press::Settings { owner: Some(id.to_string()) }, false);
            Item::Button(Button { label, help: wide_kept(&help), slot, greyed: false })
        }
    }
}

// ── Kindle's System screens ────────────────────────────────────────────────────────────────────

struct Bytes(UnsafeCell<[u8; MAX_ROWS]>);
// The game's menu code and `tick` both run on the game thread.
unsafe impl Sync for Bytes {}

/// The bytes the rows edit, and the defaults the Defaults button restores.
static VALUES: Bytes = Bytes(UnsafeCell::new([0; MAX_ROWS]));
static DEFAULTS: Bytes = Bytes(UnsafeCell::new([0; MAX_ROWS]));
/// The values last sent as requests.
static APPLIED: Mutex<[u8; MAX_ROWS]> = Mutex::new([0; MAX_ROWS]);

/// What a System screen of Kindle's shows: the first is opened from a menu or from play, each
/// after it over the one before, from one of its rows.
struct Plan {
    tabs: Vec<Tab>,
    /// Shown in place of "System" while it's the top one.
    title: Option<String>,
    /// The dialog once the game's factory has made it, else 0.
    dialog: usize,
    /// Each tab's window once the game has made it, else 0.
    windows: [usize; WINDOWS.len()],
    /// The mod's screen it shows (kindle.ui.screen), as shown or last updated in place.
    screen: Option<screens::Screen>,
    /// The tab of the row last pressed: a screen opened from it goes over that tab's window.
    pressed_tab: usize,
    /// Its value bytes and button slots (one opened over it takes those after).
    values: Range<usize>,
    buttons: Range<usize>,
    /// Each tab's rows past its window's room, shown by its More row.
    more: Vec<Vec<Item>>,
    /// When its job was made (ms): one whose dialog never comes counts as closed.
    asked_ms: u64,
    /// The game's menu was seen open with it (for when its end can't be hooked).
    seen: bool,
    /// Its dialog is gone (or never came).
    closed: bool,
    /// Its dialog never came: Kindle draws its screen.
    failed: bool,
    /// Closed by Kindle (a mod closed a screen, or changed one's layout): the mod isn't told it
    /// closed, and the mods' top screen opens anew.
    reopen: bool,
    /// The mod whose menu it shows, and the version of the buttons it shows.
    menu: Option<(String, Option<u64>)>,
}

impl Plan {
    /// Its rows with values, those past its tabs' room included.
    fn rows(&self) -> impl Iterator<Item = &RowSpec> {
        let past = self.more.iter().flatten().filter_map(|i| match i {
            Item::Row(r) => Some(r),
            Item::Button(_) => None,
        });
        self.tabs.iter().flat_map(Tab::rows).chain(past)
    }
}

static PLANS: Mutex<Vec<Plan>> = Mutex::new(Vec::new());
/// A dialog whose job was made but not yet constructed is Kindle's (the top plan's).
static PENDING: AtomicBool = AtomicBool::new(false);
static TABS_ADDED: AtomicUsize = AtomicUsize::new(0);
/// The plan and window index being made, while the game makes a window of Kindle's.
static BUILDING: Mutex<Option<(usize, i32)>> = Mutex::new(None);
/// A plan whose dialog hasn't come by then (ms) is given up.
const PENDING_MS: u64 = 3000;
/// Kindle's Mods screen from a row: EDITOR_IDLE, EDITOR_ASKED, EDITOR_OPEN, then frames left
/// to follow it after it closes.
static EDITOR: AtomicU32 = AtomicU32::new(EDITOR_IDLE);
const EDITOR_IDLE: u32 = 0;
const EDITOR_ASKED: u32 = 1;
const EDITOR_OPEN: u32 = 2;
static HOOKED: OnceLock<bool> = OnceLock::new();
/// The System screen's end is hooked: screens can open over Kindle's.
static DIALOG_END_HOOKED: AtomicBool = AtomicBool::new(false);

/// Makes a plan's rows: its value bytes and button slots follow those of the plans under it.
struct Builder {
    level: usize,
    /// The tab being made (a button's press knows its tab).
    tab: usize,
    next_value: usize,
}

impl Builder {
    /// For the plan at `level` (0 for the first): plans from that level up are dropped (their
    /// screens are closed: the one under is in use).
    fn at(level: usize) -> Self {
        let mut plans = lock(&PLANS);
        plans.truncate(level);
        let (next_value, buttons) = plans.last().map_or((0, 0), |p| (p.values.end, p.buttons.end));
        lock(&PRESS_SLOTS).truncate(buttons);
        Self { level, tab: 0, next_value }
    }

    /// A row's value byte and its default (the Defaults button's).
    fn value(&mut self, value: u8, default: u8) -> usize {
        let at = self.next_value.min(MAX_ROWS - 1);
        unsafe {
            (*VALUES.0.get())[at] = value;
            (*DEFAULTS.0.get())[at] = default;
        }
        lock(&APPLIED)[at] = value;
        self.next_value += 1;
        at
    }

    /// A button's slot, greyed or not; None once all are taken.
    fn button(&self, press: Press, greyed: bool) -> Option<usize> {
        let mut slots = lock(&PRESS_SLOTS);
        let slot = slots.len();
        GREYED.get(slot)?.store(greyed, Ordering::Relaxed);
        slots.push(PressSlot { press, plan: self.level, tab: self.tab });
        Some(slot)
    }

    /// The plan, on top of those under it; a tab longer than its window holds ends in a More row.
    fn finish(mut self, title: Option<String>, mut tabs: Vec<Tab>, screen: Option<screens::Screen>) {
        let mut more = Vec::new();
        for (i, tab) in tabs.iter_mut().enumerate() {
            let room = ROOM.get(i).copied().unwrap_or(ROOM[0]);
            if tab.items.len() <= room {
                more.push(Vec::new());
                continue;
            }
            more.push(tab.items.split_off(room - 1));
            self.tab = i;
            tab.items.push(Item::Button(Button {
                label: wide_kept("More"),
                help: wide_kept("The rest of this list."),
                slot: self.button(Press::More { tab: i }, false),
                greyed: false,
            }));
        }
        let buttons_end = lock(&PRESS_SLOTS).len();
        let mut plans = lock(&PLANS);
        plans.truncate(self.level);
        let (values, buttons) = plans.last().map_or((0, 0), |p| (p.values.end, p.buttons.end));
        plans.push(Plan {
            tabs,
            title,
            dialog: 0,
            windows: [0; WINDOWS.len()],
            screen,
            pressed_tab: 0,
            values: values..self.next_value.max(values),
            buttons: buttons..buttons_end.max(buttons),
            more,
            asked_ms: kindle_core::mods::now_ms(),
            seen: false,
            closed: false,
            failed: false,
            reopen: false,
            menu: None,
        });
    }
}

/// The System screen's title while the top plan's dialog is open: its own, or the game's.
fn show_title(title: Option<&str>) {
    match title.filter(|t| !t.is_empty()) {
        Some(t) => {
            let _ = crate::text_override::set_or_add(&[MENU_TEXT], TITLE_TEXT, t, "kindle");
        }
        None => {
            crate::text_override::reset(&[MENU_TEXT], TITLE_TEXT);
        }
    }
}

/// Before the top plan's job is made: its title shown, the dialog the job makes Kindle's.
fn begin_open() {
    let title = lock(&PLANS).last().and_then(|p| p.title.clone());
    show_title(title.as_deref());
    EDITOR.store(EDITOR_IDLE, Ordering::Relaxed);
    PENDING.store(true, Ordering::Relaxed);
    TABS_ADDED.store(0, Ordering::Relaxed);
}

/// The job opening the top plan from a menu entry's call (`base`: its window +0x48, or 0).
fn open_job(g: &Game, out: *mut usize, base: usize, in_game: bool) -> *mut usize {
    begin_open();
    let at = if in_game { g.open_in_game } else { g.open_from_title };
    let open: extern "C" fn(*mut usize, usize) -> *mut usize = unsafe { std::mem::transmute(at) };
    open(out, base)
}

fn menu_base(ctx: usize) -> usize {
    if ctx == 0 { 0 } else { ctx + MENU_BASE_AT }
}

fn ready() -> Option<&'static Game> {
    let g = game()?;
    HOOKED.get_or_init(|| install(g)).then_some(g)
}

/// Opens the native Mods screen from a menu entry's call (job `out`, the menu window `ctx`);
/// None if it can't, so the caller can open Kindle's own Mods screen.
pub fn open(out: *mut usize, ctx: usize) -> Option<*mut usize> {
    let g = ready()?;
    // (With no mods it still has the Kindle tab; it opened Kindle's own screen instead, a build
    // with none, 2026-10-14.)
    let mods = ui_bridge::mods();
    let mut b = Builder::at(0);
    let tabs = mods_screen(&mut b, &mods);
    b.finish(Some("Mods".to_string()), tabs, None);
    Some(open_job(g, out, menu_base(ctx), ui_bridge::IN_WORLD.load(Ordering::Relaxed)))
}

/// Opens a mod's menu (its title menu entry, front_menu.rs) from a menu entry's call; None if it
/// can't (or the mod has no menu).
pub fn open_mod_screen(out: *mut usize, ctx: usize, owner: &str) -> Option<*mut usize> {
    let g = ready()?;
    let version = mod_tab_version(owner);
    let mut b = Builder::at(0);
    let (title, tabs) = mod_menu(&mut b, owner)?;
    b.finish(Some(title), tabs, None);
    mark_menu(owner, version);
    Some(open_job(g, out, menu_base(ctx), ui_bridge::IN_WORLD.load(Ordering::Relaxed)))
}

/// The top plan shows `owner`'s menu, of these buttons.
fn mark_menu(owner: &str, version: Option<u64>) {
    if let Some(p) = lock(&PLANS).last_mut() {
        p.menu = Some((owner.to_string(), version));
    }
}

/// A tab of a mod's creation screen page: its title, icon and switches (key, label, help,
/// default).
pub struct PageTabSpec {
    pub title: String,
    pub icon: Option<usize>,
    pub switches: Vec<(String, String, String, bool)>,
}

/// A page has at most this many tabs (the windows Kindle's tabs use), each this many switches.
pub const PAGE_TABS: usize = WINDOWS.len();
pub const PAGE_ROWS: usize = 24;

/// Opens a mod's creation screen page (the System screen with its tabs of switches) from its
/// row's call; None if it can't.
pub fn open_page(out: *mut usize, ctx: usize, owner: &str, page: Vec<PageTabSpec>) -> Option<*mut usize> {
    let g = ready()?;
    let mut b = Builder::at(0);
    let tabs = plan_page(&mut b, owner, page);
    b.finish(None, tabs, None);
    Some(open_job(g, out, menu_base(ctx), false))
}

/// The page's tabs, from what the character being made has now.
fn plan_page(b: &mut Builder, owner: &str, page: Vec<PageTabSpec>) -> Vec<Tab> {
    let mut tabs = Vec::new();
    for t in page.into_iter().take(PAGE_TABS) {
        b.tab = tabs.len();
        let items = t
            .switches
            .into_iter()
            .take(PAGE_ROWS)
            .map(|(key, label, help, default)| {
                let slot = b.value(crate::char_make::switch(owner, &key) as u8, default as u8);
                Item::Row(RowSpec { label: wide_kept(&label), help: wide_kept(&help), row: Row::Choice { owner: owner.to_string(), key }, slot })
            })
            .collect();
        tabs.push(Tab { label: wide_kept(&t.title), items, icon: t.icon });
    }
    tabs
}

/// Opens a plan over plan `level`, from its tab `tab`'s window, as the game's rows open screens
/// over their own; false if it can't (and no plan is kept). `make` gives the plan's title, tabs
/// and the mod's screen it shows.
fn open_over(level: usize, tab: usize, make: impl FnOnce(&mut Builder) -> Option<(Option<String>, Vec<Tab>, Option<screens::Screen>)>) -> bool {
    let (Some(g), Some(o)) = (game(), over()) else { return false };
    // (Without its end hooked, Kindle couldn't tell when it closes.)
    if !DIALOG_END_HOOKED.load(Ordering::Relaxed) {
        return false;
    }
    let window = match lock(&PLANS).get(level) {
        Some(p) if !p.closed && p.dialog != 0 => p.windows.get(tab).copied().filter(|w| *w != 0).unwrap_or(p.windows[0]),
        _ => 0,
    };
    let idle: extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(o.idle) };
    if window == 0 || idle(window + JOB_RUNNER_AT) == 0 {
        return false;
    }
    let mut b = Builder::at(level + 1);
    let Some((title, tabs, screen)) = make(&mut b) else { return false };
    crate::debug!("native mods: {:?} opens over screen {level}", title.as_deref().unwrap_or(""));
    b.finish(title, tabs, screen);
    begin_open();
    let at = if ui_bridge::IN_WORLD.load(Ordering::Relaxed) { g.open_in_game } else { g.open_from_title };
    let open: extern "C" fn(*mut usize, usize) -> *mut usize = unsafe { std::mem::transmute(at) };
    let push: extern "C" fn(usize, *mut usize) = unsafe { std::mem::transmute(o.push) };
    let mut job: usize = 0;
    open(&mut job, window + MENU_BASE_AT);
    push(window + JOB_RUNNER_AT, &mut job);
    true
}

/// The Mods screen's tabs: Kindle's, the mods on or off, and the mods' menus.
fn mods_screen(b: &mut Builder, mods: &[ui_bridge::ModRow]) -> Vec<Tab> {
    let developer = ui_bridge::DEVELOPER.load(Ordering::Relaxed) as u8;
    let developer = Item::Row(RowSpec {
        label: wide_kept("Developer overlay"),
        help: wide_kept("Kindle's developer overlay: console, log, game state and Find."),
        row: Row::Developer,
        slot: b.value(developer, 0),
    });
    let all = Item::Button(Button {
        label: wide_kept("All mod settings"),
        help: wide_kept("Kindle's Mods screen: every mod and all its settings."),
        slot: b.button(Press::Settings { owner: None }, false),
        greyed: false,
    });
    let mut tabs = vec![Tab { label: wide_kept("Kindle"), items: vec![developer, all], icon: Some(menu_movies::KINDLE_TAB) }];
    // (No mods: no tab, rather than one with nothing in it.)
    if !mods.is_empty() {
        b.tab = tabs.len();
        let items = mods
            .iter()
            .map(|m| {
                Item::Row(RowSpec {
                    label: wide_kept(&m.name),
                    help: wide_kept(&format!("{} {}", m.name, m.version)),
                    row: Row::Enabled { id: m.id.clone() },
                    slot: b.value(!m.disabled as u8, 1),
                })
            })
            .collect();
        tabs.push(Tab { label: wide_kept("Mods"), items, icon: Some(menu_movies::MODS_TAB) });
    }
    let menus: Vec<&ui_bridge::ModRow> = mods.iter().filter(|m| has_menu(m)).take(MENU_ROWS).collect();
    if !menus.is_empty() {
        b.tab = tabs.len();
        let items = menus
            .iter()
            .map(|m| {
                Item::Button(Button {
                    label: wide_kept(&m.name),
                    help: wide_kept(&format!("{}'s menu and settings.", m.name)),
                    slot: b.button(Press::ModMenu { owner: m.id.clone() }, false),
                    greyed: false,
                })
            })
            .collect();
        tabs.push(Tab { label: wide_kept("Mod menus"), items, icon: Some(menu_movies::MODS_TAB) });
    }
    tabs
}

// ── mods' screens (kindle.ui.screen) on the System screen ──────────────────────────────────────
//
// A mod's screen the System screen can show (buttons, switches, sliders, lines of text and
// headings, up to 4 tabs of up to their windows' ROOM rows) opens there: from play (no menu open),
// Kindle presses the menu key for the game and the pause menu's job maker (`MenuMan.IngameTop`)
// makes the System screen's job with the screen's tabs instead (seen on B 2026-11-08: it opens
// from play, and Back returns to play, not to the pause menu); over one of Kindle's System
// screens (a mod's menu, a screen opened from a screen's row), it opens over that one as the
// game's rows open theirs. Kindle draws the others (mod_screens.rs): pictures, details, typed
// text, key rows, choice lists of more than two, more tabs or rows, a screen Back mustn't close, and any opened
// over another menu (a bonfire's, a talk's). What the player does goes back to the mod as from
// Kindle's own screens (screens::report, screens::change). Back on the game's screen closes the
// mod's (Closed); a screen the mod closes, or changes the layout of, Kindle closes with Back
// (and shows the new layout the same way it showed the first).

/// The pause menu's job maker (`MenuMan.IngameTop`, +0xB3EBB0): `make(out job*, scene)` -> out.
/// The game calls it when the player opens the pause menu (Esc, Start); its first act is
/// answering MenuMan's request 8 (`MenuMan_SetRequest(8, 0)`).
const INGAME_TOP: &str = "40 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec e0 00 00 00 48 c7 44 24 38 fe ff ff ff \
    48 89 9c 24 30 01 00 00 48 8b 05";

static INGAME_TOP_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "pause menu job",
    pattern: INGAME_TOP,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(ingame_top_hook),
    without: "mods' screens are drawn by Kindle, not the game",
};

/// The mod's screen the next pause menu opens as.
static WANT: Mutex<Option<u64>> = Mutex::new(None);
/// The mod's menu the next pause menu opens as (its buttons changed while it was open).
static WANT_MENU: Mutex<Option<String>> = Mutex::new(None);
/// A mod's menu to open anew once the game's menu has closed and settled, the tabs whose More
/// rows were open over it, and until when (ms).
static MENU_AGAIN: Mutex<Option<(String, Vec<usize>, u64)>> = Mutex::new(None);
/// Once the mod's menu has opened anew: the tabs whose More rows open over it again, and until
/// when (ms).
static MORE_AGAIN: Mutex<Option<(Vec<usize>, u64)>> = Mutex::new(None);

/// Mods' screens on the System screen.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shown {
    None,
    /// The menu key pressed for it, the game's menu not made yet (Kindle draws it after `until`, ms).
    Asked {
        id: u64,
        until: u64,
    },
    /// One or more plans show mods' screens.
    Open,
}
static SHOWN: Mutex<Shown> = Mutex::new(Shown::None);
/// Screens Kindle draws: they don't fit the System screen, or it didn't open for them.
static DRAWN: Mutex<Vec<u64>> = Mutex::new(Vec::new());
/// Kindle closed the game's menu for a mod's screen: nothing more is done until then (ms).
static BACK_UNTIL: AtomicU64 = AtomicU64::new(0);
const BACK_WAIT_MS: u64 = 400;
/// After that, the mods' top screen opens anew once the game's menu has closed, until then (ms).
static REOPEN_UNTIL: AtomicU64 = AtomicU64::new(0);
const REOPEN_WAIT_MS: u64 = 3000;
/// When the game's menu was last seen open (ms): the menu key does nothing for a moment after it
/// closes (a press 2 frames after it, 2026-11-09).
static MENU_SEEN_MS: AtomicU64 = AtomicU64::new(0);
const MENU_SETTLE_MS: u64 = 500;
/// The System screen's title, "System": menu text 101003 (the movie's `_StaticText_101003`).
const MENU_TEXT: u32 = 200;
const TITLE_TEXT: u32 = 101_003;
/// The menu key is held this long; the game's menu is made within a few frames of it.
const PRESS_SECONDS: f64 = 0.1;
const ASK_MS: u64 = 700;
/// The most buttons a mod's screen has there.
const SCREEN_BUTTONS: usize = 64;

/// Whether Kindle draws this screen (mod_screens.rs); the others are the game's to show, or not
/// decided yet (Kindle decides at the frame it opens).
pub fn drawn_by_kindle(id: u64) -> bool {
    lock(&DRAWN).contains(&id)
}

fn shown_natively(id: u64) -> bool {
    lock(&PLANS).iter().any(|p| !p.closed && p.screen.as_ref().is_some_and(|s| s.id == id))
}

/// Why a screen can't be shown on the System screen, if it can't.
fn unfit(s: &screens::Screen) -> Option<&'static str> {
    if !s.closable {
        return Some("Back mustn't close it");
    }
    if s.tabs.len() > WINDOWS.len() {
        return Some("more than 4 tabs");
    }
    if s.tab.is_some_and(|t| t > 0) {
        return Some("it opens on another tab");
    }
    if s.has_details() || s.rows.iter().any(|r| r.icon.is_some()) {
        return Some("pictures or details");
    }
    let mut buttons = 0;
    for (tab, room) in ROOM.iter().enumerate().take(s.tabs.len().max(1)) {
        let rows = &s.rows[s.tab_rows(tab)];
        if rows.is_empty() || rows.len() > *room {
            return Some("a tab has no rows, or more than its window holds");
        }
        for r in rows {
            match &r.kind {
                RowKind::Button { .. } => buttons += 1,
                _ if r.disabled => {}
                RowKind::Input { .. } | RowKind::Key { .. } => return Some("text or key rows"),
                RowKind::Choice { choices, .. } if choices.len() != 2 => return Some("choice lists of more than two"),
                RowKind::Slider { min, max, step, .. } if !byte_slider(*min, *max, *step) => return Some("a slider not of whole numbers 0 to 255"),
                _ => {}
            }
        }
    }
    (buttons > SCREEN_BUTTONS).then_some("too many buttons")
}

/// What a row shows as a line of text: its text and value.
fn row_line(r: &screens::Row) -> String {
    let value = match &r.kind {
        RowKind::Toggle(on) => Some(if *on { "On".to_string() } else { "Off".to_string() }),
        RowKind::Choice { choices, index } => choices.get(*index).cloned(),
        RowKind::Slider { value, step, .. } => Some(screens::show_number(*value, *step)),
        RowKind::Input { value, .. } | RowKind::Key { value } => Some(value.clone()),
        RowKind::Button { value } | RowKind::Info { value } => value.clone(),
        RowKind::Header => None,
    };
    match value.filter(|v| !v.is_empty()) {
        Some(v) => format!("{}: {v}", r.text),
        None => r.text.clone(),
    }
}

/// Whether a slider's numbers fit the System screen's slider: its value is a byte, shown as it is.
fn byte_slider(min: f64, max: f64, step: f64) -> bool {
    let whole = |v: f64| v.fract() == 0.0 && (0.0..=255.0).contains(&v);
    whole(min) && whole(max) && whole(step) && step >= 1.0 && max > min
}

/// A byte slider's value as its byte.
fn slider_byte(min: f64, max: f64, value: f64) -> u8 {
    value.round().clamp(min, max) as u8
}

/// The System screen's tabs for a mod's screen.
fn plan_screen(b: &mut Builder, s: &screens::Screen) -> Vec<Tab> {
    let mut tabs = Vec::new();
    for tab in 0..s.tabs.len().max(1) {
        b.tab = tab;
        let title = match s.tabs.get(tab) {
            Some(t) => t.title.as_str(),
            None if s.subtitle.is_empty() => s.title.as_str(),
            None => s.subtitle.as_str(),
        };
        let mut items = Vec::new();
        for i in s.tab_rows(tab) {
            let r = &s.rows[i];
            let (label, help) = (wide_kept(&r.text), wide_kept(&r.help));
            let item = match &r.kind {
                RowKind::Toggle(on) if !r.disabled => {
                    let slot = b.value(*on as u8, *on as u8);
                    Item::Row(RowSpec { label, help, row: Row::ScreenSwitch { screen: s.id, row: i }, slot })
                }
                RowKind::Choice { choices, index } if !r.disabled && two(choices).is_some() => {
                    let slot = b.value(*index as u8, *index as u8);
                    let choices = two(choices).unwrap_or_default();
                    Item::Row(RowSpec { label, help, row: Row::ScreenChoice { screen: s.id, row: i, choices }, slot })
                }
                RowKind::Slider { min, max, step, value } if !r.disabled => {
                    let byte = slider_byte(*min, *max, *value);
                    let slot = b.value(byte, byte);
                    Item::Row(RowSpec { label, help, row: Row::ScreenSlider { screen: s.id, row: i, min: *min, max: *max, step: *step }, slot })
                }
                // A button greyed or not by its slot: a change of it shows in place.
                RowKind::Button { .. } => {
                    let slot = b.button(Press::Screen { id: s.id, row: i }, r.disabled);
                    Item::Button(Button { label: wide_kept(&row_line(r)), help, slot, greyed: r.disabled })
                }
                _ => Item::Button(Button { label: wide_kept(&row_line(r)), help, slot: None, greyed: r.disabled }),
            };
            items.push(item);
        }
        tabs.push(Tab { label: wide_kept(title), items, icon: Some(menu_movies::MODS_TAB) });
    }
    tabs
}

/// Whether an update of a screen changes only its switches' and sliders' values and whether its
/// buttons are greyed (shown in place); any other change opens the screen again.
fn same_layout(old: &screens::Screen, new: &screens::Screen) -> bool {
    let shape = |s: &screens::Screen| -> Vec<String> {
        let tabs = s.tabs.iter().map(|t| format!("tab {}@{}", t.title, t.start));
        let rows = s.rows.iter().map(|r| match &r.kind {
            RowKind::Toggle(_) if !r.disabled => format!("switch {} {}", r.text, r.help),
            RowKind::Choice { choices, .. } if !r.disabled => format!("choice {} {} {}", r.text, r.help, choices.join("|")),
            RowKind::Slider { min, max, step, .. } if !r.disabled => format!("slider {} {} {min} {max} {step}", r.text, r.help),
            RowKind::Button { .. } => format!("button {} {}", row_line(r), r.help),
            _ => format!("line {} {} {}", row_line(r), r.help, r.disabled),
        });
        tabs.chain(rows).chain([s.title.clone(), s.subtitle.clone()]).collect()
    };
    shape(old) == shape(new)
}

/// An update's new values in a plan's value bytes (taken as applied: the mod set them), and its
/// buttons greyed or not.
fn show_values(plan: &Plan, s: &screens::Screen) {
    let values = unsafe { &mut *VALUES.0.get() };
    let mut applied = lock(&APPLIED);
    for spec in plan.tabs.iter().flat_map(Tab::rows) {
        let now = match (&spec.row, s.rows.get(spec.row_index()).map(|r| &r.kind)) {
            (Row::ScreenSwitch { .. }, Some(RowKind::Toggle(on))) => *on as u8,
            (Row::ScreenSlider { min, max, .. }, Some(RowKind::Slider { value, .. })) => slider_byte(*min, *max, *value),
            (Row::ScreenChoice { .. }, Some(RowKind::Choice { index, .. })) => *index as u8,
            _ => continue,
        };
        values[spec.slot] = now;
        applied[spec.slot] = now;
    }
    let slots = lock(&PRESS_SLOTS);
    for slot in plan.buttons.clone() {
        if let Some(PressSlot { press: Press::Screen { row, .. }, .. }) = slots.get(slot)
            && let Some(r) = s.rows.get(*row)
        {
            GREYED[slot].store(r.disabled, Ordering::Relaxed);
        }
    }
}

impl RowSpec {
    /// A mod screen's row's index in its screen (0 for others).
    fn row_index(&self) -> usize {
        match self.row {
            Row::ScreenSwitch { row, .. } | Row::ScreenSlider { row, .. } | Row::ScreenChoice { row, .. } => row,
            _ => 0,
        }
    }
}

/// Whether the System screen can open from play now: in the world, no menu of the game's or
/// screen of Kindle's open.
fn can_open_here() -> bool {
    crate::game::player_loaded()
        && ui_bridge::IN_WORLD.load(Ordering::Relaxed)
        && !crate::native_ui::menu_open()
        && !ui_bridge::OVERLAY_VISIBLE.load(Ordering::Relaxed)
}

fn hook_top() -> bool {
    static TOP_HOOKED: OnceLock<bool> = OnceLock::new();
    ready().is_some()
        && *TOP_HOOKED.get_or_init(|| match patterns::find_unique("pause menu job", INGAME_TOP) {
            Some(at) => INGAME_TOP_SITE.hook_at(at).map(std::mem::forget).is_ok(),
            None => false,
        })
}

fn draw_by_kindle(id: u64, why: &str) {
    crate::debug!("native screen: Kindle draws screen {id} ({why})");
    lock(&DRAWN).push(id);
}

fn ingame_top_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    let want = lock(&WANT).take();
    let want_menu = lock(&WANT_MENU).take();
    let Some(g) = game() else { return f(r.rcx, r.rdx) };
    if let Some(owner) = want_menu {
        let version = mod_tab_version(&owner);
        let mut b = Builder::at(0);
        let Some((title, tabs)) = mod_menu(&mut b, &owner) else { return f(r.rcx, r.rdx) };
        crate::info!("native mods: the pause menu opens as {owner}'s menu");
        b.finish(Some(title), tabs, None);
        mark_menu(&owner, version);
        if let Some(m) = crate::ds3::singleton_instance(crate::native_ui::MENU_MAN) {
            unsafe { std::ptr::write_volatile((m + crate::native_ui::ESC_MENU_AT) as *mut i32, 0) };
        }
        return open_job(g, r.rcx as *mut usize, r.rdx as usize, true) as usize;
    }
    let Some(id) = want else { return f(r.rcx, r.rdx) };
    let Some(s) = screens::top().filter(|s| s.id == id) else { return f(r.rcx, r.rdx) };
    crate::info!("native screen: the pause menu opens as {}'s screen {:?}", s.owner, s.title);
    let mut b = Builder::at(0);
    let tabs = plan_screen(&mut b, &s);
    b.finish(Some(s.title.clone()), tabs, Some(s));
    *lock(&SHOWN) = Shown::Open;
    // As the pause menu's job maker does first: its request (8) answered, else the pause menu
    // opens when this screen closes.
    if let Some(m) = crate::ds3::singleton_instance(crate::native_ui::MENU_MAN) {
        unsafe { std::ptr::write_volatile((m + crate::native_ui::ESC_MENU_AT) as *mut i32, 0) };
    }
    // The scene the pause menu would be built on is the job's base, as a menu window's +0x48 is.
    open_job(g, r.rcx as *mut usize, r.rdx as usize, true) as usize
}

/// A screen opened over plan `level` (from its tab `tab`), or drawn by Kindle if it can't be.
fn open_screen_over(level: usize, tab: usize, s: screens::Screen) -> bool {
    let id = s.id;
    if let Some(why) = unfit(&s) {
        draw_by_kindle(id, why);
        return false;
    }
    let opened = open_over(level, tab, move |b| {
        let tabs = plan_screen(b, &s);
        Some((Some(s.title.clone()), tabs, Some(s)))
    });
    if !opened {
        draw_by_kindle(id, "it couldn't open over the System screen");
    }
    opened
}

/// Each frame: mods' screens shown on the System screen, kept in step with the mods' stack.
fn screens_tick() {
    if crate::native_ui::menu_open() {
        MENU_SEEN_MS.store(kindle_core::mods::now_ms(), Ordering::Relaxed);
    }
    let mut shown = lock(&SHOWN);
    match *shown {
        Shown::Asked { id, until } => {
            if screens::top().map(|s| s.id) != Some(id) {
                lock(&WANT).take();
                *shown = Shown::None;
            } else if kindle_core::mods::now_ms() > until {
                lock(&WANT).take();
                *shown = Shown::None;
                draw_by_kindle(id, "the pause menu didn't open");
            }
        }
        Shown::Open => {
            drop(shown);
            follow_screens();
        }
        Shown::None => {
            lock(&DRAWN).retain(|id| screens::is_open(*id));
            let Some(s) = screens::top() else { return };
            if drawn_by_kindle(s.id) || shown_natively(s.id) {
                return;
            }
            if let Some(why) = unfit(&s) {
                return draw_by_kindle(s.id, why);
            }
            // Over one of Kindle's System screens (a mod's menu): it opens over that one.
            let live = lock(&PLANS).iter().enumerate().next_back().filter(|(_, p)| !p.closed && p.dialog != 0).map(|(i, p)| (i, p.pressed_tab));
            if let Some((level, tab)) = live {
                if open_screen_over(level, tab, s) {
                    *shown = Shown::Open;
                }
                return;
            }
            // Opening anew: once the game's menu has closed and settled.
            let now = kindle_core::mods::now_ms();
            if now < REOPEN_UNTIL.load(Ordering::Relaxed) && (!can_open_here() || now < MENU_SEEN_MS.load(Ordering::Relaxed) + MENU_SETTLE_MS) {
                return;
            }
            if !can_open_here() {
                return draw_by_kindle(s.id, "not opened from play");
            }
            if !hook_top() {
                return draw_by_kindle(s.id, "the System screen or the pause menu wasn't found");
            }
            *lock(&WANT) = Some(s.id);
            if let Err(e) = crate::input_block::press("esc", PRESS_SECONDS) {
                lock(&WANT).take();
                return draw_by_kindle(s.id, &e);
            }
            *shown = Shown::Asked { id: s.id, until: kindle_core::mods::now_ms() + ASK_MS };
        }
    }
}

/// While plans show mods' screens: values and greyed buttons updated in place; a screen the mod
/// opened over the top one opened over it; a screen the mod closed, or changed the layout of,
/// closes the game's menu and the mods' top screen opens anew. (The keyboard has no key that backs
/// out of one screen: Esc closes the game's menu at any depth, the game's own screens too, seen
/// 2026-11-09; Back, a controller's or the mouse's, isn't Kindle's to press.)
fn follow_screens() {
    let now = kindle_core::mods::now_ms();
    let mut back = false;
    let mut over = None;
    {
        let mut plans = lock(&PLANS);
        if !plans.iter().any(|p| p.screen.is_some()) {
            *lock(&SHOWN) = Shown::None;
            return;
        }
        let top = plans.len() - 1;
        if plans[top].dialog == 0 || now < BACK_UNTIL.load(Ordering::Relaxed) {
            return;
        }
        let mut settled = true;
        for level in 0..plans.len() {
            let Some(shown) = plans[level].screen.clone() else { continue };
            match screens::get(shown.id) {
                // Closed by the mod.
                None => back = true,
                Some(s) if s.version == shown.version => continue,
                Some(s) if same_layout(&shown, &s) => {
                    show_values(&plans[level], &s);
                    plans[level].screen = Some(s);
                    continue;
                }
                Some(_) if level == top => back = true,
                // Shown anew once the one over it closes.
                Some(_) => {}
            }
            settled = false;
            break;
        }
        if settled
            && let Some(top_id) = plans[top].screen.as_ref().map(|s| s.id)
            && let Some(s) = screens::top()
            && s.id != top_id
            && !drawn_by_kindle(s.id)
        {
            let ids = screens::ids();
            let at = |id: u64| ids.iter().position(|i| *i == id);
            if at(s.id) > at(top_id) {
                over = Some((top, plans[top].pressed_tab, s));
            }
        }
    }
    if back {
        if crate::input_block::press("esc", PRESS_SECONDS).is_ok() {
            // None of the mods' screens closed with the menu: the top one opens anew.
            for p in lock(&PLANS).iter_mut() {
                p.reopen = true;
            }
            BACK_UNTIL.store(now + BACK_WAIT_MS, Ordering::Relaxed);
            REOPEN_UNTIL.store(now + REOPEN_WAIT_MS, Ordering::Relaxed);
        }
        return;
    }
    if let Some((level, tab, s)) = over {
        open_screen_over(level, tab, s);
    }
}

/// A mod's menu shown on the top System screen whose buttons the mod changed (practice-tool's
/// labels follow its state): the game's menu is closed (Esc closes it at any depth) and, in the
/// world, the mod's menu opens anew from play with the new buttons (and the More pages that were
/// open over it); with the buttons gone (the
/// mod unloaded), it stays closed. On the title screen it's left as it is.
fn menus_tick() {
    let now = kindle_core::mods::now_ms();
    let pages = lock(&MORE_AGAIN).clone();
    if let Some((mut tabs, until)) = pages {
        if now > until || tabs.is_empty() {
            *lock(&MORE_AGAIN) = None;
            return;
        }
        let top = {
            let plans = lock(&PLANS);
            match plans.last() {
                Some(p) if p.menu.is_some() && p.dialog != 0 && !p.closed => Some((plans.len() - 1, p.menu.clone())),
                _ => None,
            }
        };
        let Some((level, Some((owner, version)))) = top else { return };
        let tab = tabs.remove(0);
        if let Some(p) = lock(&PLANS).get_mut(level) {
            p.pressed_tab = tab;
        }
        if open_over(level, tab, |b| more(b, level, tab)) {
            mark_menu(&owner, version);
            *lock(&MORE_AGAIN) = Some((tabs, until));
        }
        return;
    }
    let again = lock(&MENU_AGAIN).clone();
    if let Some((owner, mores, until)) = again {
        if now > until {
            *lock(&MENU_AGAIN) = None;
            lock(&WANT_MENU).take();
            return;
        }
        if lock(&WANT_MENU).is_some() || !lock(&PLANS).is_empty() || *lock(&SHOWN) != Shown::None {
            return;
        }
        if !can_open_here() || now < MENU_SEEN_MS.load(Ordering::Relaxed) + MENU_SETTLE_MS || !hook_top() {
            return;
        }
        *lock(&MENU_AGAIN) = None;
        *lock(&WANT_MENU) = Some(owner);
        if crate::input_block::press("esc", PRESS_SECONDS).is_err() {
            lock(&WANT_MENU).take();
        } else if !mores.is_empty() {
            *lock(&MORE_AGAIN) = Some((mores, now + REOPEN_WAIT_MS));
        }
        return;
    }
    if !ui_bridge::IN_WORLD.load(Ordering::Relaxed) || now < BACK_UNTIL.load(Ordering::Relaxed) {
        return;
    }
    let stale = {
        let plans = lock(&PLANS);
        match plans.last() {
            Some(Plan { menu: Some((owner, version)), dialog, closed: false, .. }) if *dialog != 0 && mod_tab_version(owner) != *version => {
                // The menu's own plan, then its More pages over it (each opened from the tab
                // last pressed on the one under it).
                let first = plans.iter().position(|p| p.menu.as_ref().is_some_and(|m| m.0 == *owner)).unwrap_or(plans.len() - 1);
                let mores = plans[first..plans.len() - 1].iter().map(|p| p.pressed_tab).collect::<Vec<_>>();
                Some((owner.clone(), mod_tab_version(owner).is_some(), mores))
            }
            _ => None,
        }
    };
    let Some((owner, again, mores)) = stale else { return };
    if crate::input_block::press("esc", PRESS_SECONDS).is_err() {
        return;
    }
    crate::debug!("native mods: {owner}'s menu changed while open: shown anew");
    for p in lock(&PLANS).iter_mut() {
        p.reopen = true;
    }
    BACK_UNTIL.store(now + BACK_WAIT_MS, Ordering::Relaxed);
    if again {
        *lock(&MENU_AGAIN) = Some((owner, mores, now + REOPEN_WAIT_MS));
    }
}

/// Plans whose dialogs closed come off the top (with all over them); the mods hear of their
/// screens closing, and the title is the new top one's.
fn plans_tick() {
    let now = kindle_core::mods::now_ms();
    let menu = crate::native_ui::menu_open();
    let hooked = DIALOG_END_HOOKED.load(Ordering::Relaxed);
    let (gone, title) = {
        let mut plans = lock(&PLANS);
        for p in plans.iter_mut().filter(|p| !p.closed) {
            if p.dialog == 0 && now > p.asked_ms + PENDING_MS {
                (p.closed, p.failed) = (true, true);
                PENDING.store(false, Ordering::Relaxed);
            }
            p.seen |= p.dialog != 0 && menu;
            // (Without the end hooked: closed once the game's menus are.)
            if !hooked && p.seen && !menu {
                p.closed = true;
            }
        }
        let Some(first) = plans.iter().position(|p| p.closed) else { return };
        let gone: Vec<Plan> = plans.drain(first..).rev().collect();
        (gone, plans.last().and_then(|p| p.title.clone()))
    };
    show_title(title.as_deref());
    for p in gone {
        let Some(s) = p.screen else { continue };
        if p.failed {
            draw_by_kindle(s.id, "the System screen didn't open");
        } else if !p.reopen && screens::is_open(s.id) {
            screens::report(s.id, screens::ScreenEvent::Closed);
        }
    }
}

static FACTORY_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen factory",
    pattern: FACTORY,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(factory_hook),
    without: "Mods opens Kindle's screen",
};
static ADD_TAB_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab add",
    pattern: ADD_TAB,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(add_tab_hook),
    without: "Mods opens Kindle's screen",
};
static MAKE_WINDOW_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab window",
    pattern: MAKE_WINDOW,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(make_window_hook),
    without: "Mods opens Kindle's screen",
};
static BUILDER_0_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab builder 0",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(builder_hook::<0>),
    without: "Mods opens Kindle's screen",
};
static BUILDER_1_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab builder 1",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(builder_hook::<1>),
    without: "Mods opens Kindle's screen",
};
static BUILDER_2_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab builder 2",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(builder_hook::<2>),
    without: "Mods opens Kindle's screen",
};
static BUILDER_3_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen tab builder 3",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(builder_hook::<3>),
    without: "Mods opens Kindle's screen",
};
static DIALOG_END_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "native mods",
    name: "System screen closed",
    pattern: "",
    offset: 0,
    matches: 0,
    kind: crate::hook_sites::Kind::Retn(dialog_end_hook),
    without: "mods' menus open as Kindle's own screen, and mods' screens opened from Kindle's System screens are drawn by Kindle",
};

/// (The tab builders' patterns are made from parts: Game found them.)
fn install(g: &Game) -> bool {
    let builders = [&BUILDER_0_SITE, &BUILDER_1_SITE, &BUILDER_2_SITE, &BUILDER_3_SITE];
    let hooks =
        [(&FACTORY_SITE, g.factory), (&ADD_TAB_SITE, g.add_tab), (&MAKE_WINDOW_SITE, g.make_window)].into_iter().chain(builders.into_iter().zip(g.builders));
    for (site, at) in hooks {
        match site.hook_at(at) {
            Ok(h) => std::mem::forget(h), // for the life of the process
            Err(_) => return false,
        }
    }
    true
}

/// Whether the game's code at `at` is `pattern`.
fn code_is(at: usize, pattern: &str) -> bool {
    pattern.split_whitespace().enumerate().all(|(i, t)| t == "??" || u8::from_str_radix(t, 16).ok() == crate::memory::read::<u8>(at + i))
}

/// The System screen's deleting destructor (its dialog's vtable's second slot), hooked once.
fn hook_dialog_end(dialog: usize) {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let at = crate::memory::read_u64(dialog).and_then(|vtable| crate::memory::read_u64(vtable as usize + 8)).map(|a| a as usize);
        match at {
            Some(at) if code_is(at, DIALOG_END) => {
                if DIALOG_END_SITE.hook_at(at).map(std::mem::forget).is_ok() {
                    DIALOG_END_HOOKED.store(true, Ordering::Relaxed);
                }
            }
            _ => crate::warn!("native mods: the System screen's end isn't as expected; mods' menus open as Kindle's own screen"),
        }
    });
}

fn dialog_end_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let dialog = r.rcx as usize;
    for p in lock(&PLANS).iter_mut().filter(|p| p.dialog == dialog) {
        p.closed = true;
    }
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx, r.rdx)
}

/// A row's byte for the mods as they are now.
fn current(row: &Row, mods: &[ui_bridge::ModRow]) -> Option<u8> {
    let find = |id: &str| mods.iter().find(|m| m.id == id);
    match row {
        Row::Developer => Some(ui_bridge::DEVELOPER.load(Ordering::Relaxed) as u8),
        Row::Enabled { id } => find(id).map(|m| !m.disabled as u8),
        Row::Bool { id, name } | Row::Int { id, name, .. } => {
            let s = find(id)?.settings.iter().find(|s| s.name == *name)?;
            match (row, &s.value) {
                (Row::Bool { .. }, SettingValue::Bool(on)) => Some(*on as u8),
                (Row::Int { min, max, .. }, SettingValue::Int(v)) => Some((*v).clamp(*min, *max) as u8),
                _ => None,
            }
        }
        Row::SettingChoice { id, name, choices } => {
            let s = find(id)?.settings.iter().find(|s| s.name == *name)?;
            match &s.value {
                SettingValue::Choice(c) => choices.iter().position(|x| x == c).map(|i| i as u8),
                _ => None,
            }
        }
        Row::Choice { owner, key } => Some(crate::char_make::switch(owner, key) as u8),
        Row::ScreenSwitch { .. } | Row::ScreenSlider { .. } | Row::ScreenChoice { .. } => None,
    }
}

/// The request for a row's byte (a creation screen page's rows change char_make.rs directly).
fn request(row: &Row, p: u8) -> UiRequest {
    match row {
        Row::Choice { .. } | Row::ScreenSwitch { .. } | Row::ScreenSlider { .. } | Row::ScreenChoice { .. } => {
            unreachable!("a page's or a screen's rows aren't requests")
        }
        Row::SettingChoice { id, name, choices } => UiRequest::SetSetting(id.clone(), name.clone(), SettingValue::Choice(choices[(p as usize).min(1)].clone())),
        Row::Developer => UiRequest::SetDeveloper(p != 0),
        Row::Enabled { id } => UiRequest::SetEnabled(id.clone(), p != 0),
        Row::Bool { id, name } => UiRequest::SetSetting(id.clone(), name.clone(), SettingValue::Bool(p != 0)),
        Row::Int { id, name, min, max } => UiRequest::SetSetting(id.clone(), name.clone(), SettingValue::Int((p as i64).clamp(*min, *max))),
    }
}

fn factory_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    let dialog = f(r.rcx, r.rdx);
    hook_dialog_end(dialog);
    let ours = PENDING.swap(false, Ordering::Relaxed);
    let mut plans = lock(&PLANS);
    if ours {
        if let Some(p) = plans.last_mut() {
            p.dialog = dialog;
        }
        crate::debug!("native mods: System screen {} is dialog {dialog:#x}", plans.len());
    } else {
        // The game's own System screen opens from its menus alone: Kindle's are all closed.
        for p in plans.iter_mut() {
            p.closed = true;
        }
    }
    dialog
}

/// While Kindle's dialog is constructed: Kindle's tabs instead of the game's.
fn add_tab_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    if !PENDING.load(Ordering::Relaxed) {
        return f(r.rcx, r.rdx);
    }
    let i = TABS_ADDED.fetch_add(1, Ordering::Relaxed);
    let tab = lock(&PLANS).last().and_then(|p| p.tabs.get(i)).map(|t| (t.label.as_ptr() as usize, t.icon));
    let Some((label, icon)) = tab else { return 0 };
    let entry = r.rdx as usize;
    unsafe {
        *(entry as *mut usize) = label;
        *((entry + TAB_WINDOW_AT) as *mut i32) = WINDOWS[i];
        // Kindle's icons follow the game's in the sprites (menu_movies.rs adds them).
        if let Some(k) = icon.filter(|_| menu_movies::tab_icons_added()) {
            *((entry + TAB_ICON_AT) as *mut i32) = (menu_movies::KINDLE_ICON + k + 1) as i32;
        }
    }
    f(r.rcx, r.rdx)
}

fn make_window_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    let level = {
        let plans = lock(&PLANS);
        if PENDING.load(Ordering::Relaxed) {
            plans.len().checked_sub(1)
        } else {
            plans.iter().rposition(|p| !p.closed && p.dialog != 0 && r.rcx as usize == p.dialog + WINDOWS_AT)
        }
    };
    if let Some(level) = level {
        *lock(&BUILDING) = Some((level, r.rdx as i32));
    }
    let window = f(r.rcx, r.rdx);
    if level.is_some() {
        *lock(&BUILDING) = None;
    }
    window
}

fn builder_hook<const TAB: usize>(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let building = *lock(&BUILDING);
    if let Some((level, index)) = building
        && index == WINDOWS[TAB]
    {
        let window = r.rcx as usize;
        let _ = std::panic::catch_unwind(|| {
            hide_parts(window, index);
            build_rows(window, level, TAB);
        });
        return 0;
    }
    let f: extern "C" fn(u64, u64) -> usize = unsafe { std::mem::transmute(original) };
    f(r.rcx, r.rdx)
}

#[repr(C, align(16))]
struct Buffer<const N: usize>([u8; N]);

/// Hides the parts of a game window that only its own builder would fill in.
fn hide_parts(window: usize, index: i32) {
    let Some(g) = game() else { return };
    let find: extern "C" fn(usize, usize, *const u8) -> usize = unsafe { std::mem::transmute(g.find_part) };
    let set_visible: extern "C" fn(usize, u8) = unsafe { std::mem::transmute(g.set_visible) };
    let release: extern "C" fn(usize) = unsafe { std::mem::transmute(g.release_part) };
    for (_, name) in HIDDEN_PARTS.iter().filter(|(w, _)| *w == index) {
        let name: Vec<u8> = name.bytes().chain(std::iter::once(0)).collect();
        let mut out = Buffer([0u8; 0x80]);
        let at = out.0.as_mut_ptr() as usize;
        let part = find(window + WINDOW_CLIP_AT, at, name.as_ptr());
        if part != 0 {
            set_visible(part, 0);
        }
        release(at + PART_RELEASE_AT);
    }
}

fn build_rows(window: usize, level: usize, tab: usize) {
    let (Some(g), Some(from_text)) = (game(), front_menu::label_from_text()) else { return };
    let from_text: extern "C" fn(usize, *const u16) -> usize = unsafe { std::mem::transmute(from_text) };
    let row_list: extern "C" fn(usize, usize, usize, usize, usize, u64) -> usize = unsafe { std::mem::transmute(g.row_list) };
    let row_slider: extern "C" fn(usize, usize, usize, usize, usize) -> usize = unsafe { std::mem::transmute(g.row_slider) };
    let on_off: extern "C" fn(usize) -> usize = unsafe { std::mem::transmute(g.on_off) };
    let destroy: extern "C" fn(usize) = unsafe { std::mem::transmute(g.destroy_choice) };
    let row_button: extern "C" fn(usize, usize, *const Function, *const Function, *const Function) -> usize = unsafe { std::mem::transmute(g.row_button) };
    let mut plans = lock(&PLANS);
    let Some(plan) = plans.get_mut(level) else { return };
    if let Some(w) = plan.windows.get_mut(tab) {
        *w = window;
    }
    let Some(tab) = plan.tabs.get(tab) else { return };
    let (values, defaults) = (VALUES.0.get() as usize, DEFAULTS.0.get() as usize);
    for item in &tab.items {
        let mut label = Buffer([0u8; LABEL_SIZE]);
        let at = label.0.as_mut_ptr() as usize;
        let spec = match item {
            Item::Row(spec) => spec,
            Item::Button(b) => {
                from_text(at, b.label.as_ptr());
                from_text(at + LABEL_HELP_AT, b.help.as_ptr());
                let on_press = b.slot.map_or_else(Function::empty, |s| Function::of(PRESS_CALLABLES[s].callable()));
                let greyed = match b.slot {
                    Some(s) => Function::of(GREYED_CALLABLES[s].callable()),
                    None if b.greyed => Function::of(&ALWAYS),
                    None => Function::empty(),
                };
                let key_guide = Function::of(if b.slot.is_some() { &ALWAYS } else { &NEVER });
                row_button(window, at, &on_press, &greyed, &key_guide);
                continue;
            }
        };
        from_text(at, spec.label.as_ptr());
        from_text(at + LABEL_HELP_AT, spec.help.as_ptr());
        let (value, default) = (values + spec.slot, defaults + spec.slot);
        let range = match spec.row {
            Row::Int { min, max, .. } => [min as i32, max as i32, 1, 0],
            Row::ScreenSlider { min, max, step, .. } => [min as i32, max as i32, step as i32, 0],
            _ => {
                let mut list = Buffer([0u8; CHOICE_LIST_SIZE]);
                let choices = list.0.as_mut_ptr() as usize;
                match spec.row.choices() {
                    // As the game's On/Off list (+0xAAB920): each choice its value byte, then its
                    // label and help; the list holds two (+0xAB6FA0).
                    Some(two) => {
                        for (i, text) in two.iter().enumerate() {
                            let entry = choices + i * CHOICE_SIZE;
                            unsafe { *(entry as *mut u8) = i as u8 };
                            from_text(entry + CHOICE_LABEL_AT, wide_kept(text).as_ptr());
                            from_text(entry + CHOICE_LABEL_AT + LABEL_HELP_AT, wide_kept("").as_ptr());
                        }
                        unsafe { *((choices + CHOICE_COUNT_AT) as *mut usize) = two.len() };
                    }
                    None => {
                        on_off(choices);
                    }
                }
                row_list(window, at, value, choices, default, 1);
                let count = unsafe { *((choices + CHOICE_COUNT_AT) as *const usize) };
                for i in 0..count.min(CHOICE_LIST_SIZE / CHOICE_SIZE) {
                    destroy(choices + i * CHOICE_SIZE);
                }
                continue;
            }
        };
        row_slider(window, at, value, range.as_ptr() as usize, default);
    }
}

/// While Kindle's Mods screen is open from a row, and for a while after: the rows take the
/// values set there.
fn follow_editor() {
    let open = ui_bridge::OVERLAY_VISIBLE.load(Ordering::Relaxed);
    let next = match EDITOR.load(Ordering::Relaxed) {
        EDITOR_IDLE => return,
        EDITOR_ASKED | EDITOR_OPEN if open => EDITOR_OPEN,
        EDITOR_ASKED => return,
        EDITOR_OPEN => EDITOR_OPEN + EDITOR_FOLLOW_FRAMES,
        n if n - 1 == EDITOR_OPEN => EDITOR_IDLE,
        n => n - 1,
    };
    EDITOR.store(next, Ordering::Relaxed);
    let mods = ui_bridge::mods();
    let values = unsafe { &mut *VALUES.0.get() };
    let mut applied = lock(&APPLIED);
    let plans = lock(&PLANS);
    for spec in plans.iter().flat_map(Plan::rows) {
        let Some(now) = current(&spec.row, &mods) else { continue };
        values[spec.slot] = now;
        applied[spec.slot] = now;
    }
}

/// Called every frame: Kindle's System screens kept in step, and what changed on them sent as
/// setting requests.
pub fn tick() {
    plans_tick();
    screens_tick();
    menus_tick();
    if lock(&PLANS).is_empty() {
        return;
    }
    follow_editor();
    let values = unsafe { *VALUES.0.get() };
    let mut changed = Vec::new();
    {
        let mut applied = lock(&APPLIED);
        let plans = lock(&PLANS);
        for spec in plans.iter().flat_map(Plan::rows) {
            let now = values[spec.slot];
            if applied[spec.slot] != now {
                applied[spec.slot] = now;
                changed.push((spec.row.clone(), now));
            }
        }
    }
    for (row, now) in changed {
        match row {
            Row::Choice { ref owner, ref key } => crate::char_make::set_switch(owner, key, now != 0),
            Row::ScreenSwitch { screen, row } => screens::change(screen, row, RowKind::Toggle(now != 0), now as f64),
            Row::ScreenSlider { screen, row, min, max, step } => {
                let value = f64::from(now).clamp(min, max);
                screens::change(screen, row, RowKind::Slider { min, max, step, value }, value);
            }
            Row::ScreenChoice { screen, row, ref choices } => {
                let index = (now as usize).min(1);
                screens::change(screen, row, RowKind::Choice { choices: choices.to_vec(), index }, index as f64);
            }
            ref row => ui_bridge::send(request(row, now)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The planning tests share the screens' statics.
    static PLANNING: Mutex<()> = Mutex::new(());

    #[test]
    fn whole_number_settings_are_their_bytes() {
        let row = Row::Int { id: "m".into(), name: "duration".into(), min: 5, max: 120 };
        assert!(matches!(request(&row, 45), UiRequest::SetSetting(_, _, SettingValue::Int(45))));
        assert!(matches!(request(&row, 200), UiRequest::SetSetting(_, _, SettingValue::Int(120))), "kept in range");
        assert!(byte_range(5, 120) && byte_range(0, 255));
        assert!(!byte_range(20, 300) && !byte_range(1, 999_999) && !byte_range(-1, 10));
    }

    fn screen(rows: Vec<screens::Row>) -> screens::Screen {
        screens::Screen { owner: "m".into(), id: 7, title: "T".into(), subtitle: String::new(), rows, closable: true, tabs: Vec::new(), tab: None, version: 1 }
    }

    fn row(text: &str, kind: RowKind) -> screens::Row {
        screens::Row { text: text.into(), icon: None, help: String::new(), kind, disabled: false, details: None }
    }

    #[test]
    fn mod_screens_that_fit_the_system_screen() {
        let _planning = lock(&PLANNING);
        let ok = screen(vec![
            row("God", RowKind::Toggle(true)),
            row("Count", RowKind::Slider { min: 0.0, max: 10.0, step: 1.0, value: 3.0 }),
            row("Heal", RowKind::Button { value: None }),
            row("Deaths", RowKind::Info { value: Some("12".into()) }),
            row("Basics", RowKind::Header),
        ]);
        assert_eq!(unfit(&ok), None);
        assert!(unfit(&screen(vec![row("Speed", RowKind::Slider { min: 0.5, max: 2.0, step: 0.25, value: 1.0 })])).is_some());
        assert!(unfit(&screen(vec![row("Name", RowKind::Input { value: String::new(), max: 8 })])).is_some());
        assert_eq!(
            unfit(&screen(vec![row("Mode", RowKind::Choice { choices: vec!["A".into(), "B".into()], index: 0 })])),
            None,
            "two choices: the game's list"
        );
        assert!(unfit(&screen(vec![row("Mode", RowKind::Choice { choices: vec!["A".into(), "B".into(), "C".into()], index: 0 })])).is_some());
        let mut greyed = row("Mode", RowKind::Choice { choices: vec!["A".into(), "B".into()], index: 1 });
        greyed.disabled = true;
        assert_eq!(unfit(&screen(vec![greyed.clone()])), None, "a greyed choice is a line of text");
        assert_eq!(row_line(&greyed), "Mode: B");
        assert_eq!(row_line(&ok.rows[3]), "Deaths: 12");
        let mut shut = ok.clone();
        shut.closable = false;
        assert!(unfit(&shut).is_some());
        let mut b = Builder::at(0);
        let tabs = plan_screen(&mut b, &ok);
        b.finish(None, tabs, Some(ok.clone()));
        let values = unsafe { &*VALUES.0.get() };
        assert_eq!(&values[..2], &[1, 3], "the switch's and the slider's bytes");
        let slots: Vec<Press> = lock(&PRESS_SLOTS).iter().map(|s| s.press.clone()).collect();
        assert_eq!(slots, [Press::Screen { id: 7, row: 2 }], "one button: screen 7's row 3");
        let mut moved = ok.clone();
        moved.rows[0].kind = RowKind::Toggle(false);
        moved.rows[2].disabled = true;
        assert!(same_layout(&ok, &moved), "a value changed and a button greyed: shown in place");
        show_values(&lock(&PLANS)[0], &moved);
        assert_eq!(unsafe { (*VALUES.0.get())[0] }, 0);
        assert!(GREYED[0].load(Ordering::Relaxed), "the button greyed in place");
        moved.rows[3].kind = RowKind::Info { value: Some("13".into()) };
        assert!(!same_layout(&ok, &moved), "a line's text changed: opened again");
        lock(&PLANS).clear();
    }

    fn label(text: &[u16]) -> String {
        String::from_utf16_lossy(&text[..text.len() - 1])
    }

    #[test]
    fn mods_menus_have_room_and_their_presses_reach_the_mod() {
        let _planning = lock(&PLANNING);
        let button = |label: &str, paste: bool| ModTabButton { label: label.to_string(), help: String::new(), pastes: paste, info: false };
        for owner in ["coop", "photo", "practice", "timer"] {
            set_mod_tab(owner, owner, vec![button("A", false)]).unwrap();
        }
        set_mod_tab("coop", "Co-op", vec![button("Host", false), button("Join", true)]).unwrap();
        assert!(set_mod_tab("coop", "", vec![button("A", false)]).is_err());
        // The Mods screen first: its "All mod settings" takes slot 0.
        let mut b = Builder::at(0);
        let tabs = mods_screen(&mut b, &[]);
        b.finish(Some("Mods".into()), tabs, None);
        let titles: Vec<String> = lock(&PLANS)[0].tabs.iter().map(|t| label(t.label)).collect();
        assert_eq!(titles, ["Kindle"], "no mods list or menus without mods");
        // A mod's menu over it: its buttons after the Mods screen's.
        let mut b = Builder::at(1);
        let first = mod_tab_version("coop");
        set_mod_tab("coop", "Co-op", vec![button("Host", false), button("Join", true)]).unwrap();
        assert_eq!(mod_tab_version("coop"), first, "the same buttons again: the same version");
        let (title, tabs) = mod_menu(&mut b, "coop").expect("the co-op menu");
        assert_eq!((title.as_str(), tabs.len(), label(tabs[0].label).as_str()), ("coop", 1, "Co-op"));
        b.finish(Some(title), tabs, None);
        assert_eq!(lock(&PLANS)[1].buttons, 1..3);
        button_pressed(PRESS_CALLABLES[2].callable());
        let presses = take_presses();
        assert_eq!(presses.len(), 1);
        assert_eq!((presses[0].0.as_str(), presses[0].1.as_str(), presses[0].2.is_some()), ("coop", "Join", true));
        // Another menu in its place: the first one's slots are reused.
        let mut b = Builder::at(1);
        let (title, tabs) = mod_menu(&mut b, "timer").expect("the timer's menu");
        b.finish(Some(title), tabs, None);
        assert_eq!(lock(&PLANS)[1].buttons, 1..2);
        for owner in ["coop", "photo", "practice", "timer"] {
            remove_mod_tab(owner);
        }
        assert!(mod_menu(&mut Builder::at(1), "coop").is_none(), "no buttons, no settings: no menu");
        lock(&PLANS).clear();
        lock(&PRESS_SLOTS).clear();
    }
    #[test]
    fn two_choice_settings_are_the_games_lists() {
        let _planning = lock(&PLANNING);
        let setting = |choices: &[&str]| ui_bridge::SettingRow {
            name: "side".into(),
            label: "Side".into(),
            description: String::new(),
            kind: SettingKind::Choice(choices.iter().map(|c| c.to_string()).collect()),
            value: SettingValue::Choice("right".into()),
            default: SettingValue::Choice("left".into()),
        };
        let mut b = Builder::at(0);
        let Item::Row(spec) = setting_row(&mut b, "m", &setting(&["left", "right"])) else { panic!("a list row") };
        assert_eq!(unsafe { ((*VALUES.0.get())[spec.slot], (*DEFAULTS.0.get())[spec.slot]) }, (1, 0), "the value's index, the default's");
        assert!(matches!(request(&spec.row, 0), UiRequest::SetSetting(_, _, SettingValue::Choice(c)) if c == "left"));
        assert!(matches!(setting_row(&mut b, "m", &setting(&["a", "b", "c"])), Item::Button(_)), "three: Kindle's settings screen");
        lock(&PRESS_SLOTS).clear();
    }

    #[test]
    fn long_tabs_end_in_a_more_row() {
        let _planning = lock(&PLANNING);
        let mut b = Builder::at(0);
        let tabs = (0..2)
            .map(|t| {
                b.tab = t;
                let items =
                    (0..10).map(|_| Item::Row(RowSpec { label: wide_kept("Mod"), help: wide_kept(""), row: Row::Developer, slot: b.value(1, 1) })).collect();
                Tab { label: wide_kept("Mods"), items, icon: None }
            })
            .collect();
        b.finish(None, tabs, None);
        let plans = lock(&PLANS);
        let p = &plans[0];
        let shown: Vec<usize> = p.tabs.iter().map(|t| t.items.len()).collect();
        assert_eq!(shown, [ROOM[0], ROOM[1]], "each window full, its last row More");
        assert_eq!(p.more.iter().map(Vec::len).collect::<Vec<_>>(), [10 - (ROOM[0] - 1), 10 - (ROOM[1] - 1)]);
        assert_eq!(p.rows().count(), 20, "rows past the room still count for changes");
        let slots: Vec<Press> = lock(&PRESS_SLOTS).iter().map(|s| s.press.clone()).collect();
        assert_eq!(slots, [Press::More { tab: 0 }, Press::More { tab: 1 }]);
        drop(plans);
        lock(&PLANS).clear();
        lock(&PRESS_SLOTS).clear();
    }
}
