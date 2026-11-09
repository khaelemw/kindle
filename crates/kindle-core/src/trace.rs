//! `debug.trace_next`: records every instruction one call of a game function runs, the game's
//! protected (Arxan) code included, for reverse engineering. The function's entry is hooked;
//! on its next call that thread runs with the trap flag set and a vectored exception handler
//! records each instruction (its address and its 16 bytes: protected code changes itself as it
//! runs) until the function returns. Kindle's own code met on the way (its hooks) runs without
//! being recorded. A writer thread saves the trace to `logs/trace_<n>.bin`: the game module's
//! base (u64), then records of address (u64) + 16 code bytes.
//!
//! Single-stepping is what a debugger does without one attached: the game's protection doesn't
//! notice it (debug registers it does). The handler must clear the trap flag itself when a
//! trace ends; an unhandled single-step ends the process.

use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use windows::Win32::Foundation::EXCEPTION_SINGLE_STEP;
use windows::Win32::System::Diagnostics::Debug::{AddVectoredExceptionHandler, EXCEPTION_POINTERS};

const TRAP_FLAG: u32 = 0x100;
const EXCEPTION_CONTINUE_EXECUTION: i32 = -1;
const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

thread_local! {
    /// This thread is being traced: the traced function's return address and the stack
    /// pointer at its entry (the trace ends when it returns there).
    static UNTIL: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

/// A trace: (address, the 16 bytes there) per instruction.
type Steps = Vec<(u64, [u8; 16])>;
/// The trace being recorded.
static STEPS: Mutex<Steps> = Mutex::new(Vec::new());
/// A finished trace, waiting for the writer thread.
static DONE: Mutex<Option<Steps>> = Mutex::new(None);
static MAX: AtomicUsize = AtomicUsize::new(0);
static ARMED: AtomicBool = AtomicBool::new(false);
static RUNS: AtomicUsize = AtomicUsize::new(0);
/// Kindle's own module (its code is stepped through without being recorded).
static OWN: Mutex<(usize, usize)> = Mutex::new((0, 0));

/// Traces the next call of the game function at `rva` (the function's first instruction: the
/// hook goes there), up to `max` recorded instructions.
pub fn trace_next(rva: usize, max: usize) -> Result<(), String> {
    install();
    let base = crate::memory::game_module().ok_or("the game's module isn't known")?.0;
    static HOOKED: Mutex<Vec<usize>> = Mutex::new(Vec::new());
    let mut hooked = HOOKED.lock().unwrap_or_else(|e| e.into_inner());
    if !hooked.contains(&rva) {
        use ilhook::x64::{CallbackOption, HookFlags, hook_closure_jmp_back};
        let h = unsafe { hook_closure_jmp_back(base + rva, entry, CallbackOption::None, HookFlags::empty()) }
            .map_err(|e| format!("couldn't hook +{rva:#x} ({e:?})"))?;
        std::mem::forget(h); // for the life of the process
        hooked.push(rva);
    }
    MAX.store(max.max(1), Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
    Ok(())
}

fn install() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        *OWN.lock().unwrap_or_else(|e| e.into_inner()) = own_module();
        unsafe { AddVectoredExceptionHandler(1, Some(handler)) };
        // Files are written here, not from the exception handler.
        std::thread::spawn(|| {
            loop {
                if let Some(steps) = DONE.lock().unwrap_or_else(|e| e.into_inner()).take() {
                    write(&steps);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
    });
}

/// The hooked function's entry: arms this thread (the hook restores the flags register with
/// the trap flag set, so stepping starts at the function's own first instruction).
fn entry(reg: *mut ilhook::x64::Registers) {
    if !ARMED.swap(false, Ordering::Relaxed) {
        return;
    }
    let r = unsafe { &mut *reg };
    let ret = unsafe { *(r.rsp as *const usize) };
    STEPS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    UNTIL.with(|u| u.set(Some((ret, r.rsp as usize))));
    r.rflags |= TRAP_FLAG as u64;
}

unsafe extern "system" fn handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    let (rec, ctx) = unsafe { (&*(*info).ExceptionRecord, &mut *(*info).ContextRecord) };
    let Some((ret, at)) = UNTIL.with(|u| u.get()) else { return EXCEPTION_CONTINUE_SEARCH };
    if rec.ExceptionCode != EXCEPTION_SINGLE_STEP {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let rip = ctx.Rip as usize;
    // Back at the caller: the traced call is over.
    if rip == ret && (ctx.Rsp as usize) > at {
        finish();
        ctx.EFlags &= !TRAP_FLAG;
        return EXCEPTION_CONTINUE_EXECUTION;
    }
    let (lo, hi) = *OWN.lock().unwrap_or_else(|e| e.into_inner());
    if !(lo..hi).contains(&rip)
        && let Ok(mut steps) = STEPS.try_lock()
    {
        let mut code = [0u8; 16];
        unsafe { std::ptr::copy_nonoverlapping(rip as *const u8, code.as_mut_ptr(), 16) };
        steps.push((rip as u64, code));
        if steps.len() >= MAX.load(Ordering::Relaxed) {
            drop(steps);
            finish();
            ctx.EFlags &= !TRAP_FLAG;
            return EXCEPTION_CONTINUE_EXECUTION;
        }
    }
    ctx.EFlags |= TRAP_FLAG;
    EXCEPTION_CONTINUE_EXECUTION
}

/// Ends this thread's trace and hands the steps to the writer.
fn finish() {
    UNTIL.with(|u| u.set(None));
    if let Ok(mut steps) = STEPS.try_lock() {
        let taken = std::mem::take(&mut *steps);
        if let Ok(mut done) = DONE.try_lock() {
            *done = Some(taken);
        }
    }
}

fn write(steps: &[(u64, [u8; 16])]) {
    let n = RUNS.fetch_add(1, Ordering::Relaxed);
    let base = crate::memory::game_module().map_or(0, |(b, _)| b) as u64;
    let mut out = base.to_le_bytes().to_vec();
    for (rip, code) in steps {
        out.extend_from_slice(&rip.to_le_bytes());
        out.extend_from_slice(code);
    }
    let Some(path) = log_dir().map(|d| d.join(format!("trace_{n}.bin"))) else { return };
    match std::fs::write(&path, out) {
        Ok(()) => crate::info!("trace: {} instructions recorded to {}", steps.len(), path.display()),
        Err(e) => crate::warn!("trace: couldn't write {}: {e}", path.display()),
    }
}

fn log_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("kindle").join("logs")))
}

/// Kindle's own module range.
fn own_module() -> (usize, usize) {
    use windows::Win32::System::LibraryLoader::{GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleHandleExW};
    use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut h = windows::Win32::Foundation::HMODULE::default();
    let here = own_module as *const () as *const u16;
    unsafe {
        if GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, windows::core::PCWSTR(here), &mut h)
            .is_err()
        {
            return (0, 0);
        }
        let mut info = MODULEINFO::default();
        if GetModuleInformation(GetCurrentProcess(), h, &mut info, std::mem::size_of::<MODULEINFO>() as u32).is_err() {
            return (0, 0);
        }
        (info.lpBaseOfDll as usize, info.lpBaseOfDll as usize + info.SizeOfImage as usize)
    }
}
