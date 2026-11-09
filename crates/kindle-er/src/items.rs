//! Elden Ring's items (kindle.items): given, taken and counted the game's way.
//!
//! An item is its param row's id with its category in the top four bits (weapon 0, armor 1, talisman 2,
//! goods 4, ash of war 8: EquipParamWeapon's, Protector's, Accessory's, Goods' and Gem's ids are 28 bits
//! for that). The player's inventory is the EquipInventoryData (+0x158) of the EquipGameData (+0x2B0) of
//! PlayerGameData ([GameDataMan + 8]): entries of 0x18 bytes ({the item's instance, 0 for an empty
//! entry; the item; its quantity}) that the game finds by item through an index of its own.
//!
//! Kindle gives and takes items through the function the event scripts' DirectlyGivePlayerItem calls
//! (category bits, id, count; a negative count takes, as many as are held): goods, arrows and bolts in
//! one go, what the inventory can't hold of them going on to MapItemMan [u: the storage chest], others
//! one at a time. It counts an item
//! by the game's own lookup and entry. Each is the game's code, so it runs on the game's thread, with a
//! character loaded (the game asserts without its managers).

use std::sync::OnceLock;

use kindle_core::{memory, patterns};

use crate::emevd;

/// The categories by name, with their top four bits.
pub const CATEGORIES: &[(&str, u32)] = &[("weapon", 0x0), ("armor", 0x1), ("talisman", 0x2), ("goods", 0x4), ("ash", 0x8)];
pub const ID_BITS: u32 = 0x0FFF_FFFF;
/// The most of an item given or taken at once (the game's set gives a weapon at a time).
const MOST: u32 = 999;

/// PlayerGameData in GameDataMan, EquipGameData in it, its inventory, an entry's quantity.
const PLAYER_GAME_DATA: usize = 0x08;
const EQUIP_GAME_DATA: usize = 0x2B0;
const INVENTORY: usize = 0x158;
const QUANTITY: usize = 0x08;
/// The inventory's last entry index (its entry function takes indices to it), and the most Kindle lists.
const LAST_ENTRY: usize = 0x80;
const MOST_ENTRIES: u32 = 4096;

/// The event scripts' give (category bits, id, count), which loads GameDataMan's static first: where its
/// displacement is, and where that instruction ends.
const GIVE: &str = "40 56 57 41 56 48 83 EC 50 48 C7 44 24 30 FE FF FF FF 48 89 5C 24 70 48 89 6C 24 78 41 8B F8 8B F2 48 8B 05 ?? ?? ?? ?? 48 8B 58 08 48 85 DB 0F 84 ?? ?? ?? ?? E8 ?? ?? ?? ?? 81 E6 FF FF FF 0F";
const GIVE_GAME_DATA_MAN: (usize, usize) = (0x24, 0x28);
/// The inventory's lookup (inventory, &item) -> entry index or -1, and its entry (inventory, index) -> the
/// entry or null.
const FIND: &str =
    "40 56 48 83 EC 20 83 CE FF 83 3A FF 74 ?? 48 89 5C 24 30 48 89 7C 24 38 48 8D 79 08 48 8B CF E8 ?? ?? ?? ?? 8B D8 85 C0 78 ?? 8B 4F 14 3B D9 73 ??";
const ENTRY: &str = "40 53 48 83 EC 20 8B 81 80 00 00 00 33 DB FF C0 4C 8B C1 3B D0 73 ?? 8B 41 1C 48 89 7C 24 30 3B D0 73 ??";

type GiveFn = unsafe extern "C" fn(u32, u32, i32);
type FindFn = unsafe extern "C" fn(usize, *const u32) -> i32;
type EntryFn = unsafe extern "C" fn(usize, u32) -> usize;

struct Game {
    give: GiveFn,
    find: FindFn,
    entry: EntryFn,
}

/// The give in the game's code, found once: the item functions and GameDataMan's static both start
/// from it.
fn give_at() -> Option<usize> {
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    *FOUND.get_or_init(|| patterns::find_unique("item give", GIVE))
}

/// The game's item functions, found once.
fn game() -> Result<&'static Game, String> {
    static FOUND: OnceLock<Option<Game>> = OnceLock::new();
    FOUND
        .get_or_init(|| {
            let give = give_at()?;
            let (find, entry) = (patterns::find_unique("inventory lookup", FIND)?, patterns::find_unique("inventory entry", ENTRY)?);
            // Found by their code in the game's .text: the game's own item functions.
            unsafe {
                Some(Game {
                    give: std::mem::transmute::<usize, GiveFn>(give),
                    find: std::mem::transmute::<usize, FindFn>(find),
                    entry: std::mem::transmute::<usize, EntryFn>(entry),
                })
            }
        })
        .as_ref()
        .ok_or_else(|| "the game's item functions weren't found in this build".into())
}

/// GameDataMan's static, read from the give's code (which loads it first), found once.
fn game_data_man() -> Option<usize> {
    static FOUND: OnceLock<Option<usize>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        let give = give_at()?;
        patterns::rip_target(give + GIVE_GAME_DATA_MAN.0, give + GIVE_GAME_DATA_MAN.1)
    })
}

/// The player's saved state, PlayerGameData ([GameDataMan + 8]), if the game has one.
pub(crate) fn player_game_data() -> Option<usize> {
    let man = memory::read_u64(game_data_man()?).filter(|&g| g != 0)?;
    memory::read_u64(man as usize + PLAYER_GAME_DATA).filter(|&p| p != 0).map(|p| p as usize)
}

/// An item of category `kind` ("weapon", "armor", "talisman", "goods", "ash") and row `id`.
pub fn make(kind: &str, id: i64) -> Result<u32, String> {
    let &(_, bits) = CATEGORIES.iter().find(|(k, _)| *k == kind).ok_or_else(|| format!("item category {kind:?}: weapon, armor, talisman, goods or ash"))?;
    let id = u32::try_from(id).ok().filter(|&i| i <= ID_BITS).ok_or_else(|| format!("{kind} {id}: an item's id is 28 bits"))?;
    Ok(bits << 28 | id)
}

/// An item as mods hold it (an integer): its 32 bits, with a category Elden Ring has.
pub fn check(item: i64) -> Result<u32, String> {
    let raw = u32::try_from(item).map_err(|_| format!("item {item} isn't an item"))?;
    category(raw).map(|_| raw)
}

/// An item's category name.
pub fn category(item: u32) -> Result<&'static str, String> {
    CATEGORIES.iter().find(|(_, b)| *b == item >> 28).map(|(k, _)| *k).ok_or_else(|| format!("item {item:#x} has no category Elden Ring has"))
}

/// The player's inventory, on the game's thread with a character loaded.
fn inventory() -> Result<usize, String> {
    crate::task::on_games_thread("items are changed and counted")?;
    let loaded = crate::player::main_player()?.is_some();
    match (loaded, player_game_data(), crate::er::instance("CS::MapItemMan")) {
        (true, Some(pgd), Some(_)) => Ok(pgd + EQUIP_GAME_DATA + INVENTORY),
        _ => Err("no character is loaded".into()),
    }
}

fn quantity(qty: i64) -> Result<i32, String> {
    u32::try_from(qty).ok().filter(|q| (1..=MOST).contains(q)).map(|q| q as i32).ok_or_else(|| format!("quantity {qty}: 1 to {MOST}"))
}

/// Gives the player `qty` of `item` as the event scripts do (no pop-up).
pub fn give(item: u32, qty: i64) -> Result<(), String> {
    let (qty, game) = (quantity(qty)?, game()?);
    inventory()?;
    unsafe { (game.give)(item & 0xF000_0000, item & ID_BITS, qty) };
    Ok(())
}

/// Kindle's ItemLotParam_map row for pop-ups (above the game's last, 2054400000, with nothing after
/// it), made from the game's row 10000 (one item, the Talisman Pouch) the first time.
const POPUP_LOT: u64 = 2_100_000_000;
const POPUP_TEMPLATE: u64 = 10000;
const LOTS: &str = "ItemLotParam_map";
/// A lot's item categories (as the game's own lots use them), and the most of one item a lot gives.
const LOT_CATEGORIES: [(&str, i64); 5] = [("goods", 1), ("weapon", 2), ("armor", 3), ("talisman", 4), ("ash", 5)];
const LOT_MOST: i32 = 255;

/// Fills Kindle's lot with `qty` of `item` (on the game's thread, as its award is about to run).
fn fill_popup_lot(item: u32, qty: i32) -> Result<(), String> {
    if !kindle_core::params::registry()?.table(LOTS)?.has_row(POPUP_LOT) {
        crate::param_rows::add_rows("kindle", LOTS, &[(POPUP_LOT, POPUP_TEMPLATE)])?;
    }
    let kind = category(item)?;
    let lot_category = LOT_CATEGORIES.iter().find(|(k, _)| *k == kind).map(|&(_, c)| c).ok_or_else(|| format!("a lot can't give {kind}"))?;
    let registry = kindle_core::params::registry()?;
    let lots = registry.table(LOTS)?;
    let fields = [
        ("lotItemId01", (item & ID_BITS) as i64),
        ("lotItemCategory01", lot_category),
        ("lotItemNum01", qty as i64),
        ("lotItemBasePoint01", 1000),
        ("getItemFlagId", 0),
    ];
    for (field, value) in fields {
        lots.write(POPUP_LOT, field, &kindle_paramdef::Value::Int(value))?;
    }
    Ok(())
}

/// Gives the player `qty` of `item` with the game's "item received" box, as a map's item does: Kindle's
/// lot filled with it and awarded by Award Item Lot (2003:4), on Kindle's next frame; one award per 255.
/// The commands, the last one's id the reply's.
pub fn give_with_popup(item: u32, qty: i64, owner: &str) -> Result<Vec<emevd::Command>, String> {
    let mut left = quantity(qty)?;
    category(item)?;
    inventory()?;
    let args = kindle_core::emevd::pack("i", &[POPUP_LOT as f64])?;
    let mut out = Vec::new();
    while left > 0 {
        let n = left.min(LOT_MOST);
        left -= n;
        let prepare: emevd::Prepare = Box::new(move || fill_popup_lot(item, n));
        out.push(emevd::Command { id: emevd::next_id(), owner: owner.into(), bank: 2003, index: 4, args: args.clone(), prepare: Some(prepare) });
    }
    Ok(out)
}

/// Takes `qty` of `item` from the player (as many as they have, up to that).
pub fn remove(item: u32, qty: i64) -> Result<(), String> {
    let (qty, game) = (quantity(qty)?, game()?);
    inventory()?;
    unsafe { (game.give)(item & 0xF000_0000, item & ID_BITS, -qty) };
    Ok(())
}

/// How many of `item` the player holds: the quantity of the entry the game's lookup finds (a weapon or
/// armour piece counts 1 [u: its copies]).
pub fn count(item: u32) -> Result<u32, String> {
    let game = game()?;
    let inv = inventory()?;
    let index = unsafe { (game.find)(inv, &item) };
    if index < 0 {
        return Ok(0);
    }
    let entry = unsafe { (game.entry)(inv, index as u32) };
    if entry == 0 {
        return Ok(0);
    }
    memory::read::<u32>(entry + QUANTITY).ok_or_else(|| format!("item {item:#x}'s entry is unreadable"))
}

/// What the player holds: each entry's item and quantity, in the inventory's order (the game's own
/// entry function for every index).
pub fn held() -> Result<Vec<(u32, u32)>, String> {
    let game = game()?;
    let inv = inventory()?;
    let last = memory::read::<u32>(inv + LAST_ENTRY).ok_or("the inventory is unreadable")?.min(MOST_ENTRIES);
    let mut out = Vec::new();
    for i in 0..=last {
        let entry = unsafe { (game.entry)(inv, i) };
        // {the item's instance (0: empty), the item, its quantity}
        if let Some([instance, item, qty]) = (entry != 0).then(|| memory::read::<[u32; 3]>(entry)).flatten()
            && instance != 0
            && item != u32::MAX
        {
            out.push((item, qty));
        }
    }
    Ok(out)
}

/// The natives for kindle.items.
pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::rt_err;
    n.set("item_make", lua.create_function(|_, (kind, id): (String, i64)| make(&kind, id).map_err(rt_err))?)?;
    n.set("item_give", lua.create_function(|_, (item, qty): (i64, i64)| give(check(item).map_err(rt_err)?, qty).map_err(rt_err))?)?;
    n.set(
        "item_give_popup",
        lua.create_function(|lua, (item, qty, reply): (i64, i64, Option<mlua::Function>)| {
            let owner = kindle_core::mods::ctx(lua)?.id.clone();
            let mut last = 0;
            for cmd in give_with_popup(check(item).map_err(rt_err)?, qty, &owner).map_err(rt_err)? {
                last = cmd.id;
                emevd::run(cmd).map_err(rt_err)?;
            }
            if let Some(f) = reply {
                kindle_core::mods::ctx(lua)?.handlers.replies.push((last, f));
            }
            Ok(last)
        })?,
    )?;
    n.set("item_remove", lua.create_function(|_, (item, qty): (i64, i64)| remove(check(item).map_err(rt_err)?, qty).map_err(rt_err))?)?;
    n.set("item_count", lua.create_function(|_, item: i64| count(check(item).map_err(rt_err)?).map_err(rt_err))?)?;
    n.set(
        "item_info",
        lua.create_function(|_, item: i64| {
            let item = check(item).map_err(rt_err)?;
            Ok((category(item).map_err(rt_err)?, item & ID_BITS))
        })?,
    )?;
    n.set(
        "item_held",
        lua.create_function(|lua, ()| {
            let out = lua.create_table()?;
            for (item, qty) in held().map_err(rt_err)? {
                let t = lua.create_table()?;
                t.set("item", item)?;
                t.set("kind", category(item).unwrap_or("?"))?;
                t.set("id", item & ID_BITS)?;
                t.set("quantity", qty)?;
                out.push(t)?;
            }
            Ok(out)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_carry_their_category_in_their_top_bits() {
        assert_eq!(make("weapon", 1_000_000), Ok(1_000_000));
        assert_eq!(make("armor", 40_000), Ok(0x1000_0000 | 40_000));
        assert_eq!(make("talisman", 1000), Ok(0x2000_0000 | 1000));
        assert_eq!(make("goods", 1000), Ok(0x4000_0000 | 1000));
        assert_eq!(make("ash", 10_000), Ok(0x8000_0000 | 10_000));
        assert!(make("ring", 1).unwrap_err().contains("talisman"));
        assert!(make("goods", 1 << 28).unwrap_err().contains("28 bits"));
        assert_eq!(category(0x4000_03E8), Ok("goods"));
        assert!(check(0x3000_0000).is_err() && check(-1).is_err() && check(0x8000_2710).is_ok());
        assert!(quantity(0).is_err() && quantity(1000).is_err() && quantity(999) == Ok(999));
    }

    /// The game's give, inventory lookup and entry, each found once in Elden Ring 2.7.0.0's code
    /// (KINDLE_ER_EXE), and GameDataMan's static from the give's code.
    #[test]
    fn the_games_item_functions_are_found() {
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
        assert_eq!(find(GIVE), [0x5EB7D0]);
        assert_eq!(find(FIND), [0x24C560]);
        assert_eq!(find(ENTRY), [0x24E770]);
        let disp = i32::from_le_bytes(img[0x5EB7D0 + GIVE_GAME_DATA_MAN.0..0x5EB7D0 + GIVE_GAME_DATA_MAN.0 + 4].try_into().unwrap());
        assert_eq!((0x5EB7D0 + GIVE_GAME_DATA_MAN.1).wrapping_add_signed(disp as isize), 0x3D61F98);
    }
}
