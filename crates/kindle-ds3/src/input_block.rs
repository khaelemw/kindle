//! Stops the game seeing keyboard, mouse and controller input while the overlay is open.
//!
//! DS3 reads keyboard/mouse through DirectInput 8 and controllers through XInput 1.3, not
//! window messages, so filtering messages alone isn't enough. We hook
//! `IDirectInputDevice8::GetDeviceState` and `XInputGetState` and blank their output while
//! blocking is on. DS3 also polls `GetCursorPos` (menu hover) and calls `SetCursorPos`
//! (re-centring the cursor, which made the overlay cursor jitter); while blocking, calls
//! *from the game's own code* get a frozen position / are ignored, and everyone else
//! (hudhook, Windows, Kindle) sees the real cursor.
//!
//! All hooks are in system DLLs (dinput8, xinput1_3, user32), not in the game executable,
//! so Arxan's integrity checks don't see them.
//!
//! The DirectInput approach follows fromsoftware-rs's `debug` crate (MIT/Apache).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use crate::mods::now_ms;
use ilhook::x64::{CallbackOption, HookFlags, Registers, hook_closure_retn};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows::core::{GUID, s};

static BLOCKED: AtomicBool = AtomicBool::new(false);
/// Actions blocked (kindle.input, for a co-op player who is out): its character doesn't act (hks.rs stops its action
/// requests), and the game doesn't see the D-pad or the arrow keys, which switch weapons, items
/// and spells (and pick the player to watch, read before this). The rest goes through: the
/// camera, the menus and popups (their OK, the Esc menu).
static ACTIONS_BLOCKED: AtomicBool = AtomicBool::new(false);
const SWITCH_BUTTONS: u16 = 0x0001 | 0x0002 | 0x0004 | 0x0008;
/// The arrow keys (DirectInput codes: up, left, right, down).
const SWITCH_KEYS: [usize; 4] = [0xC8, 0xCB, 0xCD, 0xD0];
/// Out, with one of the game's action prompts up ("Rest at bonfire", "Recover lost souls"): the
/// interact button and key (A, E) are kept from the game too, so the hidden character doesn't
/// take things or rest (the mod decides each frame). With no prompt they close popups and pick
/// menu entries.
static INTERACT_BLOCKED: AtomicBool = AtomicBool::new(false);
const INTERACT_BUTTON: u16 = 0x1000;
const INTERACT_KEY: usize = 0x12;
/// Keys held down for the game (debug.press): per DirectInput key code, the time (ms since
/// Kindle started) until which the game sees it pressed.
static HELD_UNTIL: [AtomicU64; 256] = [const { AtomicU64::new(0) }; 256];

/// DirectInput key codes by name.
const KEYS: &[(&str, u8)] = &[
    ("esc", 0x01),
    ("1", 0x02),
    ("2", 0x03),
    ("3", 0x04),
    ("4", 0x05),
    ("5", 0x06),
    ("6", 0x07),
    ("7", 0x08),
    ("8", 0x09),
    ("9", 0x0A),
    ("0", 0x0B),
    ("backspace", 0x0E),
    ("tab", 0x0F),
    ("q", 0x10),
    ("w", 0x11),
    ("e", 0x12),
    ("r", 0x13),
    ("t", 0x14),
    ("y", 0x15),
    ("u", 0x16),
    ("i", 0x17),
    ("o", 0x18),
    ("p", 0x19),
    ("enter", 0x1C),
    ("ctrl", 0x1D),
    ("a", 0x1E),
    ("s", 0x1F),
    ("d", 0x20),
    ("f", 0x21),
    ("g", 0x22),
    ("h", 0x23),
    ("j", 0x24),
    ("k", 0x25),
    ("l", 0x26),
    ("shift", 0x2A),
    ("z", 0x2C),
    ("x", 0x2D),
    ("c", 0x2E),
    ("v", 0x2F),
    ("b", 0x30),
    ("n", 0x31),
    ("m", 0x32),
    ("alt", 0x38),
    ("space", 0x39),
    ("f1", 0x3B),
    ("f2", 0x3C),
    ("f3", 0x3D),
    ("f4", 0x3E),
    ("f5", 0x3F),
    ("f6", 0x40),
    ("f7", 0x41),
    ("f8", 0x42),
    ("f9", 0x43),
    ("f10", 0x44),
    ("f11", 0x57),
    ("f12", 0x58),
    ("minus", 0x0C),
    ("equals", 0x0D),
    ("lbracket", 0x1A),
    ("rbracket", 0x1B),
    ("semicolon", 0x27),
    ("apostrophe", 0x28),
    ("grave", 0x29),
    ("backslash", 0x2B),
    ("comma", 0x33),
    ("period", 0x34),
    ("slash", 0x35),
    ("rshift", 0x36),
    ("capslock", 0x3A),
    ("numlock", 0x45),
    ("scrolllock", 0x46),
    ("num0", 0x52),
    ("num1", 0x4F),
    ("num2", 0x50),
    ("num3", 0x51),
    ("num4", 0x4B),
    ("num5", 0x4C),
    ("num6", 0x4D),
    ("num7", 0x47),
    ("num8", 0x48),
    ("num9", 0x49),
    ("num_multiply", 0x37),
    ("num_minus", 0x4A),
    ("num_plus", 0x4E),
    ("num_period", 0x53),
    ("num_enter", 0x9C),
    ("num_divide", 0xB5),
    ("rctrl", 0x9D),
    ("ralt", 0xB8),
    ("home", 0xC7),
    ("pageup", 0xC9),
    ("end", 0xCF),
    ("pagedown", 0xD1),
    ("insert", 0xD2),
    ("delete", 0xD3),
    ("up", 0xC8),
    ("left", 0xCB),
    ("right", 0xCD),
    ("down", 0xD0),
];

/// The player's keyboard as the game last read it (before anything of Kindle's changes it), one
/// bit per DirectInput key code; and the mouse's buttons (left, right, middle, then the rest).
static KEYS_DOWN: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static MOUSE_DOWN: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
const MOUSE_BUTTONS: &[(&str, u8)] = &[("mouse_left", 0), ("mouse_right", 1), ("mouse_middle", 2), ("mouse_4", 3), ("mouse_5", 4)];

/// A key, controller button or mouse button by name (kindle.input.down, the key_pressed event).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Input {
    Key(u8),
    Pad(u16),
    Mouse(u8),
}

pub fn input_named(name: &str) -> Option<Input> {
    let name = name.to_ascii_lowercase();
    KEYS.iter()
        .find(|(n, _)| *n == name)
        .map(|&(_, c)| Input::Key(c))
        .or_else(|| PAD_BUTTONS.iter().find(|(n, _)| *n == name).map(|&(_, b)| Input::Pad(b)))
        .or_else(|| MOUSE_BUTTONS.iter().find(|(n, _)| *n == name).map(|&(_, b)| Input::Mouse(b)))
}

/// Whether the player holds it now (the game seeing it or not; Kindle's own presses don't count).
pub fn input_down(input: Input) -> bool {
    match input {
        Input::Key(code) => KEYS_DOWN[code as usize / 64].load(Ordering::Relaxed) >> (code % 64) & 1 != 0,
        Input::Pad(bit) => PAD_RAW.load(Ordering::Relaxed) & bit != 0,
        Input::Mouse(bit) => MOUSE_DOWN.load(Ordering::Relaxed) >> bit & 1 != 0,
    }
}

/// The names of every key and button the player holds now (one name per key: the first in the
/// table, so "shift" rather than an alias).
pub fn inputs_down() -> Vec<&'static str> {
    let keys = KEYS.iter().filter(|&&(_, c)| input_down(Input::Key(c))).map(|&(n, _)| n);
    let pad = PAD_BUTTONS.iter().filter(|&&(_, b)| input_down(Input::Pad(b))).map(|&(n, _)| n);
    let mouse = MOUSE_BUTTONS.iter().filter(|&&(_, b)| input_down(Input::Mouse(b))).map(|&(n, _)| n);
    keys.chain(pad).chain(mouse).collect()
}

/// Where the cursor is on the HUD canvas (kindle_core::hud's 1920 x 1080, as layers draw), set
/// each frame the overlay draws; None while it is outside the window or not yet known.
static MOUSE_AT: AtomicU64 = AtomicU64::new(u64::MAX);

pub fn set_mouse_at(at: Option<[f32; 2]>) {
    let packed = at.map_or(u64::MAX, |[x, y]| ((x.to_bits() as u64) << 32) | y.to_bits() as u64);
    MOUSE_AT.store(packed, Ordering::Relaxed);
}

pub fn mouse_at() -> Option<[f32; 2]> {
    let packed = MOUSE_AT.load(Ordering::Relaxed);
    (packed != u64::MAX).then(|| [f32::from_bits((packed >> 32) as u32), f32::from_bits(packed as u32)])
}

/// Every name input_named knows, for messages.
pub fn input_names() -> Vec<&'static str> {
    KEYS.iter().map(|(n, _)| *n).chain(PAD_BUTTONS.iter().map(|(n, _)| *n)).chain(MOUSE_BUTTONS.iter().map(|(n, _)| *n)).collect()
}

/// Controller buttons (XInput) held for the game, as for keys: per button bit.
static PAD_HELD_UNTIL: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
static PAD_PACKET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// Hook call counts, for debugging input: (keyboard reads, failed keyboard reads, other
/// device reads, controller reads, controller reads reporting not connected).
pub static STATS: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];

/// The real controller buttons (controller 1), read before the game's copy is blanked: the
/// Mods screen reads these while the game can't. The left stick counts as the D-pad.
pub static PAD_RAW: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
pub const PAD_UP: u16 = 0x0001;
pub const PAD_DOWN: u16 = 0x0002;
pub const PAD_LEFT: u16 = 0x0004;
pub const PAD_RIGHT: u16 = 0x0008;
pub const PAD_A: u16 = 0x1000;
pub const PAD_B: u16 = 0x2000;
pub const PAD_LB: u16 = 0x0100;
pub const PAD_RB: u16 = 0x0200;
/// The keyboard's left and right (the arrows, A and D) as the game would see them, as PAD_LEFT
/// and PAD_RIGHT: a co-op player who is out switches who it watches with them (the co-op mod).
pub static KEYS_LEFT_RIGHT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
/// The keyboard's action key (E, the game's default) as the game would see it.
static KEY_ACTION: AtomicBool = AtomicBool::new(false);

/// Whether the action button is held: A on the controller (a real one, or press()'s) or E.
pub fn action_held() -> bool {
    PAD_RAW.load(Ordering::Relaxed) & PAD_A != 0 || pad_buttons() & PAD_A != 0 || KEY_ACTION.load(Ordering::Relaxed)
}
/// Stick deflection that counts as a D-pad press.
const STICK_AS_DPAD: i16 = 16000;
/// When the controller, and the keyboard or mouse, were last used (ms since Kindle started): the
/// texts name the keys of the one used last, as the game's own prompts do.
static PAD_USED_AT: AtomicU64 = AtomicU64::new(0);
static KEYS_USED_AT: AtomicU64 = AtomicU64::new(0);
/// Stick or trigger movement that counts as using the controller (past drift).
const PAD_IN_USE: i16 = 8000;
const TRIGGER_IN_USE: u8 = 40;

/// Whether the player is using the controller (it was used more lately than the keyboard and
/// mouse).
pub fn using_pad() -> bool {
    PAD_USED_AT.load(Ordering::Relaxed) > KEYS_USED_AT.load(Ordering::Relaxed)
}

/// XInput button bits by name.
const PAD_BUTTONS: &[(&str, u16)] = &[
    ("pad_up", 0x0001),
    ("pad_down", 0x0002),
    ("pad_left", 0x0004),
    ("pad_right", 0x0008),
    ("pad_start", 0x0010),
    ("pad_back", 0x0020),
    ("pad_ls", 0x0040),
    ("pad_rs", 0x0080),
    ("pad_lb", 0x0100),
    ("pad_rb", 0x0200),
    ("pad_a", 0x1000),
    ("pad_b", 0x2000),
    ("pad_x", 0x4000),
    ("pad_y", 0x8000),
];

/// After press() last held a controller button: until then controller 0 stays connected for
/// the game (no buttons), so it keeps reading it between presses.
static PAD_CONNECTED_UNTIL: AtomicU64 = AtomicU64::new(0);
/// How long the controller press() reports stays connected after its last button.
const PAD_LINGER_MS: u64 = 60_000;
/// When press() connects the controller, its button waits this long: the game takes a newly
/// connected controller's buttons only after it has read it for a moment.
const PAD_CONNECT_MS: u64 = 600;
/// Per controller button, when press() starts holding it (ms since Kindle started).
static PAD_HELD_FROM: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];
/// A real controller 0 answered the last read.
static PAD_REAL: AtomicBool = AtomicBool::new(false);
/// When the game last read controller 0 (ms since Kindle started).
static PAD_READ_AT: AtomicU64 = AtomicU64::new(0);
/// The game reads a controller it knows every frame: longer without a read, it has dropped it.
const PAD_DROPPED_MS: u64 = 500;

/// Controller buttons currently held by press().
pub(crate) fn pad_buttons() -> u16 {
    let now = now_ms();
    PAD_HELD_UNTIL
        .iter()
        .enumerate()
        .filter(|(bit, until)| until.load(Ordering::Relaxed) > now && PAD_HELD_FROM[*bit].load(Ordering::Relaxed) <= now)
        .fold(0, |acc, (bit, _)| acc | (1 << bit))
}

/// Holds a key or controller button down for the game for `seconds`: a key name like
/// "enter", a controller button like "pad_a", or a DirectInput key code. Needs the input
/// hooks (installed with the overlay).
pub fn press(key: &str, seconds: f64) -> Result<(), String> {
    if !INSTALLED.load(Ordering::Relaxed) {
        return Err("the input hooks aren't installed (the overlay is off in kindle.toml)".into());
    }
    let ms = (seconds.clamp(0.0, 60.0) * 1000.0) as u64;
    FOCUS_UNTIL.fetch_max(now_ms() + PAD_CONNECT_MS + ms + FOCUS_LINGER_MS, Ordering::Relaxed);
    if let Some(&(_, bit)) = PAD_BUTTONS.iter().find(|(n, _)| n.eq_ignore_ascii_case(key)) {
        let now = now_ms();
        // With no controller the game stops reading them; a device change makes it look again.
        // The game stops reading controllers when it thinks none is there (and on some screen
        // changes): tell it one is, and hold the button once it reads it again.
        let dropped = now.saturating_sub(PAD_READ_AT.load(Ordering::Relaxed)) > PAD_DROPPED_MS;
        let connect = (dropped || PAD_CONNECTED_UNTIL.load(Ordering::Relaxed) <= now) && !PAD_REAL.load(Ordering::Relaxed);
        let from = if connect { now + PAD_CONNECT_MS } else { now };
        PAD_CONNECTED_UNTIL.store(from + ms + PAD_LINGER_MS, Ordering::Relaxed);
        let bit = bit.trailing_zeros() as usize;
        PAD_HELD_FROM[bit].store(from, Ordering::Relaxed);
        PAD_HELD_UNTIL[bit].store(from + ms, Ordering::Relaxed);
        if connect {
            announce_controller();
        }
        return Ok(());
    }
    let code = KEYS
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(key))
        .map(|(_, c)| *c)
        .or_else(|| key.parse::<u8>().ok())
        .ok_or_else(|| format!("unknown key {key:?}"))?;
    HELD_UNTIL[code as usize].store(now_ms() + ms, Ordering::Relaxed);
    Ok(())
}
/// Tells the game's window that devices changed (WM_DEVICECHANGE, DBT_DEVNODES_CHANGED), so it
/// looks for controllers again: without one it stops reading them, and only looks again then.
fn announce_controller() {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW};
    const WM_DEVICECHANGE: u32 = 0x0219;
    const DBT_DEVNODES_CHANGED: usize = 0x0007;
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> windows::core::BOOL {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == std::process::id() && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            unsafe { *(found.0 as *mut isize) = hwnd.0 as isize };
            return false.into();
        }
        true.into()
    }
    let mut found: isize = 0;
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&raw mut found as isize));
        if found != 0 {
            let _ = PostMessageW(Some(HWND(found as *mut _)), WM_DEVICECHANGE, WPARAM(DBT_DEVNODES_CHANGED), LPARAM(0));
        }
    }
}

static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Cursor position (x in the high 32 bits, y in the low) frozen when blocking started.
static FROZEN_CURSOR: AtomicU64 = AtomicU64::new(0);
/// Address range of DarkSoulsIII.exe, to tell the game's calls apart from everyone else's.
static GAME_START: AtomicUsize = AtomicUsize::new(0);
static GAME_END: AtomicUsize = AtomicUsize::new(0);

/// Keys, mouse buttons and controller buttons held when blocking ended: hidden from the game
/// until they're let go, so the press that closed a screen of Kindle's (Esc, B, a right click)
/// isn't also the game's (it opened the pause menu).
static SWALLOW_KEYS: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];
static SWALLOW_MOUSE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static SWALLOW_PAD: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

pub fn set_blocked(blocked: bool) {
    if !blocked && BLOCKED.load(Ordering::Relaxed) {
        for (swallow, down) in SWALLOW_KEYS.iter().zip(&KEYS_DOWN) {
            swallow.store(down.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        SWALLOW_MOUSE.store(MOUSE_DOWN.load(Ordering::Relaxed), Ordering::Relaxed);
        SWALLOW_PAD.store(PAD_RAW.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    if blocked && !BLOCKED.load(Ordering::Relaxed) {
        let mut p = POINT::default();
        // From kindle.dll, so this passes through our own hook to the real position.
        if unsafe { GetCursorPos(&mut p) }.is_ok() {
            FROZEN_CURSOR.store(((p.x as u32 as u64) << 32) | p.y as u32 as u64, Ordering::Relaxed);
        }
    }
    BLOCKED.store(blocked, Ordering::Relaxed);
}

/// True if the hooked function was called from DarkSoulsIII.exe's code. At hook entry,
/// the caller's return address is the first value on the stack.
fn called_by_game(reg: *mut Registers) -> bool {
    let ret = unsafe { (*reg).get_stack(0) } as usize;
    ret >= GAME_START.load(Ordering::Relaxed) && ret < GAME_END.load(Ordering::Relaxed)
}

fn blocked() -> bool {
    BLOCKED.load(Ordering::Relaxed)
}

/// The player is out, or back (see ACTIONS_BLOCKED).
pub fn set_actions_blocked(blocked: bool) {
    ACTIONS_BLOCKED.store(blocked, Ordering::Relaxed);
    if !blocked {
        INTERACT_BLOCKED.store(false, Ordering::Relaxed);
    }
    crate::hks::set_player_actions_stopped(blocked);
}

/// Out: whether the interact button and key are kept from the game now (see INTERACT_BLOCKED).
pub fn set_interact_blocked(blocked: bool) {
    INTERACT_BLOCKED.store(blocked && ACTIONS_BLOCKED.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// Installs the hooks once. Failures are logged; the overlay still works without them,
/// the game just also reacts to input while it's open.
pub fn install() {
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    match unsafe { install_dinput() } {
        Ok(n) => crate::info!("overlay: keyboard/mouse blocking ready ({n} DirectInput hook(s))"),
        Err(e) => crate::warn!("overlay: keyboard/mouse blocking unavailable: {e}"),
    }
    match unsafe { install_xinput() } {
        Ok(()) => crate::info!("overlay: controller blocking ready (XInput)"),
        Err(e) => crate::warn!("overlay: controller blocking unavailable: {e}"),
    }
    match unsafe { install_cursor() } {
        Ok(()) => crate::info!("overlay: cursor isolation ready (GetCursorPos/SetCursorPos)"),
        Err(e) => crate::warn!("overlay: cursor isolation unavailable: {e}"),
    }
    install_focus();
}

// ── Window focus while Kindle presses buttons ───────────────────────────────────────────
//
// The game reads input only while its window is in front (`+0x23575B0`:
// `GetForegroundWindow() == its window`; the keyboard and mouse code asks the same, and
// `GetActiveWindow` is asked elsewhere). While Kindle holds a button for a script
// (`kindle.debug.press`), and for a moment after, the game's own calls of the two get its
// window, so scripted presses work with the window in the background (another program in
// front, a remote desktop session).

/// Until when (ms, `now_ms`) the game is told its window is in front.
static FOCUS_UNTIL: AtomicU64 = AtomicU64::new(0);
static FOREGROUND: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
const FOCUS_LINGER_MS: u64 = 1500;

fn game_window() -> isize {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};
    static FOUND: AtomicUsize = AtomicUsize::new(0);
    let known = FOUND.load(Ordering::Relaxed);
    if known != 0 {
        return known as isize;
    }
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> windows::core::BOOL {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == std::process::id() && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            unsafe { *(found.0 as *mut isize) = hwnd.0 as isize };
            return false.into();
        }
        true.into()
    }
    let mut found: isize = 0;
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&raw mut found as isize));
    }
    FOUND.store(found as usize, Ordering::Relaxed);
    found
}

fn focus_spoofed() -> bool {
    #[cfg(feature = "rig")]
    if crate::rig::focus_spoofed() {
        return true;
    }
    FOCUS_UNTIL.load(Ordering::Relaxed) > now_ms()
}

unsafe extern "system" fn foreground_window() -> isize {
    if focus_spoofed() {
        let w = game_window();
        if w != 0 {
            return w;
        }
    }
    let real: unsafe extern "system" fn() -> isize = unsafe { std::mem::transmute(FOREGROUND.load(Ordering::Relaxed)) };
    unsafe { real() }
}

unsafe extern "system" fn active_window() -> isize {
    if focus_spoofed() {
        let w = game_window();
        if w != 0 {
            return w;
        }
    }
    let real: unsafe extern "system" fn() -> isize = unsafe { std::mem::transmute(ACTIVE.load(Ordering::Relaxed)) };
    unsafe { real() }
}

fn install_focus() {
    let imports = crate::memory::game_imports("USER32.dll");
    for (name, slot_store, ours) in
        [("GetForegroundWindow", &FOREGROUND, foreground_window as *const () as usize), ("GetActiveWindow", &ACTIVE, active_window as *const () as usize)]
    {
        let Some(&slot) = imports.get(name) else { continue };
        let Some(real) = crate::memory::read_u64(slot).filter(|&f| f != 0) else { continue };
        slot_store.store(real as usize, Ordering::Relaxed);
        if !unsafe { crate::hooks::write_ptr(slot, ours) } {
            crate::warn!("overlay: couldn't take the game's {name}; scripted presses need its window in front");
        }
    }
}

// ── Windows cursor ──────────────────────────────────────────────────────────

// GetCursorPos(LPPOINT): rcx = POINT*
fn get_cursor_pos_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        if blocked() && called_by_game(reg) {
            let p = (*reg).rcx as *mut POINT;
            if !p.is_null() {
                let packed = FROZEN_CURSOR.load(Ordering::Relaxed);
                (*p).x = (packed >> 32) as u32 as i32;
                (*p).y = packed as u32 as i32;
            }
            return 1;
        }
        let original: unsafe extern "system" fn(u64) -> usize = std::mem::transmute(original);
        original((*reg).rcx)
    }
}

// SetCursorPos(x, y): rcx = x, rdx = y
fn set_cursor_pos_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        if blocked() && called_by_game(reg) {
            return 1; // pretend it worked; the cursor stays where the player put it
        }
        let original: unsafe extern "system" fn(u64, u64) -> usize = std::mem::transmute(original);
        original((*reg).rcx, (*reg).rdx)
    }
}

unsafe fn install_cursor() -> Result<(), String> {
    unsafe {
        let exe = GetModuleHandleA(None).map_err(|e| e.to_string())?;
        let mut info = MODULEINFO::default();
        GetModuleInformation(GetCurrentProcess(), exe, &mut info, std::mem::size_of::<MODULEINFO>() as u32)
            .map_err(|e| format!("GetModuleInformation: {e}"))?;
        let start = info.lpBaseOfDll as usize;
        GAME_START.store(start, Ordering::Relaxed);
        GAME_END.store(start + info.SizeOfImage as usize, Ordering::Relaxed);

        let user32 = GetModuleHandleA(s!("user32.dll")).map_err(|_| "user32.dll is not loaded".to_string())?;
        let get = GetProcAddress(user32, s!("GetCursorPos")).ok_or("GetCursorPos not found")? as usize;
        let set = GetProcAddress(user32, s!("SetCursorPos")).ok_or("SetCursorPos not found")? as usize;
        let h1 = hook_closure_retn(get, get_cursor_pos_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook GetCursorPos: {e:?}"))?;
        let h2 = hook_closure_retn(set, set_cursor_pos_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook SetCursorPos: {e:?}"))?;
        std::mem::forget(h1);
        std::mem::forget(h2);
        Ok(())
    }
}

// ── DirectInput ─────────────────────────────────────────────────────────────

const DIRECTINPUT_VERSION: u32 = 0x0800;
const IID_IDIRECTINPUT8W: GUID = GUID::from_values(0xbf798031, 0x483a, 0x4da2, [0xaa, 0x99, 0x5d, 0x64, 0xed, 0x36, 0x97, 0x00]);
const GUID_SYS_KEYBOARD: GUID = GUID::from_values(0x6F1D2B61, 0xD5A0, 0x11CF, [0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00]);
const GUID_SYS_MOUSE: GUID = GUID::from_values(0x6F1D2B60, 0xD5A0, 0x11CF, [0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00]);

const VTBL_RELEASE: usize = 2;
const VTBL_CREATE_DEVICE: usize = 3;
const VTBL_GET_DEVICE_STATE: usize = 9;

type RawObj = *mut *const usize;
type DInput8CreateFn = unsafe extern "system" fn(usize, u32, *const GUID, *mut RawObj, usize) -> i32;
type CreateDeviceFn = unsafe extern "system" fn(RawObj, *const GUID, *mut RawObj, usize) -> i32;
type ReleaseFn = unsafe extern "system" fn(RawObj) -> u32;

/// Creates a throwaway device to find the address of its GetDeviceState implementation.
unsafe fn device_get_state_addr(create: DInput8CreateFn, hinstance: usize, device: &GUID) -> Result<usize, String> {
    unsafe {
        let mut di8: RawObj = std::ptr::null_mut();
        let hr = create(hinstance, DIRECTINPUT_VERSION, &IID_IDIRECTINPUT8W, &mut di8, 0);
        if hr != 0 || di8.is_null() {
            return Err(format!("DirectInput8Create failed ({hr:#010x})"));
        }
        let create_device: CreateDeviceFn = std::mem::transmute(*(*di8).add(VTBL_CREATE_DEVICE));
        let release: ReleaseFn = std::mem::transmute(*(*di8).add(VTBL_RELEASE));
        let mut dev: RawObj = std::ptr::null_mut();
        let hr = create_device(di8, device, &mut dev, 0);
        if hr != 0 || dev.is_null() {
            release(di8);
            return Err(format!("CreateDevice failed ({hr:#010x})"));
        }
        let addr = *(*dev).add(VTBL_GET_DEVICE_STATE);
        let release_dev: ReleaseFn = std::mem::transmute(*(*dev).add(VTBL_RELEASE));
        release_dev(dev);
        release(di8);
        Ok(addr)
    }
}

// GetDeviceState(this, cbData, lpvData): rcx = this, rdx = size, r8 = buffer
fn get_device_state_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        let original: unsafe extern "system" fn(u64, u64, u64) -> usize = std::mem::transmute(original);
        let (this, size, data) = ((*reg).rcx, (*reg).rdx, (*reg).r8);
        let hr = original(this, size, data);
        #[cfg(feature = "rig")]
        let hr = crate::rig::direct_input_read(size, hr);
        // What the player holds, before Kindle blanks or adds anything.
        if hr == 0 && data != 0 {
            if size == 256 {
                let bytes = std::slice::from_raw_parts(data as *const u8, 256);
                for (word, chunk) in KEYS_DOWN.iter().zip(bytes.chunks(64)) {
                    word.store(chunk.iter().enumerate().fold(0u64, |acc, (i, &b)| acc | (((b >> 7) as u64) << i)), Ordering::Relaxed);
                }
            } else if size >= 16 {
                // DIMOUSESTATE(2): x, y, wheel (i32 each), then a byte per button.
                let buttons = std::slice::from_raw_parts((data as *const u8).add(12), (size as usize - 12).min(8));
                MOUSE_DOWN.store(buttons.iter().enumerate().fold(0u8, |acc, (i, &b)| acc | ((b >> 7) << i)), Ordering::Relaxed);
            }
        }
        if hr == 0 && blocked() && data != 0 {
            std::ptr::write_bytes(data as *mut u8, 0, size as usize);
        } else if hr == 0 && data != 0 && size == 256 {
            // Keys still held from a screen of Kindle's: hidden until released.
            for (word, swallow) in SWALLOW_KEYS.iter().enumerate() {
                let mut bits = swallow.load(Ordering::Relaxed);
                if bits == 0 {
                    continue;
                }
                for bit in 0..64 {
                    if bits & (1 << bit) != 0 {
                        let key = (data as *mut u8).add(word * 64 + bit);
                        if *key & 0x80 != 0 {
                            *key = 0;
                        } else {
                            bits &= !(1 << bit);
                        }
                    }
                }
                swallow.store(bits, Ordering::Relaxed);
            }
        } else if hr == 0 && data != 0 && size >= 16 {
            let mut bits = SWALLOW_MOUSE.load(Ordering::Relaxed);
            if bits != 0 {
                let buttons = std::slice::from_raw_parts_mut((data as *mut u8).add(12), (size as usize - 12).min(8));
                for (i, b) in buttons.iter_mut().enumerate() {
                    if bits & (1 << i) != 0 {
                        if *b & 0x80 != 0 {
                            *b = 0;
                        } else {
                            bits &= !(1 << i);
                        }
                    }
                }
                SWALLOW_MOUSE.store(bits, Ordering::Relaxed);
            }
        }
        STATS[if size != 256 {
            2
        } else if hr != 0 {
            1
        } else {
            0
        }]
        .fetch_add(1, Ordering::Relaxed);
        // The keyboard's state is 256 bytes, one per key code (0x80 = down).
        if hr == 0 && size == 256 && data != 0 {
            let now = now_ms();
            for (code, until) in HELD_UNTIL.iter().enumerate() {
                if until.load(Ordering::Relaxed) > now {
                    *(data as *mut u8).add(code) = 0x80;
                }
            }
        }
        if hr == 0 && data != 0 && !blocked() {
            let used = if size == 256 {
                std::slice::from_raw_parts(data as *const u8, 256).iter().any(|&b| b & 0x80 != 0)
            } else if size >= 12 {
                // DIMOUSESTATE(2): moved (x, y) or a button down.
                let p = data as *const i32;
                *p != 0 || *p.add(1) != 0 || std::slice::from_raw_parts((data as *const u8).add(12), size as usize - 12).iter().any(|&b| b & 0x80 != 0)
            } else {
                false
            };
            if used {
                KEYS_USED_AT.store(now_ms(), Ordering::Relaxed);
            }
        }
        if hr == 0 && size == 256 && data != 0 {
            let down = |code: usize| *(data as *const u8).add(code) & 0x80 != 0;
            let left = if down(0xCB) || down(0x1E) { PAD_LEFT } else { 0 };
            let right = if down(0xCD) || down(0x20) { PAD_RIGHT } else { 0 };
            KEYS_LEFT_RIGHT.store(left | right, Ordering::Relaxed);
            KEY_ACTION.store(down(0x12), Ordering::Relaxed);
        }
        if hr == 0 && data != 0 && size == 256 && ACTIONS_BLOCKED.load(Ordering::Relaxed) {
            for code in SWITCH_KEYS {
                *(data as *mut u8).add(code) = 0;
            }
            if INTERACT_BLOCKED.load(Ordering::Relaxed) {
                *(data as *mut u8).add(INTERACT_KEY) = 0;
            }
        }
        hr
    }
}

unsafe fn install_dinput() -> Result<usize, String> {
    unsafe {
        let module = GetModuleHandleA(s!("dinput8.dll")).map_err(|_| "dinput8.dll is not loaded".to_string())?;
        let create = GetProcAddress(module, s!("DirectInput8Create")).ok_or("DirectInput8Create not found")?;
        let create: DInput8CreateFn = std::mem::transmute(create);
        let hinstance = GetModuleHandleA(None).map_err(|e| e.to_string())?.0 as usize;

        let keyboard = device_get_state_addr(create, hinstance, &GUID_SYS_KEYBOARD)?;
        let mouse = device_get_state_addr(create, hinstance, &GUID_SYS_MOUSE)?;
        let mut targets = vec![keyboard];
        if mouse != keyboard {
            targets.push(mouse);
        }
        for &addr in &targets {
            let hook =
                hook_closure_retn(addr, get_device_state_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook GetDeviceState: {e:?}"))?;
            std::mem::forget(hook); // installed for the life of the process
        }
        Ok(targets.len())
    }
}

// ── XInput ──────────────────────────────────────────────────────────────────

// XInputGetState(dwUserIndex, pState): rcx = index, rdx = XINPUT_STATE*
// XINPUT_STATE = { u32 dwPacketNumber; XINPUT_GAMEPAD Gamepad (12 bytes) }
fn xinput_get_state_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        let original: unsafe extern "system" fn(u64, u64) -> usize = std::mem::transmute(original);
        let (index, state) = ((*reg).rcx, (*reg).rdx);
        let mut result = original(index, state);
        STATS[if result != 0 { 4 } else { 3 }].fetch_add(1, Ordering::Relaxed);
        if index == 0 {
            PAD_REAL.store(result == 0, Ordering::Relaxed);
            PAD_READ_AT.store(now_ms(), Ordering::Relaxed);
        }
        if result == 0 && index == 0 && state != 0 {
            // XINPUT_STATE: packet u32, then buttons u16 (+4), triggers, sticks LX (+8), LY (+10).
            let p = state as *const u8;
            let mut buttons = *(p.add(4) as *const u16);
            let (lx, ly) = (*(p.add(8) as *const i16), *(p.add(10) as *const i16));
            if ly > STICK_AS_DPAD {
                buttons |= PAD_UP;
            } else if ly < -STICK_AS_DPAD {
                buttons |= PAD_DOWN;
            }
            if lx > STICK_AS_DPAD {
                buttons |= PAD_RIGHT;
            } else if lx < -STICK_AS_DPAD {
                buttons |= PAD_LEFT;
            }
            PAD_RAW.store(buttons, Ordering::Relaxed);
            let (rx, ry) = (*(p.add(12) as *const i16), *(p.add(14) as *const i16));
            let (lt, rt) = (*p.add(6), *p.add(7));
            if *(p.add(4) as *const u16) != 0
                || [lx, ly, rx, ry].iter().any(|v| v.unsigned_abs() > PAD_IN_USE as u16)
                || lt > TRIGGER_IN_USE
                || rt > TRIGGER_IN_USE
            {
                PAD_USED_AT.store(now_ms(), Ordering::Relaxed);
            }
        }
        if result == 0 && blocked() && state != 0 {
            std::ptr::write_bytes((state as *mut u8).add(4), 0, 12);
        } else if result == 0 && index == 0 && state != 0 {
            // Buttons still held from a screen of Kindle's: hidden until released.
            let swallow = SWALLOW_PAD.load(Ordering::Relaxed);
            if swallow != 0 {
                let buttons = (state as *mut u8).add(4) as *mut u16;
                let still = swallow & *buttons;
                *buttons &= !still;
                SWALLOW_PAD.store(still, Ordering::Relaxed);
            }
        }
        // Buttons held by press(): controller 0 reports them (connected if it wasn't, and for a
        // while after).
        let held = pad_buttons();
        if (held != 0 || PAD_CONNECTED_UNTIL.load(Ordering::Relaxed) > now_ms()) && index == 0 && state != 0 {
            if result != 0 {
                std::ptr::write_bytes(state as *mut u8, 0, 16);
                result = 0;
            }
            let packet = PAD_PACKET.fetch_add(1, Ordering::Relaxed);
            *(state as *mut u32) = packet;
            *((state as *mut u8).add(4) as *mut u16) |= held;
        }
        if result == 0 && state != 0 && ACTIONS_BLOCKED.load(Ordering::Relaxed) {
            // XINPUT_GAMEPAD at +4: buttons u16 (the D-pad its low four bits).
            let interact = if INTERACT_BLOCKED.load(Ordering::Relaxed) { INTERACT_BUTTON } else { 0 };
            *((state as *mut u8).add(4) as *mut u16) &= !(SWITCH_BUTTONS | interact);
        }
        result
    }
}

unsafe fn install_xinput() -> Result<(), String> {
    unsafe {
        let module = GetModuleHandleA(s!("xinput1_3.dll")).map_err(|_| "xinput1_3.dll is not loaded".to_string())?;
        let addr = GetProcAddress(module, s!("XInputGetState")).ok_or("XInputGetState not found")? as usize;
        let hook =
            hook_closure_retn(addr, xinput_get_state_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook XInputGetState: {e:?}"))?;
        std::mem::forget(hook);
        Ok(())
    }
}
