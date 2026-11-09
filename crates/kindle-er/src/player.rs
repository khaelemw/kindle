//! The player (kindle.player): the character in the world and its saved state, read and set where the
//! game's own code reads and writes them.
//!
//! WorldChrMan's main player (+0x1E508, null with no character in the world) is a character (ChrIns):
//! its modules at +0x190, the first its data module (CSChrDataModule: HP +0x138, FP +0x148 and stamina
//! +0x154, each followed by its maximum), the one at +0x68 its physics module (its position at +0x70:
//! x, y up, z [u: in the coordinates of the map it is in, not the world's]). Its saved state is
//! PlayerGameData ([GameDataMan + 8]): the attributes from +0x3C (vigor, mind, endurance, strength,
//! dexterity, intelligence, faith, arcane: the game's stat setter's 0 to 7), the level +0x68, the runes
//! held +0x6C (0 to 999,999,999), the runes gained in all +0x70, the name +0x9C (16 UTF-16
//! characters). GameMan's +0xAC0 is the save slot (0 to 9, -1 for none).
//!
//! Each offset is checked once against the game's code that uses it (the stat setter's cases, the
//! level's read, the name's set, the main player's read with its data module, HP's share of its
//! maximum, FP's and stamina's sets, the position's read through the physics module, the save slot's
//! read), and the player is refused if any of it isn't found. Kindle reads the player from any thread
//! and changes it on the game's thread only, within the bounds the game's own sets keep each value in
//! (HP from 1: Kindle doesn't kill). The game works out the maxima from the attributes itself [u: when
//! an attribute changes; checked in game].

use std::sync::OnceLock;

use kindle_core::{memory, patterns};

/// The attributes in PlayerGameData, in the order of the game's stat setter.
pub const ATTRIBUTES: [(&str, usize); 8] =
    [("vigor", 0x3C), ("mind", 0x40), ("endurance", 0x44), ("strength", 0x48), ("dexterity", 0x4C), ("intelligence", 0x50), ("faith", 0x54), ("arcane", 0x58)];
/// The attributes as the game's menus name them.
pub const ATTRIBUTE_LABELS: [(&str, &str); 8] = [
    ("vigor", "Vigor"),
    ("mind", "Mind"),
    ("endurance", "Endurance"),
    ("strength", "Strength"),
    ("dexterity", "Dexterity"),
    ("intelligence", "Intelligence"),
    ("faith", "Faith"),
    ("arcane", "Arcane"),
];
/// Dark Souls III's names for two of them, for mods written for both.
const ALIASES: [(&str, &str); 2] = [("attunement", "mind"), ("luck", "arcane")];
const LEVEL: usize = 0x68;
const RUNES: usize = 0x6C;
const TOTAL_RUNES: usize = 0x70;
const NAME: usize = 0x9C;
const NAME_CHARS: usize = 16;
const MOST_RUNES: i64 = 999_999_999;
const MOST_LEVEL: i64 = 713;
const MOST_ATTRIBUTE: i64 = 99;

/// WorldChrMan's main player, a character's modules, the physics module among them, its position.
const MAIN_PLAYER: usize = 0x1E508;
const MODULES: usize = 0x190;
const PHYSICS: usize = 0x68;
const POSITION: usize = 0x70;
/// HP, FP and stamina in the data module (each one's maximum the value after it), and the least
/// Kindle sets each to.
const POOLS: [(&str, usize, i32); 3] = [("hp", 0x138, 1), ("fp", 0x148, 0), ("stamina", 0x154, 0)];
const MAX: usize = 4;
/// GameMan's save slot (-1 for none), and how many there are.
const SAVE_SLOT: usize = 0xAC0;
const SLOTS: i32 = 10;

/// The game's stat setter's cases 0 to 7 (each writes its attribute) and 8, which goes on to the
/// runes' set: made from ATTRIBUTES.
fn stat_cases() -> String {
    let mut p: String = ATTRIBUTES.iter().map(|(_, off)| format!("89 7B {off:02X} 48 8B 5C 24 38 48 83 C4 20 5F C3 ")).collect();
    p.push_str("8B D7 48 8B CB 48 8B 5C 24 38 48 83 C4 20 5F E9 ?? ?? ?? ??");
    p
}
/// Where the cases' jump to the runes' set keeps its displacement, and where it ends.
const STAT_CASES_RUNES: (usize, usize) = (0x80, 0x84);
/// The runes' set (PlayerGameData, runes): 0 below 0, at most 999,999,999, at +0x6C.
const RUNES_SET: &str = "89 54 24 10 C7 44 24 08 00 00 00 00 85 D2 79 ?? 48 8D 44 24 08 8B 00 89 41 6C C3";
/// The stat getter's level (+0x68), before the case that gives 99.
const LEVEL_GET: &str = "44 8B 43 68 41 8B C0 48 83 C4 20 5B C3 41 B8 63 00 00 00 41 8B C0";
/// The name's set: at most 16 characters, copied to +0x9C (8 + 0x94).
const NAME_SET: &str = "48 89 5C 24 08 57 48 83 EC 20 48 8B D9 49 83 C9 FF 49 FF C1 66 42 83 3C 4A 00 75 ?? 41 83 F9 10 7F ?? 48 83 C1 08 48 2B CA 0F 1F 80 00 00 00 00 0F B7 02 66 89 84 11 94 00 00 00";
/// WorldChrMan's main player (+0x1E508) and its data module (the first of its modules, +0x190), read
/// to ask whether it's dead; it loads WorldChrMan's static first.
const MAIN_PLAYER_READ: &str = "48 8B 05 ?? ?? ?? ?? 48 85 C0 74 ?? 48 8B B0 08 E5 01 00 48 85 F6 74 ?? 48 8B 86 90 01 00 00 48 8B 08 E8";
/// HP's share of its maximum (+0x138 / +0x13C).
const HP_SHARE: &str = "66 0F 6E 81 38 01 00 00 66 0F 6E 89 3C 01 00 00 0F 5B C0 0F 5B C9 F3 0F 5E C1";
/// FP's set (+0x148) and its bound (0 to +0x14C).
const FP_SET: &str = "48 89 5C 24 18 48 89 74 24 20 89 54 24 10 57 48 83 EC 30 33 DB 8B F2 48 8B F9 89 5C 24 20 3B 91 48 01 00 00 7D ?? F6 81 9B 01 00 00 04";
const FP_BOUND: &str = "32 C0 F6 C3 01 74 ?? 83 E3 FE 89 5C 24 20 84 C0 75 ?? C7 44 24 40 00 00 00 00 48 8D 8F 4C 01 00 00 85 F6 79 ??";
/// Stamina's set (+0x154) and its bound (-50 to +0x158).
const STAMINA_SET: &str =
    "48 89 5C 24 18 48 89 74 24 20 89 54 24 10 57 48 83 EC 30 33 DB 8B F2 48 8B F9 89 5C 24 20 3B 91 54 01 00 00 7D ?? F6 81 9A 01 00 00 40 74 ??";
const STAMINA_BOUND: &str = "C7 44 24 40 CE FF FF FF 48 8D 8F 58 01 00 00 83 FE CE 7D ??";
/// A character's position read through its physics module (+0x68 of its modules): the call goes to the
/// physics module's read of its position (+0x70), whose code is POSITION_GET.
const POSITION_READ: &str = "40 53 48 83 EC 20 48 8B 41 10 48 8B DA 48 8B 88 90 01 00 00 48 8B 49 68 E8 ?? ?? ?? ??";
const POSITION_READ_CALL: usize = 24;
const POSITION_GET: [u8; 11] = [0x0F, 0x10, 0x41, 0x70, 0x48, 0x8B, 0xC2, 0x0F, 0x29, 0x02, 0xC3];
/// The save slot's read (GameMan's +0xAC0), which loads GameMan's static.
const SAVE_SLOT_GET: &str = "48 8B 05 ?? ?? ?? ?? 8B 80 C0 0A 00 00 C3";

/// What the layout's checks found: WorldChrMan's and GameMan's statics.
struct Layout {
    world_chr_man: usize,
    game_man: usize,
}

/// The layout, checked once against the game's code.
fn layout() -> Result<&'static Layout, String> {
    static FOUND: OnceLock<Result<Layout, String>> = OnceLock::new();
    FOUND.get_or_init(check_layout).as_ref().map_err(|e| e.clone())
}

fn check_layout() -> Result<Layout, String> {
    let missing = |what: &str| format!("Kindle doesn't know where this build keeps the player's {what}");
    let cases = patterns::find_unique("player stat set", &stat_cases()).ok_or_else(|| missing("attributes"))?;
    let runes = patterns::find_unique("player runes set", RUNES_SET).ok_or_else(|| missing("runes"))?;
    if patterns::rip_target(cases + STAT_CASES_RUNES.0, cases + STAT_CASES_RUNES.1) != Some(runes) {
        return Err(missing("runes"));
    }
    let checks = [
        ("level", "player level read", LEVEL_GET),
        ("name", "player name set", NAME_SET),
        ("HP", "player HP share", HP_SHARE),
        ("FP", "player FP set", FP_SET),
        ("FP", "player FP bound", FP_BOUND),
        ("stamina", "player stamina set", STAMINA_SET),
        ("stamina", "player stamina bound", STAMINA_BOUND),
    ];
    for (what, name, pattern) in checks {
        patterns::find_unique(name, pattern).ok_or_else(|| missing(what))?;
    }
    let main = patterns::find_unique("main player read", MAIN_PLAYER_READ).ok_or_else(|| missing("character"))?;
    let world_chr_man = patterns::lea_target(main).filter(|&s| Some(s) == crate::er::static_of("CS::WorldChrMan")).ok_or_else(|| missing("character"))?;
    let reads_position =
        |at: usize| patterns::call_target(at + POSITION_READ_CALL).and_then(|t| memory::read_bytes(t, POSITION_GET.len())).is_some_and(|b| b == POSITION_GET);
    patterns::find_where("player position read", POSITION_READ, reads_position).ok_or_else(|| missing("position"))?;
    let slot = patterns::find_unique("save slot read", SAVE_SLOT_GET).ok_or_else(|| missing("save slot"))?;
    let game_man = patterns::lea_target(slot).ok_or_else(|| missing("save slot"))?;
    Ok(Layout { world_chr_man, game_man })
}

/// A player a test laid out (its character, saved state and save slot), read instead of the game's; a
/// character of 0 for none in the world.
#[cfg(test)]
pub(crate) static TEST_PLAYER: std::sync::Mutex<Option<(usize, usize, i32)>> = std::sync::Mutex::new(None);

/// The player's character, None with no character in the world.
pub(crate) fn main_player() -> Result<Option<usize>, String> {
    #[cfg(test)]
    if let Some((chr, _, _)) = *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) {
        return Ok((chr != 0).then_some(chr));
    }
    let world = memory::read_u64(layout()?.world_chr_man).filter(|&w| w != 0);
    Ok(world.and_then(|w| memory::read_u64(w as usize + MAIN_PLAYER)).filter(|&p| p != 0).map(|p| p as usize))
}

/// The player's saved state, if the game has one.
fn game_data() -> Option<usize> {
    #[cfg(test)]
    if let Some((_, pgd, _)) = *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) {
        return (pgd != 0).then_some(pgd);
    }
    crate::items::player_game_data()
}

/// Whether a character is in the world (with its saved state).
pub fn is_loaded() -> Result<bool, String> {
    Ok(main_player()?.is_some() && game_data().is_some())
}

/// The loaded player's character and saved state.
fn loaded() -> Result<(usize, usize), String> {
    match (main_player()?, game_data()) {
        (Some(chr), Some(pgd)) => Ok((chr, pgd)),
        _ => Err("no character is loaded".into()),
    }
}

/// The module at `slot` of the player's character's modules.
fn module(slot: usize) -> Result<usize, String> {
    let (chr, _) = loaded()?;
    let modules = memory::read_u64(chr + MODULES).filter(|&m| m != 0).ok_or("the player's character has no modules yet")?;
    memory::read_u64(modules as usize + slot).filter(|&m| m != 0).map(|m| m as usize).ok_or_else(|| "the player's character isn't made yet".into())
}

fn read<T: Copy + Default>(addr: usize) -> Result<T, String> {
    memory::read::<T>(addr).ok_or_else(|| "the player is unreadable".into())
}

/// Writes a value of the player's, on the game's thread.
fn write<T: Copy + Default>(addr: usize, value: T) -> Result<(), String> {
    crate::task::on_games_thread("the player")?;
    read::<T>(addr)?;
    unsafe { std::ptr::write_volatile(addr as *mut T, value) };
    Ok(())
}

pub fn name() -> Result<String, String> {
    let (_, pgd) = loaded()?;
    memory::read_wstr(pgd + NAME, NAME_CHARS).ok_or_else(|| "the player's name is unreadable".into())
}

/// An attribute's offset, by its name or Dark Souls III's.
fn attribute(name: &str) -> Result<usize, String> {
    let name = ALIASES.iter().find(|(a, _)| *a == name).map_or(name, |(_, n)| n);
    ATTRIBUTES.iter().find(|(n, _)| *n == name).map(|&(_, off)| off).ok_or_else(|| {
        let names: Vec<&str> = ATTRIBUTES.iter().map(|(n, _)| *n).collect();
        format!("attribute {name:?}: {}", names.join(", "))
    })
}

/// The attributes by name, in the game's order.
pub fn stats() -> Result<Vec<(&'static str, i32)>, String> {
    let (_, pgd) = loaded()?;
    ATTRIBUTES.iter().map(|&(n, off)| Ok((n, read::<i32>(pgd + off)?))).collect()
}

/// Sets an attribute (1 to 99). Unless `keep_level`, the level moves by as much.
pub fn set_attribute(name: &str, value: i64, keep_level: bool) -> Result<(), String> {
    let off = attribute(name)?;
    let (_, pgd) = loaded()?;
    let (old, new) = (read::<i32>(pgd + off)?, value.clamp(1, MOST_ATTRIBUTE) as i32);
    write(pgd + off, new)?;
    if !keep_level {
        set_level(read::<i32>(pgd + LEVEL)? as i64 + (new - old) as i64)?;
    }
    Ok(())
}

pub fn level() -> Result<i32, String> {
    read(loaded()?.1 + LEVEL)
}

/// Sets the level (1 to 713), the attributes as they are.
pub fn set_level(level: i64) -> Result<(), String> {
    write(loaded()?.1 + LEVEL, level.clamp(1, MOST_LEVEL) as i32)
}

pub fn runes() -> Result<i32, String> {
    read(loaded()?.1 + RUNES)
}

/// Sets the runes held (0 to 999,999,999, as the game's set keeps them).
pub fn set_runes(runes: i64) -> Result<(), String> {
    write(loaded()?.1 + RUNES, runes.clamp(0, MOST_RUNES) as i32)
}

pub fn total_runes() -> Result<u32, String> {
    read(loaded()?.1 + TOTAL_RUNES)
}

/// HP, FP or stamina ("hp", "max_hp" ...): its offset in the data module, its least, and whether it's
/// the maximum.
fn pool(what: &str) -> Result<(usize, i32, bool), String> {
    let (name, max) = what.strip_prefix("max_").map_or((what, false), |n| (n, true));
    POOLS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map(|&(_, off, least)| (off, least, max))
        .ok_or_else(|| format!("{what:?}: hp, fp or stamina, or max_ any of them"))
}

pub fn get(what: &str) -> Result<i32, String> {
    let (off, _, max) = pool(what)?;
    read(module(0)? + off + if max { MAX } else { 0 })
}

/// Sets HP, FP or stamina, within its least and its maximum (the maxima follow the attributes).
pub fn set(what: &str, value: i64) -> Result<(), String> {
    let (off, least, max) = pool(what)?;
    if max {
        return Err(format!("{what} follows the attributes: set them instead"));
    }
    let data = module(0)?;
    let most = read::<i32>(data + off + MAX)?.max(least);
    write(data + off, value.clamp(least as i64, most as i64) as i32)
}

/// The player's position: x, y (up), z, in the physics module's frame. It is shifted from the map's (MSB)
/// coordinates: the Stranded Graveyard's start point at y 13.8 reads 5.82 there [k: live, ER 2.7.0.0].
pub fn position() -> Result<(f32, f32, f32), String> {
    let [x, y, z, _] = read::<[f32; 4]>(module(PHYSICS)? + POSITION)?;
    Ok((x, y, z))
}

/// Moves the player to x, y (up), z, in the coordinates `position` gives: the physics module's position,
/// which the character follows from there (falling if nothing is under it) [k: live, ER 2.7.0.0]. Moves made
/// relative to a read land where meant.
pub fn set_position(x: f32, y: f32, z: f32) -> Result<(), String> {
    if ![x, y, z].iter().all(|v| v.is_finite()) {
        return Err("player.set_position: x, y and z are numbers".into());
    }
    let at = module(PHYSICS)? + POSITION;
    let [_, _, _, w] = read::<[f32; 4]>(at)?;
    write(at, [x, y, z, w])
}

/// The save slot the loaded character is in (None with no character loaded).
pub fn save_slot() -> Result<Option<i32>, String> {
    if !is_loaded()? {
        return Ok(None);
    }
    #[cfg(test)]
    if let Some((_, _, slot)) = *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) {
        return Ok((0..SLOTS).contains(&slot).then_some(slot));
    }
    let game_man = memory::read_u64(layout()?.game_man).filter(|&g| g != 0).ok_or("the game's save slot isn't there yet")?;
    let slot = read::<i32>(game_man as usize + SAVE_SLOT)?;
    Ok((0..SLOTS).contains(&slot).then_some(slot))
}

/// The natives for kindle.player.
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::rt_err;
    n.set("player_loaded", lua.create_function(|_, ()| is_loaded().map_err(rt_err))?)?;
    n.set("player_name", lua.create_function(|_, ()| name().map_err(rt_err))?)?;
    n.set(
        "player_stats",
        lua.create_function(|lua, ()| {
            let t = lua.create_table()?;
            for (name, value) in stats().map_err(rt_err)? {
                t.set(name, value)?;
            }
            Ok(t)
        })?,
    )?;
    n.set(
        "player_set_attribute",
        lua.create_function(|_, (name, value, keep): (String, i64, Option<bool>)| set_attribute(&name, value, keep.unwrap_or(false)).map_err(rt_err))?,
    )?;
    n.set("player_level", lua.create_function(|_, ()| level().map_err(rt_err))?)?;
    n.set("player_set_level", lua.create_function(|_, level: i64| set_level(level).map_err(rt_err))?)?;
    n.set("player_runes", lua.create_function(|_, ()| runes().map_err(rt_err))?)?;
    n.set("player_set_runes", lua.create_function(|_, runes: i64| set_runes(runes).map_err(rt_err))?)?;
    n.set("player_total_runes", lua.create_function(|_, ()| total_runes().map_err(rt_err))?)?;
    n.set("player_get", lua.create_function(|_, what: String| get(&what).map_err(rt_err))?)?;
    n.set("player_set", lua.create_function(|_, (what, value): (String, i64)| set(&what, value).map_err(rt_err))?)?;
    n.set("player_position", lua.create_function(|_, ()| position().map_err(rt_err))?)?;
    n.set("player_set_position", lua.create_function(|_, (x, y, z): (f32, f32, f32)| set_position(x, y, z).map_err(rt_err))?)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A player laid out as the game's, served to Kindle (TEST_PLAYER) until dropped: a character with
    /// its modules (the data module first, the physics module at +0x68) and a saved state.
    pub(crate) struct Player {
        pub(crate) chr: Vec<u64>,
        pub(crate) modules: Vec<u64>,
        pub(crate) data: Vec<i32>,
        pub(crate) physics: Vec<f32>,
        pub(crate) pgd: Vec<i32>,
    }

    impl Player {
        pub(crate) fn serve(slot: i32) -> Self {
            let mut p =
                Self { chr: vec![0; 0x200 / 8], modules: vec![0; 0x80 / 8], data: vec![0; 0x280 / 4], physics: vec![0.0; 0x80 / 4], pgd: vec![0; 0xC0 / 4] };
            p.modules[0] = p.data.as_ptr() as u64;
            p.modules[PHYSICS / 8] = p.physics.as_ptr() as u64;
            p.chr[MODULES / 8] = p.modules.as_ptr() as u64;
            for (i, (_, off)) in ATTRIBUTES.iter().enumerate() {
                p.pgd[off / 4] = 10 + i as i32;
            }
            p.pgd[LEVEL / 4] = 9;
            p.pgd[RUNES / 4] = 1234;
            p.pgd[TOTAL_RUNES / 4] = 5678;
            for (i, c) in "Tarnished".encode_utf16().enumerate() {
                let at = NAME / 2 + i;
                let w = &mut p.pgd[at / 2];
                *w = if at.is_multiple_of(2) { (*w & !0xFFFF) | c as i32 } else { (*w & 0xFFFF) | (c as i32) << 16 };
            }
            for (i, v) in [(0x138, 400), (0x13C, 500), (0x148, 50), (0x14C, 80), (0x154, 90), (0x158, 100)] {
                p.data[i / 4] = v;
            }
            p.physics[POSITION / 4..POSITION / 4 + 3].copy_from_slice(&[1.5, -2.0, 300.25]);
            *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) = Some((p.chr.as_ptr() as usize, p.pgd.as_ptr() as usize, slot));
            p
        }
    }

    impl Drop for Player {
        fn drop(&mut self) {
            *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    /// No character in the world, served to Kindle until dropped (the title screen).
    pub(crate) struct NoPlayer;

    impl NoPlayer {
        pub(crate) fn serve() -> Self {
            *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) = Some((0, 0, -1));
            Self
        }
    }

    impl Drop for NoPlayer {
        fn drop(&mut self) {
            *TEST_PLAYER.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    #[test]
    fn the_player_is_read_and_set_where_the_game_keeps_it() {
        let _one = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let p = Player::serve(3);
        assert_eq!(is_loaded(), Ok(true));
        assert_eq!(name().as_deref(), Ok("Tarnished"));
        assert_eq!(save_slot(), Ok(Some(3)));
        let stats = stats().unwrap();
        assert_eq!(stats[0], ("vigor", 10));
        assert_eq!(stats[7], ("arcane", 17));
        assert_eq!((level(), runes(), total_runes()), (Ok(9), Ok(1234), Ok(5678)));
        assert_eq!((get("hp"), get("max_hp"), get("fp"), get("max_stamina")), (Ok(400), Ok(500), Ok(50), Ok(100)));
        assert_eq!(position(), Ok((1.5, -2.0, 300.25)));
        set_position(10.0, 20.5, -3.0).unwrap();
        assert_eq!(position(), Ok((10.0, 20.5, -3.0)));
        assert!(set_position(f32::NAN, 0.0, 0.0).is_err() && position() == Ok((10.0, 20.5, -3.0)));
        // Values are kept within the game's bounds; the level follows an attribute unless kept.
        set("hp", 0).unwrap();
        set("fp", 1000).unwrap();
        set("stamina", -5).unwrap();
        assert_eq!((p.data[0x138 / 4], p.data[0x148 / 4], p.data[0x154 / 4]), (1, 80, 0));
        assert!(set("max_hp", 9).unwrap_err().contains("attributes") && get("poise").is_err());
        set_attribute("strength", 20, false).unwrap();
        assert_eq!((p.pgd[0x48 / 4], p.pgd[LEVEL / 4]), (20, 9 + 7));
        set_attribute("luck", 120, true).unwrap();
        assert_eq!((p.pgd[0x58 / 4], p.pgd[LEVEL / 4]), (99, 16));
        assert!(set_attribute("vitality", 10, false).unwrap_err().contains("arcane"));
        set_runes(2_000_000_000).unwrap();
        assert_eq!(runes(), Ok(999_999_999));
        set_level(0).unwrap();
        assert_eq!(level(), Ok(1));
        drop(p);
        let p = Player::serve(-1);
        assert_eq!(save_slot(), Ok(None));
        drop(p);
        let none = NoPlayer::serve();
        assert_eq!((is_loaded(), save_slot()), (Ok(false), Ok(None)));
        assert!(name().unwrap_err().contains("no character"));
        drop(none);
    }

    /// Each place the layout is checked against, found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE),
    /// with what it reads through.
    #[test]
    fn the_players_layout_is_the_games() {
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
        let rel = |disp_at: usize, next: usize| next.wrapping_add_signed(i32::from_le_bytes(img[disp_at..disp_at + 4].try_into().unwrap()) as isize);
        assert_eq!(find(&stat_cases()), [0xEA1F58]);
        assert_eq!(rel(0xEA1F58 + STAT_CASES_RUNES.0, 0xEA1F58 + STAT_CASES_RUNES.1), 0x5B0B70);
        assert_eq!(find(RUNES_SET), [0x5B0B70]);
        assert_eq!(find(LEVEL_GET), [0xEA1C31]);
        assert_eq!(find(NAME_SET), [0x2610D0]);
        assert_eq!(find(HP_SHARE), [0x4377C0]);
        assert_eq!((find(FP_SET), find(FP_BOUND)), (vec![0x438900], vec![0x438976]));
        assert_eq!((find(STAMINA_SET), find(STAMINA_BOUND)), (vec![0x4389F0], vec![0x438A8A]));
        assert_eq!(find(MAIN_PLAYER_READ), [0x767AF6]);
        assert_eq!(rel(0x767AF6 + 3, 0x767AF6 + 7), 0x3D69FF8);
        let position: Vec<usize> = find(POSITION_READ)
            .into_iter()
            .filter(|&at| {
                let target = rel(at + POSITION_READ_CALL + 1, at + POSITION_READ_CALL + 5);
                img[target..target + POSITION_GET.len()] == POSITION_GET
            })
            .collect();
        assert_eq!(position, [0x3C7000]);
        assert_eq!(find(SAVE_SLOT_GET), [0x679AF0]);
        assert_eq!(rel(0x679AF0 + 3, 0x679AF0 + 7), 0x3D6D988);
    }
}
