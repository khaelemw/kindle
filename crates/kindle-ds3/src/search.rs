//! The overlay's Find tab: every name Kindle can turn into an id, searchable in one place,
//! each with the Lua that uses it. Runs on the game thread (it reads the game's text and
//! params); the overlay asks through ui_bridge.

use std::sync::{Arc, Mutex};

use crate::ui_bridge::FindHit;

/// The Lua API, as the editor sees it (docs and `kindle check` use the same file).
const STUBS: &str = include_str!("../../../dist/stubs/kindle.lua");
/// Most results per kind of name.
const PER_KIND: usize = 40;

/// FMGs searched by name: (kind shown, text category for `text.get`, FMG ids).
const TEXT_KINDS: &[(&str, &str, &[u32])] = &[("place", "place_name", &[19, 216, 256]), ("npc name", "npc_name", &[18, 215, 255])];

/// (index into TEXT_KINDS, text id, text) for every searchable text entry.
type Texts = Arc<Vec<(usize, u32, String)>>;

/// Text tables, read once (they don't change while the game runs).
static TEXTS: Mutex<Option<Texts>> = Mutex::new(None);

fn texts() -> Option<Texts> {
    let mut slot = TEXTS.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        let mut all = Vec::new();
        for (i, (_, _, fmgs)) in TEXT_KINDS.iter().enumerate() {
            all.extend(crate::text::category_texts(fmgs).ok()?.into_iter().map(|(id, t)| (i, id, t)));
        }
        *slot = Some(Arc::new(all));
    }
    slot.clone()
}

fn hit(kind: &'static str, name: impl Into<String>, id: impl ToString, lua: impl Into<String>, note: impl Into<String>) -> FindHit {
    FindHit { kind, name: name.into(), id: id.to_string(), lua: lua.into(), note: note.into() }
}

/// Lowercase, with `_` read as a space, so "iudex gundyr" finds `iudex_gundyr`.
fn norm(s: &str) -> String {
    s.to_lowercase().replace('_', " ")
}

/// API functions from the stubs: (signature, first doc line, all doc text).
fn api() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut doc: Vec<&str> = Vec::new();
    for line in STUBS.lines() {
        if let Some(d) = line.strip_prefix("--- ") {
            doc.push(d);
        } else if let Some(sig) = line.strip_prefix("function ") {
            let sig = sig.trim_end_matches(" end").to_string();
            out.push((sig, doc.first().copied().unwrap_or("").to_string(), doc.join(" ")));
            doc.clear();
        } else if !line.starts_with("---") {
            doc.clear();
        }
    }
    out
}

/// Everything whose name contains `query` (at least 2 characters), grouped by kind.
pub fn search(query: &str) -> Vec<FindHit> {
    let q = norm(query.trim());
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();

    for (key, flag, display) in crate::game::BOSSES {
        if norm(key).contains(&q) || norm(display).contains(&q) {
            out.push(hit("boss", *display, flag, format!("flags.boss.{key}"), "defeat flag"));
        }
    }
    for e in crate::enemies::ENEMIES.iter().filter(|e| norm(e.key).contains(&q) || norm(e.name).contains(&q)).take(PER_KIND) {
        let note = format!("model c{:04}, NpcParam {}, think {}, from {}", e.model, e.npc_param, e.think_param, e.area);
        out.push(hit("enemy", e.name, e.model, format!("world.spawn{{enemy = \"{}\"}}", e.key), note));
    }
    for (key, flag) in crate::lua_values::bonfire_flags().into_iter().filter(|(k, _)| norm(k).contains(&q)).take(PER_KIND) {
        out.push(hit("bonfire", key.replace('_', " "), flag, format!("flags.bonfire.{key}"), "lit flag"));
    }
    if let Ok(names) = crate::text::item_names() {
        for (kind, id, name) in names.find(query, PER_KIND) {
            let k = kind.name();
            out.push(hit("item", name.clone(), id, format!("items.{k}({id}) -- {name}"), format!("{k} param row")));
        }
    }
    if let Some(texts) = texts() {
        for (i, (kind, category, _)) in TEXT_KINDS.iter().enumerate() {
            let matches = texts.iter().filter(|(k, _, t)| *k == i && norm(t).contains(&q)).take(PER_KIND);
            for (_, id, t) in matches {
                out.push(hit(kind, t.clone(), id, format!("text.get(\"{category}\", {id})"), "text id"));
            }
        }
    }
    if let Ok(reg) = crate::params::registry() {
        for t in reg.tables().filter(|t| t.valid && norm(&t.name).contains(&q)).take(PER_KIND) {
            out.push(hit("param table", t.name.clone(), "", format!("params.get(\"{}\", row)", t.name), "params.rows to list its rows"));
        }
    }
    for e in crate::events::all().filter(|e| norm(e).contains(&q)) {
        out.push(hit("event", e, "", format!("kindle.on(\"{e}\", function(...) end)"), "see docs/events.md"));
    }
    let mut api_hits = 0;
    for (sig, first, all) in api() {
        if api_hits < PER_KIND && (norm(&sig).contains(&q) || norm(&all).contains(&q)) {
            api_hits += 1;
            out.push(hit("api", sig.clone(), "", sig, first));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_entries_come_from_the_stubs() {
        let all = api();
        assert!(all.len() > 150, "{}", all.len());
        let give = all.iter().find(|(sig, _, _)| sig.starts_with("kindle.items.give(")).expect("items.give");
        assert!(!give.1.is_empty(), "items.give has a doc line");
    }

    #[test]
    fn finds_bosses_events_and_api_without_the_game() {
        let hits = search("gundyr");
        assert!(hits.iter().any(|h| h.kind == "boss" && h.lua == "flags.boss.iudex_gundyr"));
        assert!(hits.iter().any(|h| h.lua == "flags.boss.champion_gundyr"));
        assert!(search("black knight").iter().any(|h| h.kind == "enemy" && h.lua == "world.spawn{enemy = \"black_knight\"}"));
        let hits = search("player loaded");
        assert!(hits.iter().any(|h| h.kind == "event" && h.name == "player_loaded"));
        assert!(search("give").iter().any(|h| h.kind == "api" && h.name.starts_with("kindle.items.give(")));
        assert!(search("x").is_empty());
    }
}
