//! `debug.watch`: which game code reads a piece of memory (for reverse engineering).
//!
//! The page holding the watched range is marked PAGE_GUARD. The first access raises a guard
//! exception, caught by a vectored exception handler: if the address is in the watched range
//! it records the instruction and the return addresses on the stack, then single-steps that
//! instruction and re-arms the guard (accesses to the rest of the page re-arm silently). The
//! watch ends after `max` distinct instructions, or when cleared.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use windows::Win32::Foundation::{EXCEPTION_GUARD_PAGE, EXCEPTION_SINGLE_STEP};
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};
use windows::Win32::System::Memory::{MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_PROTECTION_FLAGS, VirtualProtect, VirtualQuery};

const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
const TRAP_FLAG: u32 = 0x100;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static LO: AtomicUsize = AtomicUsize::new(0);
static HI: AtomicUsize = AtomicUsize::new(0);
static PAGE: AtomicUsize = AtomicUsize::new(0);
static MAX: AtomicUsize = AtomicUsize::new(0);
/// Distinct instructions seen: (rip, accessed address, return addresses).
static HITS: Mutex<Vec<(usize, usize, Vec<usize>)>> = Mutex::new(Vec::new());

thread_local! {
    /// This thread took a guard hit and is single-stepping past it.
    static STEPPING: Cell<bool> = const { Cell::new(false) };
}

fn page_of(addr: usize) -> usize {
    addr & !0xFFF
}

fn guard(page: usize, on: bool) {
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    if unsafe { VirtualQuery(Some(page as *const _), &mut mbi, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) } == 0 {
        return;
    }
    let base = PAGE_PROTECTION_FLAGS(mbi.Protect.0 & !PAGE_GUARD.0);
    let new = if on { PAGE_PROTECTION_FLAGS(base.0 | PAGE_GUARD.0) } else { base };
    let mut old = PAGE_PROTECTION_FLAGS::default();
    let _ = unsafe { VirtualProtect(page as *const _, 0x1000, new, &mut old) };
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let (rec, ctx) = unsafe { (&*(*info).ExceptionRecord, &mut *(*info).ContextRecord) };
    if rec.ExceptionCode == EXCEPTION_GUARD_PAGE {
        let addr = rec.ExceptionInformation[1];
        let page = PAGE.load(Ordering::Relaxed);
        if page == 0 || page_of(addr) != page {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        if ACTIVE.load(Ordering::Relaxed) && (LO.load(Ordering::Relaxed)..HI.load(Ordering::Relaxed)).contains(&addr) {
            record(ctx.Rip as usize, addr, ctx.Rsp as usize);
        }
        // The guard is gone now: step this one instruction, then put it back.
        STEPPING.with(|s| s.set(true));
        ctx.EFlags |= TRAP_FLAG;
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    if rec.ExceptionCode == EXCEPTION_SINGLE_STEP && STEPPING.with(|s| s.replace(false)) {
        if ACTIVE.load(Ordering::Relaxed) {
            guard(PAGE.load(Ordering::Relaxed), true);
        }
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    EXCEPTION_CONTINUE_SEARCH
}

fn record(rip: usize, addr: usize, rsp: usize) {
    let Ok(mut hits) = HITS.try_lock() else { return };
    let (base, size) = crate::memory::game_module().unwrap_or((0, 0));
    if hits.iter().any(|(r, _, _)| *r == rip.wrapping_sub(base)) {
        return;
    }
    // Return addresses: stack values inside the game's code.
    let mut returns = Vec::new();
    for i in 0..64 {
        let v = unsafe { std::ptr::read_volatile((rsp + i * 8) as *const usize) };
        if v > base && v < base + size {
            returns.push(v - base);
            if returns.len() >= 6 {
                break;
            }
        }
    }
    hits.push((rip.wrapping_sub(base), addr, returns));
    if hits.len() >= MAX.load(Ordering::Relaxed) {
        ACTIVE.store(false, Ordering::Relaxed);
    }
}

/// The mod that started the current watch (it stops when that mod unloads).
static OWNER: Mutex<String> = Mutex::new(String::new());

/// Stops the watch if `owner` started it.
pub fn stop_owned_by(owner: &str) {
    if *OWNER.lock().unwrap_or_else(|e| e.into_inner()) == owner {
        stop();
    }
}

/// Starts watching reads and writes of [addr, addr+len) (one page at most) for mod `owner`.
pub fn start(owner: &str, addr: usize, len: usize, max: usize) -> Result<(), String> {
    *OWNER.lock().unwrap_or_else(|e| e.into_inner()) = owner.to_string();
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
    });
    if len == 0 || page_of(addr) != page_of(addr + len - 1) {
        return Err("the watched range must be within one 4 KB page".into());
    }
    stop();
    HITS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    LO.store(addr, Ordering::Relaxed);
    HI.store(addr + len, Ordering::Relaxed);
    MAX.store(max.max(1), Ordering::Relaxed);
    PAGE.store(page_of(addr), Ordering::Relaxed);
    ACTIVE.store(true, Ordering::Relaxed);
    guard(page_of(addr), true);
    Ok(())
}

/// Stops watching (the page's protection goes back to normal).
pub fn stop() {
    ACTIVE.store(false, Ordering::Relaxed);
    let page = PAGE.load(Ordering::Relaxed);
    if page != 0 {
        guard(page, false);
    }
}

/// What the watch saw: (instruction offset in the exe, address accessed, return offsets).
pub fn hits() -> Vec<(usize, usize, Vec<usize>)> {
    HITS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}
