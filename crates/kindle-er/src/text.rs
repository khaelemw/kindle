//! Elden Ring's text (kindle.text): what the game shows for its items, places, characters and menus,
//! read from the game's own text (in the game's language; nothing shipped) and replaced live.
//!
//! The game keeps its text in its MsgRepositoryImp, made once by the code REPOSITORY finds (which gives
//! its static): +8 its versions, +0x10 how many, +0x14 how many FMG ids each has; a version is an array
//! of FMGs (as kindle-core's fmg reads them) by id, the msgbnd's file ids. Each of the game's getters
//! (one per kind of text: WeaponName, GoodsInfo ...) reads version 0 and tries a fixed list of FMGs,
//! showing the first entry it finds: one of its own first (none loaded in 2.7.0.0, seen live), then the
//! base game's, then dlc01's and dlc02's. Kindle reads them in the same order (CATEGORIES, each list
//! checked against its getter's code).
//!
//! A text is replaced as Kindle does in Dark Souls III: the entry's string offset is pointed at a string
//! of Kindle's own (kept for the session: the game may still be showing it), and the game's offset put
//! back when the mod resets it or unloads. A screen takes its text as it is made, so a text replaced
//! while one shows is seen the next time it is made (seen live at the title, whose prompt the game's
//! lookup then found replaced). An entry the game hasn't is added to a copy of the FMG with room for
//! more, the repository's slot pointed at the copy ("added entries" below); the game would free that copy
//! as its own as it lets its text go, so its release and the repository's end are hooked, as Dark Souls
//! III's release is, and a change on text the game let go is made again once it's back.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ilhook::x64::Registers;
use kindle_core::hook_sites::{Kind, Site};
use kindle_core::{fmg, memory, patterns};

use crate::{items, task};

/// The code that makes the repository once (if its static is empty: the 0x48-byte object, its
/// constructor, the static set).
const REPOSITORY: &str = "48 83 3D ?? ?? ?? ?? 00 75 ?? 4C 8B 05 ?? ?? ?? ?? 4C 89 45 20 BA 08 00 00 00 8D 4A 40 E8 ?? ?? ?? ?? 48 89 45 30 48 85 C0 74 ?? 48 8B C8 E8 ?? ?? ?? ?? 48 8B C8 EB ?? 48 8B CB 48 89 0D";
/// Where REPOSITORY's static's displacement is, and where that instruction ends.
const REPOSITORY_STATIC: (usize, usize) = (3, 8);
/// The repository's versions, their count, and how many FMG ids a version has.
const VERSIONS: usize = 0x08;
const VERSION_COUNT: usize = 0x10;
const FMG_COUNT: usize = 0x14;

/// The game's text by name, each with the FMGs its getter tries, in its order.
pub const CATEGORIES: &[(&str, &[u32])] = &[
    ("weapon_name", &[0x73, 11, 0x136, 0x19A]),
    ("weapon_info", &[0x72, 21, 0x137, 0x19B]),
    ("weapon_caption", &[0x6A, 25, 0x138, 0x19C]),
    ("weapon_effect", &[44, 0x14D, 0x1B1]),
    ("armor_name", &[0x75, 12, 0x139, 0x19D]),
    ("armor_info", &[0x74, 22, 0x13A, 0x19E]),
    ("armor_caption", &[0x6C, 26, 0x13B, 0x19F]),
    ("talisman_name", &[0x71, 13, 0x13C, 0x1A0]),
    ("talisman_info", &[0x70, 23, 0x13D, 0x1A1]),
    ("talisman_caption", &[0x6D, 27, 0x13E, 0x1A2]),
    ("goods_name", &[0x6F, 10, 0x13F, 0x1A3]),
    ("goods_info", &[0x6E, 20, 0x140, 0x1A4]),
    ("goods_caption", &[100, 24, 0x141, 0x1A5]),
    ("goods_info2", &[46, 0x14F, 0x1B3]),
    ("goods_dialog", &[41, 0x14A, 0x1AE]),
    ("ash_name", &[35, 0x142, 0x1A6]),
    ("ash_info", &[36, 0x143, 0x1A7]),
    ("ash_caption", &[37, 0x144, 0x1A8]),
    ("ash_effect", &[45, 0x14E, 0x1B2]),
    ("skill_name", &[42, 0x14B, 0x1AF]),
    ("skill_caption", &[43, 0x14C, 0x1B0]),
    ("npc_name", &[0x77, 18, 0x148, 0x1AC]),
    ("place_name", &[0x78, 19, 0x149, 0x1AD]),
    ("talk", &[0x68, 1, 0x168, 0x1CC]),
    ("blood_message", &[0x6B, 2, 0x169, 0x1CD]),
    ("network_message", &[31, 0x16C, 0x1D0]),
    ("action_button", &[32, 0x16D, 0x1D1]),
    ("event_text", &[33, 0x16E, 0x1D2]),
    ("event_text_map", &[34, 0x16F, 0x1D3]),
    ("menu_text", &[200, 0x170, 0x1D4]),
    ("line_help", &[201, 0x171, 0x1D5]),
    ("key_guide", &[202, 0x172, 0x1D6]),
    ("system_message", &[203, 0x173, 0x1D7]),
    ("dialogues", &[204, 0x174, 0x1D8]),
    ("loading_title", &[205, 0x175, 0x1D9]),
    ("loading_text", &[206, 0x176, 0x1DA]),
    ("tutorial_title", &[207, 0x177, 0x1DB]),
    ("tutorial_body", &[208, 0x178, 0x1DC]),
];

/// Dark Souls III's names, for mods written for both.
const ALIASES: &[(&str, &str)] = &[("ring_name", "talisman_name"), ("ring_info", "talisman_info"), ("ring_caption", "talisman_caption")];

/// A weapon's highest upgrade level (its text is its level 0's).
const MOST_UPGRADE: u32 = 25;

/// The FMGs of a category given by name ("goods_name") or as one FMG id ("10").
pub fn category(spec: &str) -> Result<Vec<u32>, String> {
    if let Ok(n) = spec.parse::<u32>() {
        return Ok(vec![n]);
    }
    let name = ALIASES.iter().find(|(a, _)| *a == spec).map_or(spec, |(_, n)| *n);
    CATEGORIES.iter().find(|(n, _)| *n == name).map(|(_, ids)| ids.to_vec()).ok_or_else(|| {
        let names: Vec<&str> = CATEGORIES.iter().map(|(n, _)| *n).collect();
        format!("unknown text category {spec:?} (an FMG id or one of: {})", names.join(", "))
    })
}

#[cfg(test)]
pub(crate) static TEST_REPOSITORY: Mutex<Option<usize>> = Mutex::new(None);

/// The repository, once the game has made it.
fn repository() -> Option<usize> {
    #[cfg(test)]
    if let Some(r) = *TEST_REPOSITORY.lock().unwrap_or_else(|e| e.into_inner()) {
        return (r != 0).then_some(r);
    }
    static AT: OnceLock<Option<usize>> = OnceLock::new();
    let at = (*AT.get_or_init(|| {
        let site = patterns::find_unique("text repository", REPOSITORY)?;
        patterns::rip_target(site + REPOSITORY_STATIC.0, site + REPOSITORY_STATIC.1)
    }))?;
    memory::read_u64(at).filter(|&r| r != 0).map(|r| r as usize)
}

/// Where version 0, the one the game reads, keeps the FMG with this id.
fn slot(id: u32) -> Option<usize> {
    let repo = repository()?;
    if memory::read::<u32>(repo + VERSION_COUNT)? == 0 || id >= memory::read::<u32>(repo + FMG_COUNT)? {
        return None;
    }
    let version = memory::read_u64(memory::read_u64(repo + VERSIONS)? as usize)? as usize;
    (version != 0).then(|| version + id as usize * 8)
}

/// The FMG with this id in version 0, if loaded.
fn table(id: u32) -> Option<usize> {
    memory::read_u64(slot(id)?).filter(|&f| f != 0).map(|f| f as usize)
}

/// The base game's goods names: the game makes its repository as it starts and loads its text into it
/// later (before the title), all at once, so the text is there once these are.
const LOADED_WITH: u32 = 10;

pub(crate) fn loaded() -> Result<(), String> {
    table(LOADED_WITH).map(|_| ()).ok_or_else(|| "the game's text isn't loaded yet".into())
}

/// What the game shows for entry `id` of these FMGs (in the game's order: the first that has it).
pub fn get(fmgs: &[u32], id: u32) -> Result<Option<String>, String> {
    loaded()?;
    flush_staged();
    Ok(fmgs.iter().find_map(|&f| fmg::text(table(f)?, id, &memory::read_bytes)))
}

/// A change of Kindle's to one entry: where its offset is, the game's offset there (0 in an entry Kindle
/// added), the table it's in, the text and the mod's id.
struct Replaced {
    entry: usize,
    original: u64,
    table: usize,
    text: String,
    owner: String,
}

/// Memory of Kindle's for a copy of an FMG: `len` u64s at `at`, zeroed.
#[derive(Clone, Copy)]
struct Block {
    at: usize,
    len: usize,
}

impl Block {
    fn new(len: usize) -> Block {
        Block { at: Box::leak(vec![0u64; len].into_boxed_slice()).as_mut_ptr() as usize, len }
    }

    /// # Safety
    /// Nothing reads it any more.
    unsafe fn free(self) {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(self.at as *mut u64, self.len)) });
    }
}

/// Kindle's copy of one of the game's FMGs, with entries added: its header and groups, and its offset
/// table, each with room for more.
struct Grown {
    /// The game's FMG it copies: what the slot gets back as the game lets the FMG go.
    game: usize,
    header: Block,
    table: Block,
}

impl Grown {
    fn groups_room(&self) -> usize {
        (self.header.len - GROUPS / 8) / 2
    }
}

/// How long a copy the slot has moved on from is kept before it's freed: a thread may still be reading it
/// (the game reads an FMG through its slot each time, and keeps no pointer to it: its +0x20 records hold
/// a position, an id).
const RETIRED_FOR: Duration = Duration::from_secs(10);

/// Kindle's changes to the game's text.
#[derive(Default)]
struct State {
    replaced: HashMap<(u32, u32), Replaced>,
    /// By FMG id.
    grown: HashMap<u32, Grown>,
    /// Copies the slots have moved on from, and since when.
    retired: Vec<(Instant, Grown)>,
    /// Entries waiting to go into a new copy of their FMG together (flush): FMG -> entry -> (text, owner).
    staged: HashMap<u32, BTreeMap<u32, (String, String)>>,
    /// Changes on an FMG the game let go, made again once it's back: (FMG, entry) -> (text, owner).
    pending: HashMap<(u32, u32), (String, String)>,
    /// The repository's end has begun (the game is quitting): nothing is changed after it.
    ended: bool,
}

/// Also held while the game releases an FMG (release_hook), so no change is made on one being freed.
static STATE: Mutex<Option<State>> = Mutex::new(None);

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    f(STATE.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(State::default))
}

/// Kindle's strings, one per text, kept for the session.
static STRINGS: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

fn intern(text: &str) -> usize {
    let mut strings = STRINGS.lock().unwrap_or_else(|e| e.into_inner());
    let map = strings.get_or_insert_with(HashMap::new);
    if let Some(&p) = map.get(text) {
        return p;
    }
    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let p = Box::leak(units.into_boxed_slice()).as_ptr() as usize;
    map.insert(text.to_string(), p);
    p
}

/// Replaces entry `id`'s text in each of these FMGs that has one; how many did (0: none has).
pub fn set(fmgs: &[u32], id: u32, text: &str, owner: &str) -> Result<usize, String> {
    task::on_games_thread("text is changed")?;
    loaded()?;
    with_state(|s| replace(s, fmgs, id, text, owner))
}

/// Replaces entry `id`'s text in each of these FMGs that has one, or, if none has, adds it to the first of
/// them the game has loaded (the one its getter tries first); how many changed.
pub fn set_or_add(fmgs: &[u32], id: u32, text: &str, owner: &str) -> Result<usize, String> {
    task::on_games_thread("text is changed")?;
    loaded()?;
    with_state(|s| match replace(s, fmgs, id, text, owner)? {
        0 => {
            let f = fmgs.iter().copied().find(|&f| table(f).is_some()).ok_or("none of this text's FMGs is loaded")?;
            add(s, f, id, text, owner).map(|()| 1)
        }
        n => Ok(n),
    })
}

fn replace(s: &mut State, fmgs: &[u32], id: u32, text: &str, owner: &str) -> Result<usize, String> {
    if s.ended {
        return Err("the game is letting its text go (it's quitting)".into());
    }
    let string = intern(text) as u64;
    let mut changed = 0;
    for &f in fmgs {
        if let Some(staged) = s.staged.get_mut(&f).and_then(|m| m.get_mut(&id)) {
            *staged = (text.to_string(), owner.to_string());
            changed += 1;
            continue;
        }
        let Some(t) = table(f) else {
            // Let go by the game and not back yet: made with this text once it is.
            if let Some(p) = s.pending.get_mut(&(f, id)) {
                *p = (text.to_string(), owner.to_string());
                changed += 1;
            }
            continue;
        };
        s.pending.remove(&(f, id));
        let Some(entry) = fmg::slot(t, id, &memory::read_bytes) else { continue };
        let Some(current) = memory::read_u64(entry) else { continue };
        let r = s.replaced.entry((f, id)).or_insert(Replaced { entry, original: current, table: t, text: String::new(), owner: String::new() });
        if (r.entry, r.table) != (entry, t) {
            // The game made this table again since: its offset is the one to keep.
            (r.entry, r.original, r.table) = (entry, current, t);
        }
        r.text = text.to_string();
        r.owner = owner.to_string();
        // The game adds the offset to the table's start, wrapping.
        unsafe { std::ptr::write_volatile(entry as *mut u64, string.wrapping_sub(t as u64)) };
        changed += 1;
    }
    Ok(changed)
}

/// Puts the game's offset back (0 in an entry Kindle added: the game shows none), if the table is still
/// the one Kindle changed.
fn restore(f: u32, id: u32, r: &Replaced) {
    if table(f) == Some(r.table) && fmg::slot(r.table, id, &memory::read_bytes) == Some(r.entry) {
        unsafe { std::ptr::write_volatile(r.entry as *mut u64, r.original) };
    }
}

/// The game's text back for entry `id` of these FMGs; how many were changed.
pub fn reset(fmgs: &[u32], id: u32) -> Result<usize, String> {
    task::on_games_thread("text is changed")?;
    Ok(with_state(|s| {
        let mut n = 0;
        for &f in fmgs {
            if let Some(r) = s.replaced.remove(&(f, id)) {
                restore(f, id, &r);
                n += 1;
            }
            n += usize::from(s.pending.remove(&(f, id)).is_some());
            n += usize::from(s.staged.get_mut(&f).is_some_and(|m| m.remove(&id).is_some()));
        }
        n
    }))
}

/// The game's text back wherever a mod changed it (the mod unloading); how many.
pub fn reset_owner(owner: &str) -> usize {
    with_state(|s| {
        let keys: Vec<(u32, u32)> = s.replaced.iter().filter(|(_, r)| r.owner == owner).map(|(k, _)| *k).collect();
        for k in &keys {
            if let Some(r) = s.replaced.remove(k) {
                restore(k.0, k.1, &r);
            }
        }
        let waiting = s.pending.len() + s.staged.values().map(BTreeMap::len).sum::<usize>();
        s.pending.retain(|_, p| p.1 != owner);
        s.staged.values_mut().for_each(|m| m.retain(|_, e| e.1 != owner));
        keys.len() + waiting - s.pending.len() - s.staged.values().map(BTreeMap::len).sum::<usize>()
    })
}

// ── added entries ───────────────────────────────────────────────────────────
//
// An entry no FMG of a text has goes into a copy of the first one loaded: the game's header, its groups
// with one more (just this id, in id order), its string offsets rebased onto the copy (the strings stay
// where they are: the game adds an offset to the table's start, wrapping) and one more for Kindle's
// string, then the repository's slot pointed at the copy with one 8-byte store. The copy has room for
// more: an id above every one in it goes in in place (the offset and the group written before the
// counts, so a thread searching the table meanwhile sees the group whole or not at all). Any other entry
// (an id below the last, or a copy full) is staged, and an FMG's staged entries go into one new copy, with
// room again, at Kindle's next read of the text or the frame's end, before the game reads it (a mod naming
// hundreds of items as it loads makes a copy or two per FMG, in any order). Kindle's changes in the old
// table move with it, and the old copy is freed a while later (RETIRED_FOR).
//
// The game frees an FMG as it lets it go: its release (a bundle's unload, or a reload into a full slot)
// and the repository's end as the game quits (which frees every slot itself). Both are hooked: the
// release runs under Kindle's lock with the game's own FMG back in the slot, so the game frees its own
// and no change is made on it meanwhile, and Kindle's changes on it are made again once it's back
// (tick); at the end every slot gets the game's own back, and nothing is changed after.

/// The FMG's layout (kindle-core's fmg): the group count, the string count, the offset table, the groups
/// (16 bytes each: the first entry's index in the offsets, the first id, the last id).
const GROUP_COUNT: usize = 0x0C;
const STRING_COUNT: usize = 0x10;
const OFFSETS: usize = 0x18;
const GROUPS: usize = 0x28;
/// Entries a copy has room for beyond what it holds, at least (half as many again for a big FMG): a mod
/// adding hundreds fills one copy in place.
const ROOM: usize = 256;

/// The game's release of an FMG, FUN_14266fe90(repository, version, id), its entry: every release of a
/// loaded one goes through it (its only calls: a bundle's release, and the load into a full slot).
const RELEASE: &str =
    "40 53 48 83 EC 20 48 8B D9 3B 51 10 0F 83 ?? ?? ?? ?? 44 3B 41 14 0F 83 ?? ?? ?? ?? 48 8B 41 08 44 8B CA 48 89 74 24 30 4A 8D 34 CD 00 00 00 00";
/// The repository's end, FUN_1426700e0 (its entry; its kind's other objects end through it too), which
/// frees every FMG itself: the game's teardown as it quits.
const END: &str = "40 57 41 56 41 57 48 83 EC 30 48 C7 44 24 20 FE FF FF FF 48 89 5C 24 50 48 89 6C 24 58 48 89 74 24 60 48 8B D9 48 8D 05 ?? ?? ?? ?? 48 89 01 45 33 FF 45 8B CF 44 39 79 10";

static RELEASE_SITE: Site = Site {
    area: "text",
    name: "text release",
    pattern: RELEASE,
    offset: 0,
    matches: 1,
    kind: Kind::Retn(release_hook),
    without: "text the game doesn't have can't be added",
};

static END_SITE: Site = Site {
    area: "text",
    name: "text repository end",
    pattern: END,
    offset: 0,
    matches: 1,
    kind: Kind::JmpBack(end_hook),
    without: "text the game doesn't have can't be added",
};

/// Both hooked: text can be added.
static HOOKED: AtomicBool = AtomicBool::new(false);

/// Hooks the game's release of its text and its repository's end, which adding text needs.
pub fn install() {
    if RELEASE_SITE.install().is_some() && END_SITE.install().is_some() {
        HOOKED.store(true, Ordering::Relaxed);
        kindle_core::info!("text: the game's release of its text is hooked; text the game hasn't can be added");
    }
}

/// FMG `f` as it is in its slot: the slot, the table, its header, group and string counts, and its groups.
struct Current {
    slot: usize,
    t: usize,
    head: Vec<u8>,
    groups: usize,
    strings: usize,
    group_bytes: Vec<u8>,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    b.get(at..at + 4).map_or(0, |v| u32::from_le_bytes([v[0], v[1], v[2], v[3]]))
}

fn current(f: u32) -> Result<Current, String> {
    let not_loaded = || format!("text {f} isn't loaded");
    let slot = slot(f).ok_or_else(not_loaded)?;
    let t = memory::read_u64(slot).filter(|&t| t != 0).ok_or_else(not_loaded)? as usize;
    let unreadable = || format!("text {f} is unreadable");
    let head = memory::read_bytes(t, GROUPS).ok_or_else(unreadable)?;
    let (groups, strings) = (u32_at(&head, GROUP_COUNT) as usize, u32_at(&head, STRING_COUNT) as usize);
    if head[..4] != [0, 0, 2, 0] || groups > 1 << 20 || strings > 1 << 22 {
        return Err(format!("text {f} isn't an FMG as expected"));
    }
    let group_bytes = if groups == 0 { Vec::new() } else { memory::read_bytes(t + GROUPS, groups * 16).ok_or_else(unreadable)? };
    Ok(Current { slot, t, head, groups, strings, group_bytes })
}

/// Gives FMG `f` entry `id`, which it hasn't: in place in Kindle's copy where it can go there, else staged.
fn add(s: &mut State, f: u32, id: u32, text: &str, owner: &str) -> Result<(), String> {
    if !HOOKED.load(Ordering::Relaxed) {
        return Err(format!("text {f} {id} can't be added: the game's release of its text isn't hooked"));
    }
    let Current { t, groups, strings, group_bytes, .. } = current(f)?;
    let last = groups.checked_sub(1).map(|g| u32_at(&group_bytes, g * 16 + 8));
    if s.staged.get(&f).is_none_or(BTreeMap::is_empty)
        && let Some(g) = s.grown.get(&f).filter(|g| g.header.at == t)
        && groups < g.groups_room()
        && strings < g.table.len
        && last.is_none_or(|l| id > l)
    {
        let entry = g.table.at + strings * 8;
        unsafe {
            std::ptr::write_volatile(entry as *mut u64, (intern(text) as u64).wrapping_sub(t as u64));
            std::ptr::write_volatile((t + GROUPS + groups * 16) as *mut [u32; 4], [strings as u32, id, id, 0]);
            std::ptr::write_volatile((t + STRING_COUNT) as *mut u32, strings as u32 + 1);
            std::ptr::write_volatile((t + GROUP_COUNT) as *mut u32, groups as u32 + 1);
        }
        s.replaced.insert((f, id), Replaced { entry, original: 0, table: t, text: text.to_string(), owner: owner.to_string() });
        return Ok(());
    }
    s.staged.entry(f).or_default().insert(id, (text.to_string(), owner.to_string()));
    Ok(())
}

/// Every FMG's staged entries into a new copy of it (on the game's thread; elsewhere they wait).
fn flush_staged() {
    if task::on_games_thread("text is changed").is_err() {
        return;
    }
    with_state(|s| {
        let fmgs: Vec<u32> = s.staged.iter().filter(|(_, m)| !m.is_empty()).map(|(f, _)| *f).collect();
        for f in fmgs {
            if let Err(e) = flush(s, f) {
                kindle_core::warn!("text: {f}'s new entries couldn't be added ({e})");
            }
        }
    });
}

/// FMG `f`'s staged entries into a new copy of it with room for more, the slot pointed at it.
fn flush(s: &mut State, f: u32) -> Result<(), String> {
    let staged = s.staged.remove(&f).unwrap_or_default();
    let Current { slot, t, head, groups, strings, group_bytes } = match current(f) {
        Ok(c) => c,
        Err(e) => {
            // Let go meanwhile: made once it's back.
            s.pending.extend(staged.into_iter().map(|(id, e)| ((f, id), e)));
            return Err(e);
        }
    };
    // One the table has after all (none should) is replaced there instead.
    let (have, new): (Vec<_>, Vec<_>) = staged.into_iter().partition(|(id, _)| fmg::slot(t, *id, &memory::read_bytes).is_some());
    for (id, (text, owner)) in have {
        replace(s, &[f], id, &text, &owner)?;
    }
    if new.is_empty() {
        return Ok(());
    }
    let unreadable = || format!("text {f} is unreadable");
    let old_offsets = u64::from_le_bytes(head[OFFSETS..OFFSETS + 8].try_into().unwrap_or_default()) as usize;
    let offsets = if strings == 0 { Vec::new() } else { memory::read_bytes(old_offsets, strings * 8).ok_or_else(unreadable)? };
    let (all_groups, all_strings) = (groups + new.len(), strings + new.len());
    let header = Block::new(GROUPS / 8 + (all_groups + ROOM.max(all_groups / 2)) * 2);
    let table = Block::new(all_strings + ROOM.max(all_strings / 2));
    let (h, tb) = (header.at, table.at);
    let rebase = |off: u64| if off == 0 { 0 } else { (t as u64).wrapping_add(off).wrapping_sub(h as u64) };
    let bytes = unsafe { std::slice::from_raw_parts_mut(h as *mut u8, header.len * 8) };
    let table_entries = unsafe { std::slice::from_raw_parts_mut(tb as *mut u64, table.len) };
    bytes[..GROUPS].copy_from_slice(&head);
    // The game's groups and one per new entry, merged in id order; the new entries' offsets after the game's.
    let (mut g, mut k, mut at) = (0, 0, GROUPS);
    while g < groups || k < new.len() {
        if k < new.len() && (g == groups || new[k].0 < u32_at(&group_bytes, g * 16 + 4)) {
            let id = new[k].0;
            for (i, v) in [(strings + k) as u32, id, id, 0].into_iter().enumerate() {
                bytes[at + i * 4..at + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
            k += 1;
        } else {
            bytes[at..at + 16].copy_from_slice(&group_bytes[g * 16..g * 16 + 16]);
            g += 1;
        }
        at += 16;
    }
    bytes[GROUP_COUNT..GROUP_COUNT + 4].copy_from_slice(&(all_groups as u32).to_le_bytes());
    bytes[STRING_COUNT..STRING_COUNT + 4].copy_from_slice(&(all_strings as u32).to_le_bytes());
    bytes[OFFSETS..OFFSETS + 8].copy_from_slice(&(tb as u64).to_le_bytes());
    for (e, off) in table_entries.iter_mut().zip(offsets.as_chunks::<8>().0) {
        *e = rebase(u64::from_le_bytes(*off));
    }
    for r in s.replaced.values_mut().filter(|r| r.table == t) {
        if let Some(i) = r.entry.checked_sub(old_offsets).filter(|&i| i < strings * 8) {
            (r.entry, r.original, r.table) = (tb + i, rebase(r.original), h);
        }
    }
    for (k, (id, (text, owner))) in new.iter().enumerate() {
        let entry = tb + (strings + k) * 8;
        table_entries[strings + k] = (intern(text) as u64).wrapping_sub(h as u64);
        s.replaced.insert((f, *id), Replaced { entry, original: 0, table: h, text: text.clone(), owner: owner.clone() });
    }
    let old = s.grown.remove(&f);
    let game = old.as_ref().filter(|g| g.header.at == t).map_or(t, |g| g.game);
    unsafe { std::ptr::write_volatile(slot as *mut u64, h as u64) };
    s.retired.extend(old.map(|g| (Instant::now(), g)));
    s.grown.insert(f, Grown { game, header, table });
    kindle_core::info!("text: FMG {f} copied with {} entries the game hasn't (room for {} more)", new.len(), table.len - all_strings);
    Ok(())
}

/// The game letting FMG `f` go (it frees it next): its own back in the slot if Kindle's copy is there, and
/// Kindle's changes on it kept to be made again once it's back.
fn let_go(s: &mut State, f: u32) {
    if let Some(g) = s.grown.remove(&f) {
        if let Some(slot) = slot(f)
            && memory::read_u64(slot) == Some(g.header.at as u64)
        {
            unsafe { std::ptr::write_volatile(slot as *mut u64, g.game as u64) };
        }
        s.retired.push((Instant::now(), g));
    }
    if let Some(staged) = s.staged.remove(&f) {
        s.pending.extend(staged.into_iter().map(|(id, e)| ((f, id), e)));
    }
    let keys: Vec<(u32, u32)> = s.replaced.keys().filter(|k| k.0 == f).copied().collect();
    for k in keys {
        if let Some(r) = s.replaced.remove(&k) {
            s.pending.insert(k, (r.text, r.owner));
        }
    }
    *NAMES.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// The game's release (rcx the repository, edx the version, r8d the FMG id), run under Kindle's lock.
fn release_hook(reg: *mut Registers, original: usize) -> usize {
    let r = unsafe { &*reg };
    let (repo, version, id) = (r.rcx, r.rdx, r.r8);
    let release: extern "C" fn(u64, u64, u64) -> u64 = unsafe { std::mem::transmute(original) };
    with_state(|s| {
        if version as u32 == 0 && repository() == Some(repo as usize) {
            let_go(s, id as u32);
        }
        release(repo, version, id) as usize
    })
}

/// The repository's end (rcx): every slot Kindle's copy is in gets the game's own back.
fn end_hook(reg: *mut Registers) {
    let repo = unsafe { (*reg).rcx } as usize;
    if repository() != Some(repo) {
        return;
    }
    with_state(|s| {
        let grown: Vec<(u32, Grown)> = s.grown.drain().collect();
        for (f, g) in grown {
            if let Some(slot) = slot(f)
                && memory::read_u64(slot) == Some(g.header.at as u64)
            {
                unsafe { std::ptr::write_volatile(slot as *mut u64, g.game as u64) };
            }
            s.retired.push((Instant::now(), g));
        }
        s.staged.clear();
        s.ended = true;
    });
}

/// Each frame: the staged entries added, copies the slots moved on from a while ago freed, and Kindle's
/// changes on an FMG the game let go made again once it's back.
pub fn tick() {
    flush_staged();
    with_state(|s| {
        for (_, g) in s.retired.extract_if(.., |(since, _)| since.elapsed() >= RETIRED_FOR) {
            unsafe {
                g.header.free();
                g.table.free();
            }
        }
    });
    if task::on_games_thread("text is changed").is_err() || loaded().is_err() {
        return;
    }
    let made = with_state(|s| {
        // In id order: each fills a copy in place where it can.
        let mut ready: Vec<(u32, u32)> = s.pending.keys().filter(|k| table(k.0).is_some()).copied().collect();
        ready.sort_unstable();
        let mut made = 0;
        for (f, id) in ready {
            let Some((text, owner)) = s.pending.remove(&(f, id)) else { continue };
            let again = match replace(s, &[f], id, &text, &owner) {
                Ok(0) => add(s, f, id, &text, &owner),
                other => other.map(|_| ()),
            };
            match again {
                Ok(()) => made += 1,
                Err(e) => kindle_core::warn!("text: {f} {id} couldn't be made again ({e})"),
            }
        }
        made
    });
    if made > 0 {
        flush_staged();
        kindle_core::info!("text: {made} change(s) made again on the game's reloaded text");
    }
}

/// An item's text: the FMGs of its field ("name", "info", "description"), the entry (a weapon's
/// upgrades share their level 0's), and the weapon's upgrade level.
fn item_target(item: u32, field: &str) -> Result<(Vec<u32>, u32, u32), String> {
    let kind = items::category(item)?;
    let part = match field {
        "name" => "name",
        "info" => "info",
        "description" => "caption",
        other => return Err(format!("unknown item text {other:?} (name, info, description)")),
    };
    let id = item & items::ID_BITS;
    let level = if kind == "weapon" { id % 100 } else { 0 };
    Ok((category(&format!("{kind}_{part}"))?, id - level, level))
}

/// What the game shows for an item's name, info or description (an upgraded weapon's name with its
/// level, as "Dagger +5").
pub fn item(item: u32, field: &str) -> Result<Option<String>, String> {
    let (fmgs, id, level) = item_target(item, field)?;
    let text = get(&fmgs, id)?;
    Ok(if field == "name" && level > 0 { text.map(|n| format!("{n} +{level}")) } else { text })
}

/// Item names, for finding items by name: built once the game's text is loaded.
struct Names {
    by_item: HashMap<u32, String>,
    /// Lowercase name -> the items with it, lowest first.
    by_name: HashMap<String, Vec<u32>>,
}

static NAMES: Mutex<Option<Arc<Names>>> = Mutex::new(None);

/// A name the game keeps for a cut item ("[ERROR]Type 10", "<?null?>"), or none.
fn is_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.starts_with("[ERROR]") || t.starts_with("<?")
}

fn names() -> Result<Arc<Names>, String> {
    let mut slot = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = slot.as_ref() {
        return Ok(n.clone());
    }
    loaded()?;
    flush_staged();
    let mut by_item = HashMap::new();
    for &(kind, _) in items::CATEGORIES {
        // In reverse, so the FMG the game tries first is the one kept.
        for &f in category(&format!("{kind}_name"))?.iter().rev() {
            let Some(t) = table(f) else { continue };
            for (id, name) in fmg::read(t, &memory::read_bytes).unwrap_or_default() {
                if let Ok(item) = items::make(kind, id as i64) {
                    by_item.insert(item, name);
                }
            }
        }
    }
    by_item.retain(|_, n| !is_placeholder(n));
    if by_item.is_empty() {
        return Err("the game's item names aren't loaded yet".into());
    }
    let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
    for (&item, name) in &by_item {
        by_name.entry(name.trim().to_lowercase()).or_default().push(item);
    }
    by_name.values_mut().for_each(|v| v.sort_unstable());
    kindle_core::info!("text: {} item names", by_item.len());
    let n = Arc::new(Names { by_item, by_name });
    *slot = Some(n.clone());
    Ok(n)
}

/// The item a name is, in the game's language (a mod's rename included), of one category or any: a
/// weapon's name may end in its upgrade level ("Dagger +5").
pub fn item_by_name(name: &str, kind: Option<&str>) -> Result<u32, String> {
    let wanted = name.trim();
    let (base, level) = match wanted.rsplit_once(" +") {
        Some((n, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()) => (n, l.parse::<u32>().unwrap_or(u32::MAX)),
        _ => (wanted, 0),
    };
    let names = names()?;
    let of_kind = |item: &u32| kind.is_none_or(|k| items::category(*item) == Ok(k));
    let mut found: Vec<u32> = renamed(base).into_iter().filter(of_kind).collect();
    if found.is_empty() {
        found = names.by_name.get(&base.to_lowercase()).map(|v| v.iter().copied().filter(of_kind).collect()).unwrap_or_default();
    }
    let item = match found.as_slice() {
        [] => {
            let hint: Vec<String> = find(base, 3)?.into_iter().map(|(_, n)| format!("{n:?}")).collect();
            let scope = kind.map(|k| format!(" {k}")).unwrap_or_default();
            return Err(if hint.is_empty() {
                format!("no{scope} item is named {wanted:?}")
            } else {
                format!("no{scope} item is named {wanted:?} (did you mean {}?)", hint.join(", "))
            });
        }
        [one] => *one,
        many => {
            let mut kinds: Vec<&str> = many.iter().filter_map(|&i| items::category(i).ok()).collect();
            kinds.dedup();
            if kinds.len() > 1 {
                let ways: Vec<String> = kinds.iter().map(|k| format!("items.{k}({wanted:?})")).collect();
                return Err(format!("{wanted:?} is more than one item: {}", ways.join(" or ")));
            }
            // One category's several rows of one name: the lowest is the item.
            many[0]
        }
    };
    if level == 0 {
        return Ok(item);
    }
    if items::category(item) != Ok("weapon") {
        return Err(format!("{wanted:?}: only weapons have upgrade levels"));
    }
    if level > MOST_UPGRADE {
        return Err(format!("{wanted:?}: a weapon's upgrade level is 0 to {MOST_UPGRADE}"));
    }
    Ok(item + level)
}

/// Items a mod named `name` (any case), renamed or new.
fn renamed(name: &str) -> Vec<u32> {
    let wanted = name.to_lowercase();
    with_state(|s| {
        let mut out = Vec::new();
        for &(kind, _) in items::CATEGORIES {
            let Ok(fmgs) = category(&format!("{kind}_name")) else { continue };
            let staged = s.staged.iter().flat_map(|(&f, m)| m.iter().map(move |(&id, e)| (f, id, &e.0)));
            for (f, id, text) in s.replaced.iter().map(|(&(f, id), r)| (f, id, &r.text)).chain(staged) {
                if fmgs.contains(&f)
                    && text.to_lowercase() == wanted
                    && let Ok(item) = items::make(kind, id as i64)
                    && !out.contains(&item)
                {
                    out.push(item);
                }
            }
        }
        out.sort_unstable();
        out
    })
}

/// Items whose names hold `query` (any case): exact names first, then by item.
pub fn find(query: &str, limit: usize) -> Result<Vec<(u32, String)>, String> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let names = names()?;
    let mut hits: Vec<(bool, u32, &String)> =
        names.by_item.iter().filter(|(_, n)| n.to_lowercase().contains(&q)).map(|(&i, n)| (n.to_lowercase() != q, i, n)).collect();
    hits.sort_by_key(|h| (h.0, h.1));
    Ok(hits.into_iter().take(limit).map(|(_, i, n)| (i, n.clone())).collect())
}

pub fn register(lua: &mlua::Lua, n: &mlua::Table) -> mlua::Result<()> {
    use kindle_core::mods::{ctx, rt_err};
    n.set("text_get", lua.create_function(|_, (cat, id): (String, u32)| get(&category(&cat).map_err(rt_err)?, id).map_err(rt_err))?)?;
    n.set(
        "text_set",
        lua.create_function(|lua, (cat, id, value): (String, u32, String)| {
            let owner = ctx(lua)?.id.clone();
            set_or_add(&category(&cat).map_err(rt_err)?, id, &value, &owner).map_err(rt_err)
        })?,
    )?;
    n.set("text_reset", lua.create_function(|_, (cat, id): (String, u32)| reset(&category(&cat).map_err(rt_err)?, id).map_err(rt_err))?)?;
    n.set("text_categories", lua.create_function(|lua, ()| lua.create_sequence_from(CATEGORIES.iter().map(|(n, _)| *n)))?)?;
    n.set("text_item_get", lua.create_function(|_, (it, field): (i64, String)| item(items::check(it).map_err(rt_err)?, &field).map_err(rt_err))?)?;
    n.set(
        "text_item_set",
        lua.create_function(|lua, (it, field, value): (i64, String, String)| {
            let (fmgs, id, _) = item_target(items::check(it).map_err(rt_err)?, &field).map_err(rt_err)?;
            let owner = ctx(lua)?.id.clone();
            set_or_add(&fmgs, id, &value, &owner).map_err(rt_err)
        })?,
    )?;
    n.set(
        "text_item_reset",
        lua.create_function(|_, (it, field): (i64, String)| {
            let (fmgs, id, _) = item_target(items::check(it).map_err(rt_err)?, &field).map_err(rt_err)?;
            reset(&fmgs, id).map_err(rt_err)
        })?,
    )?;
    n.set("item_by_name", lua.create_function(|_, (name, kind): (String, Option<String>)| item_by_name(&name, kind.as_deref()).map_err(rt_err))?)?;
    n.set(
        "item_find",
        lua.create_function(|lua, (query, limit): (String, Option<usize>)| {
            let out = lua.create_table()?;
            for (k, (it, name)) in find(&query, limit.unwrap_or(20)).map_err(rt_err)?.into_iter().enumerate() {
                let t = lua.create_table()?;
                t.set("name", name)?;
                t.set("category", items::category(it).map_err(rt_err)?)?;
                t.set("id", it & items::ID_BITS)?;
                t.set("item", it)?;
                out.set(k + 1, t)?;
            }
            Ok(out)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// An FMG laid out as the game holds it (kindle-core's fmg): one group per run of ids, the offset
    /// table's address absolute.
    fn table_of(entries: &[(u32, &str)]) -> usize {
        let mut groups: Vec<(usize, u32, u32)> = Vec::new();
        for (i, &(id, _)) in entries.iter().enumerate() {
            match groups.last_mut() {
                Some((_, _, last)) if *last + 1 == id => *last = id,
                _ => groups.push((i, id, id)),
            }
        }
        let offsets_at = 0x28 + groups.len() * 16;
        let strings_at = offsets_at + entries.len() * 8;
        let strings: Vec<Vec<u16>> = entries.iter().map(|(_, t)| t.encode_utf16().chain([0]).collect()).collect();
        let size = strings_at + strings.iter().map(|s| s.len() * 2).sum::<usize>();
        let words: &'static mut [u64] = Box::leak(vec![0u64; size.div_ceil(8)].into_boxed_slice());
        let base = words.as_ptr() as usize;
        let b = unsafe { std::slice::from_raw_parts_mut(words.as_mut_ptr() as *mut u8, size) };
        b[0..4].copy_from_slice(&[0, 0, 2, 0]);
        b[0x0C..0x10].copy_from_slice(&(groups.len() as i32).to_le_bytes());
        b[0x10..0x14].copy_from_slice(&(entries.len() as i32).to_le_bytes());
        b[0x18..0x20].copy_from_slice(&((base + offsets_at) as u64).to_le_bytes());
        for (g, &(index, first, last)) in groups.iter().enumerate() {
            let at = 0x28 + g * 16;
            b[at..at + 4].copy_from_slice(&(index as i32).to_le_bytes());
            b[at + 4..at + 8].copy_from_slice(&first.to_le_bytes());
            b[at + 8..at + 12].copy_from_slice(&last.to_le_bytes());
        }
        let mut at = strings_at;
        for (k, units) in strings.iter().enumerate() {
            b[offsets_at + k * 8..offsets_at + k * 8 + 8].copy_from_slice(&(at as u64).to_le_bytes());
            for u in units {
                b[at..at + 2].copy_from_slice(&u.to_le_bytes());
                at += 2;
            }
        }
        base
    }

    /// A repository laid out as the game's, served to Kindle (TEST_REPOSITORY) until dropped: these FMGs
    /// in version 0, the only one.
    pub(crate) struct Text {
        _repository: Box<[u64; 3]>,
        _versions: Box<[u64; 1]>,
        _fmgs: Box<[u64]>,
    }

    impl Text {
        pub(crate) fn serve(tables: &[(u32, &[(u32, &str)])]) -> Self {
            let mut fmgs = vec![0u64; 0x200].into_boxed_slice();
            for &(id, entries) in tables {
                fmgs[id as usize] = table_of(entries) as u64;
            }
            let versions = Box::new([fmgs.as_ptr() as u64]);
            let repository = Box::new([0, versions.as_ptr() as u64, 1 | (0x200 << 32)]);
            *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            // As the DLL's install does in the game.
            HOOKED.store(true, Ordering::Relaxed);
            *NAMES.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *TEST_REPOSITORY.lock().unwrap_or_else(|e| e.into_inner()) = Some(repository.as_ptr() as usize);
            Text { _repository: repository, _versions: versions, _fmgs: fmgs }
        }

        /// Elden Ring's names for a few items, a patched one and one of the DLC's, and a cut ash.
        pub(crate) fn items() -> Self {
            Self::serve(&[
                (11, &[(1000000, "Dagger"), (1000100, "Heavy Dagger"), (2000000, "Longsword")]),
                (25, &[(1000000, "A common dagger.")]),
                (10, &[(10100, "Smithing Stone [1]"), (10101, "Smithing Stone [2]"), (2000000, "Longsword")]),
                (0x6F, &[(10101, "Smithing Stone [2] (patched)")]),
                (0x13F, &[(2010000, "Scadutree Fragment")]),
                (35, &[(10000, "Ash of War: Lion's Claw"), (10100, "[ERROR]")]),
            ])
        }
    }

    impl Drop for Text {
        fn drop(&mut self) {
            *TEST_REPOSITORY.lock().unwrap_or_else(|e| e.into_inner()) = Some(0);
            *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
            *NAMES.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    #[test]
    fn text_is_read_and_replaced_as_the_game_shows_it() {
        let _one = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = Text::items();
        let goods = category("goods_name").unwrap();
        // The game's order: its own FMG first, then the base game's, then the DLC's.
        assert_eq!(get(&goods, 10100).unwrap().as_deref(), Some("Smithing Stone [1]"));
        assert_eq!(get(&goods, 10101).unwrap().as_deref(), Some("Smithing Stone [2] (patched)"));
        assert_eq!(get(&goods, 2010000).unwrap().as_deref(), Some("Scadutree Fragment"));
        assert_eq!(get(&goods, 10102).unwrap(), None);
        assert_eq!(get(&[10], 10101).unwrap().as_deref(), Some("Smithing Stone [2]"));
        let weapon = |id| items::make("weapon", id).unwrap();
        assert_eq!(item(weapon(1000005), "name").unwrap().as_deref(), Some("Dagger +5"));
        assert_eq!(item(weapon(1000000), "description").unwrap().as_deref(), Some("A common dagger."));
        assert_eq!(item(weapon(1000000), "info").unwrap(), None);
        assert!(item(weapon(1000000), "lore").unwrap_err().contains("name, info, description"));

        // Replaced in each FMG that has the entry, read back, and put back by the mod or as it unloads.
        assert_eq!(set(&goods, 10100, "Stone", "m").unwrap(), 1);
        assert_eq!(get(&goods, 10100).unwrap().as_deref(), Some("Stone"));
        assert_eq!(set(&goods, 10101, "Two", "m").unwrap(), 2);
        assert_eq!((get(&goods, 10101).unwrap().as_deref(), get(&[10], 10101).unwrap().as_deref()), (Some("Two"), Some("Two")));
        assert_eq!(set(&goods, 10102, "None", "m").unwrap(), 0);
        assert_eq!(reset(&goods, 10100).unwrap(), 1);
        assert_eq!(get(&goods, 10100).unwrap().as_deref(), Some("Smithing Stone [1]"));
        assert_eq!(reset_owner("other"), 0);
        assert_eq!(reset_owner("m"), 2);
        assert_eq!(get(&goods, 10101).unwrap().as_deref(), Some("Smithing Stone [2] (patched)"));

        // Items by name: any case, a weapon's level, the category asked for; cut items have no name.
        assert_eq!(item_by_name("dagger +5", None), Ok(weapon(1000005)));
        assert_eq!(item_by_name("Smithing Stone [1]", None), items::make("goods", 10100));
        assert_eq!(item_by_name("Longsword", Some("goods")), items::make("goods", 2000000));
        assert!(item_by_name("Longsword", None).unwrap_err().contains("items.weapon(\"Longsword\") or items.goods(\"Longsword\")"));
        assert!(item_by_name("Dagger +26", None).unwrap_err().contains("0 to 25"));
        assert!(item_by_name("Smithing Stone [1] +1", None).unwrap_err().contains("only weapons"));
        let e = item_by_name("Smithing Stone", None).unwrap_err();
        assert!(e.contains("did you mean \"Smithing Stone [1]\""), "{e}");
        assert!(item_by_name("[ERROR]", None).is_err());
        let found: Vec<String> = find("dagger", 5).unwrap().into_iter().map(|(_, n)| n).collect();
        assert_eq!(found, ["Dagger", "Heavy Dagger"]);
        // A mod's rename is a name too.
        set(&category("ash_name").unwrap(), 10000, "Claw", "m").unwrap();
        assert_eq!(item_by_name("claw", None), items::make("ash", 10000));
        reset_owner("m");

        drop(text);
        assert!(get(&goods, 10100).unwrap_err().contains("isn't loaded yet"));
    }

    /// The game's release as the hook runs it (freeing nothing): the FMG it found in the slot, the slot then
    /// emptied.
    static RELEASED: Mutex<Vec<u64>> = Mutex::new(Vec::new());

    extern "C" fn game_release(_repository: u64, _version: u64, id: u64) -> u64 {
        let slot = slot(id as u32).unwrap_or_default();
        RELEASED.lock().unwrap_or_else(|e| e.into_inner()).push(memory::read_u64(slot).unwrap_or_default());
        unsafe { std::ptr::write_volatile(slot as *mut u64, 0) };
        0
    }

    fn registers(repository: usize, id: u32) -> Registers {
        let mut r: Registers = unsafe { std::mem::zeroed() };
        (r.rcx, r.rdx, r.r8) = (repository as u64, 0, id as u64);
        r
    }

    #[test]
    fn text_the_game_hasnt_is_added_and_the_game_frees_only_its_own() {
        let _one = crate::params::tests::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let text = Text::items();
        let goods = category("goods_name").unwrap();
        let games = table(0x6F).unwrap();
        // A change made before the FMG is copied stays in the copy.
        assert_eq!(set(&goods, 10101, "Two", "m").unwrap(), 2);
        // Added to the first FMG of the text's the game has loaded (its own goods names, 0x6F): staged, and
        // copied in as Kindle next reads the text (or the frame ends), found by name meanwhile.
        assert_eq!(set_or_add(&goods, 3020000, "Ember", "m").unwrap(), 1);
        assert_eq!(table(0x6F), Some(games));
        assert_eq!(item_by_name("ember", None), items::make("goods", 3020000));
        assert_eq!(get(&goods, 3020000).unwrap().as_deref(), Some("Ember"));
        let copy = table(0x6F).unwrap();
        assert_ne!(copy, games);
        assert_eq!(get(&[10], 3020000).unwrap(), None);
        assert_eq!(get(&goods, 10101).unwrap().as_deref(), Some("Two"));
        let copies = || with_state(|s| s.retired.len() + s.grown.len());
        // Hundreds more above it fill the copy in place, then the rest go into one new copy.
        for k in 1..=600 {
            assert_eq!(set_or_add(&goods, 3020000 + k, &format!("Goods {k}"), "m").unwrap(), 1);
            if k == 256 {
                assert_eq!(table(0x6F), Some(copy), "the copy's room filled in place");
            }
        }
        let before = copies();
        // Ids below the last, in any order, go into one copy together.
        for id in (10200..10250).rev() {
            assert_eq!(set_or_add(&goods, id, "Middle", "other").unwrap(), 1);
        }
        assert_eq!(get(&goods, 10200).unwrap().as_deref(), Some("Middle"));
        assert_eq!(copies(), before + 1);
        assert_ne!(table(0x6F), Some(copy));
        let ids: Vec<u32> = fmg::read(table(0x6F).unwrap(), &memory::read_bytes).unwrap().into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids.len(), 652);
        assert!(ids.is_sorted() && ids[..3] == [10101, 10200, 10201] && ids[51] == 3020000);
        assert!((1..=600).all(|k| get(&goods, 3020000 + k).unwrap() == Some(format!("Goods {k}"))));
        assert!((10200..10250).all(|id| get(&goods, id).unwrap().as_deref() == Some("Middle")));
        // Changed or reset while staged, as in a copy.
        assert_eq!(set_or_add(&goods, 10100, "Staged", "other").unwrap(), 1);
        assert_eq!(set_or_add(&goods, 10250, "Staged", "other").unwrap(), 1);
        assert_eq!(set_or_add(&goods, 10250, "Staged again", "other").unwrap(), 1);
        assert_eq!(set_or_add(&goods, 10251, "Gone", "other").unwrap(), 1);
        assert_eq!(reset(&goods, 10251).unwrap(), 1);
        assert_eq!(get(&goods, 10250).unwrap().as_deref(), Some("Staged again"));
        assert_eq!(get(&goods, 10251).unwrap(), None);
        assert_eq!(reset_owner("other"), 52);
        // Set again and reset: an added entry reset shows nothing, a replaced one the game's text.
        assert_eq!(set_or_add(&goods, 3020000, "Embers", "m").unwrap(), 1);
        assert_eq!(get(&goods, 3020000).unwrap().as_deref(), Some("Embers"));
        assert_eq!(reset(&goods, 10101).unwrap(), 2);
        assert_eq!(get(&goods, 10101).unwrap().as_deref(), Some("Smithing Stone [2] (patched)"));
        assert_eq!(get(&goods, 10200).unwrap(), None);
        assert_eq!(get(&goods, 10100).unwrap().as_deref(), Some("Smithing Stone [1]"));

        // The game lets 0x6F go: it frees its own FMG, not Kindle's copy, and Kindle's changes wait for it.
        let repo = repository().unwrap();
        RELEASED.lock().unwrap().clear();
        release_hook(&mut registers(repo, 0x6F), game_release as *const () as usize);
        assert_eq!(*RELEASED.lock().unwrap(), [games as u64]);
        assert_eq!(get(&goods, 3020000).unwrap(), None);
        assert_eq!(set_or_add(&goods, 3020001, "Goods one", "m").unwrap(), 1);
        // Loaded again: the next frame makes them again.
        let again = table_of(&[(10101, "Smithing Stone [2] (patched)")]);
        unsafe { std::ptr::write_volatile(slot(0x6F).unwrap() as *mut u64, again as u64) };
        tick();
        assert_ne!(table(0x6F), Some(again));
        assert_eq!(get(&goods, 3020000).unwrap().as_deref(), Some("Embers"));
        assert_eq!(get(&goods, 3020001).unwrap().as_deref(), Some("Goods one"));
        assert_eq!(get(&goods, 3020600).unwrap().as_deref(), Some("Goods 600"));
        assert_eq!(get(&goods, 10101).unwrap().as_deref(), Some("Smithing Stone [2] (patched)"));
        // The mod unloading takes its entries out.
        assert_eq!(reset_owner("m"), 601);
        assert_eq!(get(&goods, 3020000).unwrap(), None);

        // The repository's end: the game's own FMG back in its slot, and nothing changed after.
        assert_eq!(set_or_add(&goods, 3020000, "Last", "m").unwrap(), 1);
        end_hook(&mut registers(repo, 0));
        assert_eq!(table(0x6F), Some(again));
        assert!(set_or_add(&goods, 3020000, "After", "m").unwrap_err().contains("quitting"));
        drop(text);
    }

    #[test]
    fn categories_are_named_or_an_fmg() {
        assert_eq!(category("ring_name"), category("talisman_name"));
        assert_eq!(category("10"), Ok(vec![10]));
        assert!(category("goods").unwrap_err().contains("goods_name"));
        let mut names: Vec<&str> = CATEGORIES.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CATEGORIES.len());
    }

    /// Each category's getter in Elden Ring 2.7.0.0.
    const GETTERS: &[(&str, usize)] = &[
        ("weapon_name", 0xD129C0),
        ("weapon_info", 0xD12940),
        ("weapon_caption", 0xD12860),
        ("weapon_effect", 0xD128E0),
        ("armor_name", 0xD123E0),
        ("armor_info", 0xD12360),
        ("armor_caption", 0xD122E0),
        ("talisman_name", 0xD11470),
        ("talisman_info", 0xD113F0),
        ("talisman_caption", 0xD11370),
        ("goods_name", 0xD11CD0),
        ("goods_info", 0xD11C50),
        ("goods_caption", 0xD11B00),
        ("goods_info2", 0xD11BF0),
        ("goods_dialog", 0xD11B80),
        ("ash_name", 0xD11A40),
        ("ash_info", 0xD119E0),
        ("ash_caption", 0xD11920),
        ("ash_effect", 0xD11980),
        ("skill_name", 0xD11640),
        ("skill_caption", 0xD115D0),
        ("npc_name", 0xD121B0),
        ("place_name", 0xD12230),
        ("talk", 0xD12610),
        ("blood_message", 0xD116B0),
        ("network_message", 0xD12140),
        ("action_button", 0xD11560),
        ("event_text", 0xD11860),
        ("event_text_map", 0xD117F0),
        ("menu_text", 0xD12070),
        ("line_help", 0xD11E80),
        ("key_guide", 0xD11E20),
        ("system_message", 0xD12530),
        ("dialogues", 0xD11740),
        ("loading_title", 0xD11F40),
        ("loading_text", 0xD11EE0),
        ("tutorial_title", 0xD127D0),
        ("tutorial_body", 0xD12770),
    ];
    /// The repository's lookup (version, FMG id, entry: +0x10, +0x14, +8), going on to an FMG's search.
    const LOOKUP: &str = "3B 51 10 73 ?? 44 3B 41 14 73 ?? 48 8B 41 08 8B D2 48 8B 0C D0 48 85 C9 74 ?? 41 8B C0 48 8B 0C C1 48 85 C9 74 ?? 41 8B D1 E9";
    /// An FMG's search: the group count +0x0C, the groups from +0x28 (first id +4, last +8, index +0)
    /// searched by id, the offset table +0x18, the string at the FMG plus its offset.
    const SEARCH: &str = "44 8B 41 0C 45 33 C9 41 FF C8 4C 8B D9 3B 51 2C 72 ?? 41 8B C0 48 83 C0 03 48 03 C0 3B 14 C1 77 ?? 0F 1F 40 00 66 66 66 0F 1F 84 00 00 00 00 00 43 8D 0C 08 D1 E9 44 8B D1 49 C1 E2 04 4D 03 D3 41 3B 52 30 76 ?? 44 8D 49 01 EB ?? 41 3B 52 2C 73 ?? 44 8D 41 FF 45 3B C8 76 ?? 33 C0 C3 41 2B 52 2C 41 03 52 28 49 8B 43 18 48 8B 14 D0 48 85 D2 74 ?? 4A 8D 04 1A C3";

    /// With Elden Ring 2.7.0.0's exe (KINDLE_ER_EXE): the repository's static from the code that makes it
    /// (whose call is MsgRepositoryImp's constructor), the layout Kindle reads it and its FMGs by in the
    /// game's lookup and search, each category's FMGs, in order, in its getter's code, and the two places
    /// Kindle hooks: the release, called only by a bundle's release and the load into a full slot, and the
    /// repository's end, which MsgRepositoryImp's destructor calls.
    #[test]
    fn the_texts_layout_is_the_games() {
        let Some((img, base, sections)) = crate::singletons::tests::eldenring() else {
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
        assert_eq!(find(REPOSITORY), [0xCD91DA]);
        assert_eq!(rel(0xCD91DA + REPOSITORY_STATIC.0, 0xCD91DA + REPOSITORY_STATIC.1), 0x3D81568);
        assert_eq!(rel(0xCD91DA + 0x3E, 0xCD91DA + 0x42), 0x3D81568);
        assert_eq!(rel(0xCD91DA + 0x2F, 0xCD91DA + 0x33), 0xD11270);
        assert_eq!(find(LOOKUP), [0x266FBD0]);
        assert_eq!(rel(0x266FBD0 + 0x2A, 0x266FBD0 + 0x2E), 0x26704A0);
        assert_eq!(find(SEARCH), [0x26704A0]);
        assert_eq!(find(RELEASE), [0x266FE90]);
        let calls_of = |to: usize| -> Vec<usize> {
            (0..text.len() - 5).map(|i| rva + i).filter(|&at| matches!(img[at], 0xE8 | 0xE9) && rel(at + 1, at + 5) == to).collect()
        };
        assert_eq!(calls_of(0x266FE90), [0xD12835, 0x267003D]);
        assert_eq!(rel(0x21C03A + 1, 0x21C03A + 5), 0xD12830);
        assert_eq!(find(END), [0x26700E0]);
        // The end's call in the destructor in MsgRepositoryImp's vtable (by its RTTI), after it sets that vtable.
        assert!(calls_of(0x26700E0).contains(&0xD11347));
        let pointer = |at: usize| (u64::from_le_bytes(img[at..at + 8].try_into().unwrap()) - base) as usize;
        let vtable = rel(0xD1133D + 3, 0xD1133D + 7);
        let type_at = i32::from_le_bytes(img[pointer(vtable - 8) + 12..][..4].try_into().unwrap()) as usize;
        assert!(img[type_at + 0x10..].starts_with(b".?AVMsgRepositoryImp@CS@@\0"));
        assert_eq!(pointer(vtable), 0xD11320);
        // What a getter loads into r8d (the FMG id) before each call of the lookup, up to its pop rdi; ret.
        let ids_of = |at: usize| -> Vec<u32> {
            let (mut out, mut r8d) = (Vec::new(), None);
            for i in at..at + 0x100 {
                let b = &img[i..i + 6];
                if b[..2] == [0x5F, 0xC3] {
                    break;
                }
                if b[..2] == [0x41, 0xB8] {
                    r8d = Some(u32::from_le_bytes(b[2..6].try_into().unwrap()));
                } else if b[..2] == [0x44, 0x8D] && b[2] & 0xF8 == 0x40 {
                    r8d = Some(b[3] as u32);
                } else if b[0] == 0xE8 && rel(i + 1, i + 5) == 0x266FBD0 {
                    out.extend(r8d.take());
                }
            }
            out
        };
        assert_eq!(GETTERS.len(), CATEGORIES.len());
        for &(name, at) in GETTERS {
            assert_eq!(ids_of(at), category(name).unwrap(), "{name} at {at:#x}");
        }
    }
}
