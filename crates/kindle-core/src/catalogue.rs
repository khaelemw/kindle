//! The catalogue: Kindle's named, reviewed capabilities for mods (kindle.game).
//!
//! A distributed mod changes the game through Kindle's API only; what it can do that isn't plain
//! data (params, event scripts, text) is an entry here, which Kindle defines, implements and
//! reviews once, for any mod. A mod never gives an address, bytes, a pointer or code:
//! - a **hook point**: a named place where the game makes a decision, with the answers a mod can
//!   give (`choices`, the first being the game's own). A mod sets it as a switch (answered in
//!   native code on any thread, at any rate) or answers each call with a Lua callback (only for a
//!   call on the game's thread; elsewhere the switch answers);
//! - a **field**: a named value, typed and ranged, of the game or of one character (by its
//!   handle, looked up in the game's own lists before every use);
//! - a **function**: a named game function with typed, checked arguments;
//! - an **event**: a notification from any thread, delivered to the mods on Kindle's next frame
//!   (`kindle.on(name, fn)`).
//!
//! Review checklist for every entry, before it ships (its code is Kindle's and runs with its
//! rights; a hole in one is a hole in Kindle):
//! 1. It takes no path, address, pointer, code or size from a mod; ids it passes on are checked
//!    against the game's tables, values against their type and range.
//! 2. It reaches no file, process or network function, and writes no pointer.
//! 3. A character comes as a handle, looked up again before each use.
//! 4. The thread its game code runs on is known (the census, hook_sites.rs), and a Lua callback is
//!    allowed only where it's the game's thread at a low rate.
//! 5. It has a test, and a doc line saying what the game does at that place.

use crate::lock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

/// A value crossing the catalogue: what Lua gives and gets.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
}

/// The kind and range of a field's value or a function's argument.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
}

impl Kind {
    /// `v` as this kind, if it is one and within range.
    pub fn check(self, what: &str, v: &Value) -> Result<Value, String> {
        match (self, v) {
            (Kind::Bool, Value::Bool(b)) => Ok(Value::Bool(*b)),
            (Kind::Int { min, max }, Value::Int(i)) if (min..=max).contains(i) => Ok(Value::Int(*i)),
            (Kind::Int { min, max }, Value::Float(f)) if f.fract() == 0.0 && (min as f64..=max as f64).contains(f) => Ok(Value::Int(*f as i64)),
            (Kind::Float { min, max }, Value::Float(f)) if (min..=max).contains(f) => Ok(Value::Float(*f)),
            (Kind::Float { min, max }, Value::Int(i)) if (min..=max).contains(&(*i as f64)) => Ok(Value::Float(*i as f64)),
            (k, v) => Err(format!("{what}: {v:?} isn't {}", k.describe())),
        }
    }

    pub fn describe(self) -> String {
        match self {
            Kind::Bool => "a boolean".into(),
            Kind::Int { min, max } => format!("an integer in {min}..={max}"),
            Kind::Float { min, max } => format!("a number in {min}..={max}"),
        }
    }
}

/// A named place where the game makes a decision mods can answer.
pub struct HookPoint {
    pub name: &'static str,
    pub doc: &'static str,
    /// The answers; the first is the game's own (no change).
    pub choices: &'static [&'static str],
    switch: AtomicU8,
    callbacks: AtomicBool,
    /// Puts Kindle's hook on the game's code, the first time a mod answers (none: always there).
    install: Option<fn() -> Result<(), String>>,
    installed: OnceLock<Result<(), String>>,
}

impl HookPoint {
    pub const fn new(name: &'static str, doc: &'static str, choices: &'static [&'static str]) -> Self {
        Self { name, doc, choices, switch: AtomicU8::new(0), callbacks: AtomicBool::new(false), install: None, installed: OnceLock::new() }
    }

    /// A hook point whose hook on the game `install` puts there when a mod first answers it, so
    /// the game runs untouched there until then.
    pub const fn hooked(name: &'static str, doc: &'static str, choices: &'static [&'static str], install: fn() -> Result<(), String>) -> Self {
        Self { name, doc, choices, switch: AtomicU8::new(0), callbacks: AtomicBool::new(false), install: Some(install), installed: OnceLock::new() }
    }

    /// Its hook on the game, installed once; why not, if it couldn't be.
    fn ensure_installed(&self) -> Result<(), String> {
        let Some(install) = self.install else { return Ok(()) };
        self.installed.get_or_init(install).clone().map_err(|e| format!("{}: {e}", self.name))
    }

    /// Whether any mod answers it (a switch other than the game's, or a Lua callback).
    pub fn answered(&self) -> bool {
        self.switch() != 0 || self.callbacks.load(Ordering::Relaxed)
    }

    /// The switch's answer (an index into `choices`).
    pub fn switch(&self) -> usize {
        self.switch.load(Ordering::Relaxed) as usize
    }

    /// The answer for one call of the game's, with the call's values: a mod's Lua callback when
    /// one is registered and the call is on the game's thread, else the switch.
    pub fn decide(&'static self, args: &[i64]) -> usize {
        if self.callbacks.load(Ordering::Relaxed) {
            if crate::on_game_thread() {
                if let Some(c) = crate::runtime::dispatch_decide(self, args) {
                    return c;
                }
            } else {
                warn_once(self.name, "a Lua answer was asked for off the game's thread; the switch answered");
            }
        }
        self.switch()
    }

    fn choice(&self, name: &str) -> Result<usize, String> {
        self.choices.iter().position(|c| *c == name).ok_or_else(|| format!("{}: no answer {name:?} (answers: {})", self.name, self.choices.join(", ")))
    }
}

/// Where a field's value is, given the character (None for a field of the game).
pub type Get = fn(Option<usize>) -> Result<Value, String>;
pub type Set = fn(Option<usize>, &Value) -> Result<(), String>;

/// A named value of the game, or of one character.
pub struct Field {
    pub name: &'static str,
    pub doc: &'static str,
    pub kind: Kind,
    /// A character's (given by handle) rather than the game's.
    pub per_character: bool,
    pub get: Get,
    /// None: read only.
    pub set: Option<Set>,
}

/// A named game function.
pub struct Function {
    pub name: &'static str,
    pub doc: &'static str,
    pub args: &'static [(&'static str, Kind)],
    pub call: fn(&[Value]) -> Result<Option<Value>, String>,
}

/// A named notification.
pub struct Event {
    pub name: &'static str,
    pub doc: &'static str,
    /// Its values' names, in order.
    pub args: &'static [&'static str],
    /// Puts Kindle's hook on the game's code, the first time a mod listens (none: always there).
    install: Option<fn() -> Result<(), String>>,
    installed: OnceLock<Result<(), String>>,
}

impl Event {
    pub const fn new(name: &'static str, doc: &'static str, args: &'static [&'static str]) -> Self {
        Self { name, doc, args, install: None, installed: OnceLock::new() }
    }

    /// An event whose hook on the game `install` puts there when a mod first listens.
    pub const fn hooked(name: &'static str, doc: &'static str, args: &'static [&'static str], install: fn() -> Result<(), String>) -> Self {
        Self { name, doc, args, install: Some(install), installed: OnceLock::new() }
    }

    /// A mod listens: its hook on the game, installed once; why not, if it couldn't be.
    pub fn listen(&self) -> Result<(), String> {
        let Some(install) = self.install else { return Ok(()) };
        self.installed.get_or_init(install).clone().map_err(|e| format!("{}: {e}", self.name))
    }
}

/// Every entry.
pub struct Catalogue {
    pub hooks: &'static [&'static HookPoint],
    pub fields: &'static [&'static Field],
    pub functions: &'static [&'static Function],
    pub events: &'static [&'static Event],
}

/// The catalogue with no entries (before the game's runtime binds its own).
static EMPTY: Catalogue = Catalogue { hooks: &[], fields: &[], functions: &[], events: &[] };

/// The game's catalogue (bindings.rs), or none.
pub fn current() -> &'static Catalogue {
    crate::bindings::get().map_or(&EMPTY, |b| b.catalogue)
}

impl Catalogue {
    pub fn hook(&self, name: &str) -> Result<&'static HookPoint, String> {
        self.hooks.iter().copied().find(|h| h.name == name).ok_or_else(|| format!("no hook point {name:?}"))
    }

    pub fn field(&self, name: &str) -> Result<&'static Field, String> {
        self.fields.iter().copied().find(|f| f.name == name).ok_or_else(|| format!("no field {name:?}"))
    }

    pub fn function(&self, name: &str) -> Result<&'static Function, String> {
        self.functions.iter().copied().find(|f| f.name == name).ok_or_else(|| format!("no function {name:?}"))
    }

    pub fn event(&self, name: &str) -> Result<&'static Event, String> {
        self.events.iter().copied().find(|e| e.name == name).ok_or_else(|| format!("no event {name:?}"))
    }

    /// The names are unique across every kind (a mod names an entry by name alone).
    pub fn duplicate_names(&self) -> Vec<&'static str> {
        let mut seen = std::collections::HashSet::new();
        let names = self
            .hooks
            .iter()
            .map(|h| h.name)
            .chain(self.fields.iter().map(|f| f.name))
            .chain(self.functions.iter().map(|f| f.name))
            .chain(self.events.iter().map(|e| e.name));
        names.filter(|n| !seen.insert(*n)).collect()
    }
}

// ── switches: which mod set each, cleared at its unload ─────────────────────────────────────

type Owners = HashMap<&'static str, Vec<(String, usize)>>;
/// Hook point name -> the mods that answer it, in the order they did, with their answer (all
/// the same: a different one is refused).
static OWNERS: Mutex<Option<Owners>> = Mutex::new(None);

/// A mod sets a hook point's switch (its first answer, the game's own, takes the mod's back). Mods
/// that answer a hook point all give the same answer: a different one is refused, naming who
/// answers. It holds while any of them does.
pub fn set_switch(hook: &'static HookPoint, owner: &str, choice: &str) -> Result<(), String> {
    let c = hook.choice(choice)?;
    if c != 0 {
        hook.ensure_installed()?;
    }
    let mut owners = lock(&OWNERS);
    let holders = owners.get_or_insert_with(HashMap::new).entry(hook.name).or_default();
    if c != 0
        && let Some((other, theirs)) = holders.iter().find(|(o, a)| o != owner && *a != c)
    {
        return Err(format!("{}: {other} already answers it ({})", hook.name, hook.choices[*theirs]));
    }
    holders.retain(|(o, _)| o != owner);
    if c != 0 {
        holders.push((owner.to_string(), c));
    }
    hook.switch.store(holders.first().map_or(0, |&(_, a)| a) as u8, Ordering::Relaxed);
    Ok(())
}

/// A mod registered a Lua answer for the hook point.
pub fn add_callback(hook: &'static HookPoint) -> Result<(), String> {
    hook.ensure_installed()?;
    hook.callbacks.store(true, Ordering::Relaxed);
    Ok(())
}

/// A mod unloads: its answers are taken back (a hook point no mod answers is the game's again).
pub fn release(cat: &Catalogue, owner: &str) {
    let mut owners = lock(&OWNERS);
    let Some(owners) = owners.as_mut() else { return };
    for (name, holders) in owners.iter_mut() {
        if !holders.iter().any(|(o, _)| o == owner) {
            continue;
        }
        holders.retain(|(o, _)| o != owner);
        if let Ok(h) = cat.hook(name) {
            h.switch.store(holders.first().map_or(0, |&(_, a)| a) as u8, Ordering::Relaxed);
        }
    }
    owners.retain(|_, holders| !holders.is_empty());
}

// ── fields and functions ─────────────────────────────────────────────────────────────────────

/// A field's value (`chr`: the character's address, found from its handle by the caller).
pub fn get(field: &Field, chr: Option<usize>) -> Result<Value, String> {
    check_target(field, chr)?;
    (field.get)(chr)
}

/// Sets a field, after checking the value's kind and range.
pub fn set(field: &Field, chr: Option<usize>, v: &Value) -> Result<(), String> {
    check_target(field, chr)?;
    let set = field.set.ok_or_else(|| format!("{} is read only", field.name))?;
    let v = field.kind.check(field.name, v)?;
    set(chr, &v)
}

fn check_target(field: &Field, chr: Option<usize>) -> Result<(), String> {
    match (field.per_character, chr) {
        (true, None) => Err(format!("{} is a character's: give its handle", field.name)),
        (false, Some(_)) => Err(format!("{} is the game's, not a character's", field.name)),
        _ => Ok(()),
    }
}

/// Calls a function, after checking its arguments' count, kinds and ranges.
pub fn call(f: &Function, args: &[Value]) -> Result<Option<Value>, String> {
    if args.len() != f.args.len() {
        let names: Vec<&str> = f.args.iter().map(|(n, _)| *n).collect();
        return Err(format!("{} takes {} argument(s) ({}), not {}", f.name, f.args.len(), names.join(", "), args.len()));
    }
    let checked: Vec<Value> = f.args.iter().zip(args).map(|((n, k), v)| k.check(&format!("{} {n}", f.name), v)).collect::<Result<_, _>>()?;
    (f.call)(&checked)
}

// ── events: from any thread, delivered on Kindle's next frame ─────────────────────────────────

static QUEUE: Mutex<Vec<(&'static str, Vec<Value>)>> = Mutex::new(Vec::new());
/// Events waiting at most; more are dropped (a stuck frame task shouldn't grow the queue).
const QUEUE_MAX: usize = 4096;

/// The game did something: queued for the mods' handlers at Kindle's next frame.
pub fn emit(event: &'static Event, values: Vec<Value>) {
    let mut q = lock(&QUEUE);
    if q.len() < QUEUE_MAX {
        q.push((event.name, values));
    } else {
        drop(q);
        warn_once(event.name, "the event queue is full; events are dropped until Kindle's frame takes them");
    }
}

/// The events since the last frame, in order.
pub fn take_events() -> Vec<(&'static str, Vec<Value>)> {
    std::mem::take(&mut *lock(&QUEUE))
}

fn warn_once(name: &'static str, what: &str) {
    static WARNED: Mutex<Option<std::collections::HashSet<&'static str>>> = Mutex::new(None);
    if lock(&WARNED).get_or_insert_with(Default::default).insert(name) {
        crate::warn!("{name}: {what}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static ANSWER: HookPoint = HookPoint::new("test.answer", "a test decision", &["game", "allow", "keep"]);
    static STORED: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    static LEVEL: Field = Field {
        name: "test.level",
        doc: "a test field",
        kind: Kind::Int { min: 1, max: 802 },
        per_character: false,
        get: |_| Ok(Value::Int(STORED.load(Ordering::Relaxed))),
        set: Some(|_, v| {
            if let Value::Int(i) = v {
                STORED.store(*i, Ordering::Relaxed);
            }
            Ok(())
        }),
    };
    static ADD: Function = Function {
        name: "test.add",
        doc: "adds",
        args: &[("a", Kind::Int { min: 0, max: 10 }), ("b", Kind::Float { min: 0.0, max: 1.0 })],
        call: |a| match (&a[0], &a[1]) {
            (Value::Int(x), Value::Float(y)) => Ok(Some(Value::Float(*x as f64 + y))),
            _ => Err("kinds".into()),
        },
    };
    static INSTALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    static HOOKED: HookPoint = HookPoint::hooked("test.hooked", "a decision hooked on first use", &["game", "hold"], || {
        INSTALLS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });
    static MISSING: HookPoint = HookPoint::hooked("test.missing", "a hook not found", &["game", "hold"], || Err("not found".into()));
    static TOLD: Event = Event::new("test.told", "a test event", &["n"]);
    static TEST: Catalogue = Catalogue { hooks: &[&ANSWER, &HOOKED, &MISSING], fields: &[&LEVEL], functions: &[&ADD], events: &[&TOLD] };

    #[test]
    fn switches_are_held_by_one_mod_and_released() {
        let h = TEST.hook("test.answer").unwrap();
        assert_eq!(h.switch(), 0);
        set_switch(h, "a", "keep").unwrap();
        assert_eq!(h.switch(), 2);
        assert!(set_switch(h, "b", "allow").is_err(), "another mod's different answer is refused");
        set_switch(h, "b", "keep").unwrap();
        assert!(set_switch(h, "a", "nope").is_err());
        release(&TEST, "a");
        assert_eq!(h.switch(), 2, "b still answers keep");
        release(&TEST, "b");
        assert_eq!(h.switch(), 0);
        // Without a callback the switch answers, on any thread.
        assert_eq!(h.decide(&[1, 2]), 0);
    }

    #[test]
    fn hooks_are_installed_when_first_answered() {
        // Its own mod's name: the tests run at once, and a release takes back every answer of its mod.
        assert!(!HOOKED.answered());
        set_switch(&HOOKED, "installer", "game").unwrap();
        assert_eq!(INSTALLS.load(Ordering::Relaxed), 0, "the game's own answer needs no hook");
        set_switch(&HOOKED, "installer", "hold").unwrap();
        add_callback(&HOOKED).unwrap();
        assert_eq!(INSTALLS.load(Ordering::Relaxed), 1, "installed once");
        assert!(HOOKED.answered());
        release(&TEST, "installer");
        assert!(set_switch(&MISSING, "installer", "hold").is_err(), "a hook point that can't be hooked refuses answers");
        assert!(add_callback(&MISSING).is_err());
        assert_eq!(MISSING.switch(), 0);
    }

    #[test]
    fn fields_and_functions_check_their_values() {
        let f = TEST.field("test.level").unwrap();
        set(f, None, &Value::Int(80)).unwrap();
        assert_eq!(get(f, None).unwrap(), Value::Int(80));
        assert!(set(f, None, &Value::Int(803)).is_err());
        assert!(set(f, None, &Value::Bool(true)).is_err());
        assert!(set(f, Some(0x1000), &Value::Int(5)).is_err(), "a game field takes no character");
        let add = TEST.function("test.add").unwrap();
        assert_eq!(call(add, &[Value::Int(2), Value::Float(0.5)]).unwrap(), Some(Value::Float(2.5)));
        assert!(call(add, &[Value::Int(11), Value::Float(0.5)]).is_err());
        assert!(call(add, &[Value::Int(2)]).is_err());
    }

    #[test]
    fn events_queue_in_order_and_names_are_unique() {
        emit(&TOLD, vec![Value::Int(1)]);
        emit(&TOLD, vec![Value::Int(2)]);
        let got: Vec<_> = take_events().into_iter().filter(|(n, _)| *n == "test.told").collect();
        assert_eq!(got, vec![("test.told", vec![Value::Int(1)]), ("test.told", vec![Value::Int(2)])]);
        assert!(TEST.duplicate_names().is_empty());
    }
}
