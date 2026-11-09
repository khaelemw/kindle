//! Equipping items through the game's own equip functions (the same path as the menu).
//!
//! All three functions were located in the decrypted 1.15.0 exe (calls studied from
//! DS3RuntimeScripting), turned into patterns with relative operands wildcarded, and matched
//! exactly once in 1.15.2 (each shifted by +0x1A90 or so):
//!
//! | | 1.15.0 | 1.15.2 | signature |
//! |---|---|---|---|
//! | switch equipment | `+0x5886A0` | `+0x58A130` | `(EquipGameData*, slot, inventory index, holder*, refresh: u8)` |
//! | register handle | `+0x8611C0` | `+0x869B70` | `(EquipGameData + 0x88, slot, &gaitem handle)` |
//! | equip goods | `+0x583ED0` | `+0x585960` | `(EquipGameData*, slot, data*, inventory index, 1)` |
//!
//! `switch equipment` first invokes a small callable stored in a 0x20-byte holder (`+0x18`
//! points at it; the game builds it inside the holder): its vtable `+0x10` entry registers
//! the item's gaitem handle for the slot. Kindle supplies its own callable that makes that one
//! call, then the switch stores the inventory index and, with `refresh`, updates the
//! character's equipment (the game always passes 1).
//!
//! Inventory indexes count key items first, then normal items (`EquipGameData + 0x1A8` is the
//! inventory). Empty weapon slots hold Fists and empty armour slots the bare pieces, so
//! unequipping means equipping those.

use std::sync::OnceLock;

use crate::ds3::{EquipGameData, ItemCategory, ItemId};
use kindle_paramdef::Value;

use crate::{game, memory, patterns};

const SWITCH_PATTERN: &str = "40 57 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 40 48 89 6c 24 48 \
    48 89 74 24 50 41 8b f0 48 63 da 48 8b f9 49 8b 49 18 48 85 c9 75 06 e8 ?? ?? ?? ?? cc 48 8b 01 \
    ff 50 10 83 fb 0a 77 3d ba fe ff ff ff 48 8d 8f 88 00 00 00 e8 ?? ?? ?? ?? 3b d8 74 13 83 ca ff \
    48 8d 8f 88 00 00 00";
const HANDLE_PATTERN: &str = "40 57 48 83 ec 40 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 50 48 89 6c 24 60 \
    48 89 74 24 68 49 8b d8 48 63 fa 48 8b f1 48 83 c1 24 48 8d 0c b9 49 8b d0 e8 ?? ?? ?? ?? 48 8b d3 \
    48 8d 4c 24 28 e8 ?? ?? ?? ?? 90 48 8d 54 24 58 48 8b c8 e8 ?? ?? ?? ?? 8b 08 81 e1 ff ff ff 0f \
    83 c8 ff 81 f9 ff";
const GOODS_PATTERN: &str = "83 fa 0a 73 0f 48 81 c1 50 02 00 00 45 8b c1 e9 ?? ?? ?? ?? 83 fa 0f 73 0f \
    83 c2 f6 48 81 c1 50 02 00 00 e9 ?? ?? ?? ?? 83 fa 10 73 08 83 c2 f1 e9 ?? ?? ?? ??";

struct Functions {
    switch: usize,
    handle: usize,
    goods: usize,
}

static FUNCTIONS: OnceLock<Option<Functions>> = OnceLock::new();

fn functions() -> Result<&'static Functions, String> {
    FUNCTIONS
        .get_or_init(|| {
            Some(Functions {
                switch: patterns::find_unique("equip (switch)", SWITCH_PATTERN)?,
                handle: patterns::find_unique("equip (handle)", HANDLE_PATTERN)?,
                goods: patterns::find_unique("equip (goods)", GOODS_PATTERN)?,
            })
        })
        .as_ref()
        .ok_or_else(|| "equipping is unavailable on this game build".to_string())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Hand,
    Arrow,
    Bolt,
    Head,
    Body,
    Arms,
    Legs,
    Ring,
    Covenant,
    Quick,
    Tool,
}

/// Lua slot name → (game slot number, kind).
fn slot(name: &str) -> Result<(u32, Kind), String> {
    Ok(match name {
        "left1" => (0, Kind::Hand),
        "right1" => (1, Kind::Hand),
        "left2" => (2, Kind::Hand),
        "right2" => (3, Kind::Hand),
        "left3" => (4, Kind::Hand),
        "right3" => (5, Kind::Hand),
        "arrow1" => (6, Kind::Arrow),
        "bolt1" => (7, Kind::Bolt),
        "arrow2" => (8, Kind::Arrow),
        "bolt2" => (9, Kind::Bolt),
        "head" => (12, Kind::Head),
        "body" => (13, Kind::Body),
        "arms" => (14, Kind::Arms),
        "legs" => (15, Kind::Legs),
        "ring1" => (17, Kind::Ring),
        "ring2" => (18, Kind::Ring),
        "ring3" => (19, Kind::Ring),
        "ring4" => (20, Kind::Ring),
        "covenant" => (21, Kind::Covenant),
        _ => {
            if let Some(n) = name.strip_prefix("quick").and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=10).contains(n)) {
                (n - 1, Kind::Quick)
            } else if let Some(n) = name.strip_prefix("tool").and_then(|n| n.parse::<u32>().ok()).filter(|n| (1..=5).contains(n)) {
                (n + 9, Kind::Tool)
            } else {
                return Err(format!(
                    "unknown equipment slot {name:?} (left1-3, right1-3, arrow1-2, bolt1-2, head, body, arms, legs, ring1-4, covenant, quick1-10, tool1-5)"
                ));
            }
        }
    })
}

fn param_flag(table: &str, id: u32, field: &str) -> bool {
    let Ok(reg) = crate::params::registry() else { return false };
    let Ok(t) = reg.table(table) else { return false };
    // Upgrade levels share the base row's flags (DS3 rows are per base id + infusion).
    let base = (id - id % 100) as u64;
    let row = if t.has_row(id as u64) { id as u64 } else { base };
    matches!(t.read(row, field), Ok(Value::Int(v)) if v != 0)
}

/// Rejects items that can't go in the slot, so the game never sees an impossible state.
fn check_fits(kind: Kind, item: ItemId) -> Result<(), String> {
    let id = item.param_id();
    let ok = match (kind, item.category()) {
        (Kind::Hand, ItemCategory::Weapon) => {
            !param_flag("EquipParamWeapon", id, "arrowSlotEquipable") && !param_flag("EquipParamWeapon", id, "boltSlotEquipable")
        }
        (Kind::Arrow, ItemCategory::Weapon) => param_flag("EquipParamWeapon", id, "arrowSlotEquipable"),
        (Kind::Bolt, ItemCategory::Weapon) => param_flag("EquipParamWeapon", id, "boltSlotEquipable"),
        (Kind::Head, ItemCategory::Protector) => param_flag("EquipParamProtector", id, "headEquip"),
        (Kind::Body, ItemCategory::Protector) => param_flag("EquipParamProtector", id, "bodyEquip"),
        (Kind::Arms, ItemCategory::Protector) => param_flag("EquipParamProtector", id, "armEquip"),
        (Kind::Legs, ItemCategory::Protector) => param_flag("EquipParamProtector", id, "legEquip"),
        (Kind::Ring, ItemCategory::Accessory) => !param_flag("EquipParamAccessory", id, "vowId"),
        (Kind::Covenant, ItemCategory::Accessory) => param_flag("EquipParamAccessory", id, "vowId"),
        (Kind::Quick | Kind::Tool, ItemCategory::Goods) => true,
        _ => false,
    };
    if ok { Ok(()) } else { Err(format!("{} can't be equipped in that slot", item_label(item))) }
}

fn item_label(item: ItemId) -> String {
    crate::lua_values::display_name(item).unwrap_or_else(|| format!("item {:#x}", item.into_inner()))
}

/// (inventory index, gaitem handle) of an inventory entry holding `item` that isn't already
/// equipped in another slot (each Fists / bare-armour slot has its own entry, and the game
/// never puts one entry in two slots).
fn find_in_inventory(egd: &EquipGameData, item: ItemId, slot: u32) -> Result<(i32, u32), String> {
    let data = egd.inventory();
    let total = data.capacity();
    let in_use = |i: i32| egd.equipment_indexes().iter().enumerate().any(|(s, &idx)| s as u32 != slot && idx == i);
    let mut held = false;
    for i in 0..total {
        let Some(entry) = data.get(i) else { continue };
        if entry.item_id != item {
            continue;
        }
        held = true;
        if !in_use(i as i32) {
            return Ok((i as i32, entry.gaitem_handle));
        }
    }
    Err(if held { format!("{} is already equipped in another slot", item_label(item)) } else { format!("{} isn't in the inventory", item_label(item)) })
}

/// The item that fills an emptied slot (Fists / bare armour), or None for slots that
/// can simply be cleared.
fn empty_item(kind: Kind) -> Option<ItemId> {
    let (category, id) = match kind {
        Kind::Hand => (ItemCategory::Weapon, 110000),
        Kind::Head => (ItemCategory::Protector, 900000),
        Kind::Body => (ItemCategory::Protector, 901000),
        Kind::Arms => (ItemCategory::Protector, 902000),
        Kind::Legs => (ItemCategory::Protector, 903000),
        _ => return None,
    };
    ItemId::new(category, id).ok()
}

// The callable `switch equipment` invokes before it stores the index. Layout matches the
// game's own: vtable, EquipGameData*, slot, gaitem handle.
#[repr(C)]
struct HandleCall {
    vtable: *const [usize; 6],
    egd: usize,
    slot: u32,
    handle: u32,
}

#[repr(C)]
struct Holder {
    call: HandleCall,
    target: *mut HandleCall,
}

unsafe extern "C" fn handle_call(this: *mut HandleCall) {
    let this = unsafe { &mut *this };
    if let Some(f) = FUNCTIONS.get().and_then(|f| f.as_ref()) {
        let register: unsafe extern "C" fn(usize, u32, *const u32) = unsafe { std::mem::transmute(f.handle) };
        unsafe { register(this.egd + 0x88, this.slot, &this.handle) };
    }
}

unsafe extern "C" fn handle_noop(_: *mut HandleCall) -> usize {
    0
}

static HANDLE_VTABLE: OnceLock<[usize; 6]> = OnceLock::new();

fn vtable() -> *const [usize; 6] {
    HANDLE_VTABLE.get_or_init(|| {
        let noop = handle_noop as *const () as usize;
        [noop, noop, handle_call as *const () as usize, noop, noop, noop]
    })
}

/// What to put in a slot.
pub enum Target {
    Item(i64),
    /// A display name; the copy in the inventory that fits the slot is used (so names
    /// shared by several ids, like upgraded flasks, pick the one the player owns).
    Name(String),
    Empty,
}

/// An inventory item with this display name (case-insensitive; upgraded weapons also
/// match their base name) that fits the slot.
fn owned_by_name(egd: &EquipGameData, name: &str, kind: Kind) -> Option<ItemId> {
    let want = name.to_lowercase();
    let data = egd.inventory();
    let total = data.capacity();
    (0..total).find_map(|i| {
        let item = data.get(i)?.item_id;
        let label = crate::lua_values::display_name(item)?.to_lowercase();
        let base = label.rsplit_once(" +").map_or(label.as_str(), |(b, _)| b);
        ((label == want || base == want) && check_fits(kind, item).is_ok()).then_some(item)
    })
}

/// Equips an inventory item in the named slot, or empties it.
pub fn equip(slot_name: &str, target: Target) -> Result<(), String> {
    let (slot, kind) = slot(slot_name)?;
    let f = functions()?;
    let egd = game::player_game_data()?.equipment();
    let egd_addr = egd.addr();
    let other_slots = if matches!(kind, Kind::Quick | Kind::Tool) { u32::MAX } else { slot };

    let (index, handle) = match target {
        Target::Item(raw) => {
            let item = game::item_id(raw)?;
            check_fits(kind, item)?;
            find_in_inventory(egd, item, other_slots)?
        }
        Target::Name(name) => {
            let item = match owned_by_name(egd, &name, kind) {
                Some(item) => item,
                None => {
                    let raw = crate::lua_values::item_by_name(&name)?;
                    game::item_id(raw)?
                }
            };
            check_fits(kind, item)?;
            find_in_inventory(egd, item, other_slots)?
        }
        // Each empty weapon/armour slot holds its own hidden Fists / bare piece (the game
        // gives one per slot); add one when none is free, as the game does.
        Target::Empty => match empty_item(kind) {
            Some(item) => match find_in_inventory(egd, item, slot) {
                Ok(found) => found,
                Err(_) => {
                    game::give_item(item, 1, false)?;
                    find_in_inventory(egd, item, slot)?
                }
            },
            None => (-1, 0),
        },
    };

    match kind {
        Kind::Quick | Kind::Tool => {
            let data: [u32; 8] = [handle, 0, 0, 0, 0, 0, 0, 0];
            let equip_goods: unsafe extern "C" fn(usize, u32, *const [u32; 8], i32, u32) = unsafe { std::mem::transmute(f.goods) };
            unsafe { equip_goods(egd_addr, slot, &data, index, 1) };
        }
        _ => {
            let mut holder = Holder { call: HandleCall { vtable: vtable(), egd: egd_addr, slot, handle }, target: std::ptr::null_mut() };
            holder.target = &mut holder.call;
            let switch: unsafe extern "C" fn(usize, u32, i32, *mut Holder, u8) = unsafe { std::mem::transmute(f.switch) };
            unsafe { switch(egd_addr, slot, index, &mut holder, 1) };
        }
    }
    Ok(())
}

// ── key items: the character's own list, or a phantom's ─────────────────────────────────────

/// The game's setup of a load into a world (in +0x8F45C0) asks whether the world is someone
/// else's (+0x78C5C0 on the session, at +0x8F4867); if so it points the inventory's key items
/// (EquipInventoryData +0x10's list: +0x48 the entries used, +0x50 their count) at a second list
/// kept for a phantom (+0x28, emptied there each time: +0xA33450 through +0x5840B0, at +0x8F4979),
/// else back at the character's own (+0x18, through +0x583490). A phantom then has none of its key
/// items in the host's world, and those it picks up there go to the phantom's list, lost at its
/// next load (a co-op player's Small Doll gone at Irithyll's barrier, its Mortician's Ashes and
/// Farron Coal lost: verified on two games 2026-10-15). The switch back (+0x583490), as the game
/// does at home, is `inventory.use_own_key_items`. (Changing the switch's argument in the load's
/// code instead crashed a co-op player's game in its heap three times, 2026-10-15/16: that code is
/// rewritten by the game's protection as it runs.)
const USE_OWN_KEYS: &str = "48 83 ec 28 48 81 c1 a8 01 00 00 33 d2 e8 ?? ?? ?? ?? b0 01 48 83 c4 28 c3";
/// PlayerGameData +0x228: EquipGameData; its +0x1A8: EquipInventoryData, whose list (+0x10) has
/// the character's key entries at +0x18 and the key entries in use at +0x48.
const EQUIP_GAME_DATA: usize = 0x228;
const OWN_KEYS_AT: usize = 0x1A8 + 0x10 + 0x18;
const USED_KEYS_AT: usize = 0x1A8 + 0x10 + 0x48;

/// The EquipGameData, and whether the key items in use are the character's own.
fn key_lists() -> Result<(usize, bool), String> {
    let equip = crate::game::player_game_data()?.addr() + EQUIP_GAME_DATA;
    let (own, used) = (memory::read_u64(equip + OWN_KEYS_AT), memory::read_u64(equip + USED_KEYS_AT));
    let (Some(own), Some(used)) = (own, used) else { return Err("the character's inventory is unreadable".into()) };
    Ok((equip, own == 0 || own == used))
}

/// kindle.game: inventory.own_key_items (catalogue.rs).
pub static OWN_KEY_ITEMS: crate::catalogue::Field = crate::catalogue::Field {
    name: "inventory.own_key_items",
    doc: "Whether the player's key items in use are its character's own (at home), not the empty list the game gives a phantom in someone else's world (its own key items gone there, and those it picks up lost at its next load).",
    kind: crate::catalogue::Kind::Bool,
    per_character: false,
    get: |_| key_lists().map(|(_, own)| crate::catalogue::Value::Bool(own)),
    set: None,
};

/// kindle.game: inventory.use_own_key_items (catalogue.rs).
pub static USE_OWN_KEY_ITEMS: crate::catalogue::Function = crate::catalogue::Function {
    name: "inventory.use_own_key_items",
    doc: "Puts the player's own key items back in use (the game's own switch, as at home), where a load into someone else's world gave it a phantom's list. Gives false if they already were.",
    args: &[],
    call: |_| {
        static AT: OnceLock<Option<usize>> = OnceLock::new();
        if !crate::on_game_thread() {
            return Err("inventory.use_own_key_items: from the game's thread only".into());
        }
        let (equip, own) = key_lists()?;
        if own {
            return Ok(Some(crate::catalogue::Value::Bool(false)));
        }
        let at = AT
            .get_or_init(|| crate::patterns::find_unique("use own key items", USE_OWN_KEYS))
            .ok_or("the game's key item switch is unavailable on this game build")?;
        let f: extern "C" fn(usize) -> u8 = unsafe { std::mem::transmute(at) };
        f(equip);
        Ok(Some(crate::catalogue::Value::Bool(true)))
    },
};
