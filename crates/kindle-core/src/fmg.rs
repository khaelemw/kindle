//! Reader for FMG text tables (FMG version 2) as Dark Souls III and Elden Ring both hold them in
//! memory.
//!
//! Layout (little endian):
//!
//! | offset | type | |
//! |---|---|---|
//! | 0x00 | u8 0, u8 big-endian = 0, u8 version = 2, u8 0 | |
//! | 0x04 | u32 | total size |
//! | 0x0C | i32 | group count |
//! | 0x10 | i32 | string count |
//! | 0x18 | u64 | string offset table: an absolute pointer once the game has loaded it |
//! | 0x28 | group[] | 16 bytes each: i32 first string index, i32 first id, i32 last id, i32 pad |
//!
//! Each string offset table entry is a u64 offset *from the start of the FMG* (0 = no
//! string), pointing at a null-terminated UTF-16 string. Verified against DS3 1.15.2 memory,
//! and Elden Ring 2.7.0.0's lookup of one entry (+0x26704A0) reads the same fields: it searches
//! the groups by id and adds the entry's offset to the table's start.
//! Tables Kindle extends (new text entries) keep the game's strings where they are but get a
//! new header and offset table elsewhere, so the reader follows the pointers instead of
//! assuming one contiguous block (offsets can then be "negative": they wrap).

const HEADER_GROUPS: usize = 0x28;
/// Longest string read, in UTF-16 units.
const MAX_CHARS: usize = 4096;

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    b.get(at..at + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

/// A null-terminated UTF-16 string at `addr`, read in chunks.
fn wstr(addr: usize, mem: &dyn Fn(usize, usize) -> Option<Vec<u8>>) -> Option<String> {
    let mut units = Vec::new();
    let mut at = addr;
    while units.len() < MAX_CHARS {
        // Small chunks, so a string near the end of readable memory still reads.
        let chunk = mem(at, 64).or_else(|| mem(at, 2))?;
        for pair in chunk.as_chunks::<2>().0 {
            let u = u16::from_le_bytes(*pair);
            if u == 0 {
                return Some(String::from_utf16_lossy(&units));
            }
            units.push(u);
        }
        at += chunk.len();
    }
    Some(String::from_utf16_lossy(&units))
}

/// Reads the FMG at `base` through `mem(address, len)`. Returns (id, text) for every
/// non-null string.
pub fn read(base: usize, mem: &dyn Fn(usize, usize) -> Option<Vec<u8>>) -> Result<Vec<(u32, String)>, String> {
    let head = mem(base, HEADER_GROUPS).ok_or("unreadable header")?;
    if head.get(0..4) != Some(&[0, 0, 2, 0]) {
        return Err("not an FMG v2 table".into());
    }
    let groups = i32_at(&head, 0x0C).ok_or("truncated header")?;
    let strings = i32_at(&head, 0x10).ok_or("truncated header")?;
    if !(0..=1 << 20).contains(&groups) || !(0..=1 << 22).contains(&strings) {
        return Err("bad group/string count".into());
    }
    let table = u64_at(&head, 0x18).ok_or("truncated header")? as usize;
    let group_table = mem(base + HEADER_GROUPS, groups as usize * 16).ok_or("unreadable group table")?;

    let mut out = Vec::with_capacity(strings as usize);
    for g in 0..groups as usize {
        let at = g * 16;
        let (Some(index), Some(first), Some(last)) = (i32_at(&group_table, at), i32_at(&group_table, at + 4), i32_at(&group_table, at + 8)) else {
            return Err("truncated group table".into());
        };
        if first < 0 || last < first || index < 0 || index as i64 + (last - first) as i64 >= strings as i64 {
            continue;
        }
        let count = (last - first + 1) as usize;
        let offsets = mem(table + index as usize * 8, count * 8).ok_or("unreadable string offsets")?;
        for k in 0..count {
            let off = u64_at(&offsets, k * 8).unwrap_or(0);
            if off == 0 {
                continue;
            }
            if let Some(s) = wstr(base.wrapping_add(off as usize), mem) {
                out.push((first as u32 + k as u32, s));
            }
        }
    }
    Ok(out)
}

/// Where entry `id`'s string offset is (the u64 the game adds to the FMG's start), found as the
/// game finds it: a binary search of the groups by id, ids unsigned. None if the FMG has no entry
/// for `id`.
pub fn slot(base: usize, id: u32, mem: &dyn Fn(usize, usize) -> Option<Vec<u8>>) -> Option<usize> {
    let head = mem(base, HEADER_GROUPS)?;
    if head.get(0..4) != Some(&[0, 0, 2, 0]) {
        return None;
    }
    let groups = i32_at(&head, 0x0C)?;
    if !(1..=1 << 20).contains(&groups) {
        return None;
    }
    let table = u64_at(&head, 0x18)? as usize;
    let (mut lo, mut hi) = (0usize, groups as usize);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let g = mem(base + HEADER_GROUPS + mid * 16, 12)?;
        let (index, first, last) = (i32_at(&g, 0)? as u32, i32_at(&g, 4)? as u32, i32_at(&g, 8)? as u32);
        if last < id {
            lo = mid + 1;
        } else if id < first {
            hi = mid;
        } else {
            return Some(table + (index as usize + (id - first) as usize) * 8);
        }
    }
    None
}

/// Entry `id`'s text: None if the FMG has no entry for it, or the entry no string.
pub fn text(base: usize, id: u32, mem: &dyn Fn(usize, usize) -> Option<Vec<u8>>) -> Option<String> {
    let off = u64_at(&mem(slot(base, id, mem)?, 8)?, 0)?;
    if off == 0 {
        return None;
    }
    wstr(base.wrapping_add(off as usize), mem)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an FMG v2 blob as the game holds it at `base`, from (id, text) pairs, one group
    /// per run of consecutive ids.
    fn build(entries: &[(u32, Option<&str>)], base: usize) -> Vec<u8> {
        let mut groups: Vec<(usize, u32, u32)> = Vec::new();
        for (i, (id, _)) in entries.iter().enumerate() {
            match groups.last_mut() {
                Some((_, _, last)) if *last + 1 == *id => *last = *id,
                _ => groups.push((i, *id, *id)),
            }
        }
        let table_off = HEADER_GROUPS + groups.len() * 16;
        let strings_off = table_off + entries.len() * 8;
        let mut strings = Vec::new();
        let mut offsets = Vec::new();
        for (_, text) in entries {
            match text {
                Some(t) => {
                    offsets.push((strings_off + strings.len()) as u64);
                    for u in t.encode_utf16() {
                        strings.extend_from_slice(&u.to_le_bytes());
                    }
                    strings.extend_from_slice(&[0, 0]);
                }
                None => offsets.push(0),
            }
        }
        let size = strings_off + strings.len();
        let mut b = vec![0u8; HEADER_GROUPS];
        b[0..4].copy_from_slice(&[0, 0, 2, 0]);
        b[4..8].copy_from_slice(&(size as u32).to_le_bytes());
        b[0x0C..0x10].copy_from_slice(&(groups.len() as i32).to_le_bytes());
        b[0x10..0x14].copy_from_slice(&(entries.len() as i32).to_le_bytes());
        b[0x18..0x20].copy_from_slice(&((base + table_off) as u64).to_le_bytes());
        for (index, first, last) in &groups {
            b.extend_from_slice(&(*index as i32).to_le_bytes());
            b.extend_from_slice(&(*first as i32).to_le_bytes());
            b.extend_from_slice(&(*last as i32).to_le_bytes());
            b.extend_from_slice(&0i32.to_le_bytes());
        }
        for o in offsets {
            b.extend_from_slice(&o.to_le_bytes());
        }
        b.extend_from_slice(&strings);
        b
    }

    /// Memory made of blocks at given addresses.
    fn memory(blocks: Vec<(usize, Vec<u8>)>) -> impl Fn(usize, usize) -> Option<Vec<u8>> {
        move |addr, len| {
            blocks.iter().find_map(|(at, b)| {
                let start = addr.checked_sub(*at)?;
                b.get(start..start + len).map(|s| s.to_vec())
            })
        }
    }

    const ENTRIES: &[(u32, Option<&str>)] = &[(150, Some("Estus Flask")), (151, None), (152, Some("Ashen Estus Flask")), (500, Some("Ember"))];

    #[test]
    fn reads_the_games_layout() {
        let base = 0x7ff4_2a04_0c30usize;
        let mem = memory(vec![(base, build(ENTRIES, base))]);
        assert_eq!(read(base, &mem).unwrap(), vec![(150, "Estus Flask".into()), (152, "Ashen Estus Flask".into()), (500, "Ember".into())]);
    }

    #[test]
    fn finds_one_entry_as_the_game_does() {
        let base = 0x7ff4_2a04_0c30usize;
        let mem = memory(vec![(base, build(ENTRIES, base))]);
        let table = base + HEADER_GROUPS + 2 * 16;
        assert_eq!(slot(base, 150, &mem), Some(table));
        assert_eq!(slot(base, 152, &mem), Some(table + 2 * 8));
        assert_eq!(slot(base, 500, &mem), Some(table + 3 * 8));
        for none in [0, 149, 153, 499, 501, u32::MAX] {
            assert_eq!(slot(base, none, &mem), None, "{none}");
        }
        assert_eq!(text(base, 152, &mem).as_deref(), Some("Ashen Estus Flask"));
        assert_eq!(text(base, 500, &mem).as_deref(), Some("Ember"));
        // An entry with no string, and no entry at all.
        assert_eq!(text(base, 151, &mem), None);
        assert_eq!(text(base, 153, &mem), None);
        let garbage = memory(vec![(0x1000, [1, 2, 3, 4, 5, 6, 7, 8].repeat(8))]);
        assert_eq!(slot(0x1000, 150, &garbage), None);
    }

    #[test]
    fn reads_a_table_kindle_extended() {
        // The game's strings stay put; the header and offset table live elsewhere, below
        // them, so the offsets wrap.
        let old = 0x2000_0000usize;
        let blob = build(ENTRIES, old);
        let new = 0x1000_0000usize;
        let mut head = blob[..HEADER_GROUPS + 2 * 16].to_vec(); // two groups
        let table = 0x1800_0000usize;
        head[0x18..0x20].copy_from_slice(&(table as u64).to_le_bytes());
        let old_table = HEADER_GROUPS + 2 * 16;
        let offsets: Vec<u8> = (0..4)
            .flat_map(|i| {
                let off = u64_at(&blob, old_table + i * 8).unwrap();
                let rebased = if off == 0 { 0 } else { (old as u64 + off).wrapping_sub(new as u64) };
                rebased.to_le_bytes()
            })
            .collect();
        let mem = memory(vec![(old, blob), (new, head), (table, offsets)]);
        let got = read(new, &mem).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[2], (500, "Ember".into()));
    }

    #[test]
    fn rejects_garbage() {
        let mem = memory(vec![(0x1000, [1, 2, 3, 4, 5, 6, 7, 8].repeat(8))]);
        assert!(read(0x1000, &mem).is_err());
    }
}
