//! Mods' changes to the game's map files (`kindle.maps`): a map's parts as the game's file has
//! them (`parts`), and changes a mod declares as data (`set`), applied when the game opens the
//! map's file (files.rs serves the changed file, as for event patches), so they take effect from
//! the next time the area loads. The game opens map files on its loader thread, where Lua can't
//! run: a mod that decides its changes as an area loads does it in `map_loading`
//! (map_loading.rs), which the loader waits for.
//!
//! Changes, per map in the order mods made them:
//! - replace an enemy: a part of the map becomes an enemy of another part's type (any map's):
//!   it keeps where it stands, the collision it stands on and its walk route, and gets the
//!   other's type data (AI, NpcParam, ...), model and a name starting with that model (msb.rs
//!   `replace_enemy`). Its entity id and groups go (the spot's event scripts don't act on the
//!   new enemy, which behaves as its type does at home), unless kept (`keep_ids`: a boss arena's
//!   scripts address the boss by them). Verified in game 2026-10-07: the game makes a map's
//!   character with the model its part's name starts with; it's active only while the
//!   collision it stands on is loaded; enemies are replaced in place (an enemy part appended
//!   after the others crashed the game as the map loaded).
//! - set an enemy's type: a part becomes an enemy of a model no map has (a mod's own, `model`),
//!   with the AI row and NpcParam row given and its goals in the mod's own AI file (`ai`, loaded
//!   like another map's); everything else of the part stays (msb.rs `retype_enemy`).
//!
//! An enemy of another map's type fights only once that map's AI file is loaded: Kindle loads
//! the AI of every map whose enemies a served map file has (spawn.rs `load_ai`), each time the
//! game rebuilds its AI scripts (`frame`).

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use crate::msb::{self, Msb};

#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// Part `part` (an enemy) becomes an enemy of part `from_part` of map `from_map`'s type.
    ReplaceEnemy { part: usize, from_map: String, from_part: usize, keep_ids: bool },
    /// Part `part` (an enemy) becomes an enemy of a model no map has (a mod's), with the AI row
    /// and NpcParam row given; `ai` names the AI file (script/<ai>.luabnd) its goals are in.
    SetEnemy { part: usize, model: String, npc: Option<i32>, think: Option<i32>, ai: Option<String>, keep_ids: bool },
}

struct MapEdits {
    owner: String,
    map: String,
    edits: Vec<Edit>,
}

static EDITS: Mutex<Vec<MapEdits>> = Mutex::new(Vec::new());

fn edits() -> std::sync::MutexGuard<'static, Vec<MapEdits>> {
    EDITS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A map's name: "m30_00_00_00".
pub fn valid_map(map: &str) -> bool {
    let b = map.as_bytes();
    b.len() == 12 && b[0] == b'm' && b[1..].iter().enumerate().all(|(i, &c)| if i % 3 == 2 { c == b'_' } else { c.is_ascii_digit() })
}

/// Sets `owner`'s changes to `map` (replacing its earlier ones; none: removed).
pub fn set(owner: &str, map: &str, list: Vec<Edit>) -> Result<(), String> {
    if !valid_map(map) {
        return Err(format!("{map:?} isn't a map's name (like \"m30_00_00_00\")"));
    }
    let mut all = edits();
    all.retain(|e| !(e.owner == owner && e.map == map));
    if !list.is_empty() {
        all.push(MapEdits { owner: owner.to_string(), map: map.to_string(), edits: list });
    }
    Ok(())
}

/// Drops `owner`'s changes to `map` (every map's with None); how many changes there were.
pub fn reset(owner: &str, map: Option<&str>) -> usize {
    let mut all = edits();
    let mut n = 0;
    all.retain(|e| {
        let drop = e.owner == owner && map.is_none_or(|m| e.map == m);
        if drop {
            n += e.edits.len();
        }
        !drop
    });
    n
}

/// The map of a map file's game path ("map/mapstudio/m30_00_00_00.msb.dcx").
pub fn map_of(rel: &str) -> Option<&str> {
    rel.strip_prefix("map/mapstudio/")?.strip_suffix(".msb.dcx").filter(|m| valid_map(m))
}

/// A fingerprint of a map's changes (None without any), so a changed set is served anew.
pub fn version(rel: &str) -> Option<String> {
    let map = map_of(rel)?;
    let all = edits();
    let mine: Vec<String> = all.iter().filter(|e| e.map == map).map(|e| format!("{}:{:?}", e.owner, e.edits)).collect();
    if mine.is_empty() {
        return None;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    mine.hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// One part of a map file, as `kindle.maps.parts` gives it.
#[derive(Debug, Clone)]
pub struct Part {
    pub name: String,
    /// The part's type (0 map piece, 1 object, 2 enemy, 4 player, 5 collision, 9 dummy object,
    /// 10 dummy enemy, 11 connect collision).
    pub kind: i32,
    /// Its model's name ("c1100").
    pub model: String,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    /// Entity id (0: none) and entity groups (those set).
    pub entity: i32,
    pub groups: Vec<i32>,
    /// An enemy's AI row, NpcParam row and talk script.
    pub enemy: Option<[i32; 3]>,
}

/// A map file as Kindle read it: its parts, and each enemy part's entry with its model's entry
/// (what a replacement copies), by part index.
struct Read {
    parts: Arc<Vec<Part>>,
    enemies: HashMap<usize, (Vec<u8>, Vec<u8>)>,
}

/// In an enemy part's type data: the AI row, NpcParam row and talk script.
const ENEMY_AI: usize = 0x08;

fn f32_at(e: &[u8], at: usize) -> f32 {
    e.get(at..at + 4).map_or(0.0, |b| f32::from_le_bytes(b.try_into().expect("4 bytes")))
}

fn read_parts(m: &Msb) -> Read {
    let models = m.section(msb::MODELS).map(|s| s.entries.as_slice()).unwrap_or_default();
    let mut parts = Vec::new();
    let mut enemies = HashMap::new();
    for (i, e) in m.section(msb::PARTS).map(|s| s.entries.as_slice()).unwrap_or_default().iter().enumerate() {
        let kind = msb::rd_i32(e, msb::PART_TYPE).unwrap_or(-1);
        let model_entry = msb::rd_i32(e, msb::PART_MODEL).and_then(|k| models.get(usize::try_from(k).ok()?));
        let entity_at = msb::rd_i64(e, msb::PART_ENTITY).unwrap_or(0) as usize;
        let (entity, groups) = if entity_at == 0 {
            (0, Vec::new())
        } else {
            let groups = (0..8).filter_map(|k| msb::rd_i32(e, entity_at + msb::ENTITY_GROUPS + 4 * k)).filter(|&g| g > 0).collect();
            (msb::rd_i32(e, entity_at).filter(|&v| v > 0).unwrap_or(0), groups)
        };
        let typed = msb::rd_i64(e, msb::PART_TYPE_DATA).unwrap_or(0) as usize;
        let enemy = (kind == msb::ENEMY && typed != 0)
            .then(|| Some([msb::rd_i32(e, typed + ENEMY_AI)?, msb::rd_i32(e, typed + ENEMY_AI + 4)?, msb::rd_i32(e, typed + ENEMY_AI + 8)?]))
            .flatten();
        if let (Some(_), Some(model)) = (enemy, model_entry) {
            enemies.insert(i, (e.clone(), model.clone()));
        }
        let pose = |k: usize| f32_at(e, msb::PART_POSE + 4 * k);
        parts.push(Part {
            name: msb::entry_name(e).unwrap_or_default(),
            kind,
            model: model_entry.and_then(|m| msb::entry_name(m)).unwrap_or_default(),
            position: [pose(0), pose(1), pose(2)],
            rotation: [pose(3), pose(4), pose(5)],
            entity,
            groups,
            enemy,
        });
    }
    Read { parts: Arc::new(parts), enemies }
}

fn game_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent().map(std::path::Path::to_path_buf)
}

/// The game's own map file for `map`, unpacked (Err if the game has none: a DLC's not there).
fn game_map(map: &str) -> Result<Msb, String> {
    let dir = game_dir().ok_or("no game folder")?;
    let (_, dcx) = crate::archive::read_any(&dir, &format!("/map/mapstudio/{map}.msb.dcx"))?;
    let raw = crate::archive::undcx(&dcx).ok_or("not a DCX file Kindle can read")?;
    Msb::parse(&raw)
}

/// Map files read so far (kept for the session: a mod reads every map's parts, and changes name
/// other maps' parts), None where the game has no such map.
static READ: Mutex<Option<HashMap<String, Option<Arc<Read>>>>> = Mutex::new(None);

fn read(map: &str) -> Result<Option<Arc<Read>>, String> {
    if !valid_map(map) {
        return Err(format!("{map:?} isn't a map's name (like \"m30_00_00_00\")"));
    }
    if let Some(r) = READ.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(map)) {
        return Ok(r.clone());
    }
    let started = std::time::Instant::now();
    let r = match game_map(map) {
        Ok(m) => Some(Arc::new(read_parts(&m))),
        Err(e) if e.contains("isn't in the game's archives") => None,
        Err(e) => return Err(format!("{map}: {e}")),
    };
    crate::debug!("maps: {map} read ({:.2}s)", started.elapsed().as_secs_f32());
    READ.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(map.to_string(), r.clone());
    Ok(r)
}

/// A map's parts as the game's file has them, in the file's order (None: the game has no such
/// map, e.g. a DLC's that isn't installed).
pub fn parts(map: &str) -> Result<Option<Arc<Vec<Part>>>, String> {
    Ok(read(map)?.map(|r| r.parts.clone()))
}

/// Reads maps' parts on a thread of their own (`maps.preload`), so `parts` has them at once
/// later: a mod that reads every map as an area loads would hold the loading screen otherwise.
pub fn preload(maps: Vec<String>) -> Result<(), String> {
    if let Some(bad) = maps.iter().find(|m| !valid_map(m)) {
        return Err(format!("{bad:?} isn't a map's name (like \"m30_00_00_00\")"));
    }
    std::thread::Builder::new()
        .name("kindle maps".into())
        .spawn(move || {
            let started = std::time::Instant::now();
            let found = maps.iter().filter(|m| matches!(read(m), Ok(Some(_)))).count();
            crate::info!("maps: {found} of {} map(s) read ahead ({:.1}s)", maps.len(), started.elapsed().as_secs_f32());
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The maps whose AI the served map files' enemies need, by map.
static NEEDED_AI: Mutex<Option<HashMap<String, BTreeSet<String>>>> = Mutex::new(None);

/// The game's map file `original` (DCX) with the changes for its map, or None without any.
pub fn edited(rel: &str, original: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let Some(map) = map_of(rel) else { return Ok(None) };
    let mine: Vec<(String, Edit)> = edits().iter().filter(|e| e.map == map).flat_map(|e| e.edits.iter().map(|x| (e.owner.clone(), x.clone()))).collect();
    if mine.is_empty() {
        return Ok(None);
    }
    let raw = crate::archive::undcx(original).ok_or("not a DCX file Kindle can read")?;
    let mut m = Msb::parse(&raw)?;
    let mut from_maps = BTreeSet::new();
    let mut applied = 0;
    for (owner, edit) in &mine {
        let made = match edit {
            Edit::SetEnemy { .. } => None,
            Edit::ReplaceEnemy { part, from_map, from_part, keep_ids } => read(from_map)
                .and_then(|r| r.ok_or_else(|| format!("the game has no map {from_map}")))
                .and_then(|r| r.enemies.get(from_part).cloned().ok_or_else(|| format!("{from_map} part {} isn't an enemy", from_part + 1)))
                .and_then(|(donor, model)| {
                    let model = m.add_model(&model)?;
                    m.replace_enemy(*part, &donor, model, *keep_ids)
                })
                .map(|()| from_map.clone())
                .into(),
        };
        let made = match (made, edit) {
            (Some(made), _) => made,
            (None, Edit::SetEnemy { part, model, npc, think, ai, keep_ids }) => {
                m.retype_enemy(*part, model, *think, *npc, *keep_ids).map(|()| ai.clone().unwrap_or_else(|| map.to_string()))
            }
            (None, _) => unreachable!(),
        };
        match made {
            Ok(from) => {
                if from != map {
                    from_maps.insert(from.clone());
                }
                applied += 1;
            }
            Err(e) => crate::warn!("[{owner}] maps: {map}: {edit:?} not applied: {e}"),
        }
    }
    m.recount_models();
    crate::info!("maps: {map}: {applied} of {} change(s) applied (enemies from {} other maps)", mine.len(), from_maps.len());
    NEEDED_AI.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(map.to_string(), from_maps);
    if applied == 0 {
        return Ok(None);
    }
    crate::archive::dcx(original, &m.to_bytes(), original.len()).map(Some).ok_or_else(|| "couldn't write the map file".into())
}

/// How often (frames) the needed AI files are checked.
const AI_CHECK_FRAMES: u64 = 30;

/// Each frame in the world (the game's thread): the AI files of the maps the served map files'
/// enemies come from are loaded, and registered again whenever the game rebuilds its AI scripts.
pub fn frame(frame: u64) {
    if !frame.is_multiple_of(AI_CHECK_FRAMES) {
        return;
    }
    let maps: BTreeSet<String> = match NEEDED_AI.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(needed) => needed.values().flatten().cloned().collect(),
        None => return,
    };
    if maps.is_empty() {
        return;
    }
    let here = crate::spawn::current_area();
    static FAILED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    for map in maps {
        if here.as_deref() == Some(map.as_str()) {
            continue;
        }
        if let Err(e) = crate::spawn::load_ai(&map) {
            let mut failed = FAILED.lock().unwrap_or_else(|e| e.into_inner());
            if !failed.contains(&map) {
                crate::warn!("maps: {map}'s AI: {e}; its enemies elsewhere won't fight");
                failed.push(map);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names() {
        assert!(valid_map("m30_00_00_00"));
        assert!(!valid_map("m30_00_00"));
        assert!(!valid_map("common"));
        assert!(!valid_map("m30_00_00_0x"));
        assert_eq!(map_of("map/mapstudio/m40_00_00_00.msb.dcx"), Some("m40_00_00_00"));
        assert_eq!(map_of("event/m40_00_00_00.emevd.dcx"), None);
    }

    #[test]
    fn edits_are_per_owner_and_map() {
        let e = |part| Edit::ReplaceEnemy { part, from_map: "m31_00_00_00".into(), from_part: 3, keep_ids: false };
        let rel = "map/mapstudio/m99_00_00_00.msb.dcx";
        assert_eq!(version(rel), None);
        set("a", "m99_00_00_00", vec![e(1)]).unwrap();
        let first = version(rel).unwrap();
        set("b", "m99_00_00_00", vec![e(2)]).unwrap();
        let both = version(rel).unwrap();
        assert_ne!(first, both);
        set("b", "m99_00_00_00", vec![]).unwrap();
        assert_eq!(version(rel).unwrap(), first, "b's removed");
        assert_eq!(reset("a", None), 1);
        assert_eq!(version(rel), None);
        assert!(set("a", "common", vec![e(1)]).is_err());
    }
}
