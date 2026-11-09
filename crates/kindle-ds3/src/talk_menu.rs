//! The game's own list menus and shops, driven by mods.
//!
//! Talk scripts build a list menu with commands 20 (clear), 19 (slot, text id, flag) per
//! entry and 10 (show), then wait on two queries every frame: 23 (picked entry: -1 while
//! choosing, the slot, or 0 when backed out) and 59(menu type, 0) (is that menu open: type 1
//! the list, 5 the shop). A shop is opened with command 22(first row, last row); the script
//! then waits on 59(5, 0) and shows its list again afterwards.
//!
//! Kindle:
//! - adds mod options to menus the game builds (right before the "Leave" entry, slot 99, or
//!   before the menu is shown), by sending its own command 19 through the game's dispatcher;
//! - when one is picked, runs the mod's callback right there (inside the query, on the game
//!   thread). Nothing is sent to the game from there: the talk-event object is only valid
//!   while the game is calling it. If the callback asks for a sub-menu (`ui.menu`) or a shop
//!   (`ui.shop`), the script is told it picked an entry of its own that opens a screen (as
//!   for NPCs, below), the screen is blocked and the script builds its menu again (only some
//!   bonfire scripts do that on "closed, nothing picked", the answer without such an entry);
//!   Kindle takes over that rebuild: for a sub-menu it drops the script's entries and
//!   adds the mod's before the script's own "show", for a shop it opens the shop instead of
//!   showing the list and keeps the script waiting (query 59(1, 0) answered "open") until
//!   the shop closes. Backing out of a sub-menu brings the script's menu back; closing a
//!   shop opened from a sub-menu brings that sub-menu back. A sub-menu entry that opens
//!   nothing returns to the script's menu; a top-level option that opens nothing ends the
//!   conversation ("backed out").
//! - NPC scripts don't rebuild their menu on "closed, nothing picked" (they say goodbye), but
//!   they do after any of their screens closes (shop, reinforce, level up ...). So Kindle
//!   answers the pick as one of the NPC's own entries that opens a screen ("Purchase Item"
//!   first, see BORROWABLE): for a mod shop or screen it opens that instead of the screen the
//!   script then opens, and answers the script's wait on its own menu type with it; to come
//!   back to a menu (a mod sub-menu, or the NPC's own after a sub-menu) it blocks the screen
//!   the script opens, the script sees it closed and rebuilds its menu, and Kindle takes that
//!   over as for bonfires. NPCs with no such entry (only "Talk"): options can't open menus.
//!
//! Mod entries use slots the scripts never use (7000+, sub-menus 8000+). Sub-menu text comes
//! from a pool of menu-text (FMG 30) ids that are re-set each time a menu is shown.
//!
//! The game's other screens open the way a shop does (`ui.open`): the talk command handler
//! (+0xF035C0) has a command per screen; each calls a helper (+0xEF9FF0..+0xEFB150) that hands
//! the menu system a factory for that screen's menu job and records its menu type (the one
//! query 59 tests). Read from the game's code (the factory each command's helper passes, down to
//! the movie it loads) and the arguments its own talk scripts use: level up 31 (type 10,
//! 03_010_LevelUp), attune 28(1000, 1000) (11, 03_020_Spell), storage box 30 (3, 03_050_ItemBox),
//! travel 41(-1) (2, 02_080_Warp), reinforce 24(0) (9), infuse 48 (7, 03_040_EnhancedWeapon),
//! repair 23 (8), estus allotment 105 (14, 02_100_AllocationOfEstus). Reinforce, infuse and
//! repair verified in game from Andre's own menu (2026-10-07).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

/// True while any mod has a menu option (the hooks do nothing otherwise).
pub static ACTIVE: AtomicBool = AtomicBool::new(false);

const FIRST_SLOT: i32 = 7000;
const LEAVE_SLOT: i32 = 99;
/// Text id of "Travel", in a bonfire's menu once the player has reached Firelink.
pub const TRAVEL_TEXT: i32 = 15000150;

/// The player is sitting at a bonfire (its animation): a bonfire's menu has only "Leave"
/// before the player reaches Firelink, so "Travel" can't tell then.
pub fn sitting_at_bonfire() -> bool {
    crate::game::player_chr_addr().ok().and_then(crate::chars::animation_name).is_some_and(|a| a.starts_with("Bonfire"))
}
/// Menu text ids reused for sub-menu entries.
const POOL_FIRST: u32 = 69_500_000;
pub const POOL_SIZE: usize = 64;
/// Query ids and menu types.
pub const QUERY_PICKED: i32 = 23;
pub const QUERY_MENU_OPEN: i32 = 59;
const MENU_LIST: i32 = 1;
const MENU_SHOP: i32 = 5;
/// Talk commands that open a menu (the talk command handler's menu helpers, +0xEF9FF0..
/// +0xEFB150), logged when a script sends them.
const MENU_COMMANDS: [i32; 21] = [10, 12, 17, 18, 22, 23, 24, 28, 30, 31, 32, 41, 46, 48, 57, 58, 67, 76, 81, 82, 105];
/// Frames to wait for a shop to appear before giving up on it.
const SHOP_OPEN_WAIT: u32 = 90;
/// Frames a pending sub-menu/shop waits for the script to rebuild its menu.
const PENDING_FRAMES: u64 = 120;
/// NPC menu entries Kindle can borrow (they open a screen and the script rebuilds its menu
/// after it), in the order it prefers them: Purchase Item, Sell Item, Reinforce Weapon, Infuse
/// Weapon, Repair Equipment, Level Up, Attune Spell, Organize Storage Box. Each is one text id
/// of those words; NPCs use ids of their own, so entries are matched by text (in the game's
/// language). Allot Estus is left out: without an Ashen Estus Flask it shows a message instead.
const BORROWABLE: [u32; 8] = [15011020, 15000012, 15010002, 15010001, 15010003, 15000100, 15000130, 15000220];
/// The screen commands a script sends after such an entry, with the menu type it then waits
/// on (shop, sell, reinforce, infuse, repair, level up, attune, storage box, estus).
const SCRIPT_SCREENS: [(i32, i32); 9] = [(22, 5), (46, 6), (24, 9), (48, 7), (23, 8), (31, 10), (28, 11), (30, 3), (105, 14)];

fn script_screen(command: i32) -> Option<i32> {
    SCRIPT_SCREENS.iter().find(|&&(c, _)| c == command).map(|&(_, t)| t)
}

/// What a picked mod entry refers to.
#[derive(Clone)]
pub enum Pick {
    /// An option added with ui.add_menu_option: (mod id, option handle).
    Option(String, u64),
    /// Entry `index` of a sub-menu shown by a mod: (mod id, menu handle, index).
    Entry(String, u64, usize),
}

/// What a mod asked for while handling a pick.
#[derive(Clone)]
pub enum Request {
    /// Show a list: (mod id, menu handle, entry texts).
    Menu(String, u64, Vec<String>),
    /// Open the shop over ShopLineupParam rows first..=last.
    Shop(i32, i32),
    /// Open one of the game's other screens.
    Screen(Screen),
}

/// The game's screens a talk script can open: (talk command, its arguments, menu type).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Screen {
    LevelUp,
    Attune,
    Storage,
    Travel,
    Reinforce,
    Infuse,
    Repair,
    Estus,
}

impl Screen {
    pub const NAMES: [(&'static str, Screen); 8] = [
        ("level_up", Screen::LevelUp),
        ("attune", Screen::Attune),
        ("storage", Screen::Storage),
        ("travel", Screen::Travel),
        ("reinforce", Screen::Reinforce),
        ("infuse", Screen::Infuse),
        ("repair", Screen::Repair),
        ("estus", Screen::Estus),
    ];

    pub fn from_name(name: &str) -> Option<Screen> {
        Self::NAMES.iter().find(|(n, _)| *n == name).map(|&(_, s)| s)
    }

    fn command(self) -> (i32, &'static [i32], i32) {
        match self {
            Screen::LevelUp => (31, &[], 10),
            Screen::Attune => (28, &[1000, 1000], 11),
            Screen::Storage => (30, &[], 3),
            Screen::Travel => (41, &[-1], 2),
            Screen::Reinforce => (24, &[0], 9),
            Screen::Infuse => (48, &[], 7),
            Screen::Repair => (23, &[], 8),
            Screen::Estus => (105, &[], 14),
        }
    }
}

/// Kindle UI currently up in the conversation (a sub-menu needs no tracking: it's the
/// script's own list).
enum Ui {
    /// A shop or screen of menu type `menu_type`, opened in place of what the script waits on
    /// (query 59 for menu type `answers`: the list at a bonfire, the shop at an NPC).
    /// `back_to`: the mod sub-menu to show again when it closes.
    Open { menu_type: i32, answers: i32, seen_open: bool, waited: u32, back_to: Option<Request> },
}

/// Sends a shop or screen request's talk command; its menu type.
fn open(this: usize, dispatch: usize, r: &Request) -> Option<i32> {
    match *r {
        Request::Shop(first, last) => {
            send(this, dispatch, 22, &[first, last]);
            Some(MENU_SHOP)
        }
        Request::Screen(s) => {
            let (command, args, menu_type) = s.command();
            send(this, dispatch, command, args);
            Some(menu_type)
        }
        Request::Menu(..) => None,
    }
}

#[derive(Default)]
struct State {
    /// Entries the game added since the last clear: (slot, text id).
    entries: Vec<(i32, i32)>,
    injected: bool,
    /// Mod slots in the current menu.
    slots: HashMap<i32, Pick>,
    /// Slots of menus already handled, and what to answer when the game still reports them.
    stale: HashMap<i32, i32>,
    ui: Option<Ui>,
    /// The talk-event object that last cleared the list (the script building a menu).
    builder: Option<usize>,
    /// A sub-menu or shop to open when this builder next builds its menu: (request,
    /// builder, frame asked).
    pending: Option<(Request, usize, u64)>,
    /// The mod sub-menu currently shown (to come back to after a shop it opened).
    current_menu: Option<Request>,
    /// The shop or screen to open when the script opens its own (NPCs).
    swap: Option<Request>,
    /// Whether the menu being built is a bonfire's (it has "Travel", or the player sits at one).
    bonfire: bool,
    /// The NPC's entry Kindle borrows: (slot, its rank in BORROWABLE); kept while the same
    /// script rebuilds its menu.
    borrow: Option<(i32, usize)>,
    /// Set when Kindle answered a pick with the NPC's borrowed entry to get its menu back: the
    /// screen the script opens is blocked, and the request (if any) shown when it rebuilds.
    loop_back: Option<Option<Request>>,
    /// A pick to report at the script's next query 23: the borrowed entry, so a bonfire's
    /// script builds its menu again after a shop of a mod's closes (see on_menu_open).
    pick_next: Option<i32>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static REQUEST: Mutex<Option<Request>> = Mutex::new(None);
/// True while a pick's callback runs (ui.menu / ui.shop only work then).
static IN_PICK: AtomicBool = AtomicBool::new(false);
/// Counts sub-menus shown, so each uses a fresh slot range.
static MENUS_SHOWN: AtomicI32 = AtomicI32::new(0);

/// An option to add, as decided by the runtime: (mod id, handle, text id).
pub type Chosen = (String, u64, u32);

// ── minimal EzState event / query objects, so Kindle goes through the game's handlers ──

#[repr(C)]
struct Value {
    raw: u64,
    ty: u32,
    pad: u32,
}

fn int(v: i32) -> Value {
    Value { raw: v as u32 as u64, ty: 2, pad: 0 }
}

#[repr(C)]
struct Fake {
    vtable: *const [usize; 8],
    id: i32,
    count: i32,
    values: [Value; 4],
}

unsafe extern "C" fn fake_noop(_: *const Fake) -> usize {
    0
}
unsafe extern "C" fn fake_id(this: *const Fake) -> i32 {
    unsafe { (*this).id }
}
unsafe extern "C" fn fake_count(this: *const Fake) -> i32 {
    unsafe { (*this).count }
}
unsafe extern "C" fn fake_arg(this: *const Fake, i: i32) -> *const Value {
    let this = unsafe { &*this };
    &this.values[(i.max(0) as usize).min(this.values.len() - 1)]
}
// Queries take (this, scratch buffer, index).
unsafe extern "C" fn fake_query_arg(this: *const Fake, _buf: usize, i: i32) -> *const Value {
    unsafe { fake_arg(this, i) }
}

static EVENT_VTABLE: std::sync::OnceLock<[usize; 8]> = std::sync::OnceLock::new();
static QUERY_VTABLE: std::sync::OnceLock<[usize; 8]> = std::sync::OnceLock::new();

fn fake(id: i32, args: &[i32], query: bool) -> Fake {
    let noop = fake_noop as *const () as usize;
    let vt = if query {
        // +0x08 id, +0x10 count, +0x18 arg(buf, i)
        QUERY_VTABLE.get_or_init(|| {
            [noop, fake_id as *const () as usize, fake_count as *const () as usize, fake_query_arg as *const () as usize, noop, noop, noop, noop]
        })
    } else {
        // +0x10 id, +0x18 count, +0x20 arg(i)
        EVENT_VTABLE
            .get_or_init(|| [noop, noop, fake_id as *const () as usize, fake_count as *const () as usize, fake_arg as *const () as usize, noop, noop, noop])
    };
    let a = |i: usize| args.get(i).copied().unwrap_or(0);
    Fake { vtable: vt, id, count: 1 + args.len().min(3) as i32, values: [int(id), int(a(0)), int(a(1)), int(a(2))] }
}

/// Sends a talk command (id, up to 3 int args) through the game's dispatcher.
fn send(this: usize, dispatch: usize, id: i32, args: &[i32]) {
    let ev = fake(id, args, false);
    let f: unsafe extern "C" fn(usize, *const Fake) -> usize = unsafe { std::mem::transmute(dispatch) };
    unsafe { f(this, &ev) };
}

/// Asks the game's talk query (id, up to 3 int args); returns its integer answer.
fn ask(env: usize, query_dispatch: usize, id: i32, args: &[i32]) -> i32 {
    let q = fake(id, args, true);
    let mut out = [0u64; 4];
    let f: unsafe extern "C" fn(usize, *mut [u64; 4], *const Fake) -> usize = unsafe { std::mem::transmute(query_dispatch) };
    unsafe { f(env, &mut out, &q) };
    out[0] as u32 as i32
}

// ── building the game's menus ──

fn inject(state: &mut State, this: usize, dispatch: usize) {
    if state.injected {
        return;
    }
    state.injected = true;
    for (k, (owner, handle, text_id)) in crate::menu_options_for(&state.entries).into_iter().enumerate() {
        let slot = FIRST_SLOT + k as i32;
        send(this, dispatch, 19, &[slot, text_id as i32, -1]);
        state.slots.insert(slot, Pick::Option(owner, handle));
    }
}

/// Adds a mod sub-menu's entries (texts go into pooled menu-text ids).
fn add_menu_entries(this: usize, dispatch: usize, owner: &str, handle: u64, texts: &[String]) -> HashMap<i32, Pick> {
    let n = MENUS_SHOWN.fetch_add(1, Ordering::Relaxed);
    let first_slot = 8000 + (n % 20) * 100;
    let mut slots = HashMap::new();
    for (i, text) in texts.iter().take(POOL_SIZE).enumerate() {
        let text_id = POOL_FIRST + i as u32;
        if let Err(e) = crate::text_override::set_or_add(&[30], text_id, text, owner) {
            crate::warn!("menu: entry text: {e}");
        }
        let slot = first_slot + i as i32;
        send(this, dispatch, 19, &[slot, text_id as i32, -1]);
        slots.insert(slot, Pick::Entry(owner.to_string(), handle, i));
    }
    slots
}

/// Called by the talk command hook before the game handles a command. Returns false to
/// keep the game from handling it (the script's entries while Kindle shows its own menu).
pub fn before_command(this: usize, dispatch: usize, id: i32, args: &[i32]) -> bool {
    let frame = crate::mods::FRAME.load(Ordering::Relaxed);
    let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = slot.get_or_insert_with(State::default);
    if id == 20 {
        // A new menu: keep a pending request only for the same script and only briefly.
        let pending = state.pending.take().filter(|(_, b, f)| *b == this && frame.saturating_sub(*f) <= PENDING_FRAMES);
        let stale = std::mem::take(&mut state.stale);
        let current_menu = if pending.is_some() { state.current_menu.take() } else { None };
        // The same script rebuilding keeps what Kindle learned about it.
        let same = state.builder == Some(this);
        let (bonfire, borrow) = if same { (state.bonfire, state.borrow) } else { (sitting_at_bonfire(), None) };
        let loop_back = if same { state.loop_back.take() } else { None };
        *state = State { builder: Some(this), pending, stale, current_menu, bonfire, borrow, loop_back, ..State::default() };
        return true;
    }
    // Only the script building the menu counts (idle NPCs run talk scripts too).
    if state.builder != Some(this) {
        return true;
    }
    if MENU_COMMANDS.contains(&id) {
        crate::debug!("menu: the script sends command {id} {args:?}");
    }
    if let Some(script_type) = script_screen(id)
        && let Some(next) = state.loop_back.take()
    {
        // Kindle picked the NPC's borrowed entry to get its menu back: no screen; the script
        // sees it closed and builds its menu again (with the mod's sub-menu, if one is waiting).
        if let Some(r) = next {
            state.pending = Some((r, this, frame));
        }
        // The script's wait on that screen is answered "open" for as long as the list it was
        // picked from is still closing: a menu shown in the same frame was closed a frame later,
        // and the script took the closed list for backing out (a bonfire stood the player up).
        state.ui = Some(Ui::Open { menu_type: MENU_LIST, answers: script_type, seen_open: true, waited: 0, back_to: None });
        crate::debug!("menu: NPC screen (menu type {script_type}) blocked to return to the menu");
        return false;
    }
    if let Some(script_type) = script_screen(id)
        && let Some(r) = state.swap.take()
    {
        // The NPC is opening its own screen for a mod's pick: open the mod's shop or screen
        // instead, and answer the script's wait on its own menu type with it.
        crate::debug!("menu: NPC screen (menu type {script_type}) swapped for {}", describe(&r));
        if let Some(menu_type) = open(this, dispatch, &r)
            && menu_type != script_type
        {
            state.ui = Some(Ui::Open { menu_type, answers: script_type, seen_open: false, waited: 0, back_to: None });
        }
        return false;
    }
    if state.pending.is_some() {
        return match id {
            // The script's own entries: replaced by the mod's.
            19 => false,
            10 => match state.pending.take().map(|(r, _, _)| r) {
                Some(Request::Menu(owner, handle, texts)) => {
                    state.injected = true;
                    state.slots = add_menu_entries(this, dispatch, &owner, handle, &texts);
                    state.current_menu = Some(Request::Menu(owner, handle, texts.clone()));
                    crate::debug!("menu: showing a mod menu of {}", texts.len());
                    true
                }
                Some(r) => {
                    let menu_type = open(this, dispatch, &r).unwrap_or(MENU_SHOP);
                    let back_to = state.current_menu.take();
                    state.ui = Some(Ui::Open { menu_type, answers: MENU_LIST, seen_open: false, waited: 0, back_to });
                    crate::debug!("menu: opening {}", describe(&r));
                    false
                }
                None => true,
            },
            _ => true,
        };
    }
    if id == 19 && args.get(2) == Some(&crate::talk_menu::TRAVEL_TEXT) {
        state.bonfire = true;
    }
    match id {
        19 if args.get(1) == Some(&LEAVE_SLOT) => inject(state, this, dispatch),
        10 => inject(state, this, dispatch),
        _ => {}
    }
    true
}

/// Called by the talk command hook after the game handled a command.
pub fn after_command(this: usize, id: i32, args: &[i32]) {
    if id == 19
        && let (Some(&s), Some(&t)) = (args.get(1), args.get(2))
    {
        let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let state = slot.get_or_insert_with(State::default);
        if state.builder == Some(this) {
            state.entries.push((s, t));
            if let Some(rank) = borrowable(t)
                && state.borrow.is_none_or(|(_, best)| rank < best)
            {
                state.borrow = Some((s, rank));
            }
        }
    }
}

/// Where a menu text id's words are in BORROWABLE (NPCs use their own ids for them).
fn borrowable(text_id: i32) -> Option<usize> {
    let text = crate::text_override::get(30, text_id as u32).filter(|t| !t.is_empty())?;
    BORROWABLE.iter().position(|&id| crate::text_override::get(30, id).as_deref() == Some(text.as_str()))
}

/// Answer for query 23 that brings an NPC's menu back (showing `then` if given): its borrowed
/// entry, with the screen it opens blocked. None if the NPC has no such entry.
fn npc_return(state: &mut State, then: Option<Request>) -> Option<i32> {
    let (slot, _) = state.borrow?;
    state.loop_back = Some(then);
    Some(slot)
}

/// True if queries can concern us (cheap pre-check for the query hook).
pub fn involved() -> bool {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .is_some_and(|s| !s.slots.is_empty() || !s.stale.is_empty() || s.ui.is_some() || s.pending.is_some() || s.pick_next.is_some())
}

/// A request for the log.
fn describe(r: &Request) -> String {
    match r {
        Request::Menu(_, _, t) => format!("a menu of {}", t.len()),
        Request::Shop(a, b) => format!("shop {a}..{b}"),
        Request::Screen(s) => format!("screen {s:?}"),
    }
}

/// Recorded by `ui.menu` / `ui.shop` / `ui.open`; only valid while a menu pick is being handled.
pub fn request(r: Request) -> Result<(), String> {
    if !IN_PICK.load(Ordering::Relaxed) {
        return Err("menus and shops can only be opened from a menu option's on_select".into());
    }
    let mut slot = REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_some() {
        return Err("a menu or shop is already being opened".into());
    }
    *slot = Some(r);
    Ok(())
}

/// Runs a picked mod entry's callback. Returns the answer for query 23: -1 ("closed,
/// nothing picked": the script rebuilds its menu, which Kindle takes over) if it asked for a
/// sub-menu or shop, 0 ("backed out") otherwise.
fn handle_pick(pick: Pick) -> i32 {
    *REQUEST.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let top_screen = || kindle_core::screens::top().map(|s| s.id);
    let screen_before = top_screen();
    IN_PICK.store(true, Ordering::Relaxed);
    crate::menu_pick(&pick);
    IN_PICK.store(false, Ordering::Relaxed);
    let request = REQUEST.lock().unwrap_or_else(|e| e.into_inner()).take();
    // The callback opened a screen of the mod's (kindle.ui.screen) over the game.
    let opened_screen = top_screen().is_some_and(|id| Some(id) != screen_before);

    let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let state = slot.get_or_insert_with(State::default);
    let asked = match &request {
        Some(r) => describe(r),
        None if opened_screen => "a screen of its own".into(),
        None => "nothing".into(),
    };
    crate::debug!("menu: picked; callback asked for {asked}");
    let in_sub_menu = matches!(pick, Pick::Entry(..));
    let bonfire = state.bonfire;
    let mut pending = None;
    let answer = match request {
        // Bonfires: the menu is built again (Kindle takes the rebuild over for a sub-menu or
        // shop), by way of the borrowed entry as for NPCs: most bonfire scripts end the
        // conversation on "closed, nothing picked" (Central Irithyll's, 2026-11-05: query 59
        // says the list closed, and the script stands the player up), only some build their
        // menu again. Without a borrowable entry (the first bonfire, only "Leave"), -1.
        Some(r) if bonfire => match state.borrow {
            Some(_) => npc_return(state, Some(r)).unwrap_or(-1),
            None => {
                pending = Some(r);
                -1
            }
        },
        // A sub-menu entry that opens nothing goes back to the bonfire menu; so does a screen
        // of the mod's (kindle.ui.screen): the bonfire's menu stays under it (the game's input
        // is blocked while it's up), so closing it is back at the bonfire, not standing up.
        None if bonfire && (in_sub_menu || opened_screen) => npc_return(state, None).unwrap_or(-1),
        // NPC shop or screen: pick the NPC's borrowed entry and swap what it opens.
        Some(r @ (Request::Shop(..) | Request::Screen(_))) => match state.borrow {
            Some((slot, _)) => {
                state.swap = Some(r);
                slot
            }
            None => {
                crate::warn!("menu: ui.shop and ui.open need a bonfire or an NPC whose menu opens a screen (a shop, reinforcing, levelling up ...)");
                0
            }
        },
        // NPC sub-menu: get the NPC's menu back and show the mod's in its place.
        Some(r @ Request::Menu(..)) => npc_return(state, Some(r)).unwrap_or_else(|| {
            crate::warn!("menu: ui.menu needs a bonfire or an NPC whose menu opens a screen (a shop, reinforcing, levelling up ...)");
            0
        }),
        // A sub-menu entry that opens nothing: back to the NPC's own menu.
        None if in_sub_menu => npc_return(state, None).unwrap_or(0),
        // A top-level option that opens nothing ends the conversation.
        None => 0,
    };
    let old: Vec<i32> = state.slots.drain().map(|(s, _)| s).collect();
    for s in old {
        state.stale.insert(s, answer);
    }
    crate::debug!("menu: answered {answer} (bonfire {bonfire})");
    if let (Some(r), Some(b)) = (pending, state.builder) {
        state.pending = Some((r, b, crate::mods::FRAME.load(Ordering::Relaxed)));
    }
    answer
}

/// Query 23 (picked entry). Returns the value the script should see.
pub fn on_picked(value: i32) -> i32 {
    let pick = {
        let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = slot.as_mut() else { return value };
        if let Some(borrowed) = state.pick_next.take() {
            return borrowed;
        }
        if let Some(p) = state.slots.get(&value).cloned() {
            // Claim it so a re-read can't run the callback twice.
            state.slots.remove(&value);
            state.stale.insert(value, -1);
            p
        } else if let Some(&answer) = state.stale.get(&value) {
            return answer;
        } else if state.ui.is_some() {
            // A shop is up: nothing picked yet.
            return -1;
        } else if value == 0 && state.slots.values().any(|p| matches!(p, Pick::Entry(..))) {
            // Backed out of a mod sub-menu: back to the script's own menu.
            state.slots.clear();
            let fallback = if state.bonfire { -1 } else { 0 };
            return npc_return(state, None).unwrap_or(fallback);
        } else {
            return value;
        }
    };
    handle_pick(pick)
}

/// Query 59 (is menu `menu_type` open). Returns the value the script should see.
pub fn on_menu_open(env: usize, query_dispatch: usize, menu_type: i32, value: i32) -> i32 {
    let mut slot = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = slot.as_mut() else { return value };
    match &mut state.ui {
        Some(Ui::Open { menu_type: open_type, answers, seen_open, waited, back_to }) if *answers == menu_type => {
            let open = ask(env, query_dispatch, QUERY_MENU_OPEN, &[*open_type, 0]) == 1;
            if open {
                if !*seen_open {
                    crate::debug!("menu: menu type {open_type} is open");
                }
                *seen_open = true;
                1
            } else if !*seen_open && *waited < SHOP_OPEN_WAIT {
                *waited += 1;
                1
            } else {
                // Closed (or never opened): the script shows its menu again, or the mod
                // sub-menu that opened it.
                crate::debug!("menu: menu type {open_type} {}", if *seen_open { "closed" } else { "never opened" });
                if *answers == MENU_LIST
                    && let Some((borrowed, _)) = state.borrow
                {
                    // A bonfire's script would take its closed list for backing out (most stand
                    // the player up): it's told the borrowed entry was picked, whose screen is
                    // blocked, and it builds its menu again (the mod's sub-menu, if it opened this).
                    state.loop_back = Some(back_to.take());
                    state.pick_next = Some(borrowed);
                    state.ui = None;
                    return 1;
                }
                if let (Some(menu), Some(b)) = (back_to.take(), state.builder) {
                    state.pending = Some((menu, b, crate::mods::FRAME.load(Ordering::Relaxed)));
                }
                state.ui = None;
                value
            }
        }
        _ => value,
    }
}
