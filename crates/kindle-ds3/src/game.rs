//! Thin, defensive wrappers over Kindle's view of the game (ds3.rs). Everything here must be
//! called on the game's task thread, and every function returns an error instead of
//! panicking when the game isn't in the right state (e.g. no character loaded yet).

use crate::ds3::{self, ChrDataModule, ItemCategory, ItemId, PlayerGameData, PlayerIns};

pub type GameResult<T> = Result<T, String>;

// ── camera ─────────────────────────────────────────────────────────────────

/// The game camera: `SprjCamera` (static, found through its accessors) `+0x08` = the
/// gameplay `SprjPersCam`, whose matrix at `+0x10` is right, up, forward, position (4 f32
/// each); then fov (radians), aspect, near, far. Mapped live 2026-10-03.
const CAMERA_ACCESSOR: &str = "48 8b 0d ?? ?? ?? ?? 48 85 c9 75 ?? 4c 8d 0d ?? ?? ?? ??";

pub struct Camera {
    pub pos: [f32; 3],
    /// Where the camera looks, its right and its up (unit length).
    pub forward: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub fov: f32,
}

/// The gameplay camera's address (SprjCamera `+0x08`), None before there is one.
pub fn gameplay_camera() -> Option<usize> {
    static STATIC: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let s = (*STATIC.get_or_init(|| crate::patterns::find_singleton_static("SprjCamera", CAMERA_ACCESSOR, 0xC, "SprjCamera")))?;
    let read = |a: usize| crate::memory::read_u64(a).map(|v| v as usize).filter(|&v| v != 0);
    read(s).and_then(|c| read(c + 0x08))
}

pub fn camera() -> GameResult<Camera> {
    let cam = gameplay_camera().ok_or("there's no camera yet")?;
    let f = |off: usize| crate::memory::read::<f32>(cam + off).unwrap_or(f32::NAN);
    let c = Camera {
        pos: [f(0x40), f(0x44), f(0x48)],
        forward: [f(0x30), f(0x34), f(0x38)],
        right: [f(0x10), f(0x14), f(0x18)],
        up: [f(0x20), f(0x24), f(0x28)],
        fov: f(0x50),
    };
    if c.pos.iter().chain(&c.forward).chain(&c.right).chain(&c.up).any(|v| !v.is_finite()) || !c.fov.is_finite() {
        return Err("the camera is unreadable".into());
    }
    Ok(c)
}

// ── event flags ─────────────────────────────────────────────────────────────

/// Boss defeat flags (`kindle.flags.boss`): (name, flag, display name). From the game's own
/// event scripts: each defeat sets this flag (plus a 9300-series one); display names come
/// from the bosses' health bar text.
pub const BOSSES: &[(&str, i64, &str)] = &[
    ("iudex_gundyr", 14000800, "Iudex Gundyr"),
    ("vordt_of_the_boreal_valley", 13000800, "Vordt of the Boreal Valley"),
    ("curse_rotted_greatwood", 13100800, "Curse-rotted Greatwood"),
    ("crystal_sage", 13300850, "Crystal Sage"),
    ("abyss_watchers", 13300800, "Abyss Watchers"),
    ("deacons_of_the_deep", 13500800, "Deacons of the Deep"),
    ("high_lord_wolnir", 13800800, "High Lord Wolnir"),
    ("old_demon_king", 13800830, "Old Demon King"),
    ("pontiff_sulyvahn", 13700850, "Pontiff Sulyvahn"),
    ("aldrich_devourer_of_gods", 13700800, "Aldrich, Devourer of Gods"),
    ("yhorm_the_giant", 13900800, "Yhorm the Giant"),
    ("dancer_of_the_boreal_valley", 13000890, "Dancer of the Boreal Valley"),
    ("oceiros_the_consumed_king", 13000830, "Oceiros, the Consumed King"),
    ("dragonslayer_armour", 13010800, "Dragonslayer Armour"),
    ("twin_princes", 13410830, "Lorian, Elder Prince and Lothric, Younger Prince"),
    ("champion_gundyr", 14000830, "Champion Gundyr"),
    ("ancient_wyvern", 13200800, "Ancient Wyvern"),
    ("nameless_king", 13200850, "Nameless King"),
    ("soul_of_cinder", 14100800, "Soul of Cinder"),
    ("sister_friede", 14500800, "Sister Friede (Ashes of Ariandel)"),
    ("champions_gravetender", 14500860, "Champion's Gravetender (Ashes of Ariandel)"),
    ("demon_prince", 15000800, "Demon Prince (The Ringed City)"),
    ("halflight", 15100800, "Halflight, Spear of the Church (The Ringed City)"),
    ("darkeater_midir", 15100850, "Darkeater Midir (The Ringed City)"),
    ("slave_knight_gael", 15110800, "Slave Knight Gael (The Ringed City)"),
];

pub fn flag_get(id: i64) -> GameResult<bool> {
    ds3::get_flag(ds3::check_flag(id)?)
}

/// Sets a flag; true if it now has that state (flags of areas that aren't loaded can't be
/// set).
pub fn flag_set(id: i64, state: bool) -> GameResult<bool> {
    let id = ds3::check_flag(id)?;
    ds3::set_flag(id, state)?;
    Ok(ds3::get_flag(id)? == state)
}

// ── player ──────────────────────────────────────────────────────────────────

fn main_player() -> Option<&'static mut PlayerIns> {
    ds3::main_player()
}

/// True while a character is loaded into the world (not on the title screen or a load
/// screen between saves).
pub fn player_loaded() -> bool {
    main_player().is_some()
}

fn require_player() -> GameResult<&'static mut PlayerIns> {
    main_player().ok_or_else(|| "no character is loaded".to_string())
}

/// Address of the player's ChrIns (the start of PlayerIns), for calling game functions.
pub fn player_chr_addr() -> GameResult<usize> {
    Ok(require_player()?.addr())
}

pub fn checked_effect_id(id: i64) -> GameResult<i32> {
    let id32 = i32::try_from(id).map_err(|_| format!("SpEffect id {id} is out of range"))?;
    let reg = crate::params::registry()?;
    if !reg.table("SpEffectParam")?.has_row(id as u64) {
        return Err(format!("there is no SpEffect {id} (no SpEffectParam row)"));
    }
    Ok(id32)
}

/// Applies an SpEffect to the player after checking the id exists.
pub fn player_apply_effect(id: i64) -> GameResult<()> {
    let id = checked_effect_id(id)?;
    crate::speffect::apply(player_chr_addr()?, id)
}

/// Ends an active SpEffect on the player. Returns how many instances were ended.
pub fn player_remove_effect(id: i64) -> GameResult<usize> {
    let id = i32::try_from(id).map_err(|_| format!("SpEffect id {id} is out of range"))?;
    crate::speffect::expire(player_chr_addr()?, id)
}

pub fn player_effects() -> GameResult<Vec<crate::speffect::Active>> {
    crate::speffect::active(player_chr_addr()?)
}

pub fn chr_data() -> GameResult<&'static mut ChrDataModule> {
    require_player()?.chr().data().ok_or_else(|| "the character's data isn't available".to_string())
}

/// PlayerGameData +0x84: the character's type (0 the host, 1 a white phantom, ...).
pub const PGD_CHR_TYPE: usize = 0x84;

pub fn player_game_data() -> GameResult<&'static mut PlayerGameData> {
    require_player()?.game_data().ok_or_else(|| "the character's game data isn't available".to_string())
}

// ── items ───────────────────────────────────────────────────────────────────

pub fn item_id(raw: i64) -> GameResult<ItemId> {
    let raw = u32::try_from(raw).map_err(|_| format!("item id {raw} is out of range"))?;
    ItemId::try_from(raw).map_err(|e| format!("invalid item id {raw:#x}: {e}"))
}

pub fn make_item(category: ItemCategory, param_id: i64) -> GameResult<i64> {
    let id = u32::try_from(param_id).map_err(|_| format!("param id {param_id} is out of range"))?;
    ItemId::new(category, id).map(|i| i.into_inner() as i64)
}

pub fn category_name(item: ItemId) -> &'static str {
    match item.category() {
        ItemCategory::Weapon => "weapon",
        ItemCategory::Protector => "armor",
        ItemCategory::Accessory => "ring",
        ItemCategory::Goods => "goods",
    }
}

/// Gives an item. With `popup` the game shows its normal "item received" box.
pub fn give_item(item: ItemId, quantity: u32, popup: bool) -> GameResult<()> {
    require_player()?;
    if quantity == 0 {
        return Ok(());
    }
    if popup { ds3::grant_item(item, quantity) } else { ds3::give_item_directly(item, quantity) }
}

pub fn remove_item(item: ItemId, quantity: u32) -> GameResult<()> {
    require_player()?;
    ds3::remove_item(item, quantity)
}

/// How many of `item` the player is carrying (all stacks combined).
pub fn item_count(item: ItemId) -> GameResult<u32> {
    let inventory = player_game_data()?.equipment().inventory();
    Ok(inventory.items().filter(|e| e.item_id == item).map(|e| e.quantity).sum())
}

/// Every inventory entry as (category, param id, quantity), key items first.
pub fn inventory() -> GameResult<Vec<(&'static str, u32, u32)>> {
    let inventory = player_game_data()?.equipment().inventory();
    Ok(inventory.items().filter(|e| e.quantity > 0).map(|e| (category_name(e.item_id), e.item_id.param_id(), e.quantity)).collect())
}

// ── player state (PlayerGameData fields; offsets from the Grand Archives CE table) ─────
//
// Relative to PlayerGameData: +0x70 soul level, +0x74 souls held, +0x78 total souls gained,
// +0x100 embered (u8), +0x102 / +0x103 Estus flasks allotted to HP / FP (u8).

fn pgd_addr() -> GameResult<usize> {
    Ok(player_game_data()?.addr())
}

pub fn pgd_read_i32(off: usize) -> GameResult<i32> {
    crate::memory::read::<i32>(pgd_addr()? + off).ok_or_else(|| "player data is unreadable".to_string())
}

pub fn pgd_read_u8(off: usize) -> GameResult<u8> {
    crate::memory::read::<u8>(pgd_addr()? + off).ok_or_else(|| "player data is unreadable".to_string())
}

fn pgd_write<T: Copy>(off: usize, value: T) -> GameResult<()> {
    let addr = pgd_addr()? + off;
    unsafe { std::ptr::write_volatile(addr as *mut T, value) };
    Ok(())
}

pub fn set_souls(souls: i64) -> GameResult<()> {
    pgd_write::<i32>(0x74, souls.clamp(0, 999_999_999) as i32)
}

pub fn embered() -> GameResult<bool> {
    Ok(pgd_read_u8(0x100)? != 0)
}

/// Becoming embered goes through the game's own transition (SpEffect 3290, the Ember
/// item's effect); leaving it clears the state byte (experimental: visuals may lag).
pub fn set_embered(on: bool) -> GameResult<()> {
    if on {
        if !embered()? {
            crate::speffect::apply(player_chr_addr()?, 3290)?;
        }
        Ok(())
    } else {
        pgd_write::<u8>(0x100, 0)
    }
}

/// Estus Flask (goods 150-171) and Ashen Estus Flask (190-211) charges, with allotments.
pub struct Estus {
    pub hp: u32,
    pub hp_max: u8,
    pub fp: u32,
    pub fp_max: u8,
}

fn is_hp_estus(id: ItemId) -> bool {
    id.category() == ItemCategory::Goods && (150..=171).contains(&id.param_id())
}

fn is_fp_estus(id: ItemId) -> bool {
    id.category() == ItemCategory::Goods && (190..=211).contains(&id.param_id())
}

/// A flask level's two goods: the odd id holds its charges, the even one (id - 1) is the flask
/// empty (quantity 1). The game swaps a flask to it as its last charge goes and back as it's
/// refilled (verified 2026-10-11: 151 x3, x2, x1, then 150 x1). A quantity of 0 drops the item.
fn is_empty_flask(id: ItemId) -> bool {
    id.param_id().is_multiple_of(2)
}

/// A flask entry's charges (an empty flask has none).
fn charges(e: &ds3::InventoryEntry) -> u32 {
    if is_empty_flask(e.item_id) { 0 } else { e.quantity }
}

/// Sets a flask entry's charges as the game does: the full flask with that quantity, or at 0 the
/// empty one.
fn set_charges(e: &mut ds3::InventoryEntry, charges: u32) {
    let level = e.item_id.param_id() & !1;
    let id = if charges == 0 { level } else { level | 1 };
    if let Ok(item) = ItemId::new(e.item_id.category(), id) {
        e.item_id = item;
        e.quantity = charges.max(1);
    }
}

/// The first of the Estus Flasks' goods and of the Ashen Estus Flasks' (two per level).
const HP_FLASKS: u32 = 150;
const FP_FLASKS: u32 = 190;
/// The Ashen Estus Flask picked up (Firelink Shrine's: item lot 4000505's flag).
const ASHEN_FLASK_PICKED: u32 = 6600;

/// A flask an earlier Kindle lost (it wrote 0 charges as quantity 0: the entry stays at 0, which
/// the menus don't show, or the game drops it) is given back empty: the entry made the empty flask
/// in place, or a new one at the other flask's level (the Estus Flask always, the Ashen Estus
/// Flask once it was picked up); a rest fills it. What was given back. For the character in its
/// own world (the flag is its own there).
pub fn repair_flasks() -> GameResult<Vec<&'static str>> {
    require_player()?;
    let mut given = Vec::new();
    let mut levels = [None, None];
    let inv = player_game_data()?.equipment().inventory();
    type Kind = (fn(ItemId) -> bool, u32, &'static str);
    let kinds: [Kind; 2] = [(is_hp_estus, HP_FLASKS, "Estus Flask"), (is_fp_estus, FP_FLASKS, "Ashen Estus Flask")];
    for (k, &(is, first, name)) in kinds.iter().enumerate() {
        if let Some(e) = inv.items().find(|e| is(e.item_id) && e.quantity > 0) {
            levels[k] = Some((e.item_id.param_id() - first) / 2);
        } else if let Some(e) = inv.items_mut().find(|e| is(e.item_id)) {
            // Left at quantity 0 (the menus don't show it): made the empty flask in place.
            levels[k] = Some((e.item_id.param_id() - first) / 2);
            set_charges(e, 0);
            given.push(name);
        }
    }
    let empty = |first: u32, level: u32| ItemId::new(ItemCategory::Goods, first + 2 * level);
    if levels[0].is_none() {
        ds3::give_item_directly(empty(HP_FLASKS, levels[1].unwrap_or(0))?, 1)?;
        given.push("Estus Flask");
    }
    if levels[1].is_none() && ds3::get_flag(ASHEN_FLASK_PICKED) == Ok(true) {
        ds3::give_item_directly(empty(FP_FLASKS, levels[0].unwrap_or(0))?, 1)?;
        given.push("Ashen Estus Flask");
    }
    Ok(given)
}

pub fn estus() -> GameResult<Estus> {
    let inv = player_game_data()?.equipment().inventory();
    let hp = inv.items().filter(|e| is_hp_estus(e.item_id)).map(charges).sum();
    let fp = inv.items().filter(|e| is_fp_estus(e.item_id)).map(charges).sum();
    Ok(Estus { hp, hp_max: pgd_read_u8(0x102)?, fp, fp_max: pgd_read_u8(0x103)? })
}

/// Refills or sets Estus charges, clamped to the allotment. `None` leaves one unchanged.
pub fn set_estus(hp: Option<i64>, fp: Option<i64>) -> GameResult<()> {
    let (hp_max, fp_max) = (pgd_read_u8(0x102)? as i64, pgd_read_u8(0x103)? as i64);
    let inv = player_game_data()?.equipment().inventory();
    for e in inv.items_mut() {
        if is_hp_estus(e.item_id)
            && let Some(v) = hp
        {
            set_charges(e, v.clamp(0, hp_max) as u32);
        } else if is_fp_estus(e.item_id)
            && let Some(v) = fp
        {
            set_charges(e, v.clamp(0, fp_max) as u32);
        }
    }
    Ok(())
}

// ── attributes and soul level ──────────────────────────────────────────────
//
// The game recomputes derived stats (max HP/FP/stamina, equip load, ...) from the
// attribute block by itself, so writing an attribute is enough (verified in game).

pub(crate) const ATTRIBUTES: [(&str, usize); 9] = [
    ("vigor", 0x44),
    ("attunement", 0x48),
    ("endurance", 0x4C),
    ("vitality", 0x6C),
    ("strength", 0x50),
    ("dexterity", 0x54),
    ("intelligence", 0x58),
    ("faith", 0x5C),
    ("luck", 0x60),
];

fn attribute_offset(name: &str) -> GameResult<usize> {
    ATTRIBUTES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, off)| *off)
        .ok_or_else(|| format!("unknown attribute {name:?} (vigor, attunement, endurance, vitality, strength, dexterity, intelligence, faith, luck)"))
}

/// Sets an attribute (1-99). Unless `keep_level`, soul level moves by the same amount,
/// as if the points were levelled (soul level isn't recomputed from attributes because
/// starting classes, including randomized ones, set their own base).
pub fn set_attribute(name: &str, value: i64, keep_level: bool) -> GameResult<()> {
    let off = attribute_offset(name)?;
    let new = value.clamp(1, 99) as i32;
    let old = pgd_read_i32(off)?;
    pgd_write::<i32>(off, new)?;
    if !keep_level {
        let level = pgd_read_i32(0x70)? as i64;
        set_soul_level(level + (new - old) as i64)?;
    }
    Ok(())
}

pub fn set_soul_level(level: i64) -> GameResult<()> {
    pgd_write::<i32>(0x70, level.clamp(1, 802) as i32)
}

// ── equipment (PlayerGameData + 0x32C..; offsets from the Grand Archives CE table) ─────

/// (slot, offset, category). `None` = the slot already stores a full item id.
pub const EQUIP_SLOTS: &[(&str, usize, Option<ItemCategory>)] = &[
    ("left1", 0x32C, Some(ItemCategory::Weapon)),
    ("right1", 0x330, Some(ItemCategory::Weapon)),
    ("left2", 0x334, Some(ItemCategory::Weapon)),
    ("right2", 0x338, Some(ItemCategory::Weapon)),
    ("left3", 0x33C, Some(ItemCategory::Weapon)),
    ("right3", 0x340, Some(ItemCategory::Weapon)),
    ("arrow1", 0x344, Some(ItemCategory::Weapon)),
    ("bolt1", 0x348, Some(ItemCategory::Weapon)),
    ("arrow2", 0x34C, Some(ItemCategory::Weapon)),
    ("bolt2", 0x350, Some(ItemCategory::Weapon)),
    ("head", 0x35C, Some(ItemCategory::Protector)),
    ("body", 0x360, Some(ItemCategory::Protector)),
    ("arms", 0x364, Some(ItemCategory::Protector)),
    ("legs", 0x368, Some(ItemCategory::Protector)),
    ("ring1", 0x370, Some(ItemCategory::Accessory)),
    ("ring2", 0x374, Some(ItemCategory::Accessory)),
    ("ring3", 0x378, Some(ItemCategory::Accessory)),
    ("ring4", 0x37C, Some(ItemCategory::Accessory)),
    ("covenant", 0x380, Some(ItemCategory::Accessory)),
    ("quick1", 0x5AC, None),
    ("quick2", 0x5B0, None),
    ("quick3", 0x5B4, None),
    ("quick4", 0x5B8, None),
    ("quick5", 0x5BC, None),
    ("quick6", 0x5C0, None),
    ("quick7", 0x5C4, None),
    ("quick8", 0x5C8, None),
    ("quick9", 0x5CC, None),
    ("quick10", 0x5D0, None),
    ("tool1", 0x5D4, None),
    ("tool2", 0x5D8, None),
    ("tool3", 0x5DC, None),
    ("tool4", 0x5E0, None),
    ("tool5", 0x5E4, None),
];

/// What an emptied hand or armour slot holds: the game fills it with Fists or the bare piece
/// (equip.rs puts them back the same way), which is no item to a mod.
const EMPTY_FILLS: [(ItemCategory, i64); 5] = [
    (ItemCategory::Weapon, 110000),
    (ItemCategory::Protector, 900000),
    (ItemCategory::Protector, 901000),
    (ItemCategory::Protector, 902000),
    (ItemCategory::Protector, 903000),
];

fn equipped_item(raw: i32, category: Option<ItemCategory>) -> Option<i64> {
    if raw < 0 {
        return None;
    }
    if let Some(c) = category
        && EMPTY_FILLS.contains(&(c, raw as i64))
    {
        return None;
    }
    match category {
        Some(c) => make_item(c, raw as i64).ok(),
        None => item_id(raw as u32 as i64).ok().map(|i| i.into_inner() as i64),
    }
}

/// Equipped items by slot (empty slots are left out).
pub fn equipment() -> GameResult<Vec<(&'static str, i64)>> {
    let mut out = Vec::new();
    for &(slot, off, category) in EQUIP_SLOTS {
        if let Some(item) = equipped_item(pgd_read_i32(off)?, category) {
            out.push((slot, item));
        }
    }
    Ok(out)
}

/// Attuned spells (goods ids), in slot order; empty slots are left out.
pub fn spells() -> GameResult<Vec<i64>> {
    let list = crate::memory::read_u64(pgd_addr()? + 0x470).filter(|&p| p != 0).ok_or("no spell list")? as usize;
    let mut out = Vec::new();
    for i in 0..14 {
        let raw = crate::memory::read::<i32>(list + 0x18 + i * 8).unwrap_or(-1);
        if raw >= 0
            && let Ok(item) = make_item(ItemCategory::Goods, raw as i64)
        {
            out.push(item);
        }
    }
    Ok(out)
}

// ── playthrough progress (GameDataMan fields) ──────────────────────────────

fn gdm_read<T: Copy + Default>(off: usize) -> GameResult<T> {
    let gdm = ds3::GAME_DATA_MAN.instance().ok_or("game data is not available yet")?;
    crate::memory::read::<T>(gdm + off).ok_or_else(|| "game data is unreadable".to_string())
}

/// 0 on the first playthrough, 1 in NG+, ...
pub fn ng_cycle() -> GameResult<i32> {
    gdm_read::<i32>(0x78)
}

pub fn deaths() -> GameResult<i32> {
    gdm_read::<i32>(0x98)
}

/// Play time in seconds.
pub fn play_time() -> GameResult<f64> {
    Ok(gdm_read::<u32>(0xA4)? as f64 / 1000.0)
}

/// Play time in milliseconds. It stands still on loading screens.
pub fn play_time_ms() -> Option<u32> {
    gdm_read::<u32>(0xA4).ok()
}

// ── in the world ─────────────────────────────────────────────────────────────

/// The player is in the world: the character exists and the loading screen is over (set by
/// the runtime each frame). The character exists from the start of the loading screen, but
/// the map's scripts set the world up (dormant enemies' AI, for one) when it ends.
static IN_WORLD: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Since when the player is in the world (None while not).
static IN_WORLD_SINCE: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

pub fn in_world() -> bool {
    IN_WORLD.load(std::sync::atomic::Ordering::Relaxed)
}

/// When the character came into the world (this load), if it's there.
pub fn in_world_since() -> Option<std::time::Instant> {
    *IN_WORLD_SINCE.lock().unwrap_or_else(|e| e.into_inner())
}

/// How long the player has been in the world (None while not).
pub fn in_world_for() -> Option<std::time::Duration> {
    IN_WORLD_SINCE.lock().unwrap_or_else(|e| e.into_inner()).map(|t| t.elapsed())
}

pub fn set_in_world(v: bool) {
    IN_WORLD.store(v, std::sync::atomic::Ordering::Relaxed);
    let mut since = IN_WORLD_SINCE.lock().unwrap_or_else(|e| e.into_inner());
    match (v, since.is_some()) {
        (true, false) => *since = Some(std::time::Instant::now()),
        (false, true) => *since = None,
        _ => {}
    }
}

/// Whether a character's AI is on: bit 0 of [ChrIns+0x50]+0x182, which the game's own AI
/// setter (1.15.2 +0x8887E0, used by event command 2004[01]) writes.
pub fn ai_enabled(chr: usize) -> Option<bool> {
    let module = crate::memory::read_u64(chr + 0x50).filter(|&p| p != 0)? as usize;
    crate::memory::read::<u8>(module + 0x182).map(|b| b & 1 != 0)
}

/// Sets that bit as the game's AI setter (+0x8887E0) does; None when the character has no
/// AI module.
pub fn set_ai_enabled(chr: usize, on: bool) -> Option<()> {
    let module = crate::memory::read_u64(chr + 0x50).filter(|&p| p != 0)? as usize;
    let flags = crate::memory::read::<u8>(module + 0x182)?;
    unsafe { std::ptr::write_volatile((module + 0x182) as *mut u8, (flags & !1) | on as u8) };
    Some(())
}

// ── HUD setting (Settings > HUD: 0 off, 1 on, 2 auto) ────────────────────────────────
//
// The game keeps the option in two places, both written by its settings menu (offsets from
// the Grand Archives CE table): [GameDataMan+0x58]+0x11 and [GameDataMan+0x60]+0xF91.
// Changing it here behaves like the menu (unlike the event command that hides the HUD,
// which also blocks the pause menu).

fn hud_option_addrs() -> GameResult<[usize; 2]> {
    let base = ds3::GAME_DATA_MAN.instance().ok_or("GameDataMan is not available")?;
    let a = crate::memory::read_u64(base + 0x58).filter(|&p| p != 0).ok_or("game options aren't loaded")? as usize;
    let b = crate::memory::read_u64(base + 0x60).filter(|&p| p != 0).ok_or("game options aren't loaded")? as usize;
    Ok([a + 0x11, b + 0xF91])
}

pub fn hud() -> GameResult<u8> {
    let [a, _] = hud_option_addrs()?;
    crate::memory::read::<u8>(a).ok_or_else(|| "the HUD setting is unreadable".to_string())
}

pub fn set_hud(value: u8) -> GameResult<()> {
    if value > 2 {
        return Err("HUD setting must be 0 (off), 1 (on) or 2 (auto)".into());
    }
    for addr in hud_option_addrs()? {
        crate::memory::read::<u8>(addr).ok_or("the HUD setting is unreadable")?;
        unsafe { std::ptr::write_volatile(addr as *mut u8, value) };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `kindle.boss_flags` in the editor stubs lists exactly BOSSES.
    #[test]
    fn stubs_list_every_boss() {
        let stubs = include_str!("../../../dist/stubs/kindle.lua");
        let class = stubs.split("---@class kindle.boss_flags").nth(1).expect("boss_flags class in the stubs");
        let fields: Vec<String> = class.lines().skip(1).take_while(|l| l.starts_with("---@field")).map(|l| l.trim_end().to_string()).collect();
        let expected: Vec<String> = BOSSES.iter().map(|(name, flag, display)| format!("---@field {name} integer {flag}: {display}")).collect();
        assert_eq!(fields, expected);
    }
}
