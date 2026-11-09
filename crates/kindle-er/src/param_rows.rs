//! New param rows (`params.add_rows`): brand-new items, effects, ... with ids the mod chooses.
//!
//! An Elden Ring table in memory is the param file as on disk (a 0x40-byte header, an index of {u64
//! id, u64 data offset, u64 name offset} per row, the rows' data as one array in index order, then the
//! type name and the rows' names) with {u32 size, u32 lookup count} in the 16 bytes before it and, at
//! the table + its size rounded up to 16, the {u32 id, i32 row index} array sorted by id that every
//! read of a row binary-searches. Kindle builds a longer copy with the
//! new rows and points the table's holder (the object at its cap's +0x80, at +0x80) at it.
//!
//! Many tables end in dummy rows (999999999; 999999998 and others from 999999000 in some:
//! EquipParamAccessory, EquipParamGoods, SpEffectParam, Bullet, AtkParam_Pc ...), and an item's id is
//! 28 bits (its category is in the top four), so a table's new rows go above every row but its end
//! rows, and below those: only the end rows move (their positions: nothing should remember a dummy's
//! [u: live]). An item table's new ids stay under 2^28.
//!
//! Seven tables have an index the game builds from their rows when their cap registers (the weapons'
//! upgrade chain, graces ...): sized by the row count then, it would be read past for a new row. So
//! Kindle has the game take the cap out of its slot (which frees the index), installs the new table,
//! and has the game register the cap again (which builds the index from the new table), by the game's
//! own functions; for every table, so a table this list misses is covered too. In between, a read of
//! the param finds none, as during the game's load.
//!
//! Added rows stay for the rest of the session, even if the mod unloads: an item the player holds
//! must keep its row. Adding the same id again (a mod reloading) keeps that row.

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::sync::{Mutex, OnceLock};

use kindle_core::{memory, patterns};

use crate::params::{self, Loaded};

const HEADER: usize = 0x40;
const INDEX_ENTRY: usize = 0x18;
/// A table's end rows: its last rows, with ids in this range.
const END_ROWS: RangeInclusive<u64> = 999_999_000..=999_999_999;
/// The tables whose ids are item ids (28 bits).
const ITEM_TABLES: &[&str] = &["EquipParamWeapon", "EquipParamProtector", "EquipParamAccessory", "EquipParamGoods", "EquipParamGem"];
const ITEM_ID_LIMIT: u64 = 1 << 28;

/// The game's register-a-cap (`SoloParamRepository`, cap): puts the cap in its param's slot and
/// builds the param's index if it has one; 1 if it did.
const REGISTER: &str = "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B D9 48 8B F2 48 8B CA 40 32 FF E8 ?? ?? ?? ?? 4C 63 C0 41 81 F8 C2 00 00 00 0F 8D ?? ?? ?? ?? 4B 8D 04 C0 4C 8D 0C C3 48 63 84 C3 80 00 00 00";
/// The game's unregister-a-cap: takes it out of its slot and frees the param's index; 1 if it did.
const UNREGISTER: &str = "40 53 57 41 57 48 83 EC 20 48 8B D9 48 8B FA 48 8B CA 45 32 FF E8 ?? ?? ?? ?? 4C 63 C0 41 81 F8 C2 00 00 00 0F 8D ?? ?? ?? ?? 48 89 6C 24 40 4B 8D 04 C0 48 63 AC C3 80 00 00 00 33 D2 48 89 74 24 48";

type CapFn = unsafe extern "C" fn(usize, usize) -> u8;

/// The game's register and unregister, found once.
fn cap_fns() -> Result<(CapFn, CapFn), String> {
    #[cfg(test)]
    if params::TEST_REPO.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        return Ok((params::tests::register as CapFn, params::tests::unregister as CapFn));
    }
    static FNS: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    let found = FNS.get_or_init(|| Some((patterns::find_unique("param cap register", REGISTER)?, patterns::find_unique("param cap unregister", UNREGISTER)?)));
    let &(register, unregister) = found.as_ref().ok_or("the game's param registration wasn't found in this build")?;
    // Found by their code in the game's .text: the functions the game registers its params with.
    let register: CapFn = unsafe { std::mem::transmute(register) };
    let unregister: CapFn = unsafe { std::mem::transmute(unregister) };
    Ok((register, unregister))
}

/// A table as it is in memory: the 16 bytes before it, the table, its row lookup.
struct Image {
    prefix: [u8; 16],
    bytes: Vec<u8>,
    lookup: Vec<(u32, i32)>,
}

impl Image {
    fn read(table: usize) -> Result<Self, String> {
        let prefix: [u8; 16] = memory::read_bytes(table - 16, 16).ok_or("table unreadable")?.try_into().unwrap();
        let size = u32::from_le_bytes(prefix[0..4].try_into().unwrap()) as usize;
        let count = u32::from_le_bytes(prefix[4..8].try_into().unwrap()) as usize;
        let bytes = memory::read_bytes(table, size).ok_or("table unreadable")?;
        let raw = memory::read_bytes(table + size.next_multiple_of(16), count * 8).ok_or("row lookup unreadable")?;
        let lookup =
            raw.as_chunks::<8>().0.iter().map(|e| (u32::from_le_bytes(e[0..4].try_into().unwrap()), i32::from_le_bytes(e[4..8].try_into().unwrap()))).collect();
        Ok(Self { prefix, bytes, lookup })
    }

    fn u64_at(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.bytes[at..at + 8].try_into().unwrap())
    }

    fn rows(&self) -> usize {
        u16::from_le_bytes([self.bytes[0x0A], self.bytes[0x0B]]) as usize
    }

    /// The rows' ids in index order.
    fn ids(&self) -> Vec<u64> {
        (0..self.rows()).map(|i| self.u64_at(HEADER + i * INDEX_ENTRY)).collect()
    }

    /// Row `i`'s data offset.
    fn data_of(&self, i: usize) -> usize {
        self.u64_at(HEADER + i * INDEX_ENTRY + 8) as usize
    }
}

/// How many of a table's rows (ids in index order) are its end rows.
fn end_rows(ids: &[u64]) -> usize {
    ids.iter().rev().take_while(|id| END_ROWS.contains(id)).count()
}

/// Where new rows with ids `new` (ascending) go in a table whose rows have `ids`: the place of its
/// first end row (its end, without any). An error says why they can't go there.
fn insertion_point(table: &str, ids: &[u64], new: &[u64]) -> Result<usize, String> {
    if let Some(w) = new.windows(2).find(|w| w[1] <= w[0]) {
        return Err(format!("new {table} rows need ascending ids ({} comes after {})", w[1], w[0]));
    }
    let at = ids.len() - end_rows(ids);
    let (Some(&first), Some(&last)) = (new.first(), new.last()) else { return Ok(at) };
    if let Some(&top) = ids[..at].iter().max()
        && first <= top
    {
        return Err(format!(
            "new {table} rows need ids above {top} (the highest row so far but the table's end rows; rows are added in the order mods load, so pick ids well apart from other mods')"
        ));
    }
    if let Some(&end) = ids[at..].iter().min()
        && last >= end
    {
        return Err(format!("new {table} rows need ids below {end} (the table's end row: the game's own rows end there)"));
    }
    if ITEM_TABLES.contains(&table) && last >= ITEM_ID_LIMIT {
        return Err(format!("new {table} rows need ids below {ITEM_ID_LIMIT} (an item's id is 28 bits)"));
    }
    Ok(at)
}

/// A rebuilt table: one 16-aligned block with the 16 bytes before the table, the table at +16, then
/// its row lookup.
struct Built {
    block: Vec<u128>,
}

impl Built {
    fn table(&self) -> usize {
        self.block.as_ptr() as usize + 16
    }
}

/// `old` with rows `new` (id, data) in index place `at`, each `row_size` bytes: the rows before it
/// keep their places, those from it (the end rows) move past the new ones.
fn build(name: &str, old: &Image, at: usize, new: &[(u64, Vec<u8>)], row_size: usize) -> Result<Built, String> {
    let n = old.rows();
    let m = new.len();
    let size = old.bytes.len();
    if n + m > u16::MAX as usize {
        return Err(format!("{name} can't hold more than {} rows", u16::MAX));
    }
    if row_size == 0 {
        return Err(format!("{name} has no row size"));
    }
    if let Some((id, _)) = new.iter().find(|(_, d)| d.len() != row_size) {
        return Err(format!("{name}: row {id}'s data isn't a row of {row_size} bytes"));
    }
    let index_end = HEADER + n * INDEX_ENTRY;
    let data_end = index_end + n * row_size;
    // Every Elden Ring table's rows are one array right after the index, in index order; the game's
    // walks over a table rely on it too (the weapons' index is built row by row).
    if (0..n).any(|i| old.data_of(i) != index_end + i * row_size) || data_end > size {
        return Err(format!("{name}'s rows aren't laid out as Kindle expects (one array after the index)"));
    }
    let delta = m * INDEX_ENTRY;
    let grow = m * row_size;
    let new_size = size + delta + grow + 8;
    let new_index_end = index_end + delta;
    // An old offset's new value: past the rows (the type name, the names) by all that's new; into the
    // rows by the index's growth, and the new rows' too from the end rows on.
    let shift = |v: u64| -> u64 {
        match v as usize {
            o if o >= data_end && o <= size => v + (delta + grow) as u64,
            o if o >= index_end && o < data_end => v + (delta + if (o - index_end) / row_size >= at { grow } else { 0 }) as u64,
            _ => v,
        }
    };

    let mut lookup: Vec<(u32, i32)> = old.lookup.iter().map(|&(id, i)| (id, if i >= at as i32 { i + m as i32 } else { i })).collect();
    lookup.extend(new.iter().enumerate().map(|(k, (id, _))| (*id as u32, (at + k) as i32)));
    lookup.sort_by_key(|&(id, _)| id);

    let lookup_at = 16 + new_size.next_multiple_of(16);
    let total = lookup_at + lookup.len() * 8;
    let mut block = vec![0u128; total.div_ceil(16)];
    let all = unsafe { std::slice::from_raw_parts_mut(block.as_mut_ptr() as *mut u8, total) };
    all[..16].copy_from_slice(&old.prefix);
    all[0..4].copy_from_slice(&(new_size as u32).to_le_bytes());
    all[4..8].copy_from_slice(&(lookup.len() as u32).to_le_bytes());
    for (k, (id, i)) in lookup.iter().enumerate() {
        let e = lookup_at + k * 8;
        all[e..e + 4].copy_from_slice(&id.to_le_bytes());
        all[e + 4..e + 8].copy_from_slice(&i.to_le_bytes());
    }

    let t = &mut all[16..16 + new_size];
    let put = |t: &mut [u8], at: usize, v: u64| t[at..at + 8].copy_from_slice(&v.to_le_bytes());
    // The header: the strings' and the type name's offsets moved, the data start the first row's, the count.
    t[..HEADER].copy_from_slice(&old.bytes[..HEADER]);
    let strings = u32::from_le_bytes(old.bytes[0..4].try_into().unwrap()) as u64;
    t[0..4].copy_from_slice(&(shift(strings) as u32).to_le_bytes());
    put(t, 0x10, shift(old.u64_at(0x10)));
    let data_start = old.u64_at(0x30);
    put(t, 0x30, if data_start as usize == index_end { new_index_end as u64 } else { shift(data_start) });
    t[0x0A..0x0C].copy_from_slice(&((n + m) as u16).to_le_bytes());
    // The index: the old rows', the end rows' past the new ones; the new rows' names an empty string.
    for i in 0..n {
        let to = HEADER + (if i < at { i } else { i + m }) * INDEX_ENTRY;
        let from = HEADER + i * INDEX_ENTRY;
        put(t, to, old.u64_at(from));
        put(t, to + 8, shift(old.u64_at(from + 8)));
        put(t, to + 16, shift(old.u64_at(from + 16)));
    }
    let empty_name = (new_size - 8) as u64;
    for (k, (id, data)) in new.iter().enumerate() {
        let e = HEADER + (at + k) * INDEX_ENTRY;
        let d = new_index_end + (at + k) * row_size;
        put(t, e, *id);
        put(t, e + 8, d as u64);
        put(t, e + 16, empty_name);
        t[d..d + row_size].copy_from_slice(data);
    }
    // The rows' data: the rows before `at`, the new rows (above), the end rows; then the rest.
    t[new_index_end..new_index_end + at * row_size].copy_from_slice(&old.bytes[index_end..index_end + at * row_size]);
    let ends = new_index_end + (at + m) * row_size;
    t[ends..ends + (n - at) * row_size].copy_from_slice(&old.bytes[index_end + at * row_size..data_end]);
    t[data_end + delta + grow..size + delta + grow].copy_from_slice(&old.bytes[data_end..size]);
    Ok(Built { block })
}

/// One table's rows added so far.
struct Extended {
    /// Rows added, in order: (owner mod, id).
    added: Vec<(String, u64)>,
}

static STATE: Mutex<Option<HashMap<String, Extended>>> = Mutex::new(None);

/// Points the game at a rebuilt table: the cap out of its slot (the game frees the param's index),
/// the table into the cap's holder, the cap back in (the game builds the index from the new rows).
fn install(repo: usize, p: &Loaded, built: Built) -> Result<(), String> {
    let (register, unregister) = cap_fns()?;
    if p.caps_in_slot != 1 {
        return Err(format!("{} has {} copies loaded; Kindle adds rows to a param loaded once", p.name, p.caps_in_slot));
    }
    // Only the cap's holder is moved: unlike Dark Souls III's, no other word in the game holds a
    // table (seen live: a scan of the heap after an add found only Kindle's own copies).
    let table = built.table();
    if unsafe { unregister(repo, p.cap) } == 0 {
        return Err(format!("the game didn't take {}'s cap out of its slot", p.name));
    }
    // Leaked: the game may still be reading the old copy, and tables live as long as the game.
    std::mem::forget(built);
    unsafe { std::ptr::write_volatile((p.holder + 0x80) as *mut u64, table as u64) };
    if unsafe { register(repo, p.cap) } == 0 {
        unsafe { std::ptr::write_volatile((p.holder + 0x80) as *mut u64, p.table as u64) };
        let back = unsafe { register(repo, p.cap) } != 0;
        return Err(format!("the game didn't take {}'s rebuilt table (its own put back: {back})", p.name));
    }
    Ok(())
}

/// Adds rows to a table in one rebuild: `rows` are (id, copy_from), ids ascending, above every row
/// but the table's end rows and below those (under 2^28 in an item table); rows already added this
/// session (a mod reloading) are kept. The bindings' add_param_rows.
pub fn add_rows(owner: &str, table_name: &str, rows: &[(u64, u64)]) -> Result<(), String> {
    let (canonical, row_size, templates) = {
        let registry = params::registry()?;
        let t = registry.table(table_name)?;
        let mut templates = HashMap::new();
        for &(_, copy_from) in rows {
            templates.insert(copy_from, t.row_data(copy_from)?);
        }
        (t.name.clone(), t.def.size, templates)
    };
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let map = state.get_or_insert_with(HashMap::new);
    let have = map.get(&canonical).map(|e| e.added.iter().map(|(_, id)| *id).collect::<Vec<_>>()).unwrap_or_default();
    let rows: Vec<(u64, u64)> = rows.iter().copied().filter(|(id, _)| !have.contains(id)).collect();
    if rows.is_empty() {
        return Ok(());
    }

    let started = std::time::Instant::now();
    let repo = params::repository().ok_or("params are not loaded yet")?;
    let p = params::loaded_in(repo).into_iter().find(|p| p.name == canonical).ok_or_else(|| format!("{canonical} isn't loaded"))?;
    let old = Image::read(p.table)?;
    let ids: Vec<u64> = rows.iter().map(|&(id, _)| id).collect();
    let at = insertion_point(&canonical, &old.ids(), &ids)?;
    let new: Vec<(u64, Vec<u8>)> = rows.iter().map(|&(id, from)| (id, templates[&from].clone())).collect();
    let built = build(&canonical, &old, at, &new, row_size)?;
    install(repo, &p, built)?;
    let e = map.entry(canonical.clone()).or_insert_with(|| Extended { added: Vec::new() });
    e.added.extend(ids.iter().map(|&id| (owner.to_string(), id)));
    kindle_core::info!(
        "params: {canonical}: {} row(s) added{}, {} new in all ({} ms)",
        rows.len(),
        if at < old.rows() { format!(" before its {} end row(s)", old.rows() - at) } else { String::new() },
        e.added.len(),
        started.elapsed().as_millis()
    );
    drop(state);
    // Kindle's own view of the params is rebuilt from the new table.
    params::invalidate();
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A table laid out as the game has it in memory: the 16 bytes before it, the table (header, index,
    /// rows, the type name, the rows' names), its row lookup.
    fn image(rows: &[(u64, [u8; 4])]) -> Vec<u128> {
        let n = rows.len();
        let index_end = HEADER + n * INDEX_ENTRY;
        let data_end = index_end + n * 4;
        let size = data_end + 8 + n;
        let lookup_at = 16 + size.next_multiple_of(16);
        let mut block = vec![0u128; (lookup_at + n * 8).div_ceil(16)];
        let all = unsafe { std::slice::from_raw_parts_mut(block.as_mut_ptr() as *mut u8, block.len() * 16) };
        all[0..4].copy_from_slice(&(size as u32).to_le_bytes());
        all[4..8].copy_from_slice(&(n as u32).to_le_bytes());
        all[8..16].copy_from_slice(b"prefix!!");
        let t = &mut all[16..16 + size];
        t[0..4].copy_from_slice(&(data_end as u32).to_le_bytes());
        t[0x0A..0x0C].copy_from_slice(&(n as u16).to_le_bytes());
        t[0x10..0x18].copy_from_slice(&(data_end as u64).to_le_bytes());
        t[0x30..0x38].copy_from_slice(&(index_end as u64).to_le_bytes());
        t[data_end..data_end + 7].copy_from_slice(b"TYPE_ST");
        for (i, (id, data)) in rows.iter().enumerate() {
            let e = HEADER + i * INDEX_ENTRY;
            t[e..e + 8].copy_from_slice(&id.to_le_bytes());
            t[e + 8..e + 16].copy_from_slice(&((index_end + i * 4) as u64).to_le_bytes());
            t[e + 16..e + 24].copy_from_slice(&((data_end + 8 + i) as u64).to_le_bytes());
            t[index_end + i * 4..index_end + i * 4 + 4].copy_from_slice(data);
            t[data_end + 8 + i] = b'a' + i as u8;
        }
        let mut sorted: Vec<(usize, u64)> = rows.iter().map(|r| r.0).enumerate().collect();
        sorted.sort_by_key(|&(_, id)| id);
        for (k, (i, id)) in sorted.into_iter().enumerate() {
            let e = lookup_at + k * 8;
            all[e..e + 4].copy_from_slice(&(id as u32).to_le_bytes());
            all[e + 4..e + 8].copy_from_slice(&(i as i32).to_le_bytes());
        }
        block
    }

    /// A .param file of 4-byte rows (`image`'s table without what the game adds around it).
    pub(crate) fn file(rows: &[(u64, [u8; 4])]) -> Vec<u8> {
        let block = image(rows);
        let all = unsafe { std::slice::from_raw_parts(block.as_ptr() as *const u8, block.len() * 16) };
        let size = u32::from_le_bytes(all[0..4].try_into().unwrap()) as usize;
        all[16..16 + size].to_vec()
    }

    /// The game's read of a row: the lookup after the table, then the row's data offset.
    pub(crate) fn game_row(table: usize, id: u32, row_size: usize) -> Option<Vec<u8>> {
        let size = memory::read::<u32>(table - 0x10)? as usize;
        let count = memory::read::<u32>(table - 0x0C)? as usize;
        let lookup = table + size.next_multiple_of(16);
        let (mut lo, mut hi) = (0i64, count as i64 - 1);
        while lo <= hi {
            let mid = (lo + hi) / 2;
            let e = lookup + mid as usize * 8;
            let at = memory::read::<u32>(e)?;
            if at == id {
                let i = memory::read::<i32>(e + 4)? as usize;
                let off = memory::read_u64(table + (i + 3) * 0x18)? as usize;
                return memory::read_bytes(table + off, row_size);
            }
            if at < id { lo = mid + 1 } else { hi = mid - 1 }
        }
        None
    }

    #[test]
    fn new_rows_go_before_the_end_rows() {
        assert_eq!(end_rows(&[1, 2, 999_999_998, 999_999_999]), 2);
        assert_eq!(end_rows(&[1, 999_999_001, 2_000_000_000]), 0);
        assert_eq!(insertion_point("SpEffectParam", &[10, 20, 999_999_999], &[30, 40]), Ok(2));
        assert_eq!(insertion_point("SpEffectParam", &[10, 20], &[30]), Ok(2));
        assert!(insertion_point("SpEffectParam", &[10, 20, 999_999_999], &[20]).unwrap_err().contains("above 20"));
        assert!(insertion_point("SpEffectParam", &[10, 20, 999_999_999], &[999_999_999]).unwrap_err().contains("below 999999999"));
        assert!(insertion_point("SpEffectParam", &[10, 20], &[40, 30]).unwrap_err().contains("ascending"));
        assert!(insertion_point("EquipParamGoods", &[10, 999_999_999], &[1 << 28]).unwrap_err().contains("28 bits"));
        assert_eq!(insertion_point("SpEffectParam", &[10, 999_999_999], &[1 << 28]), Ok(1));
    }

    #[test]
    fn a_rebuilt_table_reads_as_the_game_reads_it() {
        let rows = [(10u64, [1, 2, 3, 4]), (20, [5, 6, 7, 8]), (999_999_998, [9, 9, 9, 8]), (999_999_999, [9, 9, 9, 9])];
        let block = image(&rows);
        let table = block.as_ptr() as usize + 16;
        let old = Image::read(table).unwrap();
        let at = insertion_point("SpEffectParam", &old.ids(), &[30, 40]).unwrap();
        assert_eq!(at, 2);
        let built = build("SpEffectParam", &old, at, &[(30, vec![13, 14, 15, 16]), (40, vec![17, 18, 19, 20])], 4).unwrap();
        let t = built.table();
        // Every row, old and new, where the game's read finds it.
        for (id, data) in rows.iter().map(|(id, d)| (*id, d.to_vec())).chain([(30, vec![13, 14, 15, 16]), (40, vec![17, 18, 19, 20])]) {
            assert_eq!(game_row(t, id as u32, 4), Some(data), "row {id}");
        }
        let new = Image::read(t).unwrap();
        assert_eq!(new.ids(), [10, 20, 30, 40, 999_999_998, 999_999_999]);
        // Longer by two index entries, two rows and the new rows' empty name.
        assert_eq!(new.bytes.len(), old.bytes.len() + 2 * (INDEX_ENTRY + 4) + 8);
        assert_eq!(&new.prefix[8..16], b"prefix!!");
        // One array of rows in index order, from the header's data start.
        let start = new.u64_at(0x30) as usize;
        assert_eq!(start, HEADER + 6 * INDEX_ENTRY);
        assert!((0..6).all(|i| new.data_of(i) == start + i * 4));
        // The type name and the old rows' names where the header and index say.
        let type_name = new.u64_at(0x10) as usize;
        assert_eq!(&new.bytes[type_name..type_name + 7], b"TYPE_ST");
        let name_of = |i: usize| new.bytes[new.u64_at(HEADER + i * INDEX_ENTRY + 16) as usize];
        assert_eq!([name_of(0), name_of(1), name_of(2), name_of(3), name_of(4), name_of(5)], [b'a', b'b', 0, 0, b'c', b'd']);
        assert_eq!(u32::from_le_bytes(new.bytes[0..4].try_into().unwrap()) as usize, type_name);
    }

    #[test]
    fn rows_that_dont_fit_are_refused() {
        let block = image(&[(10, [1, 2, 3, 4]), (20, [5, 6, 7, 8])]);
        let old = Image::read(block.as_ptr() as usize + 16).unwrap();
        assert!(build("T", &old, 2, &[(30, vec![1, 2, 3])], 4).err().unwrap().contains("4 bytes"));
        // A table whose rows aren't one array after the index (as a wrong row size makes it).
        assert!(build("T", &old, 2, &[(30, vec![0; 8])], 8).err().unwrap().contains("laid out"));
    }

    /// The game's register and unregister, each found once in Elden Ring 2.7.0.0's code (KINDLE_ER_EXE).
    #[test]
    fn the_games_param_registration_is_found() {
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
        assert_eq!(find(REGISTER), [0xD4EBB0]);
        assert_eq!(find(UNREGISTER), [0xD4ED10]);
    }

    /// A .param file as the game holds it in memory: 16 bytes ({u32 size, u32 lookup count}), the file
    /// (the table, at +16), the {u32 id, i32 row index} lookup sorted by id.
    pub(crate) fn in_memory(file: &[u8]) -> Vec<u128> {
        let n = u16::from_le_bytes([file[0x0A], file[0x0B]]) as usize;
        let size = file.len();
        let lookup_at = 16 + size.next_multiple_of(16);
        let mut block = vec![0u128; (lookup_at + n * 8).div_ceil(16)];
        let all = unsafe { std::slice::from_raw_parts_mut(block.as_mut_ptr() as *mut u8, block.len() * 16) };
        all[0..4].copy_from_slice(&(size as u32).to_le_bytes());
        all[4..8].copy_from_slice(&(n as u32).to_le_bytes());
        all[16..16 + size].copy_from_slice(file);
        let id_of = |i: usize| u32::from_le_bytes(file[0x40 + i * 0x18..0x44 + i * 0x18].try_into().unwrap());
        let mut sorted: Vec<(u32, i32)> = (0..n).map(|i| (id_of(i), i as i32)).collect();
        sorted.sort_by_key(|&(id, _)| id);
        for (k, (id, i)) in sorted.iter().enumerate() {
            all[lookup_at + k * 8..lookup_at + k * 8 + 4].copy_from_slice(&id.to_le_bytes());
            all[lookup_at + k * 8 + 4..lookup_at + k * 8 + 8].copy_from_slice(&i.to_le_bytes());
        }
        block
    }

    /// Every table of Elden Ring's regulation (the .param files in the folder KINDLE_ER_PARAMS names,
    /// with the lookup the game makes): a new row in each, read back as the game reads (the test passes
    /// without it: no game files come with Kindle).
    #[test]
    fn elden_rings_tables_take_new_rows() {
        let Some(dir) = std::env::var_os("KINDLE_ER_PARAMS") else {
            eprintln!("KINDLE_ER_PARAMS isn't set: Elden Ring's params not checked");
            return;
        };
        let mut checked = 0;
        let mut before_end = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let file = std::fs::read(&path).unwrap();
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let n = u16::from_le_bytes([file[0x0A], file[0x0B]]) as usize;
            if n < 2 {
                continue;
            }
            // Rows 0 and 1's data offsets, one row apart.
            let row_size = (u64::from_le_bytes(file[0x60..0x68].try_into().unwrap()) - u64::from_le_bytes(file[0x48..0x50].try_into().unwrap())) as usize;
            let block = in_memory(&file);
            let table = block.as_ptr() as usize + 16;
            let old = Image::read(table).unwrap();
            let ids = old.ids();
            let at = ids.len() - end_rows(&ids);
            let top = ids[..at].iter().max().copied().unwrap_or(0);
            let id = top + 1;
            if at < ids.len() && id >= *ids[at..].iter().min().unwrap() {
                continue;
            }
            assert_eq!(insertion_point(&name, &ids, &[id]), Ok(at), "{name}");
            let copy = game_row(table, ids[0] as u32, row_size).unwrap();
            let built = build(&name, &old, at, &[(id, copy.clone())], row_size).unwrap_or_else(|e| panic!("{name}: {e}"));
            let t = built.table();
            assert_eq!(game_row(t, id as u32, row_size), Some(copy), "{name}: the new row");
            // A sample of the old rows, the end rows among them, read the same (an id some rows share
            // reads whichever the search lands on).
            let shared = |id: u64| ids.iter().filter(|&&i| i == id).count() > 1;
            for i in (0..n).step_by((n / 50).max(1)).chain(at..n).filter(|&i| !shared(ids[i])) {
                let rid = ids[i] as u32;
                assert_eq!(game_row(t, rid, row_size), game_row(table, rid, row_size), "{name}: row {rid}");
            }
            let new = Image::read(t).unwrap();
            assert_eq!(new.rows(), n + 1, "{name}");
            let tn = |img: &Image| {
                let at = img.u64_at(0x10) as usize;
                img.bytes[at..at + 8].to_vec()
            };
            assert_eq!(tn(&new), tn(&old), "{name}: type name");
            checked += 1;
            if at < n {
                before_end += 1;
            }
        }
        assert!(checked > 150, "{checked} tables checked");
        assert!(before_end >= 12, "{before_end} tables with end rows");
    }
}
