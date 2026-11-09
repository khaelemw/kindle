//! The other players' characters in a Kindle session: the game's session members, the character
//! the game builds for each (or Kindle's own, made as the debug menu does), their look and gear.
//! Offline the game builds the others' characters only with Kindle's help (session.rs's online
//! id; the introduce request below, which the game sends only as it enters its world), so this is
//! part of Kindle's sessions, for any mod that uses them.
//!
//! Other players' characters: the game builds them itself (verified 2026-10-07 on two games
//! on one PC: the host got the guest's white phantom, the guest the host's character, both
//! moving, fighting, dying and coming back as their players do). Each game takes a member's
//! player record (messages 0x0B, 0x0C, 0x08, 0x0D) into the player data slot it reserved for the
//! member and builds the character once the slot is complete. Offline two things stop that:
//! every game's online id is 0 (FromSoftware's server gives it), and a game rejects a player
//! record carrying its own id (`ensure_online_id`); and a host only asks the members there when
//! it enters its world for their records (`ask_intro`). Kindle fixes both. If the host's game
//! hasn't built a player's character NATIVE_WAIT after the player arrived in its world (the
//! player's Kindle says when), Kindle makes one itself, the way the
//! game's debug menu makes a test player ("Create Multi Player Memory Check Chr") in the
//! member's slot, bound to the member's network id (the game matches what it receives to a
//! character by that id); a guest makes the host's right before its "OnMatchingCheck" (which
//! leaves the session if the host's character is missing) when the game hasn't.

use std::sync::OnceLock;

use crate::hook_sites::{Kind, Site};
use crate::lock;
use crate::session::{FRPG_MANAGER, manager};
use crate::{memory, patterns};

/// Another player's character, made the way the game's debug menu does ("Create Multi Player
/// Memory Check Chr", +0x8FE1E0 mode 1, then +0x8FF300): a free slot of
/// GameDataMan's other players' data (+0x596E50 -> index, +0x5989C0 init, +0x597140 -> its
/// PlayerGameData), filled from a CharaInitParam row (+0xE27760 row handle, +0x59BA40,
/// +0x59B910), its player id (+0x10) set, packed (+0x5F3F40) and made into a character in
/// WorldChrMan's players (+0x8D8F20, kind 1), with its multiplay type (+0x88F060), the
/// character's own setup (vtable +0x90), registration (+0x684D80) and a place next to the
/// main player (+0x88AAB0 position, +0x890330 warp). The packed data's +0x10 is the network id
/// the player constructor binds the character to (+0x8B5F45; the debug menu leaves it 0).
const DEBUG_PLAYER: &str =
    "4c 8b dc 49 89 5b 10 49 89 73 18 57 48 81 ec b0 00 00 00 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 84 24 a0 00 00 00 48 8b f9 85 d2 0f 84 0d 01 00 00";
const DEBUG_CREATE: &str =
    "40 53 55 56 57 41 56 48 83 ec 70 48 8b 05 ?? ?? ?? ?? 48 33 c4 48 89 44 24 60 41 8b 30 48 8b e9 48 8b 0d ?? ?? ?? ?? 45 0f b6 f1 49 8b f8";

struct PlayerFns {
    alloc: usize,
    init: usize,
    data: usize,
    row: usize,
    fill: usize,
    fill2: usize,
    pack: usize,
    create: usize,
    set_kind: usize,
    register_with: usize,
    register: usize,
    position: usize,
    warp: usize,
}

fn player_fns() -> Option<&'static PlayerFns> {
    static FOUND: OnceLock<Option<PlayerFns>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let a = patterns::find_unique("debug player", DEBUG_PLAYER)?;
            let b = patterns::find_unique("debug player create", DEBUG_CREATE)?;
            let c = |base: usize, off: usize| patterns::call_target(base + off);
            Some(PlayerFns {
                alloc: c(a, 0x60)?,
                init: c(a, 0x7F)?,
                data: c(a, 0x86)?,
                row: c(a, 0xBA)?,
                fill: c(a, 0xDD)?,
                fill2: c(a, 0xF1)?,
                pack: c(a, 0x116)?,
                create: c(b, 0x61)?,
                set_kind: c(b, 0x77)?,
                register_with: c(b, 0x9E)?,
                register: c(b, 0xEC)?,
                position: c(b, 0x140)?,
                warp: c(b, 0x15C)?,
            })
        })
        .as_ref()
}

/// A member of the game's session (SprjSessionManager +0x18 a handle, [handle] the session,
/// session +0x68..+0x70 its members): the address, its member id (+0xA8, the player id this
/// game gives it), whether it's the host (+0xAD) and its Steam id (+0xC8). Its network id
/// (+0x68) is what the game binds a remote player's character to.
#[derive(Debug, Clone, Copy)]
pub struct Member {
    pub addr: usize,
    pub id: u32,
    pub host: bool,
    pub steam: u64,
}

pub fn members() -> Vec<Member> {
    let Ok(m) = manager() else { return Vec::new() };
    let Some(session) = memory::read_u64(m + 0x18).filter(|&s| s != 0) else { return Vec::new() };
    let (b, e) = (memory::read_u64(session as usize + 0x68).unwrap_or(0) as usize, memory::read_u64(session as usize + 0x70).unwrap_or(0) as usize);
    if e < b || e - b > 8 * 16 {
        return Vec::new();
    }
    (b..e)
        .step_by(8)
        .filter_map(|at| memory::read_u64(at).filter(|&p| p != 0).map(|p| p as usize))
        .map(|addr| Member {
            addr,
            id: memory::read::<u32>(addr + 0xA8).unwrap_or(0),
            host: memory::read::<u8>(addr + 0xAD).unwrap_or(0) != 0,
            steam: memory::read_u64(addr + 0xC8).unwrap_or(0),
        })
        .collect()
}

/// What other players' characters start from (a CharaInitParam row: the first starting
/// class) before their own data goes in (`own_look`).
const REMOTE_LOOK: u32 = 1080100;

/// The parts of a player's data (PlayerGameData: GameDataMan +0x10 this player's, a
/// character's at +0x1FA0) a character is made from, sent to the other players: +0x18..+0x80
/// HP, FP and stamina, the attributes (+0x44..+0x6C) and level (+0x70); +0x88 the name (UTF-16,
/// 16 characters and the NUL); +0xAA the body type; +0x328..+0x384 what's equipped (weapons
/// +0x32C..+0x340, armor +0x35C..+0x368, hair +0x36C, rings +0x370..+0x37C, ammunition
/// between); +0x6AC the face and body ("FACE", version, size 0xF4, then the shape; no pointers
/// in it, they start at +0x7A0).
pub(crate) const LOOK_PARTS: [(usize, usize); 6] = [(0x18, 0x68), (0x88, 0x22), (0xAA, 1), GEAR_PARTS[0], GEAR_PARTS[1], (FACE_AT, FACE_LEN)];
const LOOK_VERSION: u8 = 3;
/// What a player can change while playing, sent again when it changes: the stance and the
/// active weapon slots (+0x2B8: 1 one-handed, 3 the right weapon in both hands; verified
/// 2026-10-07 by toggling and by writing it on a phantom, whose model follows at once) and
/// the equipment (+0x328, the same for armour: a phantom's helmet came off when written).
const GEAR_PARTS: [(usize, usize); 2] = [(0x2B8, 0x14), (0x328, 0x5C)];
const FACE_AT: usize = 0x6AC;
const FACE_LEN: usize = 0xF4;

/// This player's character, packed for the others: a version, then each part's bytes.
pub fn own_look() -> Option<Vec<u8>> {
    let gdm = crate::ds3::GAME_DATA_MAN.instance()?;
    let pgd = memory::read_u64(gdm + 0x10).filter(|&p| p != 0)? as usize;
    let mut out = vec![LOOK_VERSION];
    for (at, len) in LOOK_PARTS {
        out.extend(memory::read_bytes(pgd + at, len)?);
    }
    Some(out)
}

/// Writes another player's packed character (`own_look` on their side) into player data: all
/// of it, or only `GEAR_PARTS` (for a character already in the world).
fn apply_look(pgd: usize, look: &[u8], gear_only: bool) -> Result<(), String> {
    let want = 1 + LOOK_PARTS.iter().map(|p| p.1).sum::<usize>();
    if look.first() != Some(&LOOK_VERSION) || look.len() != want {
        return Err(format!("a character in another format ({} bytes)", look.len()));
    }
    let mut from = 1;
    for (at, len) in LOOK_PARTS {
        // The face only where both have one (its marker), in case a game's layout differs.
        let face_ok = at != FACE_AT || (look.get(from..from + 4) == Some(b"FACE") && memory::read_bytes(pgd + at, 4).as_deref() == Some(b"FACE"));
        if face_ok && (!gear_only || GEAR_PARTS.contains(&(at, len))) {
            unsafe { std::ptr::copy_nonoverlapping(look[from..].as_ptr(), (pgd + at) as *mut u8, len) };
        }
        from += len;
    }
    Ok(())
}

/// Asks a session member to introduce itself (message 0x0A; +0x77DA70, which looks the session
/// manager up itself): it answers with its player record and gear (0x0B, 0x0C, 0x08, 0x0D ...),
/// the game fills the member's player data slot from them, and once the slot's flags are
/// complete (0x1F, +0x597890) it builds the member's character itself (+0x78E070 -> +0x8FF480).
/// The game only asks when it enters its world (+0x78F400, the members already there); a host
/// already in its world never asks a player who joins later, so Kindle does.
const ASK_INTRO: &str = "48 83 ec 38 48 8b 0d ?? ?? ?? ?? 4c 8d 4c 24 50 41 b8 0a 00 00 00 c6 44 24 50 00 c7 44 24 20 01 00 00 00 e8";

fn ask_intro(net: usize) {
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    let Some(ask) = *FOUND.get_or_init(|| patterns::find_unique("introduce request", ASK_INTRO)) else { return };
    let ask: extern "C" fn(usize, usize) = unsafe { std::mem::transmute(ask) };
    ask(0, net);
}

/// A member's player data slot flags (GameDataMan +0x28, a byte per slot): 0x1F when the game
/// has all of the member's record and builds its character.
const SLOT_COMPLETE: u8 = 0x1F;

fn slot_flags(slot: i32) -> Option<u8> {
    let gdm = crate::ds3::GAME_DATA_MAN.instance()?;
    let flags = memory::read_u64(gdm + 0x28).filter(|&f| f != 0)? as usize;
    memory::read::<u8>(flags + slot as usize)
}

/// The player data of slot `slot` (GameDataMan +0x18, 0x950 bytes each).
fn slot_data(slot: i32) -> Option<usize> {
    let gdm = crate::ds3::GAME_DATA_MAN.instance()?;
    let base = memory::read_u64(gdm + 0x18).filter(|&b| b != 0)? as usize;
    Some(base + slot as usize * 0x950)
}

/// The character (other than this player's) whose player data is slot `slot`'s.
pub(crate) fn character_of_slot(slot: i32) -> Option<usize> {
    let pgd = slot_data(slot)?;
    let world = crate::ds3::WORLD_CHR_MAN.instance()?;
    let n = memory::read::<i32>(world + 0x38).unwrap_or(0).clamp(0, 8) as usize;
    let list = memory::read_u64(world + 0x40).unwrap_or(0) as usize;
    (0..n).find_map(|i| {
        let ins = memory::read_u64(list + i * 0x38).filter(|&p| p != 0)? as usize;
        (memory::read_u64(ins + 0x1FA0) == Some(pgd as u64)).then_some(ins)
    })
}

/// The player data slot the game reserved for the session member with the network id at
/// `net`: the character goes there, so what the game looks up by slot (the member's player
/// data) is the character's, and the game frees it when the member leaves.
///
/// FrpgSessionManagerImp's list at +0xC0 holds a record per player the game registered when they
/// joined (+0x78CA30, which also reserves their player data slot in GameDataMan): +0x10 the
/// network id, +0x50 the slot; the game drops a record as its player leaves (+0x78B160: unlinks
/// it, destroys the id's lock and frees it). A network id is a pointer to the id object and a
/// lock; the record's is set from the joining member's (+0x195A4C0 -> +0x195ACD0: the same
/// object, its count raised, not a copy), so a record is the member's when the two pointers are
/// the same. Kindle only reads: the game's own compare (+0x195A530) locks both ids and calls
/// the id object's virtual compare, and on a record being taken apart that wrote into freed
/// memory: a host's heap corrupted 9 ms after the lookup (0xc0000374, 2026-10-10, a travel with
/// a player still on its way in), and a joined player's game crashed in the heap after
/// following its host's travel (in co-op play, 2026-10-16 and twice 2026-10-20: the save on
/// arrival freeing into a corrupt heap, the lookup 0.03 s before). None while the game is on a
/// loading screen, where it takes departing members' records apart.
pub(crate) fn member_slot(net: usize) -> Option<i32> {
    if !crate::game::in_world() {
        return None;
    }
    let id = memory::read_u64(net).filter(|&p| p != 0)?;
    let frpg = crate::ds3::singleton_instance(FRPG_MANAGER)?;
    let head = memory::read_u64(frpg + 0xC0).filter(|&h| h != 0)? as usize;
    let mut node = memory::read_u64(head)? as usize;
    for _ in 0..16 {
        if node == head || node == 0 {
            return None;
        }
        if memory::read_u64(node + 0x10) == Some(id) {
            return memory::read::<i32>(node + 0x50).filter(|s| (0..5).contains(s));
        }
        node = memory::read_u64(node)? as usize;
    }
    None
}

/// Makes another player's character (a white phantom, kind 1) with player id `id` from
/// CharaInitParam row `init` and their own data if their Kindle sent it (`look`), next to the
/// main player, bound to the network id at `net` (a session member's +0x68); its address.
fn make_character(id: u32, init: u32, look: Option<&[u8]>, net: usize, host: bool) -> Result<usize, String> {
    let f = player_fns().ok_or("the game's player functions weren't found")?;
    let gdm = crate::ds3::GAME_DATA_MAN.instance().ok_or("no GameDataMan")?;
    let world = crate::ds3::WORLD_CHR_MAN.instance().ok_or("no WorldChrMan")?;
    let main = memory::read_u64(world + 0x80).filter(|&p| p != 0).ok_or("no main player")? as usize;
    unsafe {
        let alloc: extern "C" fn() -> i32 = std::mem::transmute(f.alloc);
        let init_slot: extern "C" fn(i32) = std::mem::transmute(f.init);
        let data: extern "C" fn(i32) -> usize = std::mem::transmute(f.data);
        let row: extern "C" fn(*mut u8, u32) -> usize = std::mem::transmute(f.row);
        let fill: extern "C" fn(usize, usize, *mut u8, u32) = std::mem::transmute(f.fill);
        let fill2: extern "C" fn(usize, usize, *mut u8) = std::mem::transmute(f.fill2);
        let pack: extern "C" fn(*mut u8, usize) -> usize = std::mem::transmute(f.pack);
        let create: extern "C" fn(usize, u32, usize) -> usize = std::mem::transmute(f.create);
        let set_kind: extern "C" fn(usize, u32) = std::mem::transmute(f.set_kind);
        let register_with: extern "C" fn() -> usize = std::mem::transmute(f.register_with);
        let register: extern "C" fn(usize, u32, u32, u32, i32, i32, u32) = std::mem::transmute(f.register);
        let position: extern "C" fn(usize, *mut f32) -> *const f32 = std::mem::transmute(f.position);
        let warp: extern "C" fn(usize, *const f32) = std::mem::transmute(f.warp);

        // The slot the game gave this player when they joined, else one of Kindle's.
        let slot = match member_slot(net) {
            Some(slot) => slot,
            None => {
                let slot = alloc();
                if slot < 0 {
                    return Err("no free slot for another player's data".into());
                }
                init_slot(slot);
                slot
            }
        };
        let pgd = data(slot);
        if pgd == 0 {
            return Err(format!("slot {slot} has no player data"));
        }
        // A row handle: {i32 -1, pad, ptr 0}, filled by the lookup.
        let mut handle = [0u8; 16];
        handle[..4].copy_from_slice(&(-1i32).to_le_bytes());
        row(handle.as_mut_ptr(), init);
        if handle[8..16].iter().all(|&b| b == 0) {
            return Err(format!("no CharaInitParam row {init}"));
        }
        fill(gdm, pgd, handle.as_mut_ptr(), 0);
        fill2(gdm, pgd, handle.as_mut_ptr());
        if let Some(look) = look
            && let Err(e) = apply_look(pgd, look, false)
        {
            crate::warn!("session: player {id}'s character: {e}; a starting class instead");
        }
        std::ptr::write_volatile((pgd + 0x10) as *mut u32, id);
        let mut packed = [0u8; 0x30];
        let desc = pack(packed.as_mut_ptr(), pgd);
        // The network id the character is bound to (the player constructor reads +0x10).
        std::ptr::write_unaligned((desc + 0x10) as *mut usize, net);
        const KIND: u32 = 1;
        let ins = create(world, KIND, desc);
        if ins == 0 {
            return Err("the game didn't make the character".into());
        }
        // The multiplay type: a host is the world's own player (0), the others white phantoms (1).
        set_kind(ins, if host { 0 } else { KIND });
        let vt = memory::read_u64(ins).unwrap_or(0) as usize;
        let setup: extern "C" fn(usize) = std::mem::transmute(memory::read_u64(vt + 0x90).ok_or("no vtable")? as usize);
        setup(ins);
        let flags = (ins + 0x1A09) as *mut u8;
        std::ptr::write_volatile(flags, std::ptr::read_volatile(flags) & 0xDF);
        let handle_id = memory::read::<u32>(ins + 8).unwrap_or(0);
        register(register_with(), handle_id, KIND, 1, -1, -1, 0);
        std::ptr::write_volatile(flags, std::ptr::read_volatile(flags) | 0x20);
        let mut here = [0f32; 4];
        let at = position(main, here.as_mut_ptr());
        let mut to = [*at, *at.add(1), *at.add(2), *at.add(3)];
        to[0] += 1.5;
        warp(ins, to.as_ptr());
        Ok(ins)
    }
}

/// The game's "OnMatchingCheck" (+0x495F30, r8: +8 a flag, +0xC a player id), run on a guest
/// just after it arrives: it looks up this player and the host (the session member flagged
/// host) among WorldChrMan's players and leaves the session (0xff000023) if either is missing.
pub(crate) const MATCHING_CHECK: &str = "48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 41 80 78 08 00 49 8b f0 48 8b fa 48 8b ca 74 4b e8 ?? ?? ?? ?? 84 c0 75 64 8b 5e 0c 48 8b cf e8 ?? ?? ?? ?? 44 8b c3 48 8b cf 8b d0 e8 ?? ?? ?? ?? 84 c0 75 48 44 8b 4e 0c ba e0 0f 00 00";
/// The other players whose characters are in the world this session (player id, Steam id),
/// the game's own or Kindle's, on the game's thread.
pub(crate) static MADE: std::sync::Mutex<Vec<(u32, u64)>> = std::sync::Mutex::new(Vec::new());
/// A joined player whose character isn't in the world yet.
pub(crate) struct Due {
    steam: u64,
    joined: std::time::Instant,
    /// When the host last asked them to introduce themselves.
    asked: Option<std::time::Instant>,
    /// When their Kindle said they're in the host's world (p2p ARRIVED).
    arrived: Option<std::time::Instant>,
}

pub(crate) static DUE: std::sync::Mutex<Vec<Due>> = std::sync::Mutex::new(Vec::new());
/// How long Kindle waits, from a player's arrival in the host's world, for the game to build
/// their character itself (asking them to introduce themselves every ASK_EVERY) before making
/// one of its own. Counted from joining it came too early for a player loading another area
/// (a new character from the Cemetery of Ash: the game's own came after Kindle's, two
/// characters for one player).
const NATIVE_WAIT: std::time::Duration = std::time::Duration::from_secs(20);
/// The same from joining, for a player whose Kindle doesn't say when it arrives.
const NATIVE_WAIT_UNSAID: std::time::Duration = std::time::Duration::from_secs(90);
const ASK_EVERY: std::time::Duration = std::time::Duration::from_secs(2);
/// How long the fallback waits for a joined player's look from their Kindle before making a
/// starting class instead.
const LOOK_WAIT: std::time::Duration = std::time::Duration::from_secs(3);
/// Another player joined the session (on a player, one besides the host): watched until their
/// character is in the world.
pub fn player_joined(steam: u64) {
    let mut due = lock(&DUE);
    if !due.iter().any(|d| d.steam == steam) {
        due.push(Due { steam, joined: std::time::Instant::now(), asked: None, arrived: None });
    }
}

/// A player's Kindle says they're in the host's world (p2p ARRIVED).
pub fn player_arrived(steam: u64) {
    if let Some(d) = lock(&DUE).iter_mut().find(|d| d.steam == steam) {
        d.arrived = Some(std::time::Instant::now());
    }
}

/// A player left: the game takes their character away itself (its registration of the
/// character, +0x684D80, goes to "leaving", state 7, and the game plays the departure; Kindle
/// deleting the character first crashed that, +0x686BF9), so Kindle only forgets it.
pub fn player_left(steam: u64) {
    lock(&DUE).retain(|d| d.steam != steam);
    lock(&MADE).retain(|&(_, s)| s != steam);
}

/// The session ended: nothing more to make.
pub fn session_ended() {
    lock(&DUE).clear();
    lock(&MADE).clear();
}

pub(crate) static MATCHING_CHECK_SITE: Site = Site {
    area: "session",
    name: "matching check",
    pattern: MATCHING_CHECK,
    offset: 0,
    matches: 1,
    kind: Kind::JmpBack(before_matching_check),
    without: "a guest will leave as it arrives",
};

/// A new session: nothing made yet; the matching check's and the freed floor's hooks are
/// installed (once).
pub(crate) fn reset() {
    lock(&MADE).clear();
    lock(&DUE).clear();
    static HOOKED: std::sync::Once = std::sync::Once::new();
    HOOKED.call_once(|| {
        MATCHING_CHECK_SITE.install();
        HIT_FREED_SITE.install();
    });
}

// ── the floor under another player's character ──────────────────────────────────────────────

/// A character's floor is the map collision (HitIns) it stands on, ChrIns +0x1AC8 (+0x1AD0 the
/// floor before it), set by +0x88F870; each step of its physics reads it (+0x89B560: the hit's
/// type, through the hit's map part). The game clears the floors of the characters it registers
/// only as its player warps (+0x608A00); a map's characters go with their map. Another player's
/// character stays when this game streams that player's map out without a warp: the host rode
/// the Undead Settlement's lift up from the Road of Sacrifices and the player who stayed below
/// kept a floor whose collision had been freed: the host's game crashed reading it, +0x857BF7
/// from +0x89B5D9 (in co-op play 2026-10-15, then on two games on one PC 2026-10-18). The
/// match is HitIns's destructor (+0x8596D0); before the hit goes, a player's character standing
/// on it has no floor (0, as the game's own readers expect) until its physics finds one again.
const HIT_FREED: &str = "48 89 4c 24 08 57 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 48 8b da 48 8b f9 48 8d 05 ?? ?? ?? ?? 48 89 01 e8 ?? ?? ?? ?? 90 48 8d 8f 18 01 00 00 e8 ?? ?? ?? ?? 90 48 8d 8f f0 00 00 00";
const FLOOR: usize = 0x1AC8;
const FLOOR_BEFORE: usize = 0x1AD0;

static HIT_FREED_SITE: Site = Site {
    area: "session",
    name: "map collision freed",
    pattern: HIT_FREED,
    offset: 0,
    matches: 1,
    kind: Kind::JmpBack(before_hit_freed),
    without: "a player left on a map this game streams out crashes it",
};

fn before_hit_freed(reg: *mut ilhook::x64::Registers) {
    let hit = unsafe { (*reg).rcx };
    let Some(world) = crate::ds3::WORLD_CHR_MAN.instance() else { return };
    let main = memory::read_u64(world + 0x80).unwrap_or(0);
    let n = memory::read::<i32>(world + 0x38).unwrap_or(0).clamp(0, 8) as usize;
    let list = memory::read_u64(world + 0x40).unwrap_or(0) as usize;
    if hit == 0 || list == 0 {
        return;
    }
    for i in 0..n {
        let Some(ins) = memory::read_u64(list + i * 0x38).filter(|&p| p != 0 && p != main) else { continue };
        for at in [FLOOR, FLOOR_BEFORE] {
            let field = ins as usize + at;
            if memory::read_u64(field) == Some(hit) {
                unsafe { std::ptr::write_volatile(field as *mut u64, 0) };
                if at == FLOOR {
                    crate::debug!("session: another player's floor ({hit:#x}) goes with its map; that character has none until it finds one");
                }
            }
        }
    }
}

pub(crate) fn before_matching_check(_reg: *mut ilhook::x64::Registers) {
    if crate::p2p::hosting() == Some(false) {
        // The host starts waiting for the game to build this player's character from now.
        crate::p2p::send_arrived();
        if !make_member(None) {
            crate::warn!("session: the host isn't among the game's session members yet");
        }
    }
}

// ── gear while playing ─────────────────────────────────────────────────────────────────────

/// Players whose character arrived again (from the connection's threads), to apply on the
/// game's thread.
static LOOKS_ARRIVED: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
/// This player's gear as last sent, and when it was last looked at.
static GEAR_SENT: std::sync::Mutex<(Vec<u8>, Option<std::time::Instant>)> = std::sync::Mutex::new((Vec::new(), None));
const GEAR_EVERY: std::time::Duration = std::time::Duration::from_millis(500);

/// A player's character came from their Kindle (p2p): applied to their character if it's
/// in the world already.
pub fn look_arrived(steam: u64) {
    let mut a = lock(&LOOKS_ARRIVED);
    if !a.contains(&steam) {
        a.push(steam);
    }
}

/// Gear from the parts of a packed character.
fn gear_of(look: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut from = 1;
    for (at, len) in LOOK_PARTS {
        if GEAR_PARTS.contains(&(at, len)) {
            out.extend_from_slice(look.get(from..from + len).unwrap_or(&[]));
        }
        from += len;
    }
    out
}

/// Each frame: this player's gear goes out when it changes; others' arrived gear goes on their
/// characters.
pub(crate) fn gear_frame() {
    if crate::p2p::hosting().is_none() {
        *lock(&GEAR_SENT) = (Vec::new(), None);
        lock(&LOOKS_ARRIVED).clear();
        return;
    }
    {
        let mut sent = lock(&GEAR_SENT);
        if sent.1.is_none_or(|t| t.elapsed() >= GEAR_EVERY) {
            sent.1 = Some(std::time::Instant::now());
            if crate::ds3::main_player().is_some()
                && let Some(look) = own_look()
            {
                let gear = gear_of(&look);
                if !sent.0.is_empty() && gear != sent.0 {
                    crate::p2p::send_look(&look);
                }
                sent.0 = gear;
            }
        }
    }
    let arrived = std::mem::take(&mut *lock(&LOOKS_ARRIVED));
    for steam in arrived {
        let Some(id) = lock(&MADE).iter().find(|&&(_, s)| s == steam).map(|&(id, _)| id) else { continue };
        // Their character's player data: the slot the game gave them, else by player id.
        let slot_pgd = members()
            .into_iter()
            .find(|m| m.steam == steam)
            .and_then(|m| member_slot(m.addr + 0x68))
            .filter(|&s| character_of_slot(s).is_some())
            .and_then(slot_data);
        if let (Some(pgd), Some(look)) = (slot_pgd.or_else(|| player_data(id)), crate::p2p::look(steam))
            && let Err(e) = apply_look(pgd, &look, true)
        {
            crate::warn!("session: player {id}'s gear: {e}");
        }
    }
}

/// The PlayerGameData of the WorldChrMan player with this player id (+0x38 count, +0x40
/// entries of 0x38 bytes, +0 the PlayerIns; its id [[ins+0x1FA0]+0x10]).
fn player_data(id: u32) -> Option<usize> {
    let world = crate::ds3::WORLD_CHR_MAN.instance()?;
    let n = memory::read::<i32>(world + 0x38).unwrap_or(0).clamp(0, 8) as usize;
    let list = memory::read_u64(world + 0x40).unwrap_or(0) as usize;
    (0..n).find_map(|i| {
        let ins = memory::read_u64(list + i * 0x38).filter(|&p| p != 0)? as usize;
        let pgd = memory::read_u64(ins + 0x1FA0).filter(|&p| p != 0)? as usize;
        (memory::read::<u32>(pgd + 0x10) == Some(id)).then_some(pgd)
    })
}

/// Whether WorldChrMan has a player with this player id.
fn has_player(id: u32) -> bool {
    player_data(id).is_some()
}

/// Makes the character of a member of the game's session (the host, or the player with this
/// Steam id), once per session; false if the game doesn't have the member yet.
fn make_member(steam: Option<u64>) -> bool {
    let members = members();
    let Some(m) = members.iter().find(|m| match steam {
        Some(s) => m.steam == s,
        None => m.host,
    }) else {
        return false;
    };
    if lock(&MADE).iter().any(|&(_, steam)| steam == m.steam) {
        return true;
    }
    // The game's own character (built from the member's record in its slot, see ask_intro).
    if let Some(slot) = member_slot(m.addr + 0x68)
        && character_of_slot(slot).is_some()
    {
        let id = slot_data(slot).and_then(|p| memory::read::<u32>(p + 0x10)).unwrap_or(m.id);
        lock(&MADE).push((id, m.steam));
        crate::info!("session: player {id} ({:#x}) is in the world (the game's own character)", m.steam);
        return true;
    }
    // The game numbers its session members and its own character with separate counters: after
    // a few joins a member's number can be this player's own (verified 2026-10-07: own 5, the
    // host member 5). The game finds a player's character by network id, then by its number,
    // so a taken number is swapped for a free one.
    let id = if has_player(m.id) { (m.id + 1..m.id + 64).find(|&i| !has_player(i) && !members.iter().any(|o| o.id == i)).unwrap_or(m.id) } else { m.id };
    lock(&MADE).push((id, m.steam));
    let look = crate::p2p::look(m.steam);
    match make_character(id, REMOTE_LOOK, look.as_deref(), m.addr + 0x68, m.host) {
        Ok(_) => {
            crate::info!(
                "session: player {id} ({:#x}) is in the world{}",
                m.steam,
                if look.is_some() { "" } else { " (as a starting class: no character from them yet)" }
            );
        }
        Err(e) => crate::warn!("session: couldn't make player {}'s character: {e}", m.id),
    }
    true
}

/// Each frame: makes the characters still due.
pub(crate) fn characters_frame() {
    let due = std::mem::take(&mut *lock(&DUE));
    // (A player who left goes: the connection's thread drops it from DUE, but not while it's
    // taken here.)
    let here = crate::p2p::members();
    for mut d in due {
        let steam = d.steam;
        if here.as_ref().is_some_and(|m| !m.contains(&steam)) {
            continue;
        }
        let member = members().into_iter().find(|m| m.steam == steam);
        let slot = member.and_then(|m| member_slot(m.addr + 0x68));
        // The game builds the character itself once it has the member's record (see
        // ask_intro): Kindle asks, and only makes one if the game hasn't NATIVE_WAIT after they
        // arrived.
        if let Some(slot) = slot
            && character_of_slot(slot).is_some()
        {
            let id = slot_data(slot).and_then(|p| memory::read::<u32>(p + 0x10)).unwrap_or(0);
            let mut made = lock(&MADE);
            if !made.iter().any(|&(_, s)| s == steam) {
                made.push((id, steam));
                crate::info!("session: player {id} ({steam:#x}) is in the world (the game's own character)");
            }
            continue;
        }
        let deadline = d.arrived.map_or(d.joined + NATIVE_WAIT_UNSAID, |a| a + NATIVE_WAIT);
        let now = std::time::Instant::now();
        if let (Some(m), Some(slot)) = (member, slot)
            && crate::p2p::hosting() == Some(true)
            && slot_flags(slot).is_some_and(|f| f != SLOT_COMPLETE)
            && d.asked.is_none_or(|t| t.elapsed() >= ASK_EVERY)
            && now < deadline
        {
            ask_intro(m.addr + 0x68);
            d.asked = Some(now);
        }
        // The fallback (a host's only: a player may still be on its way to the host's world,
        // where the game builds the others): Kindle's own character, from their look and in
        // the game's slot.
        let ready = crate::p2p::hosting() == Some(true) && now > deadline && (crate::p2p::look(steam).is_some() || now > deadline + LOOK_WAIT);
        if !(ready && crate::ds3::main_player().is_some() && make_member(Some(steam))) {
            lock(&DUE).push(d);
        }
    }
}

/// A session member's character in this game's world (its Steam id), and its name.
pub(crate) fn member_character(steam: u64) -> Option<(usize, String)> {
    let m = members().into_iter().find(|m| m.steam == steam)?;
    let chr = if Some(steam) == crate::p2p::me() { crate::game::player_chr_addr().ok()? } else { character_of_slot(member_slot(m.addr + 0x68)?)? };
    let name = unsafe { crate::ds3::PlayerIns::at(chr) }.game_data().map(|d| d.player_info().name()).unwrap_or_default();
    Some((chr, name))
}

/// The other players' characters in the world: (character, position, HP).
pub(crate) fn teammates(main: usize) -> Vec<(usize, [f32; 3], i32)> {
    let Some(world) = crate::ds3::WORLD_CHR_MAN.instance() else { return Vec::new() };
    let n = memory::read::<i32>(world + 0x38).unwrap_or(0).clamp(0, 8) as usize;
    let list = memory::read_u64(world + 0x40).unwrap_or(0) as usize;
    (0..n)
        .filter_map(|i| {
            let ins = memory::read_u64(list + i * 0x38).filter(|&p| p != 0)? as usize;
            if ins == main {
                return None;
            }
            // Read only through checked reads: a player's character can be on its way out.
            let modules = memory::read_u64(ins + 0x1F90).filter(|&p| p != 0)? as usize;
            let data = memory::read_u64(modules + 0x18).filter(|&p| p != 0)? as usize;
            let hp = memory::read::<i32>(data + 0xD8)?;
            let entry = memory::read_u64(ins + 0x18).filter(|&p| p != 0)? as usize;
            let physics = memory::read_u64(entry + 0x28).filter(|&p| p != 0)? as usize;
            let at = [memory::read::<f32>(physics + 0x80)?, memory::read::<f32>(physics + 0x84)?, memory::read::<f32>(physics + 0x88)?];
            at.iter().all(|f| f.is_finite()).then_some((ins, at, hp))
        })
        .collect()
}
