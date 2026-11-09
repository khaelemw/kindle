//! Mods' changes to the game's event scripts (`kindle.events.*` patches): small operations on
//! an event of a map's event file (evd.rs), applied when the game opens that file (each game's
//! files overlay serves the changed file), so they take effect from the next time the map's events
//! load (loading into the area). The game opens the file on its loader thread, so patches are data,
//! not Lua callbacks. The game's side reads a map's event file as the game has it (`set_reader`)
//! and stores the changed one (`patched` gives it unpacked).
//!
//! Operations, applied per map in the order mods made them (an index counts the event's
//! commands as they are by then): disable an event (no commands, it ends at once), remove,
//! insert or replace one of its commands, add a new event, start an event when the map's
//! events load (a `2000[0]` command, slot and arguments, in the map's event 0: what the game's
//! own event 0 does for its events). Parameters (an event's arguments copied into a command's)
//! keep pointing at the same commands. Two mods changing the same event are both named in the
//! log: their changes are applied in turn, which may not be what either wants.
//!
//! A checked edit (Op::Edit, `events.edit`) changes one event all or nothing: its steps name the
//! commands by their place in the event as it is before the edit, each may say what its command
//! must be (bank, index, leading argument bytes), and if any isn't, or a place is past the end,
//! nothing of it applies (logged with the mod's reason). Inserted and replacing commands can take
//! the event's arguments (parameter entries) and a command's layer.
//!
//! Co-op's changes to the game's events are the co-op mod's (its own repository), made this way.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::evd::{Emevd, Event, Instruction, Parameter};

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub bank: u32,
    pub index: u32,
    pub args: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Disable,
    Remove(usize),
    Insert(usize, Command),
    Replace(usize, Command),
    Add { rest: u32, commands: Vec<Command> },
    Start { slot: i32, args: Vec<u8> },
    Edit { steps: Vec<Step>, why: String },
}

/// One step of a checked edit: the command at `at` (counted in the event before the edit; an
/// insert goes before it, or at the end with `at` = the count), what it must be, and the change.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub at: usize,
    pub expect: Option<Expect>,
    pub action: Action,
}

/// What a command must be: its bank, index and first argument bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Expect {
    pub bank: u32,
    pub index: u32,
    pub head: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Only the check.
    Check,
    Remove,
    Replace(New),
    Insert(New),
}

/// A command an edit puts in: its event-argument entries, and whose layer it takes (a command of
/// the event before the edit; by default a replacement keeps the replaced one's, an insert none).
/// A replacement can keep the replaced command's entries instead (`keep_params`: a command whose
/// bank and index change, its arguments as they were).
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub command: Command,
    pub params: Vec<NewParam>,
    pub layer_of: Option<usize>,
    pub keep_params: bool,
}

/// The event's argument bytes `source..source + length` copied into the command's `target..`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewParam {
    pub target: i64,
    pub source: i64,
    pub length: i64,
}

#[derive(Debug, Clone)]
struct Patch {
    owner: String,
    map: String,
    event: i64,
    op: Op,
}

static PATCHES: Mutex<Vec<Patch>> = Mutex::new(Vec::new());
/// Reads a map's event file as the game has it (unpacked): the game's side gives it.
pub type Reader = fn(&str) -> Result<Vec<u8>, String>;
static READER: OnceLock<Reader> = OnceLock::new();
/// The game's side, told of each map whose events a mod changes (as the change is made, on the mod's
/// thread): a game that must prepare the map's file before the game opens it does it then.
static ON_CHANGE: OnceLock<fn(&str)> = OnceLock::new();
/// How changes are made (this file's text, FNV-1a without carriage returns): part of each map's
/// version, so changes kept from an earlier build are made anew.
const EDITS: u64 = fnv(include_bytes!("event_patches.rs"));
/// The command that starts an event: InitializeEvent(slot, event id, arguments...).
const START_BANK: u32 = 2000;
const START_INDEX: u32 = 0;

fn patches() -> std::sync::MutexGuard<'static, Vec<Patch>> {
    PATCHES.lock().unwrap_or_else(|e| e.into_inner())
}

/// A map's event file name ("m40_00_00_00", "common", "common_func").
pub fn valid_map(map: &str) -> bool {
    !map.is_empty() && map.len() <= 32 && map.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub fn add(owner: &str, map: &str, event: i64, op: Op) -> Result<(), String> {
    if !valid_map(map) {
        return Err(format!("{map:?} isn't a map's event file name (like \"m40_00_00_00\" or \"common\")"));
    }
    let map = map.to_lowercase();
    patches().push(Patch { owner: owner.to_string(), map: map.clone(), event, op });
    if let Some(changed) = ON_CHANGE.get() {
        changed(&map);
    }
    Ok(())
}

/// Sets what hears of each map whose events a mod changes (its name: "m40_00_00_00").
pub fn on_change(changed: fn(&str)) {
    let _ = ON_CHANGE.set(changed);
}

/// Drops a mod's patches (when it unloads); how many there were.
pub fn remove_owner(owner: &str) -> usize {
    let mut p = patches();
    let before = p.len();
    p.retain(|x| x.owner != owner);
    before - p.len()
}

/// Drops a mod's patches to one map's events (`events.reset`); how many there were.
pub fn remove_owner_map(owner: &str, map: &str) -> usize {
    let map = map.to_lowercase();
    let mut p = patches();
    let before = p.len();
    p.retain(|x| !(x.owner == owner && x.map == map));
    before - p.len()
}

/// The map of an event file's game path ("event/m40_00_00_00.emevd.dcx").
pub fn map_of(rel: &str) -> Option<&str> {
    rel.strip_prefix("event/")?.strip_suffix(".emevd.dcx")
}

/// A fingerprint of a map's patches (None without any), so a changed set is served anew: what they
/// are, and how they're made (any change to this file).
pub fn version(rel: &str) -> Option<String> {
    let map = map_of(rel)?;
    let p = patches();
    let mut mine: Vec<String> = p.iter().filter(|x| x.map == map).map(|x| format!("{}:{}:{:?}", x.owner, x.event, x.op)).collect();
    if mine.is_empty() {
        return None;
    }
    mine.push(format!("{EDITS:016x}"));
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    mine.hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// The game's event file `raw` (unpacked) with the patches for its map, or None without any.
pub fn patched(rel: &str, raw: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Some(map) = map_of(rel) else { return Ok(None) };
    let mine: Vec<Patch> = patches().iter().filter(|x| x.map == map).cloned().collect();
    if mine.is_empty() {
        return Ok(None);
    }
    let mut evd = Emevd::parse(raw)?;
    edit_all(&mut evd, map, &mine, true);
    Ok(Some(evd.to_bytes()))
}

/// Sets the game's reader of a map's event file (its name: "m40_00_00_00"), unpacked.
pub fn set_reader(read: Reader) {
    let _ = READER.set(read);
}

/// FNV-1a of `bytes` without carriage returns (a checkout's line ends don't change it).
const fn fnv(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\r' {
            h = (h ^ bytes[i] as u64).wrapping_mul(0x100000001b3);
        }
        i += 1;
    }
    h
}

/// Every change to a map's events, in the order the mods made them (`mine`). Logged if `log`.
fn edit_all(evd: &mut Emevd, map: &str, mine: &[Patch], log: bool) {
    if !mine.is_empty() {
        let applied = apply(evd, mine, map, log);
        if log {
            crate::info!("events: {map}: {applied} of {} change(s) applied", mine.len());
        }
    }
}

fn command(c: &Command) -> Instruction {
    Instruction { bank: c.bank, index: c.index, args: c.args.clone(), layer: -1 }
}

/// Applies `patches` (all for `map`) in order; how many applied. Problems are logged and that
/// change skipped.
fn apply(evd: &mut Emevd, patches: &[Patch], map: &str, log: bool) -> usize {
    let mut owners: HashMap<i64, Vec<&str>> = HashMap::new();
    for p in patches {
        let list = owners.entry(p.event).or_default();
        if !list.contains(&p.owner.as_str()) {
            list.push(&p.owner);
        }
    }
    for (event, list) in owners.iter().filter(|_| log) {
        if list.len() > 1 {
            crate::warn!("events: {map} event {event} is changed by {}: applied in turn", list.join(" and "));
        }
    }
    let mut applied = 0;
    for p in patches {
        match apply_one(evd, p) {
            Ok(()) => applied += 1,
            Err(e) if log => crate::warn!("events: {} ({map} event {}): {e}; skipped", p.owner, p.event),
            Err(_) => {}
        }
    }
    applied
}

fn apply_one(evd: &mut Emevd, p: &Patch) -> Result<(), String> {
    match &p.op {
        Op::Add { rest, commands } => {
            if evd.event(p.event).is_some() {
                return Err("an event with that id is already there".into());
            }
            evd.events.push(Event { id: p.event, rest: *rest, instructions: commands.iter().map(command).collect(), parameters: Vec::new() });
            return Ok(());
        }
        Op::Start { slot, args } => {
            let start = evd.event_mut(0).ok_or("the map has no event 0 to start events from")?;
            let mut a = slot.to_le_bytes().to_vec();
            a.extend_from_slice(&(p.event as u32).to_le_bytes());
            a.extend_from_slice(args);
            start.instructions.push(Instruction { bank: START_BANK, index: START_INDEX, args: a, layer: -1 });
            return Ok(());
        }
        _ => {}
    }
    let e = evd.event_mut(p.event).ok_or("no such event in the map")?;
    if let Op::Edit { steps, why } = &p.op {
        return edit(e, steps).map_err(|e| if why.is_empty() { e } else { format!("{e} ({why})") });
    }
    let n = e.instructions.len();
    match &p.op {
        Op::Disable => {
            e.instructions.clear();
            e.parameters.clear();
        }
        Op::Remove(i) => {
            if *i >= n {
                return Err(format!("command {} of {n}", i + 1));
            }
            remove_at(e, *i);
        }
        Op::Insert(i, c) => {
            if *i > n {
                return Err(format!("position {} of {n}", i + 1));
            }
            e.instructions.insert(*i, command(c));
            for x in &mut e.parameters {
                if x.instruction >= *i as i64 {
                    x.instruction += 1;
                }
            }
        }
        Op::Replace(i, c) => {
            if *i >= n {
                return Err(format!("command {} of {n}", i + 1));
            }
            e.instructions[*i] = command(c);
            e.parameters.retain(|x| x.instruction != *i as i64);
        }
        Op::Add { .. } | Op::Start { .. } | Op::Edit { .. } => unreachable!("handled above"),
    }
    Ok(())
}

/// A checked edit of an event (see Step), all or nothing.
fn edit(e: &mut Event, steps: &[Step]) -> Result<(), String> {
    let n = e.instructions.len();
    let mut changed = vec![false; n];
    for s in steps {
        let insert = matches!(s.action, Action::Insert(_));
        if s.at > n || (s.at == n && !insert) {
            return Err(format!("command {} of {n}", s.at + 1));
        }
        if let Some(x) = &s.expect {
            let c = e.instructions.get(s.at).ok_or_else(|| format!("no command {} to check (of {n})", s.at + 1))?;
            if (c.bank, c.index) != (x.bank, x.index) || c.args.get(..x.head.len()) != Some(&x.head[..]) {
                return Err(format!("command {} is {c:?}, not {}[{:02}] {:02x?}", s.at + 1, x.bank, x.index, x.head));
            }
        }
        if let Action::Replace(new) | Action::Insert(new) = &s.action
            && let Some(l) = new.layer_of.filter(|&l| l >= n)
        {
            return Err(format!("no command {} to take the layer of (of {n})", l + 1));
        }
        if matches!(s.action, Action::Remove | Action::Replace(_)) {
            if changed[s.at] {
                return Err(format!("command {} is removed or replaced twice", s.at + 1));
            }
            changed[s.at] = true;
        }
    }
    // The new parameter entries go after the event's, in the order of the steps.
    let put = |k: usize, new: &New, own: i64, out: &mut Vec<Instruction>, added: &mut Vec<(usize, Parameter)>| {
        let layer = new.layer_of.map_or(own, |l| e.instructions[l].layer);
        let at = out.len() as i64;
        added.extend(new.params.iter().map(|p| (k, Parameter { instruction: at, target: p.target, source: p.source, length: p.length })));
        out.push(Instruction { bank: new.command.bank, index: new.command.index, args: new.command.args.clone(), layer });
    };
    // The inserts before command i, in the order given.
    let inserts = |i: usize, out: &mut Vec<Instruction>, added: &mut Vec<(usize, Parameter)>| {
        for (k, s) in steps.iter().enumerate().filter(|(_, s)| s.at == i) {
            if let Action::Insert(new) = &s.action {
                put(k, new, -1, out, added);
            }
        }
    };
    let (mut out, mut added, mut moved) = (Vec::new(), Vec::new(), vec![None; n]);
    for (i, slot) in moved.iter_mut().enumerate() {
        inserts(i, &mut out, &mut added);
        match steps.iter().enumerate().find(|(_, s)| s.at == i && matches!(s.action, Action::Remove | Action::Replace(_))) {
            Some((k, Step { action: Action::Replace(new), .. })) => {
                if new.keep_params {
                    *slot = Some(out.len() as i64);
                }
                put(k, new, e.instructions[i].layer, &mut out, &mut added)
            }
            Some(_) => {}
            None => {
                *slot = Some(out.len() as i64);
                out.push(e.instructions[i].clone());
            }
        }
    }
    inserts(n, &mut out, &mut added);
    added.sort_by_key(|(k, _)| *k);
    let mut params: Vec<Parameter> = e
        .parameters
        .iter()
        .filter_map(|p| usize::try_from(p.instruction).ok().and_then(|i| moved.get(i).copied().flatten()).map(|at| Parameter { instruction: at, ..*p }))
        .collect();
    params.extend(added.into_iter().map(|(_, p)| p));
    (e.instructions, e.parameters) = (out, params);
    Ok(())
}

/// Removes an event's command `i`; its parameters keep pointing at the same commands.
fn remove_at(e: &mut Event, i: usize) {
    e.instructions.remove(i);
    e.parameters.retain(|x| x.instruction != i as i64);
    for x in &mut e.parameters {
        if x.instruction > i as i64 {
            x.instruction -= 1;
        }
    }
}

/// A map's event as the game has it (for `events.script`), read from the game's archives.
pub fn script(map: &str, event: i64) -> Result<Option<Event>, String> {
    Ok(map_events(map)?.event(event).cloned())
}

/// Every command `bank[index]` in a map's event file as the game has it (for `events.find`):
/// (event id, the command's place in the event, the command).
pub fn find(map: &str, bank: u32, index: u32) -> Result<Vec<(i64, usize, Instruction)>, String> {
    let evd = map_events(map)?;
    Ok(evd
        .events
        .iter()
        .flat_map(|e| e.instructions.iter().enumerate().filter(|(_, c)| (c.bank, c.index) == (bank, index)).map(|(i, c)| (e.id, i, c.clone())))
        .collect())
}

/// A map's event as the game will run it (for `events.view`): the game's, with every change the
/// mods and Kindle make to it now.
pub fn view(map: &str, event: i64) -> Result<Option<Event>, String> {
    let mut evd = map_events(map)?;
    let map = map.to_lowercase();
    let mine: Vec<Patch> = patches().iter().filter(|x| x.map == map).cloned().collect();
    edit_all(&mut evd, &map, &mine, false);
    Ok(evd.event(event).cloned())
}

/// A map's event file as the game has it, from the game's archives (kept: a mod reads many events
/// of many maps as it loads, in any order; all the game's event files read are under 2 MB).
fn map_events(map: &str) -> Result<Emevd, String> {
    static READ: Mutex<Vec<(String, Emevd)>> = Mutex::new(Vec::new());
    if !valid_map(map) {
        return Err(format!("{map:?} isn't a map's event file name"));
    }
    let map = map.to_lowercase();
    if let Some((_, e)) = READ.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(m, _)| *m == map) {
        return Ok(e.clone());
    }
    let read = READER.get().ok_or("this game's event files can't be read")?;
    let evd = Emevd::parse(&read(&map)?)?;
    let mut read = READ.lock().unwrap_or_else(|e| e.into_inner());
    if read.len() >= 32 {
        read.remove(0);
    }
    read.push((map, evd.clone()));
    Ok(evd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evd::Parameter;

    fn evd() -> Emevd {
        let mut e = Emevd::parse(&sample_bytes()).unwrap();
        e.events[1].parameters =
            vec![Parameter { instruction: 1, target: 0, source: 0, length: 4 }, Parameter { instruction: 2, target: 4, source: 4, length: 4 }];
        e
    }

    fn sample_bytes() -> Vec<u8> {
        // Two events: 0 (one command), 7 (three commands), built through the writer.
        let mut e = Emevd::parse(&crate::evd::tests_support::empty()).unwrap();
        e.events.push(Event { id: 0, rest: 0, instructions: vec![command(&c(1))], parameters: vec![] });
        e.events.push(Event { id: 7, rest: 0, instructions: vec![command(&c(10)), command(&c(11)), command(&c(12))], parameters: vec![] });
        e.to_bytes()
    }

    fn c(i: u32) -> Command {
        Command { bank: 2003, index: i, args: vec![i as u8, 0, 0, 0] }
    }

    fn patch(event: i64, op: Op) -> Patch {
        Patch { owner: "m".into(), map: "m40_00_00_00".into(), event, op }
    }

    #[test]
    fn commands_move_and_parameters_follow() {
        let mut e = evd();
        assert_eq!(apply(&mut e, &[patch(7, Op::Remove(0))], "m40", true), 1);
        let ev = e.event(7).unwrap();
        assert_eq!(ev.instructions.iter().map(|i| i.index).collect::<Vec<_>>(), vec![11, 12]);
        assert_eq!(ev.parameters.iter().map(|p| p.instruction).collect::<Vec<_>>(), vec![0, 1]);
        apply(&mut e, &[patch(7, Op::Insert(0, c(9)))], "m40", true);
        let ev = e.event(7).unwrap();
        assert_eq!(ev.instructions.iter().map(|i| i.index).collect::<Vec<_>>(), vec![9, 11, 12]);
        assert_eq!(ev.parameters.iter().map(|p| p.instruction).collect::<Vec<_>>(), vec![1, 2]);
        apply(&mut e, &[patch(7, Op::Replace(1, c(5)))], "m40", true);
        assert_eq!(e.event(7).unwrap().parameters.len(), 1);
        // Out of range: skipped.
        assert_eq!(apply(&mut e, &[patch(7, Op::Remove(9))], "m40", true), 0);
        apply(&mut e, &[patch(7, Op::Disable)], "m40", true);
        assert!(e.event(7).unwrap().instructions.is_empty());
    }

    #[test]
    fn new_events_are_added_and_started() {
        let mut e = evd();
        let n = apply(&mut e, &[patch(14009999, Op::Add { rest: 0, commands: vec![c(3)] }), patch(14009999, Op::Start { slot: 0, args: vec![] })], "m40", true);
        assert_eq!(n, 2);
        assert_eq!(e.event(14009999).unwrap().instructions.len(), 1);
        let start = e.event(0).unwrap().instructions.last().unwrap();
        assert_eq!((start.bank, start.index), (START_BANK, START_INDEX));
        assert_eq!(&start.args[4..8], &14009999u32.to_le_bytes());
        // Adding it again fails; the file still writes and reads back.
        assert_eq!(apply(&mut e, &[patch(14009999, Op::Add { rest: 0, commands: vec![] })], "m40", true), 0);
        assert_eq!(Emevd::parse(&e.to_bytes()).unwrap(), e);
    }

    fn new(i: u32, params: Vec<NewParam>, layer_of: Option<usize>) -> New {
        New { command: c(i), params, layer_of, keep_params: false }
    }

    #[test]
    fn checked_edits_count_from_the_event_before_them() {
        let mut e = evd();
        e.events[1].instructions[2].layer = 0x40;
        let is = |i: u32| Some(Expect { bank: 2003, index: i, head: vec![i as u8] });
        let steps = vec![
            Step { at: 0, expect: is(10), action: Action::Remove },
            Step { at: 1, expect: None, action: Action::Insert(new(20, vec![NewParam { target: 0, source: 8, length: 4 }], Some(2))) },
            Step { at: 1, expect: None, action: Action::Insert(new(21, vec![], None)) },
            Step { at: 2, expect: is(12), action: Action::Replace(new(22, vec![], None)) },
            Step { at: 3, expect: None, action: Action::Insert(new(23, vec![], None)) },
        ];
        let op = Op::Edit { steps, why: "test".into() };
        assert_eq!(apply(&mut e, &[patch(7, op)], "m40", true), 1);
        let ev = e.event(7).unwrap();
        let shape: Vec<(u32, i64)> = ev.instructions.iter().map(|i| (i.index, i.layer)).collect();
        assert_eq!(shape, vec![(20, 0x40), (21, -1), (11, -1), (22, 0x40), (23, -1)]);
        // Command 11's parameter follows it, 12's goes with it, the insert's is added.
        let params: Vec<(i64, i64)> = ev.parameters.iter().map(|p| (p.instruction, p.source)).collect();
        assert_eq!(params, vec![(2, 0), (0, 8)]);
    }

    #[test]
    fn a_replacement_can_keep_its_parameters() {
        let mut e = evd();
        let keep = New { command: c(30), params: vec![], layer_of: None, keep_params: true };
        // Added entries follow the steps' order, not the commands'.
        let steps = vec![
            Step { at: 3, expect: None, action: Action::Insert(new(31, vec![NewParam { target: 0, source: 12, length: 4 }], None)) },
            Step { at: 1, expect: None, action: Action::Replace(keep) },
            Step { at: 0, expect: None, action: Action::Insert(new(32, vec![NewParam { target: 0, source: 16, length: 4 }], None)) },
        ];
        assert_eq!(apply(&mut e, &[patch(7, Op::Edit { steps, why: String::new() })], "m40", true), 1);
        let ev = e.event(7).unwrap();
        assert_eq!(ev.instructions.iter().map(|i| i.index).collect::<Vec<_>>(), vec![32, 10, 30, 12, 31]);
        let params: Vec<(i64, i64)> = ev.parameters.iter().map(|p| (p.instruction, p.source)).collect();
        assert_eq!(params, vec![(2, 0), (3, 4), (4, 12), (0, 16)]);
    }

    #[test]
    fn a_checked_edit_is_all_or_nothing() {
        let mut e = evd();
        let before = e.clone();
        let wrong = Some(Expect { bank: 2003, index: 11, head: vec![99] });
        let steps = vec![Step { at: 0, expect: None, action: Action::Remove }, Step { at: 1, expect: wrong, action: Action::Check }];
        assert_eq!(apply(&mut e, &[patch(7, Op::Edit { steps, why: String::new() })], "m40", true), 0);
        // Past the end, or the same command twice.
        let steps = vec![Step { at: 3, expect: None, action: Action::Remove }];
        assert_eq!(apply(&mut e, &[patch(7, Op::Edit { steps, why: String::new() })], "m40", true), 0);
        let steps = vec![Step { at: 1, expect: None, action: Action::Remove }, Step { at: 1, expect: None, action: Action::Replace(new(5, vec![], None)) }];
        assert_eq!(apply(&mut e, &[patch(7, Op::Edit { steps, why: String::new() })], "m40", true), 0);
        assert_eq!(e, before);
    }

    #[test]
    fn event_file_paths() {
        assert_eq!(map_of("event/m40_00_00_00.emevd.dcx"), Some("m40_00_00_00"));
        assert_eq!(map_of("map/mapstudio/m40_00_00_00.msb.dcx"), None);
        assert!(valid_map("common_func") && !valid_map("../x"));
    }
}
