//! Item names, read from the game's own text (`MsgRepository`) so they match the game's
//! language exactly and nothing has to be shipped.
//!
//! Layout (DS3 1.15.2, mapped live with the console's `debug` module):
//!
//! ```text
//! MsgRepository (FD4 singleton)
//!   +0x08  *mut [*mut [*mut FMG; categories]; versions]
//!   +0x10  u32 versions (13)      +0x14  u32 categories (512)
//! ```
//!
//! Category ids are the msgbnd FMG ids: item names are 10-13 (base game), 210-213
//! (Ashes of Ariandel) and 250-253 (The Ringed City) for goods/weapons/armour/rings.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::ds3::ItemCategory;

use crate::{fmg, memory};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Goods,
    Weapon,
    Armor,
    Ring,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Goods, Kind::Weapon, Kind::Armor, Kind::Ring];

    /// FMG ids holding this kind's names: base game, DLC1, DLC2 (later ones win).
    fn fmg_ids(self) -> [u32; 3] {
        match self {
            Kind::Goods => [10, 210, 250],
            Kind::Weapon => [11, 211, 251],
            Kind::Armor => [12, 212, 252],
            Kind::Ring => [13, 213, 253],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Goods => "goods",
            Kind::Weapon => "weapon",
            Kind::Armor => "armor",
            Kind::Ring => "ring",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "goods" => Kind::Goods,
            "weapon" => Kind::Weapon,
            "armor" => Kind::Armor,
            "ring" => Kind::Ring,
            _ => return None,
        })
    }

    pub fn category(self) -> ItemCategory {
        match self {
            Kind::Goods => ItemCategory::Goods,
            Kind::Weapon => ItemCategory::Weapon,
            Kind::Armor => ItemCategory::Protector,
            Kind::Ring => ItemCategory::Accessory,
        }
    }

    pub fn from_category(c: ItemCategory) -> Kind {
        match c {
            ItemCategory::Goods => Kind::Goods,
            ItemCategory::Weapon => Kind::Weapon,
            ItemCategory::Protector => Kind::Armor,
            ItemCategory::Accessory => Kind::Ring,
        }
    }
}

pub struct ItemNames {
    by_id: HashMap<(Kind, u32), String>,
    /// lowercase name → every (kind, id) with that name, sorted
    by_name: HashMap<String, Vec<(Kind, u32)>>,
}

static NAMES: Mutex<Option<Arc<ItemNames>>> = Mutex::new(None);

fn is_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.starts_with("<?") || t == "-" || t == "?" || t.eq_ignore_ascii_case("(dummyText)")
}

/// Reads one FMG by category id from every loaded version; later versions override.
fn read_category(versions: usize, version_count: usize, category: u32) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for v in 0..version_count {
        let Some(cats) = memory::read_u64(versions + v * 8).map(|p| p as usize) else { continue };
        if cats == 0 {
            continue;
        }
        let Some(f) = memory::read_u64(cats + category as usize * 8).map(|p| p as usize) else { continue };
        if f == 0 {
            continue;
        }
        match fmg::read(f, &memory::read_bytes) {
            Ok(entries) => out.extend(entries),
            Err(e) => crate::warn!("text: FMG {category} (version {v}): {e}"),
        }
    }
    out
}

impl ItemNames {
    fn build() -> Result<Self, String> {
        let repo = crate::ds3::MSG_REPOSITORY.instance().ok_or("the game's text isn't loaded yet")?;
        let versions = memory::read_u64(repo + 0x08).ok_or("MsgRepository is unreadable")? as usize;
        let version_count = memory::read::<u32>(repo + 0x10).unwrap_or(0) as usize;
        let category_count = memory::read::<u32>(repo + 0x14).unwrap_or(0);
        if versions == 0 || !(1..=64).contains(&version_count) || category_count < 254 {
            return Err(format!("unexpected MsgRepository layout (versions={version_count}, categories={category_count})"));
        }

        let mut by_id = HashMap::new();
        for kind in Kind::ALL {
            for fmg_id in kind.fmg_ids() {
                for (id, name) in read_category(versions, version_count, fmg_id) {
                    if !is_placeholder(&name) {
                        by_id.insert((kind, id), name.trim().to_string());
                    }
                }
            }
        }
        if by_id.is_empty() {
            return Err("the game's item names aren't loaded yet".into());
        }

        let mut by_name: HashMap<String, Vec<(Kind, u32)>> = HashMap::new();
        for (key, name) in &by_id {
            by_name.entry(name.to_lowercase()).or_default().push(*key);
        }
        for v in by_name.values_mut() {
            v.sort();
        }
        crate::info!(
            "text: {} item names ({} goods, {} weapons, {} armor, {} rings)",
            by_id.len(),
            by_id.keys().filter(|k| k.0 == Kind::Goods).count(),
            by_id.keys().filter(|k| k.0 == Kind::Weapon).count(),
            by_id.keys().filter(|k| k.0 == Kind::Armor).count(),
            by_id.keys().filter(|k| k.0 == Kind::Ring).count(),
        );
        Ok(Self { by_id, by_name })
    }

    /// Every distinct name of one kind, sorted.
    pub fn names(&self, kind: Kind) -> Vec<String> {
        let mut out: Vec<String> = self.by_id.iter().filter(|((k, _), _)| *k == kind).map(|(_, n)| n.clone()).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The display name of an item. Upgraded weapons (`+N`) are named from their base id.
    pub fn name(&self, kind: Kind, id: u32) -> Option<String> {
        if let Some(n) = self.by_id.get(&(kind, id)) {
            return Some(n.clone());
        }
        if kind == Kind::Weapon {
            let level = id % 100;
            if level > 0
                && let Some(n) = self.by_id.get(&(kind, id - level))
            {
                return Some(format!("{n} +{level}"));
            }
        }
        None
    }

    /// Exact (case-insensitive) lookup, optionally restricted to one kind. Weapons accept a
    /// `+N` suffix for upgrade levels.
    pub fn lookup(&self, name: &str, kind: Option<Kind>) -> Result<(Kind, u32), String> {
        let wanted = name.trim();
        let (base_name, level) = match wanted.rsplit_once(" +") {
            Some((n, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()) => (n, l.parse::<u32>().unwrap_or(0)),
            _ => (wanted, 0),
        };
        let candidates: Vec<(Kind, u32)> =
            self.by_name.get(&base_name.to_lowercase()).map(|v| v.iter().copied().filter(|(k, _)| kind.is_none_or(|w| w == *k)).collect()).unwrap_or_default();

        let pick = match candidates.as_slice() {
            [] => {
                let hint = self.find(base_name, 3).into_iter().map(|(_, _, n)| format!("{n:?}")).collect::<Vec<_>>().join(", ");
                let scope = kind.map(|k| format!(" {}", k.name())).unwrap_or_default();
                return Err(if hint.is_empty() {
                    format!("no{scope} item named {wanted:?}")
                } else {
                    format!("no{scope} item named {wanted:?} (did you mean {hint}?)")
                });
            }
            [one] => *one,
            many => {
                // Same name in several categories: make the caller choose.
                let kinds: Vec<Kind> = {
                    let mut k: Vec<Kind> = many.iter().map(|(k, _)| *k).collect();
                    k.dedup();
                    k
                };
                if kinds.len() > 1 {
                    let opts = kinds.iter().map(|k| format!("items.{}({wanted:?})", k.name())).collect::<Vec<_>>().join(" or ");
                    return Err(format!("{wanted:?} is ambiguous; use {opts}"));
                }
                many[0] // same kind, several ids: lowest id is the canonical one
            }
        };

        if level > 0 {
            if pick.0 != Kind::Weapon {
                return Err(format!("{wanted:?}: only weapons have +N upgrade levels"));
            }
            if level > 15 {
                return Err(format!("{wanted:?}: upgrade level must be 0-15"));
            }
            return Ok((pick.0, pick.1 + level));
        }
        Ok(pick)
    }

    /// Substring search (case-insensitive), exact matches first, then by kind and id.
    pub fn find(&self, query: &str, limit: usize) -> Vec<(Kind, u32, String)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(bool, Kind, u32, &String)> =
            self.by_id.iter().filter(|(_, n)| n.to_lowercase().contains(&q)).map(|((k, id), n)| (n.to_lowercase() != q, *k, *id, n)).collect();
        hits.sort_by_key(|h| (h.0, h.1, h.2));
        hits.into_iter().take(limit).map(|(_, k, id, n)| (k, id, n.clone())).collect()
    }
}

/// Every real entry of the given FMGs (later versions and later ids in `fmg_ids` win), by id.
pub fn category_texts(fmg_ids: &[u32]) -> Result<Vec<(u32, String)>, String> {
    let repo = crate::ds3::MSG_REPOSITORY.instance().ok_or("the game's text isn't loaded yet")?;
    let versions = memory::read_u64(repo + 0x08).ok_or("MsgRepository is unreadable")? as usize;
    let version_count = (memory::read::<u32>(repo + 0x10).unwrap_or(0) as usize).min(64);
    if versions == 0 || version_count == 0 {
        return Err("the game's text isn't loaded yet".into());
    }
    let mut by_id = std::collections::BTreeMap::new();
    for &fmg in fmg_ids {
        for (id, text) in read_category(versions, version_count, fmg) {
            if !is_placeholder(&text) {
                by_id.insert(id, text.trim().to_string());
            }
        }
    }
    Ok(by_id.into_iter().collect())
}

/// Item names, built on first use once the game has loaded its text.
pub fn item_names() -> Result<Arc<ItemNames>, String> {
    let mut slot = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(n) = slot.as_ref() {
        return Ok(n.clone());
    }
    let n = Arc::new(ItemNames::build()?);
    *slot = Some(n.clone());
    Ok(n)
}

/// `s` as a NUL-terminated UTF-16 string that lives for the rest of the game (the game keeps
/// pointers to the text it's given).
pub(crate) fn wide(s: &str) -> &'static [u16] {
    Box::leak(s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>().into_boxed_slice())
}

/// As `wide`, but each distinct text is made once and reused (for text set again and again).
pub(crate) fn wide_kept(s: &str) -> &'static [u16] {
    static KEPT: Mutex<Vec<(String, &'static [u16])>> = Mutex::new(Vec::new());
    let mut kept = crate::lock(&KEPT);
    if let Some((_, w)) = kept.iter().find(|(t, _)| t == s) {
        return w;
    }
    let w = wide(s);
    kept.push((s.to_string(), w));
    w
}
