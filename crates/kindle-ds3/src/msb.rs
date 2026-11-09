//! Map files (MSB, Dark Souls III) as sections of raw entries, read and written back whole, so
//! entries can be added and changed (mods' map changes: an enemy of another type, map_edits.rs).
//!
//! Layout (Kindle's own reading of the game's files): "MSB " and a fixed 16-byte header, then
//! sections, each {i32 version, i32 count, i64 name offset, i64 offsets[count]}: count - 1
//! entries, the last offset the next section's (0 ends the file). The section's name (UTF-16,
//! NUL-ended) follows the offsets, then its entries; names and entries start 8-aligned.
//! Offsets inside an entry (its name, a part's entity and type data, ...) are relative to the
//! entry, so an entry's bytes (up to the next one, with their padding) can move as they are.
//! Verified: the game's map files are written back byte for byte.

const MAGIC: &[u8; 16] = b"MSB \x01\x00\x00\x00\x10\x00\x00\x00\x00\x00\x01\xff";

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub name: String,
    pub version: i32,
    /// Each entry's bytes, with any padding after it.
    pub entries: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Msb {
    pub sections: Vec<Section>,
}

fn i32_at(d: &[u8], at: usize) -> Result<i32, String> {
    Ok(i32::from_le_bytes(d.get(at..at + 4).ok_or("cut short")?.try_into().unwrap()))
}

fn i64_at(d: &[u8], at: usize) -> Result<i64, String> {
    Ok(i64::from_le_bytes(d.get(at..at + 8).ok_or("cut short")?.try_into().unwrap()))
}

fn wstr(d: &[u8], at: usize) -> Result<String, String> {
    let units: Vec<u16> = d.get(at..).ok_or("cut short")?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&u| u != 0).collect();
    Ok(String::from_utf16_lossy(&units))
}

fn pad8(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
}

impl Msb {
    pub fn parse(d: &[u8]) -> Result<Self, String> {
        if d.get(..16) != Some(&MAGIC[..]) {
            return Err("not a Dark Souls III map file".into());
        }
        let (mut at, mut sections) = (0x10usize, Vec::new());
        while at != 0 {
            let version = i32_at(d, at)?;
            let count = i32_at(d, at + 4)?;
            if !(1..=1_000_000).contains(&count) {
                return Err(format!("section at {at:#x}: bad count {count}"));
            }
            let name = wstr(d, i64_at(d, at + 8)? as usize)?;
            let offsets: Vec<usize> = (0..count as usize).map(|i| i64_at(d, at + 0x10 + 8 * i).map(|o| o as usize)).collect::<Result<_, _>>()?;
            let next = *offsets.last().unwrap();
            let end = if next == 0 { d.len() } else { next };
            let mut entries = Vec::new();
            for (i, &o) in offsets[..offsets.len() - 1].iter().enumerate() {
                let stop = if i + 2 < offsets.len() { offsets[i + 1] } else { end };
                entries.push(d.get(o..stop).ok_or_else(|| format!("{name}: entry {i} out of range"))?.to_vec());
            }
            sections.push(Section { name, version, entries });
            if next != 0 && next <= at {
                return Err("sections go backwards".into());
            }
            at = next;
        }
        Ok(Msb { sections })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for (s, section) in self.sections.iter().enumerate() {
            let head = out.len();
            let count = section.entries.len() + 1;
            out.extend_from_slice(&section.version.to_le_bytes());
            out.extend_from_slice(&(count as i32).to_le_bytes());
            out.extend_from_slice(&[0; 8]);
            let table = out.len();
            out.resize(table + 8 * count, 0);
            let name_at = out.len();
            out[head + 8..head + 16].copy_from_slice(&(name_at as i64).to_le_bytes());
            for u in section.name.encode_utf16().chain([0]) {
                out.extend_from_slice(&u.to_le_bytes());
            }
            pad8(&mut out);
            for (i, e) in section.entries.iter().enumerate() {
                let at = out.len();
                out[table + 8 * i..table + 8 * i + 8].copy_from_slice(&(at as i64).to_le_bytes());
                out.extend_from_slice(e);
                pad8(&mut out);
            }
            let next = if s + 1 < self.sections.len() { out.len() } else { 0 };
            out[table + 8 * (count - 1)..table + 8 * count].copy_from_slice(&(next as i64).to_le_bytes());
        }
        out
    }

    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    pub fn section_mut(&mut self, name: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.name == name)
    }

    /// The models: (name, type; 2 for characters).
    pub fn models(&self) -> Vec<(String, i32)> {
        let Some(s) = self.section(MODELS) else { return Vec::new() };
        s.entries.iter().map(|e| (entry_name(e).unwrap_or_default(), rd_i32(e, MODEL_TYPE).unwrap_or(-1))).collect()
    }

    /// Adds a model (an entry from another map file, `entry`) after the last of its type, as
    /// the next of that type, unless one of its name is there; its index. Parts after it are
    /// renumbered.
    pub fn add_model(&mut self, entry: &[u8]) -> Result<usize, String> {
        let name = entry_name(entry).ok_or("a model without a name")?;
        let kind = rd_i32(entry, MODEL_TYPE).ok_or("a model cut short")?;
        let models = self.models();
        if let Some(i) = models.iter().position(|(n, _)| *n == name) {
            return Ok(i);
        }
        let same = models.iter().filter(|(_, t)| *t == kind).count();
        let at = models.iter().rposition(|(_, t)| *t == kind).map_or(models.len(), |i| i + 1);
        let mut e = entry.to_vec();
        wr_i32(&mut e, MODEL_INDEX, same as i32);
        wr_i32(&mut e, MODEL_COUNT, 0);
        self.section_mut(MODELS).ok_or("no model section")?.entries.insert(at, e);
        for p in &mut self.section_mut(PARTS).ok_or("no parts section")?.entries {
            if let Some(m) = rd_i32(p, PART_MODEL).filter(|&m| m >= at as i32) {
                wr_i32(p, PART_MODEL, m + 1);
            }
        }
        Ok(at)
    }

    /// Turns enemy part `at` into another type, in place (its index, position, rotation,
    /// ground and walk route stay): the type data (+0xB8: AI, NpcParam, talk script, ...) and
    /// the block after it (+0xC0, which follows the model) from `donor`, an enemy part of
    /// another map file, on model `model` of this file; its entity id and groups kept only with
    /// `keep_ids` (a boss arena's scripts address the boss by them). The game makes
    /// a map's character with the model its part's NAME starts with (with only the model index
    /// changed, an enemy had its new NpcParam on its old body), so the name gets the donor's
    /// model ("c1430_0002" -> "c1280_0002", another number if that's taken; same length).
    pub fn replace_enemy(&mut self, at: usize, donor: &[u8], model: usize, keep_ids: bool) -> Result<(), String> {
        let name = self.unique_name(at, donor)?;
        let e = self.section_mut(PARTS).ok_or("no parts section")?.entries.get_mut(at).ok_or("no such part")?;
        if rd_i32(e, PART_TYPE) != Some(ENEMY) || rd_i32(donor, PART_TYPE) != Some(ENEMY) {
            return Err("not enemies".into());
        }
        let span = |x: &[u8]| -> Option<(usize, usize)> { Some((rd_i64(x, PART_TYPE_DATA)? as usize, rd_i64(x, PART_TYPE_DATA + 8)? as usize)) };
        let (t, g) = span(e).ok_or("cut short")?;
        let (dt, dg) = span(donor).ok_or("cut short")?;
        let len = g - t + 0x20;
        if g - t != dg - dt || e.len() < t + len || donor.len() < dt + len {
            return Err("enemy parts laid out differently".into());
        }
        let ground = rd_i32(e, t + ENEMY_GROUND).ok_or("cut short")?;
        let route = e[t + ENEMY_ROUTE..t + ENEMY_ROUTE + 2].to_vec();
        e[t..t + len].copy_from_slice(&donor[dt..dt + len]);
        wr_i32(e, t + ENEMY_GROUND, ground);
        e[t + ENEMY_ROUTE..t + ENEMY_ROUTE + 2].copy_from_slice(&route);
        wr_i32(e, PART_MODEL, model as i32);
        let name_at = rd_i64(e, 0).ok_or("cut short")? as usize;
        for (k, u) in name.encode_utf16().enumerate() {
            e[name_at + 2 * k..name_at + 2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
        if !keep_ids {
            let entity = rd_i64(e, PART_ENTITY).ok_or("cut short")? as usize;
            wr_i32(e, entity, -1);
            for k in 0..8 {
                wr_i32(e, entity + ENTITY_GROUPS + 4 * k, -1);
            }
        }
        Ok(())
    }

    /// Part `at`'s name with the donor's model in front, unique among the parts and as long as
    /// the old name.
    fn unique_name(&self, at: usize, donor: &[u8]) -> Result<String, String> {
        let model = entry_name(donor).and_then(|n| Some(n.split_once('_')?.0.to_string())).ok_or("an unusual enemy name")?;
        self.unique_name_for(at, &model)
    }

    /// Part `at`'s name on `model` (the first part of the name: "c1430" in "c1430_0002").
    fn unique_name_for(&self, at: usize, model: &str) -> Result<String, String> {
        let parts = &self.section(PARTS).ok_or("no parts section")?.entries;
        let old = parts.get(at).and_then(|e| entry_name(e)).ok_or("no such part")?;
        let (stem, number) = old.split_once('_').ok_or("an unusual enemy name")?;
        if stem.len() != model.len() {
            return Err(format!("{old}: a name of another form"));
        }
        let names: std::collections::HashSet<String> = parts.iter().filter_map(|e| entry_name(e)).collect();
        let first = format!("{model}_{number}");
        if !names.contains(&first) {
            return Ok(first);
        }
        // The same first two digits (the game names the character "<model>_<those>").
        let (lead, rest) = number.split_at(number.len().min(2));
        let digits = rest.len();
        (0..10usize.pow(digits as u32).min(100_000))
            .rev()
            .map(|k| format!("{model}_{lead}{k:0digits$}"))
            .find(|n| !names.contains(n))
            .ok_or_else(|| format!("no free name for a {model}"))
    }

    /// Turns enemy part `at` into an enemy of model `model_name` (a model no map has, e.g. a
    /// mod's: its files are served by the mod) in place, keeping everything else of it (position,
    /// ground, walk route, loot...): the model (a copy of the part's own model entry renamed,
    /// which must have a name of the same length), the part's name (starting with the model), and
    /// the AI row and NpcParam row given. Its entity id and groups go unless `keep_ids`.
    pub fn retype_enemy(&mut self, at: usize, model_name: &str, think: Option<i32>, npc: Option<i32>, keep_ids: bool) -> Result<(), String> {
        let name = self.unique_name_for(at, model_name)?;
        let entry = {
            let parts = &self.section(PARTS).ok_or("no parts section")?;
            let part = parts.entries.get(at).ok_or("no such part")?;
            if rd_i32(part, PART_TYPE) != Some(ENEMY) {
                return Err("not an enemy".into());
            }
            let model = rd_i32(part, PART_MODEL).and_then(|m| usize::try_from(m).ok()).ok_or("no model")?;
            self.section(MODELS).and_then(|s| s.entries.get(model)).ok_or("no such model")?.clone()
        };
        let model = self.add_model(&rename_entry(&entry, model_name)?)?;
        let e = self.section_mut(PARTS).ok_or("no parts section")?.entries.get_mut(at).ok_or("no such part")?;
        let t = rd_i64(e, PART_TYPE_DATA).ok_or("cut short")? as usize;
        if let Some(v) = think {
            wr_i32(e, t + ENEMY_THINK, v);
        }
        if let Some(v) = npc {
            wr_i32(e, t + ENEMY_NPC, v);
        }
        wr_i32(e, PART_MODEL, model as i32);
        let name_at = rd_i64(e, 0).ok_or("cut short")? as usize;
        for (k, u) in name.encode_utf16().enumerate() {
            e[name_at + 2 * k..name_at + 2 * k + 2].copy_from_slice(&u.to_le_bytes());
        }
        if !keep_ids {
            let entity = rd_i64(e, PART_ENTITY).ok_or("cut short")? as usize;
            wr_i32(e, entity, -1);
            for k in 0..8 {
                wr_i32(e, entity + ENTITY_GROUPS + 4 * k, -1);
            }
        }
        Ok(())
    }

    /// Sets each model's count of parts using it (+0x18), as the game's files have it.
    pub fn recount_models(&mut self) {
        let mut counts = vec![0i32; self.section(MODELS).map_or(0, |s| s.entries.len())];
        for p in self.section(PARTS).map(|s| s.entries.as_slice()).unwrap_or_default() {
            if let Some(m) = rd_i32(p, PART_MODEL).and_then(|m| usize::try_from(m).ok()).filter(|&m| m < counts.len()) {
                counts[m] += 1;
            }
        }
        if let Some(s) = self.section_mut(MODELS) {
            for (e, c) in s.entries.iter_mut().zip(counts) {
                wr_i32(e, MODEL_COUNT, c);
            }
        }
    }
}

pub const MODELS: &str = "MODEL_PARAM_ST";
pub const PARTS: &str = "PARTS_PARAM_ST";
/// A model entry: +0 name offset, +0x08 type, +0x0C its index among its type, +0x18 parts using it.
const MODEL_TYPE: usize = 0x08;
const MODEL_INDEX: usize = 0x0C;
const MODEL_COUNT: usize = 0x18;
/// A part: +0 name offset, +0x08 type, +0x10 model index, +0x20 position and rotation (6 f32),
/// +0xB0 entity data offset (+0 entity id, +0x1C eight groups), +0xB8 type data offset (an
/// enemy's: +0x08 AI row, +0x0C NpcParam row, +0x10 talk script, +0x1C the index of the
/// collision part it stands on, +0x20 its walk route, an i16).
pub const PART_TYPE: usize = 0x08;
pub const PART_MODEL: usize = 0x10;
pub const PART_POSE: usize = 0x20;
pub const PART_ENTITY: usize = 0xB0;
pub const PART_TYPE_DATA: usize = 0xB8;
pub const ENTITY_GROUPS: usize = 0x1C;
pub const ENEMY: i32 = 2;
const ENEMY_THINK: usize = 0x08;
const ENEMY_NPC: usize = 0x0C;
const ENEMY_GROUND: usize = 0x1C;
const ENEMY_ROUTE: usize = 0x20;

pub fn rd_i32(e: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(e.get(at..at + 4)?.try_into().ok()?))
}

pub fn rd_i64(e: &[u8], at: usize) -> Option<i64> {
    Some(i64::from_le_bytes(e.get(at..at + 8)?.try_into().ok()?))
}

fn wr_i32(e: &mut [u8], at: usize, v: i32) {
    if let Some(b) = e.get_mut(at..at + 4) {
        b.copy_from_slice(&v.to_le_bytes());
    }
}

/// A copy of an entry with its name `new` wherever the old one stands as text (the name, a model's
/// sib path): the same number of characters, so nothing moves.
pub fn rename_entry(entry: &[u8], new: &str) -> Result<Vec<u8>, String> {
    let old = entry_name(entry).ok_or("an entry without a name")?;
    if old.chars().count() != new.chars().count() || old.is_empty() {
        return Err(format!("{old:?} can't be renamed {new:?}: names of another length"));
    }
    let wide = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let (from, to) = (wide(&old), wide(new));
    let mut out = entry.to_vec();
    let mut at = 0;
    while at + from.len() <= out.len() {
        if out[at..at + from.len()] == from[..] {
            out[at..at + from.len()].copy_from_slice(&to);
            at += from.len();
        } else {
            at += 2;
        }
    }
    Ok(out)
}

/// An entry's name (its first field, an offset into the entry).
pub fn entry_name(e: &[u8]) -> Option<String> {
    wstr(e, rd_i64(e, 0)? as usize).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small map file: two sections, the first with two entries.
    fn sample() -> Msb {
        Msb {
            sections: vec![
                Section { name: "MODEL_PARAM_ST".into(), version: 3, entries: vec![vec![1; 0x10], vec![2; 0x0C]] },
                Section { name: "PARTS_PARAM_ST".into(), version: 3, entries: vec![] },
            ],
        }
    }

    #[test]
    fn writes_and_reads_back() {
        let m = sample();
        let bytes = m.to_bytes();
        let back = Msb::parse(&bytes).unwrap();
        assert_eq!(back.sections.len(), 2);
        assert_eq!(back.sections[0].entries[0], vec![1; 0x10]);
        // Entries come back with their padding.
        assert_eq!(&back.sections[0].entries[1][..0x0C], &[2; 0x0C]);
        assert_eq!(back.to_bytes(), bytes);
    }

    /// A parts section with enemies named `names` (each part: name offset, type, model index,
    /// then its name).
    fn with_parts(names: &[&str]) -> Msb {
        let part = |n: &str| {
            let mut e = vec![0u8; 0x18];
            e[..8].copy_from_slice(&0x18i64.to_le_bytes());
            e[PART_TYPE..PART_TYPE + 4].copy_from_slice(&ENEMY.to_le_bytes());
            e.extend(n.encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
            e
        };
        Msb { sections: vec![Section { name: PARTS.into(), version: 3, entries: names.iter().map(|n| part(n)).collect() }] }
    }

    #[test]
    fn new_names_are_unique() {
        let m = with_parts(&["c1430_0002", "c1280_0002", "c1280_0099"]);
        let donor = with_parts(&["c1280_0000"]).sections[0].entries[0].clone();
        assert_eq!(m.unique_name(0, &donor).unwrap(), "c1280_0098", "taken: the same first digits");
        let m = with_parts(&["c1430_0003"]);
        assert_eq!(m.unique_name(0, &donor).unwrap(), "c1280_0003");
    }

    #[test]
    fn entries_are_renamed_where_the_name_stands() {
        let mut e = vec![0u8; 0x18];
        e[..8].copy_from_slice(&0x18i64.to_le_bytes());
        e.extend("c2110".encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
        e.extend(r"x\c2110\c2110.sib".encode_utf16().chain([0]).flat_map(u16::to_le_bytes));
        let r = rename_entry(&e, "c7200").unwrap();
        assert_eq!(entry_name(&r).unwrap(), "c7200");
        assert_eq!(r.len(), e.len());
        assert_eq!(wstr(&r, 0x18 + 12).unwrap(), r"x\c7200\c7200.sib");
        assert!(rename_entry(&e, "c72000").is_err());
    }

    #[test]
    fn rejects_other_files() {
        assert!(Msb::parse(b"BND4").is_err());
    }
}
