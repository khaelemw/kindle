//! Elden Ring's event script (EMEVD) commands, run on demand (kindle.events), as Kindle runs Dark Souls
//! III's.
//!
//! The game's map event scripts are lists of commands: a bank (2003 events, 2004 characters, 2006
//! effects, 2007 messages, 2010 sound ...), an index and packed arguments. The event system runs each one
//! through a dispatcher (`FUN_140568b90(handlers, dt, CSEmkEventIns*) -> bool`, the only caller of the
//! bank handlers, called from the event's update with `[CSEmkSystem + 0x28]`) that switches on the bank
//! and calls its handler; the handlers read the command header from `ins+0xD0` (`{u32 bank, u32 index,
//! u64 args length, u64 args offset, i64 layer}`, as Dark Souls III's at +0xB0) and its arguments from
//! `ins+0xD8` when that's set (otherwise from the event's file, `ins+0xC8`). An entity id of 10000 (the
//! player) is swapped for `[ins+0x78]` (`FUN_1405c60f0`), the player the event runs for: Kindle's
//! instance holds 10000 there, so the player stays this game's player. A command its handler finished is
//! passed on to `FUN_140584fd0`, which first asks the instance's vtable (+0x48) what it is: Kindle's
//! instance has a vtable of its own whose every entry returns 0, which that function takes as nothing to
//! do.
//!
//! The game only calls the dispatcher as an event moves on to its next command, so Kindle calls it
//! itself, from its frame on the game's thread, with an instance of its own, zeroed but for those: the
//! game's own handler runs the command, exactly as a script would. No hook.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use kindle_core::{memory, patterns};

use crate::{er, task, text};

/// The dispatcher's start (unique).
const DISPATCH: &str = "48 89 5C 24 08 57 48 83 EC 20 49 8B 80 D0 00 00 00 49 8B F8 8B 10 81 FA E8 03 00 00 0F 87";
/// The event system, and where in it the dispatcher's first argument (the bank handlers) is.
const SYSTEM: &str = "CS::CSEmkSystem";
const HANDLERS: usize = 0x28;

/// Kindle's event instance: the player the event runs for, the command's header and its arguments.
const INS_PLAYER: usize = 0x78;
const INS_HEADER: usize = 0xD0;
const INS_ARGS: usize = 0xD8;
/// Size of Kindle's event instance (the game's is smaller; the rest is zeroed room).
const INS_SIZE: usize = 0x400;
/// The entity id event scripts give the player.
pub const PLAYER: u32 = 10000;

/// "Warp Player" (area, block, region, index, the entity to start at, a place name to show).
pub const WARP_PLAYER: (i32, i32) = (2003, 14);
pub const WARP_PLAYER_ARGS: &str = "BBBBIi";

/// A grace's BonfireWarpParam row names its asset (bonfireEntityId); the player's start beside it is
/// that id less this (419 of the game's 422 graces, from their maps).
const GRACE_START: u32 = 970;

pub struct Command {
    pub id: u64,
    /// The mod that ran it.
    pub owner: String,
    pub bank: i32,
    pub index: i32,
    pub args: Vec<u8>,
    /// Run on the game's thread just before the command (what the command reads set up then).
    pub prepare: Option<Prepare>,
}

pub type Prepare = Box<dyn FnOnce() -> Result<(), String> + Send>;

static QUEUE: Mutex<Vec<Command>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// An id for a command's reply.
pub fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

fn dispatcher() -> Option<usize> {
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    #[cfg(test)]
    if let Some(f) = tests::FAKE.with(|f| f.get()) {
        return Some(f);
    }
    *AT.get_or_init(|| patterns::find_unique("event command dispatcher", DISPATCH))
}

/// The dispatcher's first argument, once the game's event system is running.
fn handlers() -> Result<usize, String> {
    #[cfg(test)]
    if tests::FAKE.with(|f| f.get()).is_some() {
        return Ok(0x1000);
    }
    let system = er::instance(SYSTEM).ok_or("the game's event system isn't running yet")?;
    memory::read_u64(system + HANDLERS).map(|p| p as usize).filter(|&p| p != 0).ok_or_else(|| "the game's event system isn't running yet".into())
}

/// Queues a command; it runs on Kindle's next frame on the game's thread.
pub fn run(cmd: Command) -> Result<(), String> {
    dispatcher().ok_or("the event command dispatcher wasn't found on this game build")?;
    QUEUE.lock().unwrap_or_else(|e| e.into_inner()).push(cmd);
    Ok(())
}

/// Runs the queued commands, on the game's thread (they wait there till Kindle's frame is the game's):
/// each one's reply id with whether the game's handler finished it (None: it couldn't run).
pub fn run_queued() -> Vec<(u64, Option<bool>)> {
    if task::on_games_thread("event commands run").is_err() {
        return Vec::new();
    }
    let queued = std::mem::take(&mut *QUEUE.lock().unwrap_or_else(|e| e.into_inner()));
    let mut out = Vec::with_capacity(queued.len());
    for mut cmd in queued {
        let prepare = cmd.prepare.take();
        let done = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Some(p) = prepare {
                p()?;
            }
            unsafe { execute(&cmd) }
        }))
        .unwrap_or(Err("the command crashed in Kindle".into()));
        match done {
            Ok(d) => out.push((cmd.id, Some(d))),
            Err(e) => {
                kindle_core::warn!("events.run {}[{}]: {e}", cmd.bank, cmd.index);
                out.push((cmd.id, None));
            }
        }
    }
    out
}

type Dispatch = unsafe extern "C" fn(usize, f32, usize) -> u8;

#[repr(C)]
struct Header {
    bank: i32,
    index: i32,
    args_len: u64,
    args_offset: u64,
    layer: i64,
}

extern "C" fn nothing() -> usize {
    0
}

/// The vtable of Kindle's event instance: every entry returns 0.
static VTABLE: [extern "C" fn() -> usize; 32] = [nothing; 32];

/// Runs one command through the game's dispatcher on an event instance of Kindle's own.
unsafe fn execute(cmd: &Command) -> Result<bool, String> {
    let dispatch = dispatcher().ok_or("the event command dispatcher wasn't found")?;
    let handlers = handlers()?;
    let header = Header { bank: cmd.bank, index: cmd.index, args_len: cmd.args.len() as u64, args_offset: 0, layer: -1 };
    // Handlers read past the declared arguments in a few places: give them zeroed room.
    let mut args = cmd.args.clone();
    args.resize(args.len().max(64), 0);
    let mut ins = vec![0u64; INS_SIZE / 8];
    let base = ins.as_mut_ptr() as usize;
    unsafe {
        std::ptr::write(base as *mut u64, VTABLE.as_ptr() as u64);
        std::ptr::write((base + INS_PLAYER) as *mut u32, PLAYER);
        std::ptr::write((base + INS_HEADER) as *mut u64, &header as *const Header as u64);
        std::ptr::write((base + INS_ARGS) as *mut u64, args.as_ptr() as u64);
        let f: Dispatch = std::mem::transmute(dispatch);
        Ok(f(handlers, 1.0 / 60.0, base) != 0)
    }
}

/// A site of grace, from its BonfireWarpParam row.
pub struct Grace {
    pub id: u64,
    pub name: Option<String>,
    /// The grace's asset.
    pub entity: u32,
    /// Its map: area, block, region (m60_42_36_00 is 60, 42, 36).
    pub map: (u8, u8, u8),
}

fn grace_row(table: &kindle_core::params::Table, id: u64, place_names: &[u32]) -> Result<Grace, String> {
    let int = |field: &str| -> Result<i64, String> {
        match table.read(id, field)? {
            kindle_paramdef::Value::Int(v) => Ok(v),
            v => Err(format!("BonfireWarpParam {id} {field} is {v:?}")),
        }
    };
    let text_id = int("textId1")?;
    let name = if text_id > 0 { text::get(place_names, text_id as u32).ok().flatten() } else { None };
    Ok(Grace { id, name, entity: int("bonfireEntityId")? as u32, map: (int("areaNo")? as u8, int("gridXNo")? as u8, int("gridZNo")? as u8) })
}

/// Every site of grace the game has (BonfireWarpParam's rows with an asset), with its name.
pub fn graces() -> Result<Vec<Grace>, String> {
    let registry = kindle_core::params::registry()?;
    let table = registry.table("BonfireWarpParam")?;
    let place_names = text::category("place_name")?;
    let mut out = Vec::new();
    for &id in &table.row_ids {
        let g = grace_row(table, id, &place_names)?;
        if g.entity != 0 {
            out.push(g);
        }
    }
    Ok(out)
}

/// The grace a mod names: its BonfireWarpParam row, or its name (any case).
pub fn grace(which: GraceRef) -> Result<Grace, String> {
    match which {
        GraceRef::Row(id) => {
            let registry = kindle_core::params::registry()?;
            let table = registry.table("BonfireWarpParam")?;
            let g = grace_row(table, id, &text::category("place_name")?)?;
            if g.entity == 0 {
                return Err(format!("BonfireWarpParam {id} isn't a site of grace"));
            }
            Ok(g)
        }
        GraceRef::Name(name) => {
            let mut found: Vec<Grace> = graces()?.into_iter().filter(|g| g.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(&name))).collect();
            match found.len() {
                0 => Err(format!("no site of grace is named {name:?} (see player.graces())")),
                1 => Ok(found.remove(0)),
                n => Err(format!(
                    "{n} sites of grace are named {name:?}: give its BonfireWarpParam row ({})",
                    found.iter().map(|g| g.id.to_string()).collect::<Vec<_>>().join(", ")
                )),
            }
        }
    }
}

pub enum GraceRef {
    Row(u64),
    Name(String),
}

/// Warp Player's arguments to a grace: its map, and the player's start beside it.
pub fn warp_to(g: &Grace) -> Result<Vec<u8>, String> {
    let (area, block, region) = g.map;
    kindle_core::emevd::pack(WARP_PLAYER_ARGS, &[area as f64, block as f64, region as f64, 0.0, (g.entity - GRACE_START) as f64, 0.0])
}

/// The natives for kindle.events (and the player's warp, which is one).
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::{ctx, rt_err};
    let queue = |lua: &mlua::Lua, bank: i32, index: i32, args: Vec<u8>, reply: Option<mlua::Function>| -> mlua::Result<u64> {
        let id = next_id();
        let owner = ctx(lua)?.id.clone();
        run(Command { id, owner, bank, index, args, prepare: None }).map_err(rt_err)?;
        if let Some(f) = reply {
            ctx(lua)?.handlers.replies.push((id, f));
        }
        Ok(id)
    };
    n.set(
        "events_run",
        lua.create_function(move |lua, (bank, index, format, values, reply): (i32, i32, String, Vec<f64>, Option<mlua::Function>)| {
            let args = kindle_core::emevd::pack(&format, &values).map_err(rt_err)?;
            queue(lua, bank, index, args, reply)
        })?,
    )?;
    let grace_ref = |v: mlua::Value| -> mlua::Result<GraceRef> {
        match v {
            mlua::Value::Integer(i) => Ok(GraceRef::Row(i as u64)),
            mlua::Value::Number(f) => Ok(GraceRef::Row(f as u64)),
            mlua::Value::String(s) => Ok(GraceRef::Name(s.to_str()?.to_string())),
            _ => Err(rt_err("a site of grace is its BonfireWarpParam row or its name")),
        }
    };
    n.set(
        "player_warp",
        lua.create_function(move |lua, (which, reply): (mlua::Value, Option<mlua::Function>)| {
            let g = grace(grace_ref(which)?).map_err(rt_err)?;
            queue(lua, WARP_PLAYER.0, WARP_PLAYER.1, warp_to(&g).map_err(rt_err)?, reply)
        })?,
    )?;
    n.set(
        "player_graces",
        lua.create_function(|lua, ()| {
            let list = lua.create_table()?;
            for g in graces().map_err(rt_err)? {
                let t = lua.create_table()?;
                t.set("id", g.id)?;
                t.set("name", g.name)?;
                t.set("entity", g.entity)?;
                t.set("map", format!("m{:02}_{:02}_{:02}_00", g.map.0, g.map.1, g.map.2))?;
                list.push(t)?;
            }
            Ok(list)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use super::*;

    /// A command as the fake dispatcher saw it: bank, index, the arguments, the player, what the vtable said.
    type Seen = (i32, i32, Vec<u8>, u32, usize);

    thread_local! {
        /// A dispatcher standing in for the game's, for this thread's tests.
        pub(crate) static FAKE: Cell<Option<usize>> = const { Cell::new(None) };
        /// What the fake dispatcher saw.
        pub(crate) static SEEN: std::cell::RefCell<Vec<Seen>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    /// The game's dispatcher as Kindle's instance meets it: the header and arguments where its handlers
    /// read them, the player where the entity swap reads it, the vtable as the finished command's
    /// function calls it. Finishes every command but bank 9999's.
    unsafe extern "C" fn fake(handlers: usize, _dt: f32, ins: usize) -> u8 {
        assert_eq!(handlers, 0x1000);
        unsafe {
            let header = *((ins + INS_HEADER) as *const *const Header);
            let (bank, index, len) = ((*header).bank, (*header).index, (*header).args_len as usize);
            let args = std::slice::from_raw_parts(*((ins + INS_ARGS) as *const *const u8), len).to_vec();
            let player = *((ins + INS_PLAYER) as *const u32);
            let vtable = *(ins as *const *const extern "C" fn() -> usize);
            let what = (*vtable.add(0x48 / 8))();
            SEEN.with(|s| s.borrow_mut().push((bank, index, args, player, what)));
            (bank != 9999) as u8
        }
    }

    pub(crate) fn serve() {
        FAKE.with(|f| f.set(Some(fake as *const () as usize)));
        SEEN.with(|s| s.borrow_mut().clear());
        QUEUE.lock().unwrap().clear();
    }

    #[test]
    fn commands_run_through_the_games_dispatcher_on_kindles_instance() {
        let _one_at_a_time = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        serve();
        let warp = kindle_core::emevd::pack(WARP_PLAYER_ARGS, &[11.0, 10.0, 0.0, 0.0, 11100980.0, 0.0]).unwrap();
        assert_eq!(warp.len(), 12);
        run(Command { id: 7, owner: "m".into(), bank: 2003, index: 14, args: warp.clone(), prepare: None }).unwrap();
        run(Command { id: 8, owner: "m".into(), bank: 9999, index: 0, args: vec![], prepare: None }).unwrap();
        // A command whose preparation fails doesn't run.
        run(Command { id: 9, owner: "m".into(), bank: 2007, index: 2, args: vec![1, 0, 0, 0], prepare: Some(Box::new(|| Err("no".into()))) }).unwrap();
        assert_eq!(run_queued(), [(7, Some(true)), (8, Some(false)), (9, None)]);
        assert!(run_queued().is_empty(), "a command ran twice");
        let seen = SEEN.with(|s| s.borrow().clone());
        assert_eq!(seen[0], (2003, 14, warp, PLAYER, 0));
        assert_eq!(seen[1].0, 9999);
        assert_eq!(seen.len(), 2);
    }

    #[test]
    fn a_grace_is_warped_to_at_the_players_start_beside_it() {
        let elleh = Grace { id: 61423600, name: Some("Church of Elleh".into()), entity: 1042361950, map: (60, 42, 36) };
        let args = warp_to(&elleh).unwrap();
        assert_eq!(&args[..4], &[60, 42, 36, 0]);
        assert_eq!(u32::from_le_bytes(args[4..8].try_into().unwrap()), 1042360980);
        assert_eq!(i32::from_le_bytes(args[8..12].try_into().unwrap()), 0);
    }

    /// The dispatcher found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE), its only call reading the
    /// bank handlers from CSEmkSystem's static (+0x28), and the finished command's function asking the
    /// instance's vtable at +0x48 first.
    #[test]
    fn the_games_dispatcher_is_found() {
        let Some((img, _, sections)) = crate::singletons::tests::eldenring() else {
            eprintln!("KINDLE_ER_EXE isn't set: Elden Ring's exe not checked");
            return;
        };
        let (rva, size) = sections.iter().find(|(n, _, _)| n == ".text").map(|&(_, r, s)| (r, s)).unwrap();
        let text = &img[rva..rva + size];
        let find = |pattern: &str| -> Vec<usize> {
            let p: Vec<Option<u8>> = pattern.split_whitespace().map(|t| u8::from_str_radix(t, 16).ok()).collect();
            (0..text.len() - p.len()).filter(|&i| p.iter().zip(&text[i..]).all(|(p, b)| p.is_none_or(|v| v == *b))).map(|i| rva + i).collect()
        };
        assert_eq!(find(DISPATCH), [0x568B90]);
        let calls: Vec<usize> = (rva..rva + size - 5)
            .filter(|&a| img[a] == 0xE8 && (a + 5).wrapping_add_signed(i32::from_le_bytes(img[a + 1..a + 5].try_into().unwrap()) as isize) == 0x568B90)
            .collect();
        assert_eq!(calls, [0x583E52]);
        // mov rax, [rip+CSEmkSystem]; mov rdi, [rax+0x28] ... mov rcx, rdi; call
        let site = 0x583E20;
        assert_eq!(&img[site..site + 3], &[0x48, 0x8B, 0x05]);
        let disp = i32::from_le_bytes(img[site + 3..site + 7].try_into().unwrap());
        assert_eq!((site + 7).wrapping_add_signed(disp as isize), 0x3D6BC40);
        assert_eq!(&img[site + 7..site + 11], &[0x48, 0x8B, 0x78, HANDLERS as u8]);
        assert_eq!(&img[0x583E4F..0x583E52], &[0x48, 0x8B, 0xCF]);
        // FUN_140584fd0: ... mov rax, [rcx]; ... call [rax+0x48]
        let post = &img[0x584FD0..0x585040];
        assert!(post.windows(3).any(|w| w == [0xFF, 0x50, 0x48]), "the finished command's function no longer asks the vtable at +0x48");
    }
}
