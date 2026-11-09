//! The overlay's Find tab, as Kindle for Dark Souls III's search.rs: items by name, sites of grace and
//! param tables, each with the Lua that uses it. Runs on the game's thread (it reads the game's text and
//! params); the overlay asks through ui_bridge.

use kindle_core::ui_bridge::FindHit;

/// Most results per kind of name.
const PER_KIND: usize = 40;

fn hit(kind: &'static str, name: impl Into<String>, id: impl ToString, lua: impl Into<String>, note: impl Into<String>) -> FindHit {
    FindHit { kind, name: name.into(), id: id.to_string(), lua: lua.into(), note: note.into() }
}

/// Everything named like `query` (any case; two letters at least).
pub fn search(query: &str) -> Vec<FindHit> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let lower = q.to_lowercase();
    let mut out = Vec::new();
    for (item, name) in crate::text::find(q, PER_KIND).unwrap_or_default() {
        let kind = crate::items::category(item).unwrap_or("item");
        let id = item & crate::items::ID_BITS;
        out.push(hit("item", name.clone(), format!("{kind} {id}"), format!("items.give({name:?}, 1)"), format!("items.{kind}({id})")));
    }
    for g in
        crate::emevd::graces().unwrap_or_default().into_iter().filter(|g| g.name.as_ref().is_some_and(|n| n.to_lowercase().contains(&lower))).take(PER_KIND)
    {
        let name = g.name.unwrap_or_default();
        let (area, x, z) = g.map;
        out.push(hit("grace", name.clone(), g.id, format!("player.warp({name:?})"), format!("m{area}_{x:02}_{z:02}_00, asset {}", g.entity)));
    }
    if let Ok(registry) = kindle_core::params::registry() {
        for t in registry.tables().filter(|t| t.name.to_lowercase().contains(&lower)).take(PER_KIND) {
            out.push(hit("param table", t.name.clone(), "", format!("params.get({:?}, id)", t.name), format!("{} rows", t.row_ids.len())));
        }
    }
    out
}
