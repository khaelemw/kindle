//! Deaths, falls and bloodstains: the catalogue's entries for a mod that changes how the player
//! dies (a co-op mod's "nobody dies alone"), each at the place the game decides it.
//!
//! | | |
//! |---|---|
//! | fall death | the death floor's check of a hit by type (+0x88D690, for the physics' contacts): the event `player.fall_death_blocked` |
//! | departure | the same check for another player's character: the hook point `chr.departure_hit` |
//! | bloodstain at a load | ParamInitialize's type test (+0x484D7B): the hook point `load.bloodstain` |
//! | bloodstain record | the death record's getters (+0x4D2150): `bloodstain.mark`, `bloodstain.leave` |
//! | floor | the floor setter (+0x88F870): `chr.stand_on` |
//! | air time | the physics module's fall timers: the field `chr.fall_time` |

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::catalogue::{self, HookPoint, Kind, Value};
use crate::hook_sites::{self, Patch, Site};
use crate::{memory, patterns};

// ── the fall death ──────────────────────────────────────────────────────────────────────────

/// The game's fall death: a hit the character touches is checked by its type (+0x88D690, for the
/// physics' contacts); a death floor's (0xd) counts for the main player unless it can't die
/// (+0x9E4FE0: the data module's no-death bit, or the debug "no dead"). Counted, its hit gives the
/// fall death (the camera held, FallDeathLoop, HP 0). With the no-death flag it doesn't count: the
/// player fell on through the void, out of reach of anything (the host about 440 m, 2026-10-12, off
/// the cliff past Vordt's arena, before a floor far below killed it outright; in single player the
/// same fall ended 1.5 s in, verified with two games on one PC). The match is that check in the 0xd
/// case, the hook on its "test al, al": a "can't die" there for the main player is a fall death
/// its no-death flag kept off, told to mods (player.fall_death_blocked) at most once a frame.
const FATAL_FALL: &str = "80 b8 90 03 00 00 00 75 ?? 48 8b 49 18 e8 ?? ?? ?? ?? 84 c0 75 ?? 48 8b cf e8 ?? ?? ?? ?? 84 c0 74 ?? b3 01";
/// Kindle's frame when the event last went out (on the physics' thread).
static FALL_TOLD_AT: AtomicU64 = AtomicU64::new(u64::MAX);

fn fatal_fall_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &*reg };
    // al: can't die; rdi: the character.
    if r.rax as u8 == 0 || r.rdi == 0 || crate::ds3::main_player().map(|p| p.addr()) != Some(r.rdi as usize) {
        return;
    }
    let frame = crate::mods::FRAME.load(Ordering::Relaxed);
    if FALL_TOLD_AT.swap(frame, Ordering::Relaxed) != frame {
        catalogue::emit(&FALL_DEATH_BLOCKED, Vec::new());
    }
}

static FATAL_FALL_SITE: Site = Site {
    area: "death",
    name: "fatal fall",
    pattern: FATAL_FALL,
    offset: 0x12,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(fatal_fall_hook),
    without: "mods aren't told of a fall death the player's no-death flag kept off",
};

/// kindle.game: a fall death the no-death flag kept off (catalogue.rs).
pub static FALL_DEATH_BLOCKED: catalogue::Event = catalogue::Event::hooked(
    "player.fall_death_blocked",
    "The player hit a death floor (a fall out of the world) that would have killed it but for its no-death flag (kindle.player.set_no_death): without it the game's fall death would run. Comes on Kindle's frame, at most once a frame while it lasts.",
    &[],
    || FATAL_FALL_SITE.install().map(|_| ()).ok_or_else(|| FATAL_FALL_SITE.without.to_string()),
);

// ── another player's departure ──────────────────────────────────────────────────────────────

/// The same check's other case: a death floor's hit (types 0xe and 0xf) on a character that isn't
/// this game's player counts unless it can't die; counted, it starts a phantom's departure
/// (+0x888910: a flag at ChrIns +0x1A0C, the ghost alpha, the departure timer), the fade it leaves
/// the world by. A co-op player who falls goes down and comes back, so on the others' screens it
/// stayed faded: revived, there and hit, but not drawn (seen in co-op play, 2026-10-14). The hook is
/// on the check itself (+0x88D690: the character, the hit; whether the hit counts), around the
/// game's answer. (It was on the check's last "test al, al", whose 14 bytes covered the two places
/// most of the check's paths jump to: those hits crashed, seen on a test game 2026-10-14.)
const PHANTOM_DEPARTURE: &str = "48 89 5c 24 08 57 48 83 ec 20 32 db 48 8b f9 48 85 d2 0f 84 ?? ?? ?? ?? 48 8d 4a 10 e8 ?? ?? ?? ?? 83 c0 f3 83 f8 0a";
/// The check's call of the hit's type (+0x857960, the hit's +0x10): its "call rel32".
const HIT_TYPE_CALL: usize = 0x1C;
static HIT_TYPE: AtomicUsize = AtomicUsize::new(0);
const DEPARTURE_HITS: [u8; 2] = [0xE, 0xF];

fn phantom_departure_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (chr, hit) = (r.rcx as usize, r.rdx as usize);
    let check: extern "C" fn(usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    let counts = check(chr, hit) & 0xFF;
    if counts == 0 || hit == 0 || DEPARTURE_HIT.switch() != 1 {
        return counts;
    }
    let Some(main) = crate::ds3::main_player().map(|p| p.addr()) else { return counts };
    // Another player: not this game's own, of the player's class (its vtable).
    let other = chr != main && memory::read_u64(chr).is_some() && memory::read_u64(chr) == memory::read_u64(main);
    let type_fn = HIT_TYPE.load(Ordering::Relaxed);
    if !other || type_fn == 0 {
        return counts;
    }
    let hit_type: extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(type_fn) };
    if DEPARTURE_HITS.contains(&hit_type(hit + 0x10)) { 0 } else { counts }
}

static PHANTOM_DEPARTURE_SITE: Site = Site {
    area: "death",
    name: "phantom departure",
    pattern: PHANTOM_DEPARTURE,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::Retn(phantom_departure_hook),
    without: "mods can't keep another player who falls from fading out",
};

fn install_departure() -> Result<(), String> {
    let found = PHANTOM_DEPARTURE_SITE.find().ok_or(PHANTOM_DEPARTURE_SITE.without)?;
    let call = found[0] + HIT_TYPE_CALL;
    let rel = memory::read::<i32>(call + 1).ok_or("the hit's type call is unreadable")?;
    HIT_TYPE.store((call + 5).wrapping_add_signed(rel as isize), Ordering::Relaxed);
    PHANTOM_DEPARTURE_SITE.hook_all(&found).then_some(()).ok_or_else(|| PHANTOM_DEPARTURE_SITE.without.to_string())
}

/// kindle.game: a death floor's hit on another player's character (catalogue.rs). Answered on the
/// physics' thread: a switch.
pub static DEPARTURE_HIT: HookPoint = HookPoint::hooked(
    "chr.departure_hit",
    "A death floor's hit on another player's character (in a session: one this game doesn't play) would start its departure, the fade a phantom leaves the world by, on this game: \"count\" it, or \"ignore\" it (that player's own game has its fall). On the physics' thread: a switch only.",
    &["count", "ignore"],
    install_departure,
);

// ── the bloodstain at a load ────────────────────────────────────────────────────────────────

/// A load's "ParamInitialize" (the event script's, +0x484CE0) shows the main player's bloodstain
/// from its record when its type (PlayerGameData +0x84) is 0, a host, and takes it away for any
/// other: a white phantom has no bloodstain in someone else's world. Answered "any", its type
/// test's jump to the host's branch (+0x484D7B, "je") is taken always; the type 8 branch before it
/// stays the game's.
const PARAM_INITIALIZE: &str = "48 83 ec 28 48 83 3d ?? ?? ?? ?? 00 0f 84 ?? ?? ?? ?? 48 8b 05 ?? ?? ?? ?? 48 89 5c 24 20 48 8b 58 10";
static KEEP_BLOODSTAIN_PATCH: Patch = Patch {
    area: "death",
    name: "the bloodstain kept at a load",
    pattern: PARAM_INITIALIZE,
    offset: 0x9B,
    matches: 1,
    from: &[0x74],
    to: &[0xEB],
    without: "a player's bloodstain is taken away at its load as a phantom's",
};

/// kindle.game: which players' bloodstains a load shows (catalogue.rs).
pub static LOAD_BLOODSTAIN: HookPoint = HookPoint::hooked(
    "load.bloodstain",
    "A load shows the player's bloodstain (its souls where it last died) only when it's a host in its own world: \"host_only\", the game's; \"any\" keeps it for a phantom too (a co-op player in another's world, whose death the mod made its own). Answered \"any\", the game's code is changed until it closes.",
    &["host_only", "any"],
    || KEEP_BLOODSTAIN_PATCH.apply().then_some(()).ok_or_else(|| KEEP_BLOODSTAIN_PATCH.without.to_string()),
);

// ── the bloodstain record ───────────────────────────────────────────────────────────────────

/// The game's death record (+0x4D2150: the character, whether to record): it fills the
/// character's bloodstain record (GameDataMan +0x40, 0x44 bytes, saved with the character) and
/// its "has a bloodstain" byte (GameDataMan +0x38) from the character's own getters: +0x00 its
/// last safe point (ChrIns +0x1FE8's), +0x0C that point's facing, +0x3C its map, +0x18 where it
/// died, +0x24 its facing there, +0x30 -1, +0x34 humanity, +0x38 the souls it held (then 0),
/// +0x40 a protected death (SpEffect state 0x75 or 0x88). For a guest it records its home's
/// return point instead and sets the flow's home type, so Kindle calls only its getters, found
/// through their calls in it.
const DEATH_RECORD: &str = "48 85 d2 0f 84 ?? ?? ?? ?? 55 56 57 41 54 41 55 41 56 41 57 48 83 ec 70";
/// Its calls: the record (+0x597120); the last safe point and its facing, where the character
/// is and its facing (+0x8BB5F0, +0x8BB560, +0x88AAB0, +0x8893A0) with the record field each
/// fills; the safe point's map (+0x8BB5C0); the "has a bloodstain" setter (+0x59CF00).
const RECORD_CALL_AT: usize = 0x4C;
const PLACE_CALLS_AT: [(usize, usize); 4] = [(0xC4, 0x00), (0xE1, 0x0C), (0x148, 0x18), (0x166, 0x24)];
const SAFE_MAP_CALL_AT: usize = 0xFF;
/// The protected death's test (+0x88D2D0: SpEffect state 0x75 or 0x88; non-zero: protected).
const PROTECTED_CALL_AT: usize = 0x17F;
const HAS_BLOODSTAIN_CALL_AT: usize = 0x2A0;
/// The record's fields after its places.
const RECORD_PLACES: usize = 0x30;
const RECORD_UNUSED_AT: usize = 0x30;
const RECORD_HUMANITY_AT: usize = 0x34;
const RECORD_SOULS_AT: usize = 0x38;
const RECORD_MAP_AT: usize = 0x3C;
const RECORD_PROTECTED_AT: usize = 0x40;
/// PlayerGameData: the souls held.
const SOULS_AT: usize = 0x74;

/// Where the player went down, as the record's places by the game's getters, and the map.
struct Place {
    places: [u8; RECORD_PLACES],
    map: i32,
    protected: bool,
}

static MARKED: std::sync::Mutex<Option<Place>> = std::sync::Mutex::new(None);

/// The game's functions for a bloodstain.
struct BloodstainFns {
    record: usize,
    places: [(usize, usize); 4],
    safe_map: usize,
    protected: usize,
    has_bloodstain: usize,
}

fn bloodstain_fns() -> Option<&'static BloodstainFns> {
    static FOUND: OnceLock<Option<BloodstainFns>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let found = patterns::find_unique("death record", DEATH_RECORD).and_then(|at| {
                let mut places = [(0, 0); 4];
                for (slot, &(call, field)) in places.iter_mut().zip(PLACE_CALLS_AT.iter()) {
                    *slot = (patterns::call_target(at + call)?, field);
                }
                Some(BloodstainFns {
                    record: patterns::call_target(at + RECORD_CALL_AT)?,
                    places,
                    safe_map: patterns::call_target(at + SAFE_MAP_CALL_AT)?,
                    protected: patterns::call_target(at + PROTECTED_CALL_AT)?,
                    has_bloodstain: patterns::call_target(at + HAS_BLOODSTAIN_CALL_AT)?,
                })
            });
            if found.is_none() {
                crate::warn!("death: the game's death record wasn't found; mods can't leave a bloodstain");
            }
            found
        })
        .as_ref()
}

/// Where the character is going down, by the game's own getters.
fn place_of(chr: usize) -> Option<Place> {
    #[repr(C, align(16))]
    struct Out([f32; 4]);
    let f = bloodstain_fns()?;
    let mut places = [0u8; RECORD_PLACES];
    for &(get, field) in &f.places {
        let mut out = Out([0.0; 4]);
        let get: extern "C" fn(usize, *mut f32) -> usize = unsafe { std::mem::transmute(get) };
        get(chr, out.0.as_mut_ptr());
        for (i, v) in out.0[..3].iter().enumerate() {
            places[field + 4 * i..field + 4 * i + 4].copy_from_slice(&v.to_le_bytes());
        }
    }
    let mut map = -1i32;
    let safe_map: extern "C" fn(usize, *mut i32) -> usize = unsafe { std::mem::transmute(f.safe_map) };
    safe_map(chr, &mut map);
    let protected: extern "C" fn(usize) -> i32 = unsafe { std::mem::transmute(f.protected) };
    Some(Place { places, map, protected: protected(chr) != 0 })
}

fn main_chr() -> Result<usize, String> {
    if !crate::on_game_thread() {
        return Err("called from the game's thread only".into());
    }
    crate::game::player_chr_addr()
}

/// Marks where the player is now as where it went down (bloodstain.leave puts its souls there).
pub fn mark() -> Result<(), String> {
    let place = place_of(main_chr()?).ok_or("the game's death record is unavailable on this game build")?;
    *MARKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(place);
    Ok(())
}

/// The player's souls are left where it was marked going down: its bloodstain, as its own death
/// leaves them (its souls then 0). The souls left.
pub fn leave() -> Result<i64, String> {
    main_chr()?;
    let f = bloodstain_fns().ok_or("the game's death record is unavailable on this game build")?;
    let place = MARKED.lock().unwrap_or_else(|e| e.into_inner()).take().ok_or("no place is marked (bloodstain.mark)")?;
    let get_record: extern "C" fn() -> usize = unsafe { std::mem::transmute(f.record) };
    let record = get_record();
    if record == 0 {
        return Err("the character's death record isn't there".into());
    }
    let souls = crate::game::pgd_read_i32(SOULS_AT)?;
    unsafe {
        std::ptr::copy_nonoverlapping(place.places.as_ptr(), record as *mut u8, RECORD_PLACES);
        std::ptr::write_volatile((record + RECORD_UNUSED_AT) as *mut i32, -1);
        std::ptr::write_volatile((record + RECORD_HUMANITY_AT) as *mut i32, 0);
        std::ptr::write_volatile((record + RECORD_SOULS_AT) as *mut i32, souls);
        std::ptr::write_volatile((record + RECORD_MAP_AT) as *mut i32, place.map);
        std::ptr::write_volatile((record + RECORD_PROTECTED_AT) as *mut u8, place.protected as u8);
    }
    crate::game::set_souls(0)?;
    let has: extern "C" fn(u8) = unsafe { std::mem::transmute(f.has_bloodstain) };
    has(1);
    crate::info!("death: {souls} souls left where the player went down (map {:#x})", place.map);
    Ok(souls as i64)
}

/// kindle.game: bloodstain.mark (catalogue.rs).
pub static MARK: catalogue::Function = catalogue::Function {
    name: "bloodstain.mark",
    doc: "Marks where the player is now as where it went down (its place and facing, its last safe point and map, as the game's death records them), for bloodstain.leave.",
    args: &[],
    call: |_| mark().map(|_| None),
};

/// kindle.game: bloodstain.leave (catalogue.rs).
pub static LEAVE: catalogue::Function = catalogue::Function {
    name: "bloodstain.leave",
    doc: "Leaves the player's souls in its bloodstain where bloodstain.mark marked it, as its own death would (the souls then 0, recovered by touching the bloodstain there). Gives the souls left.",
    args: &[],
    call: |_| leave().map(|s| Some(Value::Int(s))),
};

// ── the floor and the air ───────────────────────────────────────────────────────────────────

/// The game's floor setter (ChrIns, the hit under it; +0x88F870): keeps the character's floor
/// (+0x1AC8) and from it the map's draw groups the character sees (+0x1A5C..), its play region and
/// its area. The game's hit update (+0x89B170) calls it each frame with the floor its physics finds
/// under the character; it does nothing while the floor stays the same. A character with its map
/// collision off finds no floor, and the rooms it sees stayed where it last stood (an out co-op
/// player's view: rooms flashed in and out, 2026-10-15): it takes another's floor, as if it stood
/// there.
const FLOOR_SETTER: &str = "40 55 56 57 48 8d 6c 24 b9 48 81 ec 90 00 00 00 48 c7 45 1f fe ff ff ff 48 89 9c 24 c0 00 00 00 48 8b 05 ?? ?? ?? ?? 48 33 c4";
const FLOOR_AT: usize = 0x1AC8;

fn stand_on(chr: usize, other: usize) -> Result<(), String> {
    static SETTER: OnceLock<Option<usize>> = OnceLock::new();
    let floor = memory::read::<usize>(other + FLOOR_AT).filter(|&f| f != 0).ok_or("that character has no floor now")?;
    if memory::read::<usize>(chr + FLOOR_AT) == Some(floor) {
        return Ok(());
    }
    let set = SETTER.get_or_init(|| patterns::find_unique("floor setter", FLOOR_SETTER)).ok_or("the game's floor setter is unavailable on this game build")?;
    let set: extern "C" fn(usize, usize) = unsafe { std::mem::transmute(set) };
    set(chr, floor);
    Ok(())
}

/// A character's address from a handle given as a function argument (looked up now).
fn character(v: &Value) -> Result<usize, String> {
    match v {
        Value::Int(h) => crate::chars::find(*h).map(|c| c.addr),
        _ => Err("a character's handle".into()),
    }
}

/// kindle.game: chr.stand_on (catalogue.rs).
pub static STAND_ON: catalogue::Function = catalogue::Function {
    name: "chr.stand_on",
    doc: "A character (its handle) takes another's floor (the other's handle), and with it the rooms of the map it sees, as if it stood there: for one held where it has no floor of its own (its map collision off).",
    args: &[("character", Kind::Int { min: 0, max: u32::MAX as i64 }), ("other", Kind::Int { min: 0, max: u32::MAX as i64 })],
    call: |a| {
        if !crate::on_game_thread() {
            return Err("chr.stand_on: from the game's thread only".into());
        }
        stand_on(character(&a[0])?, character(&a[1])?).map(|_| None)
    },
};

/// The character's physics module (SprjChrPhysicsModule, [ChrIns +0x1F90] +0x68): +0x1B0 and
/// +0x1B4 how long it has been in the air (s). A character held in the air for a minute landed
/// straight into its fall-death animation (FallDeathLoop), stuck there: no animation event took
/// any more, nor a teleport (verified 2026-10-09 with two games on one PC; the timer read 66.7
/// against a standing player's 0).
fn fall_timers(chr: usize) -> Result<usize, String> {
    memory::read_u64(chr + 0x1F90)
        .filter(|&p| p != 0)
        .and_then(|c| memory::read_u64(c as usize + 0x68))
        .filter(|&p| p != 0)
        .map(|m| m as usize)
        .ok_or_else(|| "the character's physics isn't there".into())
}
const FALL_TIMERS_AT: [usize; 2] = [0x1B0, 0x1B4];

/// kindle.game: chr.fall_time (catalogue.rs).
pub static FALL_TIME: catalogue::Field = catalogue::Field {
    name: "chr.fall_time",
    doc: "How long a character has been in the air, in seconds (the game's fall timers). A character held in the air (teleported each frame) counts as falling: set it to 0 each frame, or it lands into its fall death.",
    kind: Kind::Float { min: 0.0, max: 1.0e6 },
    per_character: true,
    get: |chr| {
        let m = fall_timers(chr.ok_or("chr.fall_time is a character's")?)?;
        memory::read::<f32>(m + FALL_TIMERS_AT[0]).map(|t| Value::Float(t as f64)).ok_or_else(|| "unreadable".into())
    },
    set: Some(|chr, v| {
        let (Some(chr), Value::Float(t)) = (chr, v) else { return Err("chr.fall_time: a character and a number".into()) };
        let m = fall_timers(chr)?;
        for at in FALL_TIMERS_AT {
            unsafe { std::ptr::write_volatile((m + at) as *mut f32, *t as f32) };
        }
        Ok(())
    }),
};
