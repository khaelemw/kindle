//! Kindle's own view of Dark Souls III 1.15.2: where the game keeps the objects Kindle reads,
//! the handful of game functions it calls, and the struct fields it uses.
//!
//! Statics and functions are found by byte pattern in the game's code (patterns.rs), so
//! nothing is a hard-coded address. Every pattern was built from the 1.15.2 exe and checked to
//! match once and land on the right place. Struct views are opaque references to game memory
//! with accessors at fixed offsets; the few with field layouts carry compile-time offset
//! checks.

use std::ffi::c_void;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::{memory, patterns};

// ── version ─────────────────────────────────────────────────────────────────────────────

/// Versions Kindle supports: 1.15.2.0 (worldwide) and 1.15.2.1 (Japan).
pub fn supported_version() -> Result<String, String> {
    match memory::exe_version() {
        Some(v) if v == "1.15.2.0" || v == "1.15.2.1" => Ok(v),
        Some(v) => Err(format!("this DarkSoulsIII.exe is version {v}; Kindle needs 1.15.2")),
        None => Err("couldn't read DarkSoulsIII.exe's version".into()),
    }
}

// ── statics ─────────────────────────────────────────────────────────────────────────────

/// A static in the game's data that points to a singleton, found through a
/// `mov reg, [rip+disp]` that reads it (patterns::find_static).
pub struct Global {
    pub name: &'static str,
    pattern: &'static str,
    addr: OnceLock<Option<usize>>,
}

impl Global {
    const fn new(name: &'static str, pattern: &'static str) -> Self {
        Self { name, pattern, addr: OnceLock::new() }
    }

    /// Address of the static (None if the pattern didn't match).
    pub fn static_addr(&self) -> Option<usize> {
        *self.addr.get_or_init(|| patterns::find_static(self.name, self.pattern))
    }

    /// The instance it points to (None until the game creates it).
    pub fn instance(&self) -> Option<usize> {
        memory::read_u64(self.static_addr()?).map(|p| p as usize).filter(|&p| p != 0)
    }
}

pub static WORLD_CHR_MAN: Global = Global::new("WorldChrMan", "4c 8b 3d ?? ?? ?? ?? 33 ff");
pub static EVENT_FLAG_MAN: Global = Global::new("SprjEventFlagMan", "48 8b 0d ?? ?? ?? ?? 85 eb");
pub static REGULATION_MAN: Global = Global::new("CSRegulationManager", "48 8b 05 ?? ?? ?? ?? 8b 40 64");
pub static DLC: Global = Global::new("CSDlc", "48 8b 05 ?? ?? ?? ?? 80 78 43 00");
pub static TASK: Global = Global::new("SprjTask", "48 8b 0d ?? ?? ?? ?? 4c 8b c3 33 d2");
pub static MSG_REPOSITORY: Global = Global::new("MsgRepository", "48 8b 35 ?? ?? ?? ?? 8b e8");
pub static LUA_EVENT_MAN: Global = Global::new("SprjLuaEventMan", "4c 8b 35 ?? ?? ?? ?? 75 ??");
pub static SOLO_PARAM_REPOSITORY: Global = Global::new("SoloParamRepository", "48 8b 0d ?? ?? ?? ?? 8d 53 07");
pub static FIELD_AREA: Global = Global::new("FieldArea", "48 8b 05 ?? ?? ?? ?? 48 3b c7");
pub static GAME_DATA_MAN: Global = Global::new("GameDataMan", "48 8b 15 ?? ?? ?? ?? 32 c0");
pub static MAP_ITEM_MAN: Global = Global::new("MapItemMan", "48 8b 1d ?? ?? ?? ?? 44 8b 47 08");
/// The game's HINSTANCE, set once its window system is up.
pub static HINSTANCE: Global = Global::new("hInstance", "48 8b 05 ?? ?? ?? ?? c3 cc 8d 42 f1");

/// Every static Kindle knows, for the debug module.
pub static GLOBALS: [&Global; 12] = [
    &WORLD_CHR_MAN,
    &EVENT_FLAG_MAN,
    &REGULATION_MAN,
    &DLC,
    &TASK,
    &MSG_REPOSITORY,
    &LUA_EVENT_MAN,
    &SOLO_PARAM_REPOSITORY,
    &FIELD_AREA,
    &GAME_DATA_MAN,
    &MAP_ITEM_MAN,
    &HINSTANCE,
];

/// Statics by name: Kindle's own (GLOBALS) plus every class found by `memory::class_statics`.
pub fn singletons() -> Vec<(String, usize)> {
    let mut v: Vec<_> = GLOBALS.iter().filter_map(|g| Some((g.name.to_string(), g.static_addr()?))).collect();
    for (name, addr) in memory::class_statics() {
        if !v.iter().any(|(n, _)| *n == name) {
            v.push((name, addr));
        }
    }
    v.sort();
    v
}

/// The current instance address of a named singleton (None if unknown or not created yet).
pub fn singleton_instance(name: &str) -> Option<usize> {
    if let Some(g) = GLOBALS.iter().find(|g| g.name == name) {
        return g.instance();
    }
    memory::class_instance(name)
}

// ── functions ───────────────────────────────────────────────────────────────────────────

struct Function {
    name: &'static str,
    pattern: &'static str,
    /// Offset of a `call rel32` in the pattern whose target is the function (for functions
    /// another mod may have hooked at their first bytes); None: the match itself.
    call_at: Option<usize>,
    addr: OnceLock<Option<usize>>,
}

impl Function {
    const fn new(name: &'static str, pattern: &'static str) -> Self {
        Self { name, pattern, call_at: None, addr: OnceLock::new() }
    }

    const fn called_from(name: &'static str, pattern: &'static str, call_at: usize) -> Self {
        Self { name, pattern, call_at: Some(call_at), addr: OnceLock::new() }
    }

    fn get(&self) -> Result<usize, String> {
        self.addr
            .get_or_init(|| {
                let at = patterns::find_unique(self.name, self.pattern)?;
                match self.call_at {
                    None => Some(at),
                    Some(off) => {
                        let rel = memory::read::<i32>(at + off + 1)?;
                        Some((at + off + 5).wrapping_add_signed(rel as isize))
                    }
                }
            })
            .ok_or_else(|| format!("the game's {} function wasn't found on this game build", self.name))
    }
}

static REGISTER_TASK: Function =
    Function::new("register task", "41 56 48 83 ec 50 48 c7 44 24 20 fe ff ff ff 48 89 5c 24 60 48 89 6c 24 68 48 89 74 24 70 48 89 7c 24 78 49 8b e9");
static GIVE_ITEM: Function = Function::new("give item", "48 8b c4 41 54 41 56 41 57 48 83 ec 60 48 c7 40 c8 fe ff ff ff");
static REMOVE_ITEM: Function = Function::new("remove item", "48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 57 41 56 41 57 48 83 ec 20 48 8b 05 ?? ?? ?? ??");
static GRANT_ITEMS: Function = Function::new("grant items", "4c 89 44 24 18 55 56 57 41 54 41 55 41 56 41 57 48 8d 6c 24 d9 48 81 ec 00 01 00 00");
static GET_FLAG: Function = Function::new("get event flag", "40 53 48 83 ec 20 80 b9 28 02 00 00 00 8b da");
/// Found through a small wrapper that calls it: Seamless Co-op hooks its first bytes.
static SET_FLAG: Function = Function::called_from("set event flag", "48 89 5c 24 08 57 48 83 ec 30 41 0f b6 d8 8b fa e8 ?? ?? ?? ??", 16);

// ── items ───────────────────────────────────────────────────────────────────────────────

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemCategory {
    Weapon = 0,
    Protector = 1,
    Accessory = 2,
    Goods = 4,
}

impl ItemCategory {
    fn from_bits(bits: u32) -> Option<Self> {
        Some(match bits {
            0 => Self::Weapon,
            1 => Self::Protector,
            2 => Self::Accessory,
            4 => Self::Goods,
            _ => return None,
        })
    }
}

/// An item: its category in the top 4 bits, its param row id in the low 28.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemId(u32);

impl ItemId {
    pub fn new(category: ItemCategory, param_id: u32) -> Result<Self, String> {
        if param_id >= 1 << 28 {
            return Err(format!("item param id {param_id} is too large"));
        }
        Ok(Self((category as u32) << 28 | param_id))
    }

    pub fn category(self) -> ItemCategory {
        ItemCategory::from_bits(self.0 >> 28).unwrap_or(ItemCategory::Goods)
    }

    pub fn param_id(self) -> u32 {
        self.0 & 0x0FFF_FFFF
    }

    pub fn into_inner(self) -> u32 {
        self.0
    }

    /// Whether the raw value is a real item (empty inventory slots hold other values).
    pub fn is_valid(self) -> bool {
        ItemCategory::from_bits(self.0 >> 28).is_some()
    }
}

impl TryFrom<u32> for ItemId {
    type Error = String;
    fn try_from(raw: u32) -> Result<Self, String> {
        let id = Self(raw);
        if id.is_valid() { Ok(id) } else { Err(format!("invalid item category {}", raw >> 28)) }
    }
}

/// Adds items to the inventory without the "item received" box.
pub fn give_item_directly(item: ItemId, quantity: u32) -> Result<(), String> {
    let f: extern "C" fn(usize, u32, u32, u32) = unsafe { std::mem::transmute(GIVE_ITEM.get()?) };
    f(0, (item.category() as u32) << 28, item.param_id(), quantity);
    Ok(())
}

pub fn remove_item(item: ItemId, quantity: u32) -> Result<(), String> {
    let f: extern "C" fn(usize, u32, u32, u32) = unsafe { std::mem::transmute(REMOVE_ITEM.get()?) };
    f(0, (item.category() as u32) << 28, item.param_id(), quantity);
    Ok(())
}

/// Gives items the way picking them up does, with the game's "item received" box.
pub fn grant_item(item: ItemId, quantity: u32) -> Result<(), String> {
    #[repr(C)]
    struct Buffer {
        len: u32,
        id: u32,
        quantity: u32,
        durability: i32,
    }
    let man = MAP_ITEM_MAN.instance().ok_or("MapItemMan is not available")?;
    let f: extern "C" fn(usize, *const Buffer, *const i32) = unsafe { std::mem::transmute(GRANT_ITEMS.get()?) };
    let buffer = Buffer { len: 1, id: item.into_inner(), quantity, durability: -1 };
    f(man, &buffer, &1);
    Ok(())
}

// ── event flags ─────────────────────────────────────────────────────────────────────────

/// Checks an event flag id the way the game numbers them: at most 8 digits, area below 90.
pub fn check_flag(id: i64) -> Result<u32, String> {
    let raw = u32::try_from(id).ok().filter(|&v| v <= 99_999_999).ok_or_else(|| format!("event flag {id} is out of range"))?;
    if (raw / 100_000) % 100 >= 90 {
        return Err(format!("event flag {id} has an invalid area"));
    }
    Ok(raw)
}

fn flag_man() -> Result<usize, String> {
    EVENT_FLAG_MAN.instance().ok_or_else(|| "event flags are not available yet".to_string())
}

pub fn get_flag(id: u32) -> Result<bool, String> {
    let f: extern "C" fn(usize, u32) -> u8 = unsafe { std::mem::transmute(GET_FLAG.get()?) };
    Ok(f(flag_man()?, id) != 0)
}

/// Sets a flag through the game's own function (as its event scripts do; mods that hook it,
/// like Seamless Co-op, see the change).
pub fn set_flag(id: u32, state: bool) -> Result<(), String> {
    let f: extern "C" fn(usize, u32, u8, u8) = unsafe { std::mem::transmute(SET_FLAG.get()?) };
    let man = flag_man()?;
    crate::flag_blocks::kindle_sets(|| f(man, id, state as u8, 0));
    Ok(())
}

/// Sets a flag through the game's own function as the game's code would, reported by
/// flags.changed (the rig's stand-in for a summon sign's or a lever's set).
#[cfg(feature = "rig")]
pub fn set_flag_as_game(id: u32, state: bool) -> Result<(), String> {
    let f: extern "C" fn(usize, u32, u8, u8) = unsafe { std::mem::transmute(SET_FLAG.get()?) };
    f(flag_man()?, id, state as u8, 0);
    Ok(())
}

/// The character's two sets of event flags (SprjEventFlagMan: +0x218 the set in use, +0x220 its
/// index; tables by the flag's first digit, 0x18 bytes a digit): its own (set 0 at +0x28, saved
/// with it) and a session's (set 1 at +0x120), which a player in a session uses: the host's
/// world's flags as its game has them, apart from the character's own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlagSet {
    Own,
    Session,
}

impl FlagSet {
    /// Where the set is in SprjEventFlagMan, and its index (+0x220 while in use).
    pub(crate) fn at(self) -> (usize, u32) {
        match self {
            FlagSet::Own => (0x28, 0),
            FlagSet::Session => (0x120, 1),
        }
    }
}

/// The set of event flags in use now.
pub fn flag_set_in_use() -> Result<FlagSet, String> {
    match memory::read::<u32>(flag_man()? + 0x220) {
        Some(0) => Ok(FlagSet::Own),
        Some(1) => Ok(FlagSet::Session),
        other => Err(format!("the event flags' set in use is {other:?}")),
    }
}

/// `f` (a get or set of `id`) with `set` the set in use: on the game's thread only, where nothing
/// else reads the flags until it's put back; the set must have a table for the flag's first digit
/// (a session's set has none for some digits).
pub fn with_flag_set<R>(set: FlagSet, id: u32, f: impl FnOnce() -> Result<R, String>) -> Result<R, String> {
    if !crate::on_game_thread() {
        return Err("a flag of another set is read or set on the game's thread only".into());
    }
    let man = flag_man()?;
    let (at, index) = set.at();
    let digit = (id / 10_000_000) as usize % 10;
    if memory::read_u64(man + at + digit * 0x18).is_none_or(|t| t == 0) {
        return Err(format!("the {set:?} flag set has no table for flag {id} now"));
    }
    let (Some(was), Some(was_index)) = (memory::read_u64(man + 0x218), memory::read::<u32>(man + 0x220)) else {
        return Err("the event flags' set in use isn't readable".into());
    };
    unsafe {
        std::ptr::write_volatile((man + 0x218) as *mut u64, (man + at) as u64);
        std::ptr::write_volatile((man + 0x220) as *mut u32, index);
    }
    let r = f();
    unsafe {
        std::ptr::write_volatile((man + 0x218) as *mut u64, was);
        std::ptr::write_volatile((man + 0x220) as *mut u32, was_index);
    }
    r
}

// ── characters ──────────────────────────────────────────────────────────────────────────

/// A view of game memory at a fixed address. Only ever handed out as a reference to the
/// game's own object.
macro_rules! opaque {
    ($($(#[$m:meta])* $name:ident;)*) => {$(
        $(#[$m])*
        #[repr(C)]
        pub struct $name {
            _opaque: [u8; 0],
        }

        impl $name {
            /// # Safety
            /// `addr` must be a live object of this type.
            pub unsafe fn at<'a>(addr: usize) -> &'a mut Self {
                unsafe { &mut *(addr as *mut Self) }
            }

            pub fn addr(&self) -> usize {
                self as *const Self as usize
            }

            fn field<T>(&self, off: usize) -> *mut T {
                (self.addr() + off) as *mut T
            }
        }
    )*};
}

opaque! {
    /// A character (enemy, NPC, player).
    ChrIns;
    /// The player's character; starts with its ChrIns.
    PlayerIns;
    /// A player's saved state: stats, name, inventory, equipment.
    PlayerGameData;
    /// Equipment and inventory.
    EquipGameData;
}

impl ChrIns {
    /// The handle the world block mappings use for this character.
    pub fn field_ins_handle(&self) -> u32 {
        unsafe { *self.field::<u32>(0x08) }
    }

    /// Its entry in a character set (holds the physics module at +0x28).
    pub fn chr_set_entry(&self) -> usize {
        unsafe { *self.field::<usize>(0x18) }
    }

    /// HP, FP and stamina ([ChrIns+0x1F90] module container -> +0x18).
    pub fn data(&self) -> Option<&'static mut ChrDataModule> {
        let modules = memory::read_u64(self.addr() + 0x1F90).filter(|&p| p != 0)? as usize;
        let data = memory::read_u64(modules + 0x18).filter(|&p| p != 0)? as usize;
        Some(unsafe { &mut *(data as *mut ChrDataModule) })
    }
}

impl PlayerIns {
    pub fn chr(&self) -> &ChrIns {
        unsafe { ChrIns::at(self.addr()) }
    }

    pub fn game_data(&self) -> Option<&'static mut PlayerGameData> {
        let p = unsafe { *self.field::<usize>(0x1FA0) };
        (p != 0).then(|| unsafe { PlayerGameData::at(p) })
    }
}

#[repr(C)]
pub struct ChrDataModule {
    _unk00: [u8; 0xD8],
    pub hp: i32,
    pub max_hp: i32,
    pub base_hp: i32,
    pub fp: i32,
    pub max_fp: i32,
    pub base_fp: i32,
    pub stamina: i32,
    pub max_stamina: i32,
    pub base_stamina: i32,
    _unkfc: [u8; 0x34],
    /// The model, e.g. "c1100".
    model: [u16; 8],
}

impl ChrDataModule {
    /// The character's model id, e.g. "c1100".
    pub fn id(&self) -> String {
        let len = self.model.iter().position(|&c| c == 0).unwrap_or(self.model.len());
        String::from_utf16_lossy(&self.model[..len])
    }
}

/// The player in the world (None on the title screen).
pub fn main_player() -> Option<&'static mut PlayerIns> {
    let world = WORLD_CHR_MAN.instance()?;
    let p = memory::read_u64(world + 0x80).filter(|&p| p != 0)? as usize;
    Some(unsafe { PlayerIns::at(p) })
}

/// Every character in the loaded world blocks, with the entity id its block maps it to:
/// (ChrIns address, entity id).
pub fn block_characters() -> Option<Vec<(usize, Option<i32>)>> {
    let world = WORLD_CHR_MAN.instance()?;
    let mut out = Vec::new();
    for b in 0..32 {
        let block = world + 0x8E0 + b * 0x138;
        // An empty block slot has no mappings.
        let mappings = memory::read_u64(block + 0xB0).unwrap_or(0) as usize;
        if mappings == 0 {
            continue;
        }
        let mapping_count = memory::read::<i32>(block + 0xA8).unwrap_or(0).clamp(0, 100_000) as usize;
        let mapping_bytes = memory::read_bytes(mappings, mapping_count * 8).unwrap_or_default();
        let entity_of = |handle: u32| {
            mapping_bytes.as_chunks::<8>().0.iter().find_map(|m| {
                let entity = i32::from_le_bytes(m[0..4].try_into().unwrap());
                let selector = u32::from_le_bytes(m[4..8].try_into().unwrap());
                (selector == handle).then_some(entity)
            })
        };
        // The block's character set: {u32 capacity, entries}; entries are 0x38 bytes, the
        // first being the character (null = free slot).
        let capacity = memory::read::<u32>(block + 0x80).unwrap_or(0).min(100_000) as usize;
        let entries = memory::read_u64(block + 0x88).unwrap_or(0) as usize;
        if entries == 0 {
            continue;
        }
        for i in 0..capacity {
            let chr = memory::read_u64(entries + i * 0x38).unwrap_or(0) as usize;
            if chr != 0 {
                let handle = unsafe { ChrIns::at(chr) }.field_ins_handle();
                out.push((chr, entity_of(handle)));
            }
        }
    }
    // Characters made by the developer creator (spawn.rs) are in a set of their own, laid
    // out like a block's: {u32 capacity, entries}.
    let set = world + 0x68;
    let capacity = memory::read::<u32>(set).unwrap_or(0).min(10_000) as usize;
    let entries = memory::read_u64(set + 8).unwrap_or(0) as usize;
    if entries != 0 {
        for i in 0..capacity {
            let chr = memory::read_u64(entries + i * 0x38).unwrap_or(0) as usize;
            if chr != 0 {
                out.push((chr, None));
            }
        }
    }
    Some(out)
}

// ── player game data ────────────────────────────────────────────────────────────────────

#[repr(C)]
pub struct PlayerInfo {
    _unk00: [u8; 0x34],
    pub vigor: u32,
    pub attunement: u32,
    pub endurance: u32,
    pub strength: u32,
    pub dexterity: u32,
    pub intelligence: u32,
    pub faith: u32,
    pub luck: u32,
    _unk54: [u32; 2],
    pub vitality: u32,
    _unk60: [u8; 0x18],
    character_name: [u16; 17],
}

impl PlayerInfo {
    pub fn name(&self) -> String {
        let len = self.character_name.iter().position(|&c| c == 0).unwrap_or(self.character_name.len());
        String::from_utf16_lossy(&self.character_name[..len])
    }
}

impl PlayerGameData {
    pub fn player_info(&self) -> &PlayerInfo {
        unsafe { &*self.field::<PlayerInfo>(0x10) }
    }

    pub fn equipment(&self) -> &'static mut EquipGameData {
        unsafe { EquipGameData::at(self.addr() + 0x228) }
    }
}

impl EquipGameData {
    /// The inventory index equipped in each of the 22 slots (-1 = none).
    pub fn equipment_indexes(&self) -> &[i32; 22] {
        unsafe { &*self.field::<[i32; 22]>(0x24) }
    }

    pub fn inventory(&self) -> &'static mut Inventory {
        unsafe { &mut *self.field::<Inventory>(0x1A8 + 0x10) }
    }
}

/// The inventory: key items, then normal items, each a fixed array of entries.
#[repr(C)]
pub struct Inventory {
    _total_capacity: u32,
    pub normal_capacity: u32,
    normal_head: *mut InventoryEntry,
    _normal_count: u32,
    pub key_capacity: u32,
    key_head: *mut InventoryEntry,
    _key_count: u32,
}

#[repr(C)]
pub struct InventoryEntry {
    pub gaitem_handle: u32,
    pub item_id: ItemId,
    pub quantity: u32,
    _unk0c: u32,
}

impl Inventory {
    fn slots(&self) -> impl Iterator<Item = *mut InventoryEntry> {
        let key = (0..self.key_capacity as usize).map(|i| unsafe { self.key_head.add(i) });
        let normal = (0..self.normal_capacity as usize).map(|i| unsafe { self.normal_head.add(i) });
        key.chain(normal)
    }

    pub fn capacity(&self) -> u32 {
        self.key_capacity + self.normal_capacity
    }

    /// The entry at an inventory index (key items first), if it holds an item.
    pub fn get(&self, index: u32) -> Option<&InventoryEntry> {
        let p = if index < self.key_capacity {
            unsafe { self.key_head.add(index as usize) }
        } else if index - self.key_capacity < self.normal_capacity {
            unsafe { self.normal_head.add((index - self.key_capacity) as usize) }
        } else {
            return None;
        };
        let e = unsafe { &*p };
        e.item_id.is_valid().then_some(e)
    }

    /// Every entry holding an item, key items first.
    pub fn items(&self) -> impl Iterator<Item = &InventoryEntry> {
        self.slots().map(|p| unsafe { &*p }).filter(|e| e.item_id.is_valid())
    }

    pub fn items_mut(&mut self) -> impl Iterator<Item = &mut InventoryEntry> {
        self.slots().map(|p| unsafe { &mut *p }).filter(|e| e.item_id.is_valid())
    }
}

// ── params ──────────────────────────────────────────────────────────────────────────────

/// One loaded param table: its name, its resource (FD4ParamResCap: +0x60 size, +0x68
/// table) and the table.
pub struct ParamCap {
    pub name: String,
    pub res: usize,
    pub table: usize,
    pub table_size: usize,
}

/// The regulation's param tables (CSRegulationManager +0x10: a vector of ParamResCap*).
/// None until the regulation is loaded.
pub fn param_caps() -> Option<Vec<ParamCap>> {
    let man = REGULATION_MAN.instance()?;
    let first = memory::read_u64(man + 0x10)? as usize;
    let last = memory::read_u64(man + 0x18)? as usize;
    if first == 0 || last < first || (last - first) / 8 > 4096 {
        return None;
    }
    let mut out = Vec::new();
    for i in 0..(last - first) / 8 {
        let cap = memory::read_u64(first + i * 8)? as usize;
        // ParamResCap: +0x08 name (FD4BasicHashString: vtable, then the string), +0x68 res.
        let name = memory::msvc_wstring(cap + 0x10)?;
        let res = memory::read_u64(cap + 0x68)? as usize;
        let table = memory::read_u64(res + 0x68)? as usize;
        let table_size = memory::read_u64(res + 0x60)? as usize;
        out.push(ParamCap { name, res, table, table_size });
    }
    Some(out)
}

// ── DLC ─────────────────────────────────────────────────────────────────────────────────

/// Whether a DLC is installed (1 Ashes of Ariandel, 2 The Ringed City). None if unknown.
pub fn dlc_installed(dlc: u32) -> Option<bool> {
    let d = DLC.instance()?;
    let off = match dlc {
        1 => 0x11,
        2 => 0x12,
        _ => return None,
    };
    memory::read::<u8>(d + off).map(|b| b != 0)
}

// ── game tasks ──────────────────────────────────────────────────────────────────────────
//
// A task is an object whose vtable's third entry, execute(this, data), the game calls once a
// frame from the task group it was registered to. Kindle's never unregisters.

#[repr(C)]
struct TaskVtable {
    runtime_class: extern "C" fn(*const Task) -> usize,
    destructor: extern "C" fn(*mut Task),
    execute: extern "C" fn(*mut Task, *const c_void),
}

#[repr(C)]
struct Task {
    vtable: &'static TaskVtable,
    _unk08: usize,
    run: fn(),
}

extern "C" fn task_runtime_class(_: *const Task) -> usize {
    0
}

extern "C" fn task_destructor(_: *mut Task) {}

extern "C" fn task_execute(task: *mut Task, _: *const c_void) {
    (unsafe { &*task }.run)();
}

static TASK_VTABLE: TaskVtable = TaskVtable { runtime_class: task_runtime_class, destructor: task_destructor, execute: task_execute };

/// The task group run first in each frame.
pub const FRAME_BEGIN: u32 = 0;

/// Waits (up to `timeout`) for the game's task system. Call from a thread of Kindle's own:
/// the game's main thread creates it.
pub fn wait_for_task_system(timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    let wait = |what: &str, ready: &dyn Fn() -> bool| -> Result<(), String> {
        while !ready() {
            if start.elapsed() > timeout {
                return Err(format!("{what} never became available"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    };
    wait("the game's window system", &|| HINSTANCE.instance().is_some())?;
    HINSTANCE.static_addr().ok_or("the game's HINSTANCE wasn't found")?;
    TASK.static_addr().ok_or("the game's task system wasn't found")?;
    wait("the game's task system", &|| TASK.instance().is_some())
}

/// Registers `run` to be called once a frame in `group` (waiting for the task system first).
pub fn register_task(run: fn(), group: u32, timeout: Duration) -> Result<(), String> {
    wait_for_task_system(timeout)?;
    let register: extern "C" fn(usize, u32, u64, *const Task) = unsafe { std::mem::transmute(REGISTER_TASK.get()?) };
    // Lives for the rest of the process: the game keeps a pointer to it.
    let task: &'static Task = Box::leak(Box::new(Task { vtable: &TASK_VTABLE, _unk08: 0, run }));
    register(TASK.instance().ok_or("the game's task system went away")?, group, 0, task);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::offset_of;

    #[test]
    fn layouts_match_the_game() {
        assert_eq!(offset_of!(ChrDataModule, hp), 0xD8);
        assert_eq!(offset_of!(ChrDataModule, max_hp), 0xDC);
        assert_eq!(offset_of!(ChrDataModule, fp), 0xE4);
        assert_eq!(offset_of!(ChrDataModule, max_fp), 0xE8);
        assert_eq!(offset_of!(ChrDataModule, stamina), 0xF0);
        assert_eq!(offset_of!(ChrDataModule, max_stamina), 0xF4);
        assert_eq!(offset_of!(ChrDataModule, model), 0x130);
        assert_eq!(offset_of!(PlayerInfo, vigor), 0x34);
        assert_eq!(offset_of!(PlayerInfo, luck), 0x50);
        assert_eq!(offset_of!(PlayerInfo, vitality), 0x5C);
        assert_eq!(offset_of!(PlayerInfo, character_name), 0x78);
        assert_eq!(offset_of!(Inventory, normal_capacity), 0x04);
        assert_eq!(offset_of!(Inventory, normal_head), 0x08);
        assert_eq!(offset_of!(Inventory, key_capacity), 0x14);
        assert_eq!(offset_of!(Inventory, key_head), 0x18);
        assert_eq!(std::mem::size_of::<InventoryEntry>(), 0x10);
        assert_eq!(offset_of!(InventoryEntry, item_id), 0x04);
        assert_eq!(offset_of!(InventoryEntry, quantity), 0x08);
    }

    #[test]
    fn item_ids() {
        let id = ItemId::new(ItemCategory::Goods, 500).unwrap();
        assert_eq!(id.into_inner(), 0x4000_01F4);
        assert_eq!(id.category(), ItemCategory::Goods);
        assert_eq!(id.param_id(), 500);
        assert!(ItemId::try_from(0xFFFF_FFFF).is_err());
        assert_eq!(ItemId::try_from(0x1000_0000 | 900000).unwrap().category(), ItemCategory::Protector);
    }

    #[test]
    fn flag_ids() {
        assert!(check_flag(13009990).is_ok());
        assert!(check_flag(100_000_000).is_err());
        assert!(check_flag(19_000_000).is_err()); // area 90
        assert!(check_flag(-1).is_err());
    }
}
