//! Text overrides: change what the game displays for any FMG entry (item names and
//! descriptions, NPC names, event text...), live.
//!
//! Each FMG's string offset table holds, per entry, an offset from the FMG base to the
//! string. The game resolves it on every lookup, so pointing an entry at our own UTF-16
//! buffer (offset = buffer - base, wrapping) changes the displayed text immediately.
//! Only data is written, never game code, and the original offset is kept for reverting.
//! Override strings are interned and never freed, because the game may still be showing one.
//!
//! The game frees its text tables when it unloads a text bundle (see RELEASE: as it closes). A
//! slot Kindle pointed at a header of its own gets the game's FMG back first, so the game frees
//! only what it allocated; Kindle's changes to a released category go with it, and are made
//! again if the game loads that text again.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::memory;
use crate::text::Kind;

/// Named text categories → FMG ids (base game, then DLC1, DLC2 where they exist).
/// Mapped from DS3 1.15.2's MsgRepository.
pub const CATEGORIES: &[(&str, &[u32])] = &[
    ("goods_name", &[10, 210, 250]),
    ("weapon_name", &[11, 211, 251]),
    ("armor_name", &[12, 212, 252]),
    ("ring_name", &[13, 213, 253]),
    ("spell_name", &[14, 214, 254]),
    ("npc_name", &[18, 215, 255]),
    ("place_name", &[19, 216, 256]),
    ("goods_info", &[20, 217, 257]),
    ("weapon_info", &[21]),
    ("armor_info", &[22]),
    ("ring_info", &[23, 220, 260]),
    ("goods_caption", &[24, 221, 261]),
    ("weapon_caption", &[25, 222, 262]),
    ("armor_caption", &[26, 223, 263]),
    ("ring_caption", &[27, 224, 264]),
    ("spell_info", &[28, 225, 265]),
    ("spell_caption", &[29, 226, 266]),
    ("talk", &[1]),
    ("event_text", &[30]),
];

/// Resolves a category given by name ("goods_name") or raw FMG id ("10").
pub fn category_ids(spec: &str) -> Result<Vec<u32>, String> {
    if let Ok(n) = spec.parse::<u32>() {
        return Ok(vec![n]);
    }
    CATEGORIES.iter().find(|(name, _)| *name == spec).map(|(_, ids)| ids.to_vec()).ok_or_else(|| {
        let names: Vec<&str> = CATEGORIES.iter().map(|(n, _)| *n).collect();
        format!("unknown text category {spec:?} (use an FMG id or one of: {})", names.join(", "))
    })
}

/// FMG ids for an item's name / info (short) / description (long) text.
pub fn item_categories(kind: Kind, field: &str) -> Result<Vec<u32>, String> {
    let name = match (kind, field) {
        (Kind::Goods, "name") => "goods_name",
        (Kind::Weapon, "name") => "weapon_name",
        (Kind::Armor, "name") => "armor_name",
        (Kind::Ring, "name") => "ring_name",
        (Kind::Goods, "info") => "goods_info",
        (Kind::Weapon, "info") => "weapon_info",
        (Kind::Armor, "info") => "armor_info",
        (Kind::Ring, "info") => "ring_info",
        (Kind::Goods, "description") => "goods_caption",
        (Kind::Weapon, "description") => "weapon_caption",
        (Kind::Armor, "description") => "armor_caption",
        (Kind::Ring, "description") => "ring_caption",
        (_, other) => return Err(format!("unknown item text field {other:?} (name, info, description)")),
    };
    category_ids(name)
}

/// Address of the string-offset entry for `id` in FMG `category`, plus that FMG's base.
fn entry_address(category: u32, id: u32) -> Option<(usize, usize)> {
    let repo = crate::ds3::MSG_REPOSITORY.instance()?;
    let versions = memory::read_u64(repo + 0x08)? as usize;
    let version_count = (memory::read::<u32>(repo + 0x10)? as usize).min(64);
    // Later versions take precedence.
    for v in (0..version_count).rev() {
        let cats = memory::read_u64(versions + v * 8)? as usize;
        if cats == 0 {
            continue;
        }
        let f = memory::read_u64(cats + category as usize * 8)? as usize;
        if f == 0 {
            continue;
        }
        let groups = memory::read::<i32>(f + 0x0C)?;
        let table = memory::read_u64(f + 0x18)? as usize;
        for g in 0..groups.max(0) as usize {
            let at = f + 0x28 + g * 16;
            let index = memory::read::<i32>(at)?;
            let first = memory::read::<i32>(at + 4)?;
            let last = memory::read::<i32>(at + 8)?;
            if (first..=last).contains(&(id as i32)) {
                return Some((table + (index + id as i32 - first) as usize * 8, f));
            }
        }
    }
    None
}

/// Current text of an entry, as the game would show it (overrides included).
pub fn get(category: u32, id: u32) -> Option<String> {
    let (entry, base) = entry_address(category, id)?;
    let off = memory::read_u64(entry)?;
    if off == 0 {
        return None;
    }
    memory::read_wstr(base.wrapping_add(off as usize), 4096)
}

/// First non-empty text across categories, checking DLC2, then DLC1, then base.
pub fn get_any(categories: &[u32], id: u32) -> Option<String> {
    categories.iter().rev().find_map(|c| get(*c, id).filter(|s| !s.is_empty()))
}

struct Override {
    entry: usize,
    original: u64,
    owner: String,
    text: String,
}

static OVERRIDES: Mutex<Option<HashMap<(u32, u32), Override>>> = Mutex::new(None);
static INTERNED: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);
/// Repository slots Kindle pointed at an FMG header of its own (add_entry), each with the game's
/// FMG that was there before Kindle's first.
static REPLACED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
/// Overrides whose FMG the game released, to make again once it has the category's text back:
/// (category, id, text, owner).
static PENDING: Mutex<Vec<(u32, u32, String, String)>> = Mutex::new(Vec::new());
static RELEASE_HOOKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The game's release of a text table (+0x2354460; the repository, a version, a category): it
/// frees the FMG in the repository's slot and empties it. It runs as the game unloads a text
/// bundle (MsgbndFileCap's destructor, for each category in it), which its start-up steps do as
/// it closes (SysStep, +0x9067E0, once the title and game task ends). Given Kindle's header
/// (add_entry), the game's allocator freed memory it never allocated and the game crashed as it
/// closed (heap corruption, or reading 0xffffffffffffffff; verified 2026-10-12 by closing a game
/// whose event text had Kindle's header, as every game's had for a mod's menu option).
const RELEASE: &str = "48 89 5c 24 18 48 89 74 24 20 57 48 83 ec 20 41 8b f8 8b f2 48 8b d9 44 3b 41 14 73 ?? 48 8b 41 08";

static RELEASE_SITE: crate::hook_sites::Site = crate::hook_sites::Site {
    area: "text",
    name: "text release",
    pattern: RELEASE,
    offset: 0,
    matches: 1,
    kind: crate::hook_sites::Kind::JmpBack(release_hook),
    without: "text ids the game doesn't have can't be added",
};

pub fn install() {
    if RELEASE_SITE.install().is_some() {
        RELEASE_HOOKED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn release_hook(reg: *mut ilhook::x64::Registers) {
    let r = unsafe { &*reg };
    let repo = r.rcx as usize;
    if crate::ds3::MSG_REPOSITORY.instance() != Some(repo) {
        return;
    }
    let (version, category) = (r.rdx as u32 as usize, r.r8 as u32);
    let Some(cats) = memory::read_u64(repo + 0x08).and_then(|v| memory::read_u64(v as usize + version * 8)).filter(|&c| c != 0) else {
        return;
    };
    let slot = cats as usize + category as usize * 8;
    {
        let mut replaced = REPLACED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(i) = replaced.iter().position(|&(s, _)| s == slot) {
            let (_, game) = replaced.swap_remove(i);
            unsafe { std::ptr::write_volatile(slot as *mut u64, game as u64) };
        }
    }
    let mut overrides = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = overrides.as_mut() else { return };
    let keys: Vec<(u32, u32)> = map.keys().filter(|k| k.0 == category).copied().collect();
    if keys.is_empty() {
        return;
    }
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    for k in keys {
        if let Some(ov) = map.remove(&k) {
            pending.push((k.0, k.1, ov.text, ov.owner));
        }
    }
    crate::info!("text: the game released its text {category}; Kindle's changes to it are made again once it's back");
}

/// Each frame: overrides of a category the game released, made again once it's back.
pub fn tick() {
    let waiting = {
        let Ok(mut pending) = PENDING.try_lock() else { return };
        if pending.is_empty() {
            return;
        }
        std::mem::take(&mut *pending)
    };
    let mut made = 0;
    let mut later = Vec::new();
    for (category, id, text, owner) in waiting {
        if fmg_slot(category).is_none() {
            later.push((category, id, text, owner));
            continue;
        }
        match set_or_add(&[category], id, &text, &owner) {
            Ok(_) => made += 1,
            Err(e) => crate::warn!("text: {category} {id} couldn't be made again ({e})"),
        }
    }
    if made > 0 {
        crate::info!("text: {made} change(s) made again on the game's reloaded text");
    }
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).extend(later);
}

/// A change made or undone now isn't one to make again later.
fn drop_pending(keep: impl Fn(&(u32, u32, String, String)) -> bool) {
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).retain(keep);
}

fn intern(text: &str) -> usize {
    let mut slot = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.get_or_insert_with(HashMap::new);
    if let Some(&p) = map.get(text) {
        return p;
    }
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    let ptr = Box::leak(units.into_boxed_slice()).as_ptr() as usize;
    map.insert(text.to_string(), ptr);
    ptr
}

/// Overrides the text of every listed category that has an entry for `id`.
/// Returns how many entries changed (0 = the id has no text there to replace).
pub fn set(categories: &[u32], id: u32, text: &str, owner: &str) -> usize {
    drop_pending(|p| !(categories.contains(&p.0) && p.1 == id));
    let ptr = intern(text);
    let mut slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.get_or_insert_with(HashMap::new);
    let mut changed = 0;
    for &cat in categories {
        let Some((entry, base)) = entry_address(cat, id) else { continue };
        let Some(current) = memory::read_u64(entry) else { continue };
        let ov = map.entry((cat, id)).or_insert(Override { entry, original: current, owner: owner.to_string(), text: String::new() });
        ov.owner = owner.to_string();
        ov.text = text.to_string();
        // The game computes base + offset with 64-bit wrapping arithmetic.
        unsafe { std::ptr::write_volatile(entry as *mut u64, (ptr as u64).wrapping_sub(base as u64)) };
        changed += 1;
    }
    changed
}

fn restore(ov: &Override) {
    if memory::read_u64(ov.entry).is_some() {
        unsafe { std::ptr::write_volatile(ov.entry as *mut u64, ov.original) };
    }
}

/// Restores the original text for these categories/id. Returns how many were restored.
pub fn reset(categories: &[u32], id: u32) -> usize {
    drop_pending(|p| !(categories.contains(&p.0) && p.1 == id));
    let mut slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = slot.as_mut() else { return 0 };
    let mut n = 0;
    for cat in categories {
        if let Some(ov) = map.remove(&(*cat, id)) {
            restore(&ov);
            n += 1;
        }
    }
    n
}

/// Restores every override owned by `owner` (a mod unloading). Returns how many.
pub fn reset_owner(owner: &str) -> usize {
    drop_pending(|p| p.3 != owner);
    let mut slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let Some(map) = slot.as_mut() else { return 0 };
    let keys: Vec<(u32, u32)> = map.iter().filter(|(_, o)| o.owner == owner).map(|(k, _)| *k).collect();
    for k in &keys {
        if let Some(ov) = map.remove(k) {
            restore(&ov);
        }
    }
    keys.len()
}

/// The item a mod renamed to `name` (case-insensitive), optionally of one kind.
pub fn find_renamed_item(name: &str, kind: Option<Kind>) -> Option<(Kind, u32)> {
    let wanted = name.trim().to_lowercase();
    let slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.as_ref()?;
    Kind::ALL.into_iter().filter(|k| kind.is_none_or(|w| w == *k)).find_map(|k| {
        let cats = item_categories(k, "name").ok()?;
        map.iter().find(|((cat, _), o)| cats.contains(cat) && o.text.to_lowercase() == wanted).map(|((_, id), _)| (k, *id))
    })
}

/// The overriding text for an item name, if a mod renamed it.
pub fn item_name_override(kind: Kind, id: u32) -> Option<String> {
    let cats = item_categories(kind, "name").ok()?;
    let slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let map = slot.as_ref()?;
    cats.iter().rev().find_map(|c| map.get(&(*c, id)).map(|o| o.text.clone()))
}

// ── new entries ─────────────────────────────────────────────────────────────
//
// An id with no slot in an FMG gets one by rebuilding that FMG's header: a copy with one more
// group (just this id, sorted in by id) and a string-offset array one longer. Existing
// strings stay where they are (their offsets are rewritten relative to the new header with
// wrapping arithmetic, as the game adds them to the FMG base), and the repository's slot
// for the category is pointed at the new header. Overrides already pointing into the old
// offset array move with it.

/// The FMG currently serving `category` (latest version first), and its repository slot.
fn fmg_slot(category: u32) -> Option<(usize, usize)> {
    let repo = crate::ds3::MSG_REPOSITORY.instance()?;
    let versions = memory::read_u64(repo + 0x08)? as usize;
    let version_count = (memory::read::<u32>(repo + 0x10)? as usize).min(64);
    for v in (0..version_count).rev() {
        let cats = memory::read_u64(versions + v * 8)? as usize;
        if cats == 0 {
            continue;
        }
        let slot = cats + category as usize * 8;
        let f = memory::read_u64(slot)? as usize;
        if f != 0 {
            return Some((f, slot));
        }
    }
    None
}

fn add_entry(category: u32, id: u32, text: &str, owner: &str) -> Result<(), String> {
    // (The game would free Kindle's header as its own; see RELEASE.)
    if !RELEASE_HOOKED.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(format!("text {category} {id} can't be added: the game's text release isn't hooked"));
    }
    let (old, slot) = fmg_slot(category).ok_or_else(|| format!("text category {category} isn't loaded"))?;
    let rd32 = |at: usize| memory::read::<i32>(at).ok_or_else(|| "text table is unreadable".to_string());
    let groups = rd32(old + 0x0C)?.max(0) as usize;
    let strings = rd32(old + 0x10)?.max(0) as usize;
    let old_table = memory::read_u64(old + 0x18).ok_or("text table is unreadable")? as usize;

    let header_len = 0x28 + (groups + 1) * 16;
    let header: &'static mut [u8] = Box::leak(vec![0u8; header_len + 16].into_boxed_slice());
    let pad = (16 - header.as_ptr() as usize % 16) % 16;
    let new = header.as_mut_ptr() as usize + pad;
    let table: &'static mut [u64] = Box::leak(vec![0u64; strings + 1].into_boxed_slice());
    let new_table = table.as_mut_ptr() as usize;

    let head = memory::read_bytes(old, 0x28).ok_or("text table is unreadable")?;
    let h = unsafe { std::slice::from_raw_parts_mut(new as *mut u8, header_len) };
    h[..0x28].copy_from_slice(&head);
    h[0x0C..0x10].copy_from_slice(&((groups + 1) as i32).to_le_bytes());
    h[0x10..0x14].copy_from_slice(&((strings + 1) as i32).to_le_bytes());
    h[0x18..0x20].copy_from_slice(&(new_table as u64).to_le_bytes());

    // Groups, with the new one inserted in id order.
    let mut at = 0x28;
    let mut inserted = false;
    let new_group = |h: &mut [u8], at: &mut usize| {
        h[*at..*at + 4].copy_from_slice(&(strings as i32).to_le_bytes());
        h[*at + 4..*at + 8].copy_from_slice(&(id as i32).to_le_bytes());
        h[*at + 8..*at + 12].copy_from_slice(&(id as i32).to_le_bytes());
        *at += 16;
    };
    for g in 0..groups {
        let src = memory::read_bytes(old + 0x28 + g * 16, 16).ok_or("text table is unreadable")?;
        let first = i32::from_le_bytes(src[4..8].try_into().unwrap());
        if !inserted && (id as i32) < first {
            new_group(h, &mut at);
            inserted = true;
        }
        h[at..at + 16].copy_from_slice(&src);
        at += 16;
    }
    if !inserted {
        new_group(h, &mut at);
    }

    // Offsets: existing strings rebased onto the new header; the new one appended.
    for (i, e) in table.iter_mut().take(strings).enumerate() {
        let off = memory::read_u64(old_table + i * 8).unwrap_or(0);
        *e = if off == 0 { 0 } else { (old as u64).wrapping_add(off).wrapping_sub(new as u64) };
    }
    table[strings] = (intern(text) as u64).wrapping_sub(new as u64);

    {
        let mut replaced = REPLACED.lock().unwrap_or_else(|e| e.into_inner());
        if !replaced.iter().any(|&(s, _)| s == slot) {
            replaced.push((slot, old));
        }
    }
    let mut map_slot = OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
    let map = map_slot.get_or_insert_with(HashMap::new);
    for ov in map.values_mut() {
        if (old_table..old_table + strings * 8).contains(&ov.entry) {
            ov.entry = new_table + (ov.entry - old_table);
            if ov.original != 0 {
                ov.original = (old as u64).wrapping_add(ov.original).wrapping_sub(new as u64);
            }
        }
    }
    // Publish (an aligned 8-byte store).
    unsafe { std::ptr::write_volatile(slot as *mut u64, new as u64) };
    // Removing the entry later just empties its slot.
    map.insert((category, id), Override { entry: new_table + strings * 8, original: 0, owner: owner.to_string(), text: text.to_string() });
    Ok(())
}

/// Like `set`, but gives the id an entry in the first category if none of them has one.
pub fn set_or_add(categories: &[u32], id: u32, text: &str, owner: &str) -> Result<usize, String> {
    match set(categories, id, text, owner) {
        0 => {
            let cat = *categories.first().ok_or("no text category")?;
            add_entry(cat, id, text, owner)?;
            Ok(1)
        }
        n => Ok(n),
    }
}
