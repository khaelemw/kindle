//! The parts of Kindle no game's code is in, shared by each game's runtime (kindle.dll for Dark Souls III,
//! and the games after it): the mod runtime (the mods' Lua states, reloads, the console, running their
//! handlers) with Kindle's own natives and the catalogue's framework, which reach the game through the table
//! of bindings its runtime sets (bindings.rs); logging, Kindle's settings (kindle.toml), the mods folder's hot reload, the
//! clipboard, the file formats Kindle writes (zlib streams, Scaleform movies), mods' stores (kindle.store), the
//! param tables by name (params, the game's bindings naming its tables and embedding their paramdefs), the
//! live console and the overlay's link to the runtime (ui_bridge), and the ways into the game's process:
//! reading its memory, finding its code by byte pattern, hooking and patching it (hook_sites), crash reports,
//! and the debug module's tracer and memory watch.

pub mod aes;
pub mod bindings;
pub mod catalogue;
pub mod clipboard;
pub mod clock;
pub mod config;
pub mod console;
pub mod crash;
pub mod dds;
pub mod deflate;
pub mod emevd;
pub mod evd;
pub mod event_patches;
pub mod events;
pub mod fmg;
pub mod gfx;
pub mod hook_sites;
pub mod hud;
pub mod json;
pub mod log;
pub mod memory;
pub mod mods;
pub mod natives;
pub mod params;
pub mod patterns;
pub mod runtime;
pub mod screens;
pub mod store;
pub mod trace;
pub mod ui_bridge;
pub mod watch;
pub mod watcher;

use std::sync::atomic::{AtomicU32, Ordering};

/// Locks a mutex, going on with its data if a thread panicked while holding it.
pub fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Thread running Kindle's frame task (set each tick).
static TICK_THREAD: AtomicU32 = AtomicU32::new(0);

/// Marks the calling thread as the one Kindle's frame task runs on (each tick).
pub fn set_tick_thread() {
    TICK_THREAD.store(unsafe { windows::Win32::System::Threading::GetCurrentThreadId() }, Ordering::Relaxed);
}

/// Whether the caller is on the game's thread (the one Kindle's frame task runs on).
pub fn on_game_thread() -> bool {
    let me = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
    me == TICK_THREAD.load(Ordering::Relaxed)
}
