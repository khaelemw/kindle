//! Kindle's frame as one of the game's tasks, so the mods run on the game's own thread once a frame, as
//! in Dark Souls III.
//!
//! The game's managers update through `CS::CSEzTask`s (`CSEzUpdateTask<CSEzTask, ...>`): {vtable, a u32
//! at +8, at +0x10 the proxy the game makes when it registers the task}. CSEzTask's vtable has six slots:
//! 0 the task's runtime class, 1 the destructor, 2 execute (this, data), which goes to 3, the class's own
//! (abstract in CSEzTask), 4 register (this, group), which unregisters it from another group, then makes
//! the proxy in the game's task group (one of 0xA9; the asserts there stop the game if CSTaskGroup isn't
//! made yet), 5 unregister. Kindle's task has the game's slots 0, 4 and 5 (from CSEzTask's vtable, which
//! its constructor stores: found by its code) and its own 1, 2 and 3, and registers in group 0, taken to
//! be the frame's first [u: which slot the game's task manager calls, and on which thread; checked at the
//! first run].
//!
//! Until the game first runs the task, Kindle's frame keeps running on a thread of its own (frame.rs);
//! from then on it is the game's, and that thread stops.

use std::panic::{self, AssertUnwindSafe};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, TryLockError};

use kindle_core::runtime::{HALTED, RUNTIME, Runtime};
use kindle_core::{memory, patterns};

/// The task group Kindle's frame runs in.
const GROUP: u32 = 0;
/// CSEzTask's constructor: {the base's constructor, the vtable stored, the proxy cleared}.
const CTOR: &str = "48 89 4C 24 08 53 48 83 EC 30 48 C7 44 24 20 FE FF FF FF 48 8B D9 E8 ?? ?? ?? ?? 90 48 8D 05 ?? ?? ?? ?? 48 89 03 48 C7 43 10 00 00 00 00 48 8B C3 48 83 C4 30 5B C3 CC";
/// Where the constructor's `lea rax, [vtable]` keeps its displacement, and where the instruction ends.
const CTOR_VTABLE: (usize, usize) = (0x1F, 0x23);
const SLOTS: usize = 6;

/// Kindle's task, laid out as a CSEzTask.
#[repr(C)]
struct Task {
    vtable: *const usize,
    base: u32,
    _pad: u32,
    proxy: usize,
}

/// Not registered yet, registered, or given up on (the game's task parts weren't found).
const WAITING: u8 = 0;
const REGISTERED: u8 = 1;
const GIVEN_UP: u8 = 2;
static STATE: AtomicU8 = AtomicU8::new(WAITING);
/// Set the first time the game runs Kindle's task: the frame is the game's from then on.
static TAKEN: AtomicBool = AtomicBool::new(false);

/// Whether Kindle's frame runs on the game's task now.
pub fn taken() -> bool {
    TAKEN.load(Ordering::Acquire)
}

/// Ok on the game's thread once Kindle's frame is the game's task, where what changes the game runs:
/// `what` says what ("the player is changed").
pub fn on_games_thread(what: &str) -> Result<(), String> {
    #[cfg(not(test))]
    if !(taken() && kindle_core::on_game_thread()) {
        return Err(format!("{what} on the game's thread only, and Kindle's frame isn't the game's yet"));
    }
    #[cfg(test)]
    let _ = what;
    Ok(())
}

/// Kindle's vtable from CSEzTask's: the game's runtime class, register and unregister, Kindle's own
/// destructor and execute.
fn vtable_from(game: &[usize; SLOTS]) -> [usize; SLOTS] {
    [game[0], destroy as *const () as usize, execute as *const () as usize, execute as *const () as usize, game[4], game[5]]
}

/// The destructor: Kindle's task lives as long as the game, so nothing is freed.
extern "C" fn destroy(task: *mut Task, _flags: u32) -> *mut Task {
    task
}

/// The game runs the task: Kindle's frame, on the game's thread. A frame Kindle's own thread is still
/// running is skipped rather than waited for.
extern "C" fn execute(_task: *mut Task, _data: usize) {
    if !TAKEN.swap(true, Ordering::AcqRel) {
        kindle_core::set_tick_thread();
        kindle_core::info!("Kindle's frame runs on the game's task now (group {GROUP})");
    }
    if HALTED.load(Ordering::Relaxed) {
        return;
    }
    let result = panic::catch_unwind(AssertUnwindSafe(|| with_runtime(&RUNTIME, crate::frame::tick)));
    if result.is_err() {
        HALTED.store(true, Ordering::Relaxed);
        kindle_core::error!("Kindle hit an internal error and has stopped running mods (the game continues). See the panic above.");
    }
}

/// `f` with the runtime, if it's there and free now.
fn with_runtime(runtime: &Mutex<Option<Runtime>>, f: impl FnOnce(&mut Runtime)) {
    let mut guard = match runtime.try_lock() {
        Ok(g) => g,
        Err(TryLockError::Poisoned(e)) => e.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if let Some(rt) = guard.as_mut() {
        f(rt);
    }
}

/// Registers Kindle's task once the game's task system is up (CSTask and CSTaskGroup made): called each
/// frame of Kindle's own until it is, or until the parts it needs turn out missing.
pub fn try_register() {
    if STATE.load(Ordering::Relaxed) != WAITING || crate::er::instance("CS::CSTask").is_none() || crate::er::instance("CS::CSTaskGroup").is_none() {
        return;
    }
    match register() {
        Ok(()) => {
            STATE.store(REGISTERED, Ordering::Relaxed);
            kindle_core::info!("Kindle's frame task is registered in the game's task group {GROUP}; Kindle's own frame runs until the game first runs it");
        }
        Err(e) => {
            STATE.store(GIVEN_UP, Ordering::Relaxed);
            kindle_core::warn!("{e}: Kindle's frame stays on its own thread");
        }
    }
}

fn register() -> Result<(), String> {
    static VTABLE: OnceLock<[usize; SLOTS]> = OnceLock::new();
    let ctor = patterns::find_unique("CSEzTask constructor", CTOR).ok_or("the game's CSEzTask wasn't found in this build")?;
    let game_vtable = patterns::rip_target(ctor + CTOR_VTABLE.0, ctor + CTOR_VTABLE.1).ok_or("CSEzTask's vtable is unreadable")?;
    let slots = memory::read::<[usize; SLOTS]>(game_vtable).ok_or("CSEzTask's vtable is unreadable")?;
    let (base, size) = memory::game_module().ok_or("the game's module isn't found")?;
    if [0, 4, 5].iter().any(|&i| !(base..base + size).contains(&slots[i])) {
        return Err(format!("CSEzTask's vtable at {game_vtable:#x} doesn't point into the game"));
    }
    let vtable = VTABLE.get_or_init(|| vtable_from(&slots));
    let task = Box::leak(Box::new(Task { vtable: vtable.as_ptr(), base: 0, _pad: 0, proxy: 0 }));
    // CSEzTask's register, from its vtable: the game's own way of adding a task to a group.
    let register: extern "C" fn(*mut Task, u32) = unsafe { std::mem::transmute(slots[4]) };
    register(task, GROUP);
    if unsafe { std::ptr::read_volatile(&task.proxy) } == 0 {
        return Err("the game didn't take Kindle's frame task".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kindles_task_is_a_csez_task_of_its_own() {
        let vt = vtable_from(&[10, 11, 12, 13, 14, 15]);
        assert_eq!([vt[0], vt[4], vt[5]], [10, 14, 15]);
        assert_eq!([vt[1], vt[2], vt[3]], [destroy as *const () as usize, execute as *const () as usize, execute as *const () as usize]);
        // The layout the game's register and execute read: the proxy at +0x10.
        assert_eq!(std::mem::offset_of!(Task, proxy), 0x10);
        let mut t = Task { vtable: vt.as_ptr(), base: 0, _pad: 0, proxy: 0 };
        assert_eq!(destroy(&mut t, 1), &mut t as *mut Task);
        // A runtime another thread holds is left for the next frame, not waited for.
        let rt: Mutex<Option<Runtime>> = Mutex::new(None);
        let held = rt.lock().unwrap();
        let mut ran = false;
        std::thread::scope(|s| {
            s.spawn(|| with_runtime(&rt, |_| ran = true));
        });
        drop(held);
        assert!(!ran);
    }

    /// CSEzTask's constructor found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE), and the vtable it
    /// stores: the runtime class, register and unregister Kindle takes, slot 3 abstract.
    #[test]
    fn the_games_task_parts_are_found() {
        let Some((img, base, sections)) = crate::singletons::tests::eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let (rva, size) = sections.iter().find(|(n, _, _)| n == ".text").map(|&(_, r, s)| (r, s)).unwrap();
        let text = &img[rva..rva + size];
        let p: Vec<Option<u8>> = CTOR.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect();
        let found: Vec<usize> =
            (0..text.len() - p.len()).filter(|&i| p.iter().zip(&text[i..]).all(|(p, b)| p.is_none_or(|v| v == *b))).map(|i| rva + i).collect();
        assert_eq!(found, [0xEB30D0]);
        let disp = i32::from_le_bytes(img[found[0] + CTOR_VTABLE.0..found[0] + CTOR_VTABLE.0 + 4].try_into().unwrap());
        let vtable = (found[0] + CTOR_VTABLE.1).wrapping_add_signed(disp as isize);
        assert_eq!(vtable, 0x2C048A0);
        let slot = |i: usize| u64::from_le_bytes(img[vtable + i * 8..vtable + i * 8 + 8].try_into().unwrap()) as usize - base as usize;
        assert_eq!((0..SLOTS).map(slot).collect::<Vec<_>>(), [0x2652E80, 0xEB33F0, 0x1CC170, 0x251EC90, 0xEB3460, 0xEB34C0]);
    }
}
