//! The world's objects: doors, levers, chests, ladders, breakables, the map's other placed things.
//!
//! Read from the game's object manager (`WorldObjManImp`), mapped live at Firelink:
//!
//! | | |
//! |---|---|
//! | blocks | `[man + 0x28]`, `man + 0x20` of them, 0x40288 bytes each (one per map) |
//! | a block's map | `[block + 0x8]` (WorldBlockInfo) `+ 0x8`: the map id, 0xAABBCCDD = mAA_BB_CC_DD |
//! | its objects | `block + 0x200E8` slots; slot n's index at `block + 0xE8 + 4n`, the ObjIns at `block + 0x100E8 + 8 * index` |
//! | entity ids | `[block + 0x40138]`: (entity id, handle) pairs sorted by entity id, `block + 0x40134` of them |
//! | an object | handle `+0x8`; a 3x4 matrix at `+0x260` stored transposed, so its position is `+0x26C`, `+0x27C`, `+0x28C` (the game's own reading, +0x6643E0) |
//! | destroyed | `+0x18F` bit 0x04 (+0x6645D0 reads it; the event condition "object destroyed", EcTestObjectDestoryState, counts objects through Obj_GetDestroyedNum +0x52CCA0, whose visit +0x52C590 asks each object's EventObjEntity at `+0x180`, which asks its ObjIns at `+0x10` this) |

use std::collections::HashMap;

use crate::{ds3, memory};

const BLOCK_SIZE: usize = 0x40288;

pub struct Object {
    pub handle: u32,
    pub entity_id: Option<i32>,
    pub map: String,
    pub pos: [f32; 3],
    /// Broken (a pot, a breakable wall), as the game's event scripts read it.
    pub destroyed: Option<bool>,
}

/// Every loaded object, block by block.
pub fn all() -> Result<Vec<Object>, String> {
    let man = ds3::singleton_instance("WorldObjManImp").ok_or("the game's objects aren't there yet")?;
    let read_ptr = |a: usize| memory::read::<usize>(a).filter(|&p| p != 0);
    let (Some(blocks), Some(count)) = (read_ptr(man + 0x28), memory::read::<i32>(man + 0x20)) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for b in 0..count.clamp(0, 256) as usize {
        let block = blocks + b * BLOCK_SIZE;
        let slots = memory::read::<u32>(block + 0x200E8).unwrap_or(0).min(0x4000) as usize;
        if slots == 0 {
            continue;
        }
        let map = read_ptr(block + 0x8).and_then(|info| memory::read::<u32>(info + 0x8)).map(map_name).unwrap_or_default();
        let mut entities = HashMap::new();
        if let (Some(pairs), Some(n)) = (read_ptr(block + 0x40138), memory::read::<i32>(block + 0x40134)) {
            for i in 0..n.clamp(0, 0x4000) as usize {
                if let (Some(entity), Some(handle)) = (memory::read::<i32>(pairs + i * 8), memory::read::<u32>(pairs + i * 8 + 4)) {
                    entities.insert(handle, entity);
                }
            }
        }
        for slot in 0..slots {
            let Some(index) = memory::read::<u32>(block + 0xE8 + slot * 4) else { continue };
            let Some(obj) = read_ptr(block + 0x100E8 + index as usize * 8) else { continue };
            let Some(handle) = memory::read::<u32>(obj + 0x8) else { continue };
            let f = |off: usize| memory::read::<f32>(obj + off).unwrap_or(f32::NAN);
            out.push(Object {
                handle,
                entity_id: entities.get(&handle).copied(),
                map: map.clone(),
                pos: [f(0x26C), f(0x27C), f(0x28C)],
                destroyed: memory::read::<u8>(obj + 0x18F).map(|b| b & 0x04 != 0),
            });
        }
    }
    Ok(out)
}

/// 0x28000000 → "m40_00_00_00".
fn map_name(id: u32) -> String {
    let [a, b, c, d] = id.to_be_bytes();
    format!("m{a:02}_{b:02}_{c:02}_{d:02}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn map_ids_read_as_names() {
        assert_eq!(super::map_name(0x2800_0000), "m40_00_00_00");
        assert_eq!(super::map_name(0x1F05_0000), "m31_05_00_00");
    }
}
