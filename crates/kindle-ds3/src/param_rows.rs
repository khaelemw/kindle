//! New param rows (`params.add_row`): brand-new items, effects, ... with ids the mod chooses.
//!
//! A param table is one block: a 0x40-byte header, an index of {u64 id, u64 data offset, u64
//! name offset} per row (sorted by id), then the rows' data and names. The game's row lookup
//! (1.15.2 `+0xE2F200`) also needs what surrounds it in memory: the table's size as a u32 16
//! bytes before it, and right after it (at table + size rounded up to 16) a sorted array of
//! {u32 id, u32 row index} that it binary-searches. Kindle builds a longer copy with all three
//! (the old rows, then the new ones) and points the game at it. The game reaches a table
//! in several places, all of which must move together or lookups mix old and new:
//!
//! - `FD4ParamResCap+0x68` (and `+0x60`, the size), the regulation manager's copy;
//! - a shared lookup record that many game objects point into: a slot holding the table, and
//!   0x30 bytes before it a slot holding a sorted copy of the row ids the game searches to
//!   find a row's position, with the row count at slot-0x38, slot-0x28 and slot+0x20;
//! - a resource record holding {table, res} (table, then res at +0x18).
//!
//! Kindle finds the last two by scanning its own process for the table's address and checking
//! each hit's layout. Game objects created after a rebuild copy the rebuilt table's address,
//! so each later rebuild also scans for the table Kindle installed last and takes those over
//! (otherwise they keep a table without the newest rows). New ids must be above every
//! existing id: the rows keep their positions, so nothing that remembers a position goes
//! stale.
//!
//! Added rows stay for the rest of the session, even if the mod unloads: an item the player
//! holds must keep its row. Adding the same id again (a mod reloading) returns that row.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::memory;

const HEADER: usize = 0x40;
const INDEX_ENTRY: usize = 0x18;

/// One table's state once rows were added.
struct Extended {
    /// Where the game holds the table's address (all rewritten together).
    table_slots: Vec<usize>,
    /// The shared lookup records, by their table slot (also in table_slots): the id cache's
    /// address 0x30 before it, the row count at -0x38, -0x28 and +0x20.
    records: Vec<usize>,
    /// The FD4ParamResCap (its +0x60 is the size).
    res: usize,
    original_table: usize,
    /// The table installed last (the original until a row is added), its size, id cache and row
    /// count: a rebuild starts from it, so edits made to its rows survive.
    current_table: usize,
    current_size: u64,
    current_ids: usize,
    current_count: u64,
    /// Rows added, in order: (owner mod, id, data).
    added: Vec<(String, u64, Vec<u8>)>,
}

static STATE: Mutex<Option<HashMap<String, Extended>>> = Mutex::new(None);

/// The regulation manager's (res, table) for a table name (camel case, e.g. EquipParamGoods).
fn find_table(name: &str) -> Result<(usize, usize), String> {
    let caps = crate::ds3::param_caps().ok_or("params are not loaded yet")?;
    for cap in caps {
        if cap.name.eq_ignore_ascii_case(name) {
            return Ok((cap.res, cap.table));
        }
    }
    Err(format!("no param table named {name:?}"))
}

/// Finds where the game holds a table (see the module notes) and records the originals.
fn discover(name: &str) -> Result<Extended, String> {
    let (res, table) = find_table(name)?;
    let count = memory::read::<u16>(table + 0x0A).ok_or("table header unreadable")? as u64;
    let first_id = memory::read_u64(table + HEADER).ok_or("table index unreadable")?;
    let last_id = memory::read_u64(table + HEADER + (count as usize - 1) * INDEX_ENTRY).ok_or("table index unreadable")?;
    let mut table_slots = vec![res + 0x68];
    let mut records = Vec::new();
    let mut id_cache = None;
    for h in memory::holders_of(table as u64) {
        if h == res + 0x68 {
            continue;
        }
        if let Some(ids) = lookup_record_ids(h, count, first_id, last_id) {
            id_cache.get_or_insert(ids);
            records.push(h);
            table_slots.push(h);
        } else if is_resource_record(h, res) {
            table_slots.push(h);
        }
    }
    let size = memory::read_u64(res + 0x60).ok_or("table size unreadable")?;
    // Early in startup the game hasn't built its shared lookup records yet (a mod adding rows as
    // it loaded met none for SpEffectParam at 10 s, every table at 13 s, 2026-10-26): it builds
    // them later from the table it then holds, Kindle's, and the next rebuild takes them over
    // (adopt_new_holders). Until then the ids come from the {id, row index} lookup after the
    // table (the same 8-byte entries, the id in the low half).
    let ids = id_cache.unwrap_or_else(|| {
        crate::info!("params: {name}: the game's row lookup isn't built yet (it will be from the rebuilt table)");
        row_lookup(table, size)
    });
    Ok(Extended {
        table_slots,
        records,
        res,
        original_table: table,
        current_table: table,
        current_size: size,
        current_ids: ids,
        current_count: count,
        added: Vec::new(),
    })
}

/// If `h` is the table slot of a shared lookup record for a table of `count` rows from
/// `first_id` to `last_id`: the address of its id cache.
fn lookup_record_ids(h: usize, count: u64, first_id: u64, last_id: u64) -> Option<usize> {
    let ids = memory::read_u64(h - 0x30).filter(|&p| p != 0)? as usize;
    let counts_ok = [h - 0x38, h - 0x28, h + 0x20].iter().all(|&c| memory::read_u64(c) == Some(count));
    (counts_ok
        && memory::read::<u32>(ids).map(|v| v as u64) == Some(first_id)
        && memory::read::<u32>(ids + (count as usize - 1) * 8).map(|v| v as u64) == Some(last_id))
    .then_some(ids)
}

/// Where the {u32 id, u32 row index} lookup after a table of `size` bytes starts.
fn row_lookup(table: usize, size: u64) -> usize {
    table + (size as usize).next_multiple_of(16)
}

/// If `h` is the table slot of a resource record: {table, _, _, res}.
fn is_resource_record(h: usize, res: usize) -> bool {
    memory::read_u64(h + 0x18) == Some(res as u64)
}

/// Takes over places that picked up the installed table's address since Kindle wrote it (game
/// objects created in between), so the next install moves them too.
fn adopt_new_holders(e: &mut Extended) {
    if e.current_table == e.original_table {
        return; // nothing installed yet: discover() just found them all
    }
    let table = e.current_table;
    let count = memory::read::<u16>(table + 0x0A).unwrap_or(0) as u64;
    if count == 0 {
        return;
    }
    let first_id = memory::read_u64(table + HEADER).unwrap_or(0);
    let last_id = memory::read_u64(table + HEADER + (count as usize - 1) * INDEX_ENTRY).unwrap_or(0);
    for h in memory::holders_of(table as u64) {
        if e.table_slots.contains(&h) {
            continue;
        }
        if lookup_record_ids(h, count, first_id, last_id).is_some() {
            crate::info!("params: taking over a new row lookup at {h:#x}");
            e.records.push(h);
            e.table_slots.push(h);
        } else if is_resource_record(h, e.res) {
            crate::info!("params: taking over a new holder at {h:#x}");
            e.table_slots.push(h);
        } else {
            crate::debug!("params: unrecognised holder of a rebuilt table at {h:#x}");
        }
    }
}

/// A copy of the table installed last (the game's own until a row is added) with `new` rows
/// appended, so edits made to its rows are kept; returns (table, size, ids).
fn build(e: &Extended, new: &[(String, u64, Vec<u8>)]) -> Result<(usize, u64, usize), String> {
    let n = e.current_count as usize;
    let m = new.len();
    let old_size = e.current_size as usize;
    let old = memory::read_bytes(e.current_table, old_size).ok_or("table unreadable")?;
    let old_index_end = HEADER + n * INDEX_ENTRY;
    let delta = m * INDEX_ENTRY;
    let row_size = new.first().map(|(_, _, d)| d.len()).unwrap_or(0);
    // The rows' data is one array the game also walks as such (from the first row's data, count
    // rows of the row size: BonfireWarpParam's lit flags, +0x770440), so new rows go right after
    // the old ones, before the names.
    let data_end = (0..n)
        .map(|i| u64::from_le_bytes(old[HEADER + i * INDEX_ENTRY + 8..HEADER + i * INDEX_ENTRY + 16].try_into().unwrap()) as usize + row_size)
        .max()
        .unwrap_or(old_index_end)
        .clamp(old_index_end, old_size);
    let grow = m * row_size;
    let new_size = old_size + delta + grow + 8;
    // Around the table: its size 16 bytes before, the {id, row index} lookup after it.
    let lookup_at = 16 + new_size.next_multiple_of(16);
    let total = lookup_at + (n + m) * 8;

    // 16-aligned; leaked: the game may still be reading the old copy this frame, and tables
    // live as long as the game.
    let mut buf = vec![0u128; total.div_ceil(16)];
    let all = unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, total) };
    all[0..4].copy_from_slice(&(new_size as u32).to_le_bytes());
    let lookup_old = memory::read_bytes(row_lookup(e.current_table, old_size as u64), n * 8).ok_or("row lookup unreadable")?;
    all[lookup_at..lookup_at + n * 8].copy_from_slice(&lookup_old);
    for (k, (_, id, _)) in new.iter().enumerate() {
        let at = lookup_at + (n + k) * 8;
        all[at..at + 4].copy_from_slice(&(*id as u32).to_le_bytes());
        all[at + 4..at + 8].copy_from_slice(&((n + k) as u32).to_le_bytes());
    }
    let bytes = &mut all[16..16 + new_size];
    // Offsets into the rows' data move past the longer index; into the names, past the new rows too.
    let shift = |v: u64| match v as usize {
        at if at >= data_end && at <= old_size => v + (delta + grow) as u64,
        at if at >= old_index_end && at < data_end => v + delta as u64,
        _ => v,
    };

    // Header, with its offsets past the index moved.
    bytes[..HEADER].copy_from_slice(&old[..HEADER]);
    let strings = u32::from_le_bytes(old[0..4].try_into().unwrap()) as u64;
    bytes[0..4].copy_from_slice(&(shift(strings) as u32).to_le_bytes());
    for at in [0x10usize, 0x30] {
        let v = u64::from_le_bytes(old[at..at + 8].try_into().unwrap());
        bytes[at..at + 8].copy_from_slice(&shift(v).to_le_bytes());
    }
    bytes[0x0A..0x0C].copy_from_slice(&((n + m) as u16).to_le_bytes());
    // Old index entries.
    for i in 0..n {
        let at = HEADER + i * INDEX_ENTRY;
        let id = u64::from_le_bytes(old[at..at + 8].try_into().unwrap());
        let data = u64::from_le_bytes(old[at + 8..at + 16].try_into().unwrap());
        let name = u64::from_le_bytes(old[at + 16..at + 24].try_into().unwrap());
        bytes[at..at + 8].copy_from_slice(&id.to_le_bytes());
        bytes[at + 8..at + 16].copy_from_slice(&shift(data).to_le_bytes());
        bytes[at + 16..at + 24].copy_from_slice(&shift(name).to_le_bytes());
    }
    // The old rows' data, moved down past the index; the names after the new rows' data.
    bytes[old_index_end + delta..data_end + delta].copy_from_slice(&old[old_index_end..data_end]);
    bytes[data_end + delta + grow..old_size + delta + grow].copy_from_slice(&old[data_end..old_size]);
    // New rows: index entries, then data after the old rows'; their names point at a NUL.
    let empty_name = (new_size - 8) as u64;
    for (k, (_, id, data)) in new.iter().enumerate() {
        let at = old_index_end + k * INDEX_ENTRY;
        let data_off = data_end + delta + k * row_size;
        bytes[at..at + 8].copy_from_slice(&id.to_le_bytes());
        bytes[at + 8..at + 16].copy_from_slice(&(data_off as u64).to_le_bytes());
        bytes[at + 16..at + 24].copy_from_slice(&empty_name.to_le_bytes());
        bytes[data_off..data_off + data.len()].copy_from_slice(data);
    }
    let table = Box::leak(buf.into_boxed_slice()).as_ptr() as usize + 16;

    // The id cache: the old entries, then the new ids (same 8-byte entries, id in the low half).
    let mut ids = memory::read_bytes(e.current_ids, n * 8).ok_or("row lookup unreadable")?;
    for (_, id, _) in new {
        ids.extend((*id as u32 as u64).to_le_bytes());
    }
    let ids = Box::leak(ids.into_boxed_slice()).as_ptr() as usize;
    Ok((table, new_size as u64, ids))
}

/// Points the game at a table, its size, id cache and count.
fn install(e: &mut Extended, table: usize, size: u64, ids: usize, count: u64) {
    // Places taken over from game objects can be freed later: write only to those that still
    // hold the table Kindle installed last, and forget the rest.
    let current = e.current_table as u64;
    e.table_slots.retain(|&s| memory::read_u64(s) == Some(current));
    let slots = &e.table_slots;
    e.records.retain(|r| slots.contains(r));
    unsafe {
        std::ptr::write_volatile((e.res + 0x60) as *mut u64, size);
        for &r in &e.records {
            std::ptr::write_volatile((r - 0x30) as *mut u64, ids as u64);
            for c in [r - 0x38, r - 0x28, r + 0x20] {
                std::ptr::write_volatile(c as *mut u64, count);
            }
        }
        for &s in &e.table_slots {
            std::ptr::write_volatile(s as *mut u64, table as u64);
        }
    }
}

fn apply(e: &mut Extended, new: &[(String, u64, Vec<u8>)]) -> Result<(), String> {
    let (table, size, ids) = build(e, new)?;
    let count = e.current_count + new.len() as u64;
    adopt_new_holders(e);
    install(e, table, size, ids, count);
    e.current_table = table;
    e.current_size = size;
    e.current_ids = ids;
    e.current_count = count;
    // Kindle's own view of the params is rebuilt from the new tables.
    crate::params::invalidate();
    Ok(())
}

/// Adds rows to a table in one rebuild (each rebuild takes a second and moves every place the
/// game holds the table: the fewer, the safer): `rows` are (id, copy_from), ids ascending and
/// above every existing id; rows already added this session (a mod reloading) are kept.
pub fn add_rows(owner: &str, table_name: &str, rows: &[(u64, u64)]) -> Result<(), String> {
    let canonical = crate::params::registry()?.table(table_name)?.name.clone();
    let have = |id: u64| {
        STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(&canonical)).is_some_and(|e| e.added.iter().any(|(_, i, _)| *i == id))
    };
    let rows: Vec<(u64, u64)> = rows.iter().copied().filter(|&(id, _)| !have(id)).collect();
    if rows.is_empty() {
        return Ok(());
    }
    if let Some(w) = rows.windows(2).find(|w| w[1].0 <= w[0].0) {
        return Err(format!("new {canonical} rows need ascending ids ({} comes after {})", w[1].0, w[0].0));
    }
    let (templates, max) = {
        let registry = crate::params::registry()?;
        let t = registry.table(table_name)?;
        let mut templates = Vec::with_capacity(rows.len());
        for &(_, copy_from) in &rows {
            templates.push(t.row_data(copy_from)?);
        }
        (templates, t.row_ids.iter().copied().max().unwrap_or(0))
    };

    let started = std::time::Instant::now();
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let map = state.get_or_insert_with(HashMap::new);
    if !map.contains_key(&canonical) {
        map.insert(canonical.clone(), discover(&canonical)?);
    }
    let e = map.get_mut(&canonical).unwrap();
    // Kindle's view already includes added rows, so `max` covers them too.
    if rows[0].0 <= max {
        let by = e.added.iter().find(|(_, i, _)| *i == max).map(|(o, _, _)| format!(", added by {o}")).unwrap_or_default();
        return Err(format!(
            "new {canonical} rows need ids above {max} (the highest row so far{by}; rows are added in the order mods load, so pick ids well apart from other mods')"
        ));
    }
    let new: Vec<(String, u64, Vec<u8>)> = rows.iter().zip(templates).map(|(&(id, _), data)| (owner.to_string(), id, data)).collect();
    apply(e, &new)?;
    e.added.extend(new);
    crate::info!("params: {canonical}: {} row(s) added, {} new in all ({} ms)", rows.len(), e.added.len(), started.elapsed().as_millis());
    drop(state);
    for &(id, copy_from) in &rows {
        copy_item_text(&canonical, copy_from, id);
    }
    Ok(())
}

/// A new item row gets its template's name, effect line and description (until the mod sets
/// its own), so the game never shows its "?GoodsInfo?" placeholders. Kept for the session,
/// like the row.
fn copy_item_text(table: &str, from: u64, to: u64) {
    use crate::text::Kind;
    let kind = match table {
        "EquipParamGoods" => Kind::Goods,
        "EquipParamWeapon" => Kind::Weapon,
        "EquipParamProtector" => Kind::Armor,
        "EquipParamAccessory" => Kind::Ring,
        _ => return,
    };
    // Weapons share text across upgrade levels: the +0 id holds it.
    let base = |id: u64| (if kind == Kind::Weapon { id - id % 100 } else { id }) as u32;
    for field in ["name", "info", "description"] {
        let Ok(cats) = crate::text_override::item_categories(kind, field) else { continue };
        let Some(text) = cats.iter().find_map(|&c| crate::text_override::get(c, base(from))) else { continue };
        if let Err(e) = crate::text_override::set_or_add(&cats, base(to), &text, "kindle") {
            crate::warn!("params: {table} row {to}: copying the {field} text: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small table laid out like the game's: header, index, row data, names.
    fn fake_table(rows: &[(u64, [u8; 4])]) -> Vec<u8> {
        let n = rows.len();
        let index_end = HEADER + n * INDEX_ENTRY;
        let data_end = index_end + n * 4;
        let mut t = vec![0u8; data_end + n * 3];
        t[0..4].copy_from_slice(&(data_end as u32).to_le_bytes()); // strings offset
        t[0x0A..0x0C].copy_from_slice(&(n as u16).to_le_bytes());
        t[0x10..0x18].copy_from_slice(&(data_end as u64).to_le_bytes()); // type name offset
        t[0x30..0x38].copy_from_slice(&(index_end as u64).to_le_bytes()); // data start
        for (i, (id, data)) in rows.iter().enumerate() {
            let at = HEADER + i * INDEX_ENTRY;
            t[at..at + 8].copy_from_slice(&id.to_le_bytes());
            t[at + 8..at + 16].copy_from_slice(&((index_end + i * 4) as u64).to_le_bytes());
            t[at + 16..at + 24].copy_from_slice(&((data_end + i * 3) as u64).to_le_bytes());
            t[index_end + i * 4..index_end + i * 4 + 4].copy_from_slice(data);
            t[data_end + i * 3] = b'a' + i as u8;
        }
        t
    }

    fn row(table: &[u8], i: usize) -> (u64, Vec<u8>, u8) {
        let at = HEADER + i * INDEX_ENTRY;
        let id = u64::from_le_bytes(table[at..at + 8].try_into().unwrap());
        let off = u64::from_le_bytes(table[at + 8..at + 16].try_into().unwrap()) as usize;
        let name = u64::from_le_bytes(table[at + 16..at + 24].try_into().unwrap()) as usize;
        (id, table[off..off + 4].to_vec(), table[name])
    }

    #[test]
    fn rebuilt_tables_keep_old_rows_and_append_new_ones() {
        let rows = [(10u64, [1, 2, 3, 4]), (20, [5, 6, 7, 8]), (30, [9, 10, 11, 12])];
        let mut old = fake_table(&rows);
        let size = old.len();
        old.resize(size.next_multiple_of(16), 0);
        for (i, (id, _)) in rows.iter().enumerate() {
            old.extend((*id as u32).to_le_bytes());
            old.extend((i as u32).to_le_bytes());
        }
        let ids: Vec<u8> = rows.iter().flat_map(|(id, _)| (*id).to_le_bytes()).collect();
        let e = Extended {
            table_slots: vec![],
            records: vec![],
            res: 0,
            original_table: old.as_ptr() as usize,
            current_table: old.as_ptr() as usize,
            current_size: size as u64,
            current_ids: ids.as_ptr() as usize,
            current_count: 3,
            added: vec![],
        };
        let (table, size, new_ids) = build(&e, &[("m".into(), 40, vec![13, 14, 15, 16])]).unwrap();
        let t = unsafe { std::slice::from_raw_parts(table as *const u8, size as usize) };
        assert_eq!(u16::from_le_bytes([t[0x0A], t[0x0B]]), 4);
        for (i, (id, data)) in rows.iter().enumerate() {
            assert_eq!(row(t, i), (*id, data.to_vec(), b'a' + i as u8), "row {i}");
        }
        assert_eq!(row(t, 3).0, 40);
        assert_eq!(row(t, 3).1, vec![13, 14, 15, 16]);
        // The game's walk over the rows: from the first row's data (the header's data start too),
        // one row size apart, the new row included.
        let first = u64::from_le_bytes(t[HEADER + 8..HEADER + 16].try_into().unwrap()) as usize;
        assert_eq!(u64::from_le_bytes(t[0x30..0x38].try_into().unwrap()) as usize, first);
        let walked: Vec<&[u8]> = (0..4).map(|i| &t[first + 4 * i..first + 4 * i + 4]).collect();
        assert_eq!(walked, [&[1u8, 2, 3, 4][..], &[5, 6, 7, 8], &[9, 10, 11, 12], &[13, 14, 15, 16]]);
        // header offsets still point at the same things
        let tname = u64::from_le_bytes(t[0x10..0x18].try_into().unwrap()) as usize;
        assert_eq!(t[tname], b'a');
        let ids_new = unsafe { std::slice::from_raw_parts(new_ids as *const u64, 4) };
        assert_eq!(ids_new, &[10, 20, 30, 40]);
        // The game's lookup: size 16 bytes before, {id, index} pairs after.
        let size_before = unsafe { std::ptr::read((table - 16) as *const u32) } as usize;
        assert_eq!(size_before, size as usize);
        let lookup = unsafe { std::slice::from_raw_parts(row_lookup(table, size_before as u64) as *const u32, 8) };
        assert_eq!(lookup, &[10, 0, 20, 1, 30, 2, 40, 3]);
    }
}
