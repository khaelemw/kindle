//! Playing animations (Havok behaviour events) on characters.
//!
//! Chain (from DS3RuntimeScripting, re-verified for 1.15.2):
//!
//! ```text
//! B = [[[[ChrIns + 0x1F90] + 0x28] + 0x10] + 0x28]
//! hkbCharacter = [B + 0xB8]          event queue owner, passed to the play function
//! names        = [B + 0xA0]          event name table: [names + 0x40] = char*[count],
//!                                    [names + 0x48] = count (entries have bit 0 set as a flag)
//! ```
//!
//! The play function `(hkbCharacter, const i32 event[6] = { id, 0, ... })` queues the event.
//! It was located in the decrypted 1.15.0 exe (`+0xD84870`, used by DS3RuntimeScripting),
//! turned into a pattern (relative call wildcarded) and matched once in 1.15.2
//! (`+0xD913E0`). Names are read straight from the table, so no lookup function is needed.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::{memory, patterns};

pub(crate) const PLAY_PATTERN: &str = "48 89 5c 24 08 57 48 83 ec 20 48 8b fa 8b 51 08 48 8b d9 39 51 14 7c 12 \
    85 d2 75 07 ba 08 00 00 00 eb 02 03 d2 e8 ?? ?? ?? ?? 8b 43 08 39 43 10 75 07 c7 43 10 00 00 00 00 \
    48 63 43 10 48 8d 0c 40 48 8b 03 48 8d 14 c8 48 8b 07 48 89 02 48 8b 47 08 48 89 42 08 \
    48 8b 47 10 48 89 42 10 ff 43";

static PLAY: OnceLock<Option<usize>> = OnceLock::new();
/// Event names of one behaviour, by id and by lowercase name.
struct Names {
    by_id: Vec<String>,
    by_name: HashMap<String, i32>,
}

/// names-table address → names, built once per behaviour.
static NAME_CACHE: Mutex<Option<HashMap<usize, Names>>> = Mutex::new(None);

/// The game's "queue animation event" function.
pub fn play_function() -> Option<usize> {
    *PLAY.get_or_init(|| patterns::find_unique("animation", PLAY_PATTERN))
}

/// The hkbCharacter the play function receives for this character.
pub fn hkb_character(chr: usize) -> Option<usize> {
    let root = behaviour_root(chr)?;
    memory::read_u64(root + 0xB8).filter(|&p| p != 0).map(|p| p as usize)
}

fn with_names<R>(chr: usize, f: impl FnOnce(&Names) -> R) -> Result<R, String> {
    let root = behaviour_root(chr).ok_or("this character has no animation behaviour (inactive or not loaded in)")?;
    let table = memory::read_u64(root + 0xA0).unwrap_or(0) as usize;
    let mut slot = NAME_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = slot.get_or_insert_with(HashMap::new);
    if let std::collections::hash_map::Entry::Vacant(slot) = cache.entry(table) {
        let list = names(chr)?;
        let mut by_id = vec![String::new(); list.iter().map(|(i, _)| *i as usize + 1).max().unwrap_or(0)];
        let mut by_name = HashMap::new();
        for (id, n) in list {
            by_name.insert(n.to_lowercase(), id);
            by_id[id as usize] = n;
        }
        slot.insert(Names { by_id, by_name });
    }
    Ok(f(&cache[&table]))
}

/// Name of an animation event id for this character.
pub fn name_of(chr: usize, id: i32) -> Option<String> {
    with_names(chr, |n| n.by_id.get(id as usize).filter(|s| !s.is_empty()).cloned()).ok().flatten()
}

fn behaviour_root(chr: usize) -> Option<usize> {
    let mut p = memory::read_u64(chr + 0x1F90)? as usize;
    for off in [0x28, 0x10, 0x28] {
        if p == 0 {
            return None;
        }
        p = memory::read_u64(p + off)? as usize;
    }
    (p != 0).then_some(p)
}

fn read_cstr(addr: usize, max: usize) -> Option<String> {
    let bytes = memory::read_bytes(addr, max)?;
    let end = bytes.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

/// All animation event names a character's behaviour understands, by id.
pub fn names(chr: usize) -> Result<Vec<(i32, String)>, String> {
    let root = behaviour_root(chr).ok_or("this character has no animation behaviour (inactive or not loaded in)")?;
    let table = memory::read_u64(root + 0xA0).filter(|&p| p != 0).ok_or("no animation name table")? as usize;
    let arr = memory::read_u64(table + 0x40).ok_or("no animation name table")? as usize;
    let count = memory::read::<i32>(table + 0x48).unwrap_or(0).clamp(0, 100_000) as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let Some(p) = memory::read_u64(arr + i * 8) else { break };
        let p = (p & !1) as usize;
        if p != 0
            && let Some(s) = read_cstr(p, 256)
        {
            out.push((i as i32, s));
        }
    }
    Ok(out)
}

/// Resolves an animation event name (case-insensitive) to its id for this character.
pub fn id_of(chr: usize, name: &str) -> Result<i32, String> {
    with_names(chr, |n| n.by_name.get(&name.to_lowercase()).copied())?
        .ok_or_else(|| format!("this character has no animation event {name:?} (see world.animations)"))
}

/// Queues an animation event on a character.
pub fn play(chr: usize, id: i32) -> Result<(), String> {
    let root = behaviour_root(chr).ok_or("this character has no animation behaviour (inactive or not loaded in)")?;
    let hkb = memory::read_u64(root + 0xB8).filter(|&p| p != 0).ok_or("no behaviour character")? as usize;
    let count = names(chr).map(|n| n.len()).unwrap_or(0);
    if id < 0 || (count > 0 && id as usize >= count) {
        return Err(format!("animation id {id} is out of range (0..{count})"));
    }
    let f = play_function().ok_or("animations are unavailable on this game build")?;
    let f: unsafe extern "C" fn(usize, *const [i32; 6]) = unsafe { std::mem::transmute(f) };
    let event = [id, 0, 0, 0, 0, 0];
    unsafe { f(hkb, &event) };
    Ok(())
}
