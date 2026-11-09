//! Loading home after a visit to another world, as hook points of the catalogue: what the game
//! gives a phantom back as it loads home, and the character's own type. A mod (co-op) answers them
//! so a player home from a session keeps what it had there, and is itself.
//!
//! | | |
//! |---|---|
//! | the phantom's copy | World_LoadPlayerData's check of the ask (+0x8F7861): `load.phantom_copy` |
//! | a phantom's type at home | World_LoadPlayerData's start (+0x8F7420): `load.player_type` |
//! | the kept home type | the game's home type ([+0x475C1B0]+4), each frame: `chr_type.keep_home_type` |

use std::sync::OnceLock;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::catalogue::HookPoint;
use crate::hook_sites::{self, Site};
use crate::{memory, patterns};

// ── the phantom's copy ──────────────────────────────────────────────────────────────────────

/// As a world ends with the session joining another (+0x8F36F0), the game keeps a copy of the
/// character's counts in its player data (+0x8F7FC0: +0x128 HP, +0x12C FP, +0x130 and +0x131 the
/// Estus and Ashen Estus charges, +0x132.. four more) and of the goods it carries (+0x58DC40); the
/// flow's ways home (+0x4851F0, from OnLeave_Limit, a dead phantom's go-home and others) ask for
/// them back (GameMan +0xAC5), and the load home (World_LoadPlayerData, +0x8F7420) puts them back,
/// restocking the goods from the storage box, and clears the ask. So a phantom's HP, FP and Estus
/// at home are those it joined with, and goods it used in the host's world come back to it. The
/// match is the load's check, the hook on its `test al, al` (rsi the player data); the call before
/// it reads the ask (`mov rax, [GameMan]; movzx eax, byte [rax + ask]`).
const RETURN_RESTORE: &str = "48 8b 05 ?? ?? ?? ?? c6 80 80 00 00 00 00 e8 ?? ?? ?? ?? 84 c0 0f 84 ?? ?? ?? ?? 48 8b 85 90 1f 00 00 48 8b 58 18 e8 ?? ?? ?? ?? 8b d0 48 8b cb e8 ?? ?? ?? ?? e8 ?? ?? ?? ?? 8b 4e 1c";
const RETURN_RESTORE_CALL_AT: usize = 0x0E;
const RETURN_RESTORE_TEST_AT: usize = 0x13;
/// Player data: HP and its maximum, FP and its maximum, the flasks owned (Estus, Ashen Estus), and
/// the copy's HP, FP and charges.
const DATA_HP_MAX: usize = 0x1C;
const DATA_FP_MAX: usize = 0x28;
const DATA_FLASKS: usize = 0x102;
const DATA_ASHEN_FLASKS: usize = 0x103;
const COPY_HP: usize = 0x128;
const COPY_FP: usize = 0x12C;
const COPY_ESTUS: usize = 0x130;
const COPY_ASHEN_ESTUS: usize = 0x131;
/// The ask (GameMan +0xAC5), as the load's reader of it names it: the static holding GameMan, and
/// the ask's offset.
static RESTORE_ASK: OnceLock<Option<(usize, usize)>> = OnceLock::new();

/// The ask's place, from its reader (`48 8b 05 rel32` then `0f b6 80 off32`, `c3`).
fn restore_ask(reader: usize) -> Option<(usize, usize)> {
    let code = memory::read_bytes(reader, 15)?;
    if code[..3] != [0x48, 0x8B, 0x05] || code[7..10] != [0x0F, 0xB6, 0x80] || code[14] != 0xC3 {
        return None;
    }
    let offset = u32::from_le_bytes(code[10..14].try_into().ok()?) as usize;
    Some((patterns::rip_target(reader + 3, reader + 7)?, offset))
}

const KEEP_CURRENT: usize = 1;
const FULL: usize = 2;

fn return_restore_hook(reg: *mut ilhook::x64::Registers) {
    if !PHANTOM_COPY.answered() {
        return;
    }
    let r = unsafe { &mut *reg };
    let data = r.rsi as usize;
    let asked = r.rax as u8 != 0;
    match PHANTOM_COPY.decide(&[asked as i64]) {
        KEEP_CURRENT => {
            if asked {
                r.rax &= !0xFF;
                if let Some(&Some((game_man, offset))) = RESTORE_ASK.get()
                    && let Some(instance) = memory::read_u64(game_man).filter(|&g| g != 0)
                {
                    unsafe { std::ptr::write_volatile((instance as usize + offset) as *mut u8, 0) };
                }
            }
            crate::info!("load: home with what it has now, not its copy (a mod's answer)");
        }
        FULL => {
            let full = [(COPY_HP, memory::read::<i32>(data + DATA_HP_MAX)), (COPY_FP, memory::read::<i32>(data + DATA_FP_MAX))];
            let flasks = [(COPY_ESTUS, memory::read::<u8>(data + DATA_FLASKS)), (COPY_ASHEN_ESTUS, memory::read::<u8>(data + DATA_ASHEN_FLASKS))];
            unsafe {
                for (at, value) in full {
                    if let Some(value) = value {
                        std::ptr::write_volatile((data + at) as *mut i32, value);
                    }
                }
                for (at, value) in flasks {
                    if let Some(value) = value {
                        std::ptr::write_volatile((data + at) as *mut u8, value);
                    }
                }
            }
            // Nothing for the load to put back: the Estus refilled here, as the copy would have.
            if !asked {
                let _ = crate::game::set_estus(Some(i64::MAX), Some(i64::MAX));
            }
            crate::info!("load: home with full HP, FP and Estus (a mod's answer)");
        }
        _ => {}
    }
}

static RETURN_RESTORE_SITE: Site = Site {
    area: "load",
    name: "return restore",
    pattern: RETURN_RESTORE,
    offset: RETURN_RESTORE_TEST_AT as isize,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(return_restore_hook),
    without: "mods can't change what a player has as it loads home from another world",
};

fn install_return_restore() -> Result<(), String> {
    let found = RETURN_RESTORE_SITE.find().ok_or(RETURN_RESTORE_SITE.without)?;
    let _ = RESTORE_ASK.set(patterns::call_target(found[0] + RETURN_RESTORE_CALL_AT).and_then(restore_ask));
    RETURN_RESTORE_SITE.hook_all(&found).then_some(()).ok_or_else(|| RETURN_RESTORE_SITE.without.to_string())
}

/// kindle.game: load.phantom_copy (catalogue.rs).
pub static PHANTOM_COPY: HookPoint = HookPoint::hooked(
    "load.phantom_copy",
    "The player's character loads (1 if the game asks to put back the copy it kept as the character left for another world: the HP, FP, Estus and goods it had then): \"game\", \"keep_current\" (not put back: the character keeps what it has now) or \"full\" (put back with full HP, FP and Estus, as a death leaves a character at home). On the game's thread, at loads.",
    &["game", "keep_current", "full"],
    install_return_restore,
);

// ── the character's own type ────────────────────────────────────────────────────────────────
//
// A character's type (PlayerGameData +0x84, saved with it; the character's live one is ChrIns
// +0x70, its team +0x74) is 0 at home (8 unembered) and a phantom's in a host's world (1 white,
// 2 dark, 12 and 13). As a player joins, the game's JoinSession step (+0x470920) keeps its live
// type as its "home type" ([+0x475C1B0]+4), and as a world unloads (+0x8F36F0) a phantom's
// player data gets that type back (+0x68FCA0). A player that joins again without going home
// (still a phantom) has a phantom's type kept as its home one, and goes home a white phantom, and
// is saved as one (verified 2026-10-08 with two games on one PC: after one rejoin the home type was
// 1; at home at Firelink Shrine it was drawn as a white phantom, team 2). World_LoadPlayerData
// (+0x8F7420) makes the character from its player data's type (after a pending one, GameDataMan
// +0x9C, the game's own way home); a loaded type not its own is set to the home type there first,
// with the game's own setter (+0x5A8110, which also updates the equipment's), and the game makes
// the character, its team and look from it (verified: the phantom at Firelink was drawn as itself
// again, team 1, and after a reload).

/// The game's restore of a phantom's player data to the home type (+0x68FCA0): the home type's
/// static at +0x14 (mov rdi, [rip]), the player data's type setter called at +0x48.
const HOME_RESTORE: &str = "48 89 5c 24 10 57 48 83 ec 20 48 8b 05 ?? ?? ?? ?? 48 8b 3d ?? ?? ?? ?? 48 8b 58 10 48 85 db 74 ?? 48 8d 54 24 30 48 8b cb e8 ?? ?? ?? ?? 8b 44 24 30 8d 50 ff 83 fa 01 76 ?? 83 c0 f4 83 f8 01 77 ?? 8b 57 04 48 8b cb e8";
/// The world's load of the player's character (World_LoadPlayerData, +0x8F7420), at its start.
const LOAD_PLAYER: &str = "48 8b c4 55 56 57 41 56 41 57 48 83 ec 70 48 c7 40 a8 fe ff ff ff 48 89 58 08 0f 29 70 c8 0f 29 78 b8 4c 8b f9 48 8d 48 10 e8 ?? ?? ?? ?? 48 8b 05 ?? ?? ?? ?? 48 8b 70 10 48 85 f6 0f 84";
/// The home type's block: the type at +4.
const HOME_TYPE_AT: usize = 4;

struct HomeFns {
    /// The static holding the home type's block.
    home: usize,
    set_data_type: usize,
}

fn home_fns() -> Option<&'static HomeFns> {
    static FOUND: OnceLock<Option<HomeFns>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let Some(at) = patterns::find_unique("home chr type", HOME_RESTORE) else {
                crate::warn!("load: the game's home chr type wasn't found; a character saved as a phantom loads as one at home");
                return None;
            };
            Some(HomeFns { home: patterns::rip_target(at + 0x14, at + 0x18)?, set_data_type: patterns::call_target(at + 0x48)? })
        })
        .as_ref()
}

/// A phantom's type (the game's restore checks the same ones).
fn phantom_type(t: i32) -> bool {
    matches!(t, 1 | 2 | 12 | 13)
}

/// The type earlier Kindle builds kept as an unembered player's own (as it went down in a session),
/// which the game never gives a player: its ember state is a byte of its player data, its type
/// stays 0. A character saved with it was type 8, team 4 at home (verified 2026-10-13); it gets
/// its own type back as a phantom's does.
const NOT_A_HOME_TYPE: i32 = 8;

/// A type the game wouldn't keep as a character's own: a phantom's, or Kindle's old one.
fn not_own_type(t: i32) -> bool {
    phantom_type(t) || t == NOT_A_HOME_TYPE
}

/// The home type the game last kept that was one (0 until then).
static HOME_TYPE: AtomicI32 = AtomicI32::new(0);

/// Each frame, answered "own": the home type the game keeps stays a home one.
pub(crate) fn frame() {
    if KEEP_HOME_TYPE.switch() != 1 {
        return;
    }
    let Some(fns) = home_fns() else { return };
    let Some(block) = memory::read::<usize>(fns.home).filter(|&p| p != 0) else { return };
    let Some(home) = memory::read::<i32>(block + HOME_TYPE_AT) else { return };
    if not_own_type(home) {
        let own = HOME_TYPE.load(Ordering::Relaxed);
        unsafe { std::ptr::write_volatile((block + HOME_TYPE_AT) as *mut i32, own) };
        crate::info!("load: the game kept a type not this character's own ({home}) as its own; it's {own} again");
    } else {
        HOME_TYPE.store(home, Ordering::Relaxed);
    }
}

/// kindle.game: chr_type.keep_home_type (catalogue.rs).
pub static KEEP_HOME_TYPE: HookPoint = HookPoint::hooked(
    "chr_type.keep_home_type",
    "The game keeps the character's type as it joins another world, to give it back at home, and a player joining again without going home has a phantom's kept: \"game\", or \"own\" (Kindle keeps the kept type a home one, the last it was, each frame). A switch.",
    &["game", "own"],
    || home_fns().map(|_| ()).ok_or_else(|| "the game's home chr type wasn't found".to_string()),
);

/// The start of a world's load of the player's character: one whose player data has a type not
/// its own, and no pending type from the game's way home, is asked about.
fn load_player_hook(_reg: *mut ilhook::x64::Registers) {
    if !PLAYER_TYPE.answered() {
        return;
    }
    let Some(fns) = home_fns() else { return };
    let Some(gdm) = crate::ds3::GAME_DATA_MAN.instance() else { return };
    if memory::read::<i32>(gdm + crate::multiplayer_flow::PENDING_TYPE_AT) != Some(-1) {
        return;
    }
    let Some(data) = memory::read_u64(gdm + 0x10).filter(|&p| p != 0).map(|p| p as usize) else { return };
    let Some(saved) = memory::read::<i32>(data + crate::game::PGD_CHR_TYPE).filter(|&t| not_own_type(t)) else { return };
    if PLAYER_TYPE.decide(&[saved as i64]) != 1 {
        return;
    }
    let own = HOME_TYPE.load(Ordering::Relaxed);
    let set: extern "C" fn(usize, i32) = unsafe { std::mem::transmute(fns.set_data_type) };
    set(data, own);
    crate::info!("load: this character was type {saved} (saved as one); it loads as itself (type {own}, a mod's answer)");
}

static LOAD_PLAYER_SITE: Site = Site {
    area: "load",
    name: "load player",
    pattern: LOAD_PLAYER,
    offset: 0,
    matches: 1,
    kind: hook_sites::Kind::JmpBack(load_player_hook),
    without: "a character saved as a phantom loads as one at home",
};

fn install_load_player() -> Result<(), String> {
    home_fns().ok_or("the game's home chr type wasn't found")?;
    LOAD_PLAYER_SITE.install().map(|_| ()).ok_or_else(|| LOAD_PLAYER_SITE.without.to_string())
}

/// kindle.game: load.player_type (catalogue.rs).
pub static PLAYER_TYPE: HookPoint = HookPoint::hooked(
    "load.player_type",
    "The player's character loads with a type the game wouldn't keep as its own (a phantom's, 1, 2, 12 or 13, or 8, which earlier Kindle builds left; the value given) and no type from the game's way home: \"game\", or \"home\" (it loads as the last home type the game kept, with its team and look). On the game's thread, at loads.",
    &["game", "home"],
    install_load_player,
);
