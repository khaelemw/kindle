//! Applying and removing SpEffects (buffs, debuffs, states) on characters.
//!
//! Calls the game's own "apply or remove SpEffect by id" helper:
//!
//! ```text
//! helper(obj, sp_effect_id, mode):
//!     chr = [obj + 8]
//!     if mode != 0: return apply_variant(chr, id, 1)   // another apply path, NOT a removal
//!     apply_sp_effect(chr, id, source = chr, 0, &{1,1,1,1}, <const>, 0, 0, 0)
//! ```
//!
//! Active effects are a linked list (layout from the Grand Archives CE table, verified live):
//! `chr+0x18 -> +0x18 = SpEffect manager; +0x08 = first node`; per node: `+0x00 f32 time
//! left`, `+0x04 f32 duration` (-1 = permanent), `+0x08 f32 interval`, `+0x60 i32 id`,
//! `+0x78 next`. Removal sets the node's time left to 0 so the game expires it itself.
//!
//! Found by reverse engineering (2026-10-02): located in the decrypted DS3 1.15.0 exe as the
//! smallest caller of the full apply function (the one DS3RuntimeScripting hooks at
//! 1.15.0 `+0x87CE00`), turned into a byte pattern with build-specific operands
//! wildcarded, and matched exactly once in 1.15.2 (`+0x8CE710`). The helper only reads
//! `obj + 8`, so Kindle passes a small shim struct holding the character pointer.

use std::sync::OnceLock;

use crate::{memory, patterns};

/// `helper` from the module docs; `?` bytes are relative jump/call/data offsets.
const PATTERN: &str = "48 83 ec 68 48 8b 49 08 45 85 c0 74 0c 41 b0 01 48 83 c4 68 e9 ?? ?? ?? ?? \
    f3 0f 10 0d ?? ?? ?? ?? c6 44 24 40 00 c7 44 24 50 00 00 80 3f c7 44 24 54 00 00 80 3f \
    c7 44 24 58 00 00 80 3f c7 44 24 5c 00 00 80 3f 0f 28 44 24 50 66 0f 7f 44 24 50 \
    c7 44 24 38 00 00 00 00 48 8d 44 24 50 c6 44 24 30 00 f3 0f 11 4c 24 28 45 33 c9 \
    4c 8b c1 48 89 44 24 20 e8 ?? ?? ?? ?? 48 83 c4 68 c3";

static HELPER: OnceLock<Option<usize>> = OnceLock::new();

fn helper() -> Result<usize, String> {
    HELPER
        .get_or_init(|| {
            let found = patterns::find_unique("speffect: game helper", PATTERN);
            if found.is_none() {
                crate::warn!("speffect: effects are unavailable");
            }
            found
        })
        .ok_or_else(|| "SpEffects are unavailable on this game build".to_string())
}

#[repr(C)]
struct Shim {
    _unused: usize,
    chr: usize,
}

type HelperFn = unsafe extern "C" fn(*const Shim, i32, i32);

/// Applies an SpEffect to the character at `chr`. The id must already be validated
/// against SpEffectParam.
pub fn apply(chr: usize, id: i32) -> Result<(), String> {
    if chr == 0 {
        return Err("no character".into());
    }
    let f: HelperFn = unsafe { std::mem::transmute(helper()?) };
    let shim = Shim { _unused: 0, chr };
    unsafe { f(&shim, id, 0) };
    Ok(())
}

pub struct Active {
    pub id: i32,
    pub time_left: f32,
    pub duration: f32,
    node: usize,
}

/// The character's active SpEffects (oldest first).
pub fn active(chr: usize) -> Result<Vec<Active>, String> {
    let manager = memory::read_u64(chr + 0x18)
        .and_then(|p| memory::read_u64(p as usize + 0x18))
        .filter(|&p| p != 0)
        .ok_or("the character's SpEffect list is unavailable")? as usize;
    let mut node = memory::read_u64(manager + 0x08).unwrap_or(0) as usize;
    let mut out = Vec::new();
    while node != 0 && out.len() < 512 {
        let (Some(id), Some(time_left), Some(duration)) = (memory::read::<i32>(node + 0x60), memory::read::<f32>(node), memory::read::<f32>(node + 0x04))
        else {
            break;
        };
        out.push(Active { id, time_left, duration, node });
        node = memory::read_u64(node + 0x78).unwrap_or(0) as usize;
    }
    Ok(out)
}

/// Ends every active instance of `id` by setting its time left to 0; the game removes it
/// on its next update. Returns how many instances were ended.
pub fn expire(chr: usize, id: i32) -> Result<usize, String> {
    let list = active(chr)?;
    let mut n = 0;
    for a in list.iter().filter(|a| a.id == id) {
        unsafe { std::ptr::write_volatile(a.node as *mut f32, 0.0) };
        n += 1;
    }
    Ok(n)
}
