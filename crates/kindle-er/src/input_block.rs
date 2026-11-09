//! Keeps keyboard, mouse and controller input from the game while the overlay is open, as Kindle for
//! Dark Souls III does (its input_block.rs, whose hooks these are).
//!
//! Elden Ring reads the keyboard and mouse through DirectInput 8 and controllers through XInput 1.4,
//! not window messages, so hudhook's message filter alone isn't enough: `IDirectInputDevice8::
//! GetDeviceState` and `XInputGetState` are hooked and their output blanked while blocking is on. The
//! game also keeps the cursor in the middle of its window (`SetCursorPos`, `ClipCursor`) and reads it
//! (`GetCursorPos`); while blocking, calls *from the game's own code* get the position frozen when it
//! started or are ignored, and the cursor is let go, so the overlay can be clicked. Everyone else
//! (hudhook, Windows, Kindle) sees the real cursor.
//!
//! All hooks are in system DLLs (dinput8, xinput1_4, user32), not in the game's exe.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use ilhook::x64::{CallbackOption, HookFlags, Registers, hook_closure_retn};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::WindowsAndMessaging::{ClipCursor, GetCursorPos};
use windows::core::{GUID, s};

static BLOCKED: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Cursor position (x in the high 32 bits, y in the low) frozen when blocking started.
static FROZEN_CURSOR: AtomicU64 = AtomicU64::new(0);
/// The game's exe in memory, to tell its calls apart from everyone else's.
static GAME_START: AtomicUsize = AtomicUsize::new(0);
static GAME_END: AtomicUsize = AtomicUsize::new(0);

pub fn set_blocked(blocked: bool) {
    if blocked && !BLOCKED.load(Ordering::Relaxed) {
        let mut p = POINT::default();
        // From Kindle's code, so these pass through the hooks: the real position, and the cursor let go.
        if unsafe { GetCursorPos(&mut p) }.is_ok() {
            FROZEN_CURSOR.store(((p.x as u32 as u64) << 32) | p.y as u32 as u64, Ordering::Relaxed);
        }
        let _ = unsafe { ClipCursor(None) };
    }
    BLOCKED.store(blocked, Ordering::Relaxed);
}

fn blocked() -> bool {
    BLOCKED.load(Ordering::Relaxed)
}

/// Whether the hooked function was called from the game's exe: at the hook's entry the caller's
/// return address is the first value on the stack.
fn called_by_game(reg: *mut Registers) -> bool {
    let ret = unsafe { (*reg).get_stack(0) } as usize;
    ret >= GAME_START.load(Ordering::Relaxed) && ret < GAME_END.load(Ordering::Relaxed)
}

/// Installs the hooks once. Failures are logged; the overlay still works without them, the game just
/// also reacts to input while it's open.
pub fn install() {
    if INSTALLED.swap(true, Ordering::Relaxed) {
        return;
    }
    match unsafe { install_dinput() } {
        Ok(n) => kindle_core::info!("overlay: keyboard/mouse blocking ready ({n} DirectInput hook(s))"),
        Err(e) => kindle_core::warn!("overlay: keyboard/mouse blocking unavailable: {e}"),
    }
    match unsafe { install_xinput() } {
        Ok(()) => kindle_core::info!("overlay: controller blocking ready (XInput)"),
        Err(e) => kindle_core::warn!("overlay: controller blocking unavailable: {e}"),
    }
    match unsafe { install_cursor() } {
        Ok(()) => kindle_core::info!("overlay: cursor isolation ready (GetCursorPos/SetCursorPos/ClipCursor)"),
        Err(e) => kindle_core::warn!("overlay: cursor isolation unavailable: {e}"),
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

// SetCursorPos(x, y): rcx = x, rdx = y. ClipCursor(RECT*): rcx. While blocking, the game's are
// taken as done: the cursor stays where the player puts it.
fn set_cursor_pos_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        if blocked() && called_by_game(reg) {
            return 1;
        }
        let original: unsafe extern "system" fn(u64, u64) -> usize = std::mem::transmute(original);
        original((*reg).rcx, (*reg).rdx)
    }
}

fn clip_cursor_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        if blocked() && called_by_game(reg) {
            return 1;
        }
        let original: unsafe extern "system" fn(u64) -> usize = std::mem::transmute(original);
        original((*reg).rcx)
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
        let clip = GetProcAddress(user32, s!("ClipCursor")).ok_or("ClipCursor not found")? as usize;
        let h1 = hook_closure_retn(get, get_cursor_pos_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook GetCursorPos: {e:?}"))?;
        let h2 = hook_closure_retn(set, set_cursor_pos_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook SetCursorPos: {e:?}"))?;
        let h3 = hook_closure_retn(clip, clip_cursor_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook ClipCursor: {e:?}"))?;
        std::mem::forget(h1);
        std::mem::forget(h2);
        std::mem::forget(h3);
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

/// Creates a throwaway device to find the address of its GetDeviceState.
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
        if hr == 0 && blocked() && data != 0 {
            std::ptr::write_bytes(data as *mut u8, 0, size as usize);
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
            std::mem::forget(hook);
        }
        Ok(targets.len())
    }
}

// ── XInput ──────────────────────────────────────────────────────────────────

// XInputGetState(dwUserIndex, pState): rcx = index, rdx = XINPUT_STATE* ({u32 packet, the gamepad's
// 12 bytes}).
fn xinput_get_state_hook(reg: *mut Registers, original: usize) -> usize {
    unsafe {
        let original: unsafe extern "system" fn(u64, u64) -> usize = std::mem::transmute(original);
        let (index, state) = ((*reg).rcx, (*reg).rdx);
        let result = original(index, state);
        if result == 0 && blocked() && state != 0 {
            std::ptr::write_bytes((state as *mut u8).add(4), 0, 12);
        }
        result
    }
}

unsafe fn install_xinput() -> Result<(), String> {
    unsafe {
        let module = GetModuleHandleA(s!("xinput1_4.dll")).map_err(|_| "xinput1_4.dll is not loaded".to_string())?;
        let addr = GetProcAddress(module, s!("XInputGetState")).ok_or("XInputGetState not found")? as usize;
        let hook =
            hook_closure_retn(addr, xinput_get_state_hook, CallbackOption::None, HookFlags::empty()).map_err(|e| format!("hook XInputGetState: {e:?}"))?;
        std::mem::forget(hook);
        Ok(())
    }
}
