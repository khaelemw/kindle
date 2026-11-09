//! The `map_loading` event: mods hear that the game is about to read an area's file (its map
//! file, "map", or its event file, "events"), before it reads it, so what they change for that
//! load (params, `kindle.maps` and `kindle.events` changes) is in place when the area is built:
//! e.g. item lots before the area rolls its pickups, a character's own map changes before its
//! map file is read.
//!
//! The game opens those files on its loader thread, where Lua can't run, while its own thread
//! keeps running frames (the loading screen). So the loader thread waits (`wait`) while the next
//! frame runs the mods' handlers (`take`, then `Request::finish`), and only when a mod has a
//! handler. If no frame picks the request up in PICKUP (the game's thread busy elsewhere), the
//! file is read without waiting further, and so is every file opened while no frame has run for
//! QUIET (a load that holds the game's thread would otherwise wait PICKUP for each of its files).
//! Not yet seen in game (2026-10-18): whether frames run while each area's files are read.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Whether any mod has a `map_loading` handler (set each frame).
static WANTED: AtomicBool = AtomicBool::new(false);
static QUEUE: Mutex<Vec<Request>> = Mutex::new(Vec::new());

/// How long the loader waits for a frame to take its request.
const PICKUP: Duration = Duration::from_secs(3);
/// How long it waits in all (the handlers' time limit while loading, and some).
const TOTAL: Duration = Duration::from_secs(15);
/// No frame for this long: the game's thread is held, files are read without waiting.
const QUIET: Duration = Duration::from_secs(1);

/// When Kindle's frame last ran (set_wanted is called every frame).
static LAST_FRAME: Mutex<Option<Instant>> = Mutex::new(None);

type Done = Arc<(Mutex<State>, Condvar)>;

#[derive(Clone, Copy, PartialEq)]
enum State {
    Queued,
    Running,
    Finished,
}

pub struct Request {
    pub map: String,
    pub file: &'static str,
    done: Done,
}

impl Request {
    /// The handlers ran: the loader goes on.
    pub fn finish(self) {
        let (state, wake) = &*self.done;
        *state.lock().unwrap_or_else(|e| e.into_inner()) = State::Finished;
        wake.notify_all();
    }
}

/// Each frame: whether any mod has a handler.
pub fn set_wanted(wanted: bool) {
    WANTED.store(wanted, Ordering::Relaxed);
    *LAST_FRAME.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
}

/// The requests waiting for this frame (the game's thread), marked running.
pub fn take() -> Vec<Request> {
    let taken: Vec<Request> = std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()));
    for r in &taken {
        *r.done.0.lock().unwrap_or_else(|e| e.into_inner()) = State::Running;
    }
    taken
}

/// On the loader thread, as the game opens `file` ("map" or "events") of `map`: waits for the
/// mods' handlers (if any mod has one).
pub fn wait(map: &str, file: &'static str) {
    if !WANTED.load(Ordering::Relaxed) {
        return;
    }
    if crate::on_game_thread() {
        crate::debug!("map_loading: {map}'s {file} opened on the game's own thread; not waited for");
        return;
    }
    if LAST_FRAME.lock().unwrap_or_else(|e| e.into_inner()).is_none_or(|t| t.elapsed() > QUIET) {
        crate::warn!("map_loading: {map}'s {file} opened while no frame runs; read without the mods' handlers");
        return;
    }
    let done: Done = Arc::new((Mutex::new(State::Queued), Condvar::new()));
    QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(Request { map: map.to_string(), file, done: done.clone() });
    let started = Instant::now();
    let (state, wake) = &*done;
    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let waited = started.elapsed();
        let limit = if *s == State::Queued { PICKUP } else { TOTAL };
        if *s == State::Finished || waited >= limit {
            break;
        }
        s = wake.wait_timeout(s, limit - waited).unwrap_or_else(|e| e.into_inner()).0;
    }
    match *s {
        State::Finished => crate::debug!("map_loading: {map} ({file}): mods' handlers ran ({} ms)", started.elapsed().as_millis()),
        State::Queued => {
            drop(s);
            // Not taken: no frame ran. Withdrawn, so it isn't run after the file was read.
            QUEUE.lock().unwrap_or_else(|e| e.into_inner()).retain(|r| !Arc::ptr_eq(&r.done, &done));
            crate::warn!("map_loading: {map} ({file}): no frame ran in {} s; read without the mods' handlers", PICKUP.as_secs());
        }
        State::Running => crate::warn!("map_loading: {map} ({file}): mods' handlers still running after {} s; read without waiting more", TOTAL.as_secs()),
    }
}
