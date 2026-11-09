//! Every event `kindle.on` accepts, in one list. The editor stubs (the `kindle.event` alias)
//! and docs/events.md are checked against it by tests (with the game's events, in its crate).

/// Events the runtime itself raises.
pub const CORE: &[&str] = &["frame", "player_loaded", "player_unloaded", "unload", "setting_changed", "map_loading"];

/// Every event name: the runtime's, then the game's (its state between frames, its hooks; bindings.rs).
pub fn all() -> impl Iterator<Item = &'static str> {
    let game = crate::bindings::get().map_or(&[][..], |b| b.events);
    CORE.iter().chain(game.iter().flat_map(|list| list.iter())).copied()
}

pub fn is_known(event: &str) -> bool {
    all().any(|e| e == event) || crate::catalogue::current().event(event).is_ok()
}
