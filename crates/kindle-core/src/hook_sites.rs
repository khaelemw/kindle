//! Every place Kindle hooks or patches the game's code, declared in one shape: a `Site` (a hook)
//! or a `Patch` (bytes changed in place), each with its name, the part of Kindle it serves, where
//! it is (a byte pattern found in the game's code, how many times it matches, and the offset from
//! a match to the hook or the bytes), and what doesn't work without it. Installing through here
//! logs each one the same way and keeps the list for reports (watchdog).
//!
//! The declarations are also read offline by a hook audit of the game's code: a jmp-back hook moves
//! the instructions covering the 14 bytes at its address out to a trampoline, so no jump of the
//! game's may land inside them, and a return hook must be at a function's entry. (A jmp-back hook
//! placed over two of a function's jump targets crashed the game, 2026-10-14.) Keep every
//! declaration a plain `Site { .. }` / `Patch { .. }` with its pattern a const or a literal, so the
//! check can read it.
//!
//! The game puts some of its own code back while it runs (its anti-tamper's repair, as it reloads
//! a world after a death and on other paths: verified 2026-10-15 on two games, the Estus allotment
//! hooks and the load's guest check gone mid-session, every other hook in place), which took
//! Kindle's hooks there out unseen. Each hook and patch keeps the bytes it replaced and the bytes
//! it wrote; every frame (keep_frame) a place whose code is the game's own again gets Kindle's back,
//! logged, and counted in the reports.
//!
//! Not here: Kindle's hooks on Windows' own functions (input_block.rs, party_saves.rs's file
//! calls) and the debug tracer's (trace.rs), which hooks wherever it's asked.

use std::sync::Mutex;

use crate::lock;
use ilhook::x64::{CallbackOption, ClosureHookPoint, HookFlags, Registers, hook_closure_jmp_back, hook_closure_retn};

#[derive(Clone, Copy)]
pub enum Kind {
    /// Runs before the instruction at the address, the registers changeable, then the game's
    /// code goes on (ilhook's jmp-back).
    JmpBack(fn(*mut Registers)),
    /// Takes the place of the function whose entry is the address, given the original's address.
    Retn(fn(*mut Registers, usize) -> usize),
}

/// A hook on the game's code.
pub struct Site {
    /// The part of Kindle it serves, as its log lines start ("co-op", "events"...).
    pub area: &'static str,
    pub name: &'static str,
    /// "" for a hook whose address its part of Kindle finds another way (a call's target, the one
    /// match of several near another function): `hook_at` only.
    pub pattern: &'static str,
    /// From a match of the pattern to the hook's address.
    pub offset: isize,
    /// How many times the pattern matches in the game; each match is hooked (0 with no pattern).
    pub matches: usize,
    pub kind: Kind,
    /// What doesn't work without it, for the log.
    pub without: &'static str,
}

/// Bytes of the game's code changed in place (a branch taken or skipped).
pub struct Patch {
    pub area: &'static str,
    pub name: &'static str,
    pub pattern: &'static str,
    /// From a match of the pattern to the bytes.
    pub offset: isize,
    pub matches: usize,
    /// What's there in the game (the first bytes at least), and what Kindle writes.
    pub from: &'static [u8],
    pub to: &'static [u8],
    pub without: &'static str,
}

/// A place Kindle changed: its name, where, the game's bytes there before and Kindle's after (as
/// many as a hook's jump covers), and how many times the game put its own back.
struct Installed {
    name: &'static str,
    at: usize,
    game: Vec<u8>,
    kindle: Vec<u8>,
    put_back: u32,
    /// Neither the game's nor Kindle's any more (something else changed it): no longer kept.
    lost: bool,
}

/// What's installed.
static INSTALLED: Mutex<Vec<Installed>> = Mutex::new(Vec::new());

/// How often the game runs a hook and on which threads (the catalogue's thread census: a Lua
/// callback is allowed only for a hook point the game runs on its own thread at a low rate).
pub struct Census {
    pub area: &'static str,
    pub name: &'static str,
    pub calls: std::sync::atomic::AtomicU64,
    /// Calls on another thread than the game's (Kindle's frame task's).
    pub off_game_thread: std::sync::atomic::AtomicU64,
    /// The threads seen (the first few, 0 = none).
    pub threads: [std::sync::atomic::AtomicU32; 4],
    first: std::sync::OnceLock<std::time::Instant>,
}

impl Census {
    fn record(&self) {
        use std::sync::atomic::Ordering::Relaxed;
        self.first.get_or_init(std::time::Instant::now);
        self.calls.fetch_add(1, Relaxed);
        if !crate::on_game_thread() {
            self.off_game_thread.fetch_add(1, Relaxed);
        }
        let me = unsafe { windows::Win32::System::Threading::GetCurrentThreadId() };
        for t in &self.threads {
            match t.compare_exchange(0, me, Relaxed, Relaxed) {
                Ok(_) => break,
                Err(seen) if seen == me => break,
                Err(_) => {}
            }
        }
    }

    /// Seconds since the first call (None before one).
    pub fn seconds(&self) -> Option<f64> {
        self.first.get().map(|t| t.elapsed().as_secs_f64())
    }
}

static CENSUS: Mutex<Vec<&'static Census>> = Mutex::new(Vec::new());

fn census_slot(area: &'static str, name: &'static str) -> &'static Census {
    let mut all = lock(&CENSUS);
    if let Some(c) = all.iter().find(|c| c.name == name) {
        return c;
    }
    let c: &'static Census = Box::leak(Box::new(Census {
        area,
        name,
        calls: Default::default(),
        off_game_thread: Default::default(),
        threads: Default::default(),
        first: std::sync::OnceLock::new(),
    }));
    all.push(c);
    c
}

/// Every hooked site's counts.
pub fn census() -> Vec<&'static Census> {
    lock(&CENSUS).clone()
}
/// How many bytes of a hook's place are kept: its jump (14) and a little more.
const HOOK_BYTES: usize = 16;

/// The pattern's matches, if there are as many as declared (logged either way).
fn matches(area: &str, name: &str, pattern: &str, count: usize, without: &str) -> Option<Vec<usize>> {
    let found: Vec<usize> = if count == 1 { crate::patterns::find_unique(name, pattern).into_iter().collect() } else { crate::patterns::find_all(pattern) };
    if found.len() != count {
        crate::warn!("{area}: the game's {name} wasn't found ({} of {count} matches); {without}", found.len());
        return None;
    }
    Some(found)
}

impl Site {
    /// The pattern's matches (as many as declared, else None, logged).
    pub fn find(&'static self) -> Option<Vec<usize>> {
        matches(self.area, self.name, self.pattern, self.matches, self.without)
    }

    /// Hooks the game at `at`, the hook's own address (a match plus the offset): the hook, or why
    /// not (also logged).
    pub fn hook_at(&'static self, at: usize) -> Result<ClosureHookPoint<'static>, String> {
        let game = crate::memory::read_bytes(at, HOOK_BYTES).unwrap_or_default();
        let counts = census_slot(self.area, self.name);
        let hooked = match self.kind {
            Kind::JmpBack(f) => unsafe {
                hook_closure_jmp_back(
                    at,
                    move |r| {
                        counts.record();
                        f(r)
                    },
                    CallbackOption::None,
                    HookFlags::empty(),
                )
            },
            Kind::Retn(f) => unsafe {
                hook_closure_retn(
                    at,
                    move |r, original| {
                        counts.record();
                        f(r, original)
                    },
                    CallbackOption::None,
                    HookFlags::empty(),
                )
            },
        };
        match hooked {
            Ok(h) => {
                let base = crate::memory::game_module().map_or(0, |(b, _)| b);
                let kind = match self.kind {
                    Kind::JmpBack(_) => "jmp-back",
                    Kind::Retn(_) => "return",
                };
                crate::debug!("hooks: {} at {}+{:#x} ({kind})", self.name, crate::bindings::exe(), at.wrapping_sub(base));
                let kindle = crate::memory::read_bytes(at, HOOK_BYTES).unwrap_or_default();
                keep(self.name, at, game, kindle);
                Ok(h)
            }
            Err(e) => {
                crate::warn!("{}: couldn't hook the game's {} ({e:?}); {}", self.area, self.name, self.without);
                Err(format!("couldn't hook the game's {} ({e:?})", self.name))
            }
        }
    }

    /// Where the hook goes for a match.
    pub fn at(&self, found: usize) -> usize {
        found.wrapping_add_signed(self.offset)
    }

    /// Hooks these matches (from `find`) for the life of the process; whether all were.
    pub fn hook_all(&'static self, found: &[usize]) -> bool {
        let mut ok = true;
        for &m in found {
            match self.hook_at(self.at(m)) {
                Ok(h) => std::mem::forget(h),
                Err(_) => ok = false,
            }
        }
        ok
    }

    /// Finds and hooks every match, for the life of the process: the first match (for reads near
    /// it), or None if a match wasn't found or hooked (logged).
    /// Installs the site, or says what's lost without it.
    pub fn install_or_why(&'static self) -> Result<(), String> {
        self.install().map(|_| ()).ok_or_else(|| self.without.to_string())
    }

    pub fn install(&'static self) -> Option<usize> {
        let found = self.find()?;
        self.hook_all(&found).then(|| found[0])
    }
}

impl Patch {
    /// The pattern's matches, if there are as many as declared (logged either way).
    pub fn find(&'static self) -> Option<Vec<usize>> {
        matches(self.area, self.name, self.pattern, self.matches, self.without)
    }

    /// Changes every match's bytes, if they're as expected (logged otherwise); whether all were.
    pub fn apply(&'static self) -> bool {
        self.find().is_some_and(|found| self.apply_at(&found))
    }

    /// Changes these matches' bytes (for a place a hook shares: found once, before either changes
    /// what the pattern matches).
    pub fn apply_at(&'static self, found: &[usize]) -> bool {
        let mut ok = true;
        for &m in found {
            let at = m.wrapping_add_signed(self.offset);
            if crate::memory::read_bytes(at, self.from.len()).as_deref() != Some(self.from) {
                crate::warn!("{}: the game's {} isn't as expected; {}", self.area, self.name, self.without);
                ok = false;
            } else if let Some(game) = crate::memory::read_bytes(at, self.to.len())
                && write_code(at, self.to)
            {
                // The game's bytes as long as Kindle's (`from` may be only the first of them).
                keep(self.name, at, game, self.to.to_vec());
            } else {
                crate::warn!("{}: couldn't change the game's {}; {}", self.area, self.name, self.without);
                ok = false;
            }
        }
        ok
    }
}

/// Writes bytes over the game's code (kept executable: the game's other threads may run it).
pub fn write_code(at: usize, bytes: &[u8]) -> bool {
    use windows::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, VirtualProtect};
    let mut old = PAGE_PROTECTION_FLAGS(0);
    let p = at as *const std::ffi::c_void;
    if unsafe { VirtualProtect(p, bytes.len(), PAGE_EXECUTE_READWRITE, &mut old) }.is_err() {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        unsafe { std::ptr::write_volatile((at + i) as *mut u8, *b) };
    }
    let mut ignored = PAGE_PROTECTION_FLAGS(0);
    let _ = unsafe { VirtualProtect(p, bytes.len(), old, &mut ignored) };
    true
}

/// Keeps a place Kindle changed (see keep_frame); a place's bytes unreadable: not kept.
fn keep(name: &'static str, at: usize, game: Vec<u8>, kindle: Vec<u8>) {
    let kept = !game.is_empty() && game.len() == kindle.len() && game != kindle;
    lock(&INSTALLED).push(Installed { name, at, game, kindle, put_back: 0, lost: !kept });
}

/// Each frame: a place whose code the game put back gets Kindle's again (see the module's notes).
pub fn keep_frame() {
    let base = crate::memory::game_module().map_or(0, |(b, _)| b);
    let mut list = lock(&INSTALLED);
    for p in list.iter_mut().filter(|p| !p.lost) {
        let Some(now) = crate::memory::read_bytes(p.at, p.kindle.len()) else { continue };
        if now == p.kindle {
            continue;
        }
        if now != p.game {
            p.lost = true;
            crate::warn!(
                "hooks: {} at {}+{:#x} was changed by something else ({}); left as it is",
                p.name,
                crate::bindings::exe(),
                p.at.wrapping_sub(base),
                hex(&now)
            );
            continue;
        }
        if write_code(p.at, &p.kindle) {
            p.put_back += 1;
            if p.put_back <= 3 || p.put_back.is_power_of_two() {
                crate::info!(
                    "hooks: the game put its own code back at {} ({}+{:#x}); Kindle's is in again ({} time(s))",
                    p.name,
                    crate::bindings::exe(),
                    p.at.wrapping_sub(base),
                    p.put_back
                );
            }
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// What's installed, as "name +0x.." (the address in the game) and how often the game put its own
/// code back there, for reports.
pub fn installed() -> Vec<String> {
    let base = crate::memory::game_module().map_or(0, |(b, _)| b);
    lock(&INSTALLED)
        .iter()
        .map(|p| {
            let note = match (p.lost, p.put_back) {
                (true, _) => " (not kept)".to_string(),
                (false, 0) => String::new(),
                (false, n) => format!(" (put back by the game {n} time(s))"),
            };
            format!("{} +{:#x}{note}", p.name, p.at.wrapping_sub(base))
        })
        .collect()
}
