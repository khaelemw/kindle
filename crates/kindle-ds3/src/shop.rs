//! The game's shop purchases, for the `shop_buy` event.
//!
//! A purchase (+0xBA43F0 in 1.15.2, on the purchase being confirmed; the ShopLineupParam row
//! at +0x44, the quantity at +0x58) gets the bought item, adds the quantity to the inventory
//! (the storage box takes what doesn't fit) and counts it against the row's stock flag. The
//! souls are taken after it returns, by its caller (code the game decodes as it runs), so the
//! event reports a purchase; it can't undo one (verified: 3 of row 110018 at 600 each, the
//! souls unchanged when the purchase returned and 1800 lower after).

use std::sync::{Mutex, OnceLock};

const PURCHASE: &str =
    "40 53 48 83 ec 30 48 c7 44 24 20 fe ff ff ff 48 8b d9 48 8d 44 24 40 48 89 44 24 48 48 8d 54 24 40 e8 ?? ?? ?? ?? 90 8b 53 58 48 8b c8 e8";
const ROW: usize = 0x44;
const QUANTITY: usize = 0x58;

/// Purchases since the last `take`: (ShopLineupParam row, quantity).
static BOUGHT: Mutex<Vec<(i32, i32)>> = Mutex::new(Vec::new());

static PURCHASE_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "shop",
    name: "shop purchase",
    pattern: PURCHASE,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::Retn(purchase_hook),
    without: "mods don't see purchases",
};

/// Watches the game's purchases (once).
pub fn watch() -> Result<(), String> {
    static DONE: OnceLock<Result<(), String>> = OnceLock::new();
    DONE.get_or_init(|| PURCHASE_SITE.install().map(|_| ()).ok_or_else(|| "the game's shop purchase wasn't found or hooked on this game build".to_string()))
        .clone()
}

fn purchase_hook(reg: *mut ilhook::x64::Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let purchase = r.rcx as usize;
    let row = crate::memory::read::<i32>(purchase + ROW);
    let quantity = crate::memory::read::<i32>(purchase + QUANTITY);
    let f: extern "C" fn(usize, usize, usize, usize) -> usize = unsafe { std::mem::transmute(original) };
    let out = f(r.rcx as usize, r.rdx as usize, r.r8 as usize, r.r9 as usize);
    if let (Some(row), Some(quantity)) = (row, quantity)
        && quantity > 0
    {
        BOUGHT.lock().unwrap_or_else(|e| e.into_inner()).push((row, quantity));
    }
    out
}

/// Purchases since the last call: (ShopLineupParam row, quantity).
pub fn take() -> Vec<(i32, i32)> {
    std::mem::take(&mut *BOUGHT.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The item a ShopLineupParam row sells (its equipType: 0 weapon, 1 armor, 2 ring, 3 goods),
/// as a full item id.
pub fn item_of(row: i32) -> Option<i64> {
    use kindle_paramdef::Value;
    let reg = crate::params::registry().ok()?;
    let t = reg.table("ShopLineupParam").ok()?;
    let int = |field: &str| match t.read(row as u64, field) {
        Ok(Value::Int(i)) => Some(i),
        _ => None,
    };
    let kind = ["weapon", "armor", "ring", "goods"].get(usize::try_from(int("equipType")?).ok()?)?;
    let category = crate::text::Kind::parse(kind)?.category();
    crate::game::make_item(category, int("equipId")?).ok()
}
