//! Game events found by comparing the game's state between frames, as Kindle for Dark Souls III's
//! (its game_events.rs): the player's death, the map block the player is in, characters killed and
//! appearing, items gained and lost, the player's effects, and level ups.
//!
//! Only the events some mod listens to are tracked; whole-world scans (characters, the inventory,
//! effects) run every few frames. The baselines start over whenever the player leaves the world (but
//! the map), so a load never fires a burst of stale events.

use std::collections::{HashMap, HashSet};

use crate::world::Listed;

/// How often (in frames) the heavier scans run.
const SCAN_EVERY: u64 = 5;

pub const EVENTS: &[&str] =
    &["player_died", "map_changed", "character_killed", "character_spawned", "item_gained", "item_lost", "effect_applied", "effect_expired", "level_up"];

pub enum Fired {
    PlayerDied,
    /// The map block the player is in (its name, "m60_42_36_00"), and the one before.
    MapChanged {
        new: String,
        old: String,
    },
    CharacterKilled(Listed),
    CharacterSpawned(Listed),
    ItemGained {
        item: u32,
        count: u32,
    },
    ItemLost {
        item: u32,
        count: u32,
    },
    EffectApplied(i32),
    EffectExpired(i32),
    /// The level rose: the new and old level, the attributes that rose (name, new value), and the runes
    /// that went at the same time (its cost).
    LevelUp {
        level: i32,
        old: i32,
        raised: Vec<(&'static str, i32)>,
        runes: i32,
    },
}

impl Fired {
    pub fn name(&self) -> &'static str {
        match self {
            Fired::PlayerDied => "player_died",
            Fired::MapChanged { .. } => "map_changed",
            Fired::CharacterKilled(_) => "character_killed",
            Fired::CharacterSpawned(_) => "character_spawned",
            Fired::ItemGained { .. } => "item_gained",
            Fired::ItemLost { .. } => "item_lost",
            Fired::EffectApplied(_) => "effect_applied",
            Fired::EffectExpired(_) => "effect_expired",
            Fired::LevelUp { .. } => "level_up",
        }
    }
}

/// A map block's name: area, block, region and index ("m60_42_36_00").
pub fn map_name(map: u32) -> String {
    format!("m{:02}_{:02}_{:02}_{:02}", map >> 24, (map >> 16) & 0xFF, (map >> 8) & 0xFF, map & 0xFF)
}

#[derive(Default)]
pub struct Tracker {
    player_alive: Option<bool>,
    map: Option<u32>,
    hp: Option<HashMap<i64, i32>>,
    inventory: Option<HashMap<u32, u32>>,
    effects: Option<HashSet<i32>>,
    /// Level, attributes and runes held, last frame.
    level: Option<LevelState>,
}

impl Tracker {
    /// The player left the world: everything starts over but the map, so a load into another map (a
    /// warp) is a map change.
    pub fn reset(&mut self) {
        *self = Self { map: self.map, ..Self::default() };
    }

    /// Compares the game's state with the last frame's: the events that happened (of those in `wants`).
    pub fn poll(&mut self, frame: u64, wants: &HashSet<&'static str>) -> Vec<Fired> {
        let mut out = Vec::new();
        let scan = frame.is_multiple_of(SCAN_EVERY);
        let (killed, spawned) = (wants.contains("character_killed"), wants.contains("character_spawned"));

        if wants.contains("player_died")
            && let Ok(hp) = crate::player::get("hp")
        {
            let alive = hp > 0;
            if self.player_alive == Some(true) && !alive {
                out.push(Fired::PlayerDied);
            }
            self.player_alive = Some(alive);
        }

        if wants.contains("map_changed")
            && let Some(map) = crate::world::player_map()
        {
            if let Some(old) = self.map
                && old != map
            {
                out.push(Fired::MapChanged { new: map_name(map), old: map_name(old) });
            }
            self.map = Some(map);
        }

        if scan
            && (killed || spawned)
            && let Ok(all) = crate::world::characters()
        {
            let mut now = HashMap::with_capacity(all.len());
            for c in all {
                if c.is_player || c.kind == "helper" {
                    continue;
                }
                let before = self.hp.as_ref().map(|m| m.get(&c.handle).copied());
                now.insert(c.handle, c.hp);
                // A character not there at the last scan (none on the first: the area's characters as
                // the player arrives aren't news).
                if spawned && before == Some(None) && c.hp > 0 {
                    out.push(Fired::CharacterSpawned(c));
                } else if killed && before.flatten().is_some_and(|hp| hp > 0) && c.hp <= 0 {
                    out.push(Fired::CharacterKilled(c));
                }
            }
            self.hp = Some(now);
        }

        if scan
            && (wants.contains("item_gained") || wants.contains("item_lost"))
            && let Ok(held) = crate::items::held()
        {
            let mut now: HashMap<u32, u32> = HashMap::new();
            for (item, qty) in held {
                *now.entry(item).or_default() += qty;
            }
            if let Some(prev) = &self.inventory {
                let items: HashSet<u32> = prev.keys().chain(now.keys()).copied().collect();
                for item in items {
                    let (a, b) = (prev.get(&item).copied().unwrap_or(0), now.get(&item).copied().unwrap_or(0));
                    if b > a {
                        out.push(Fired::ItemGained { item, count: b - a });
                    } else if a > b {
                        out.push(Fired::ItemLost { item, count: a - b });
                    }
                }
            }
            self.inventory = Some(now);
        }

        if scan
            && (wants.contains("effect_applied") || wants.contains("effect_expired"))
            && let Ok(list) = crate::world::effects(crate::world::PLAYER as i64)
        {
            let now: HashSet<i32> = list.into_iter().collect();
            if let Some(prev) = &self.effects {
                out.extend(now.difference(prev).map(|&id| Fired::EffectApplied(id)));
                out.extend(prev.difference(&now).map(|&id| Fired::EffectExpired(id)));
            }
            self.effects = Some(now);
        }

        if wants.contains("level_up")
            && let Some(now) = level_state()
        {
            if let Some((old, attrs, runes)) = &self.level
                && now.0 > *old
            {
                let raised = now.1.iter().zip(attrs).filter(|(n, o)| n.1 > o.1).map(|(n, _)| *n).collect();
                out.push(Fired::LevelUp { level: now.0, old: *old, raised, runes: (runes - now.2).max(0) });
            }
            self.level = Some(now);
        }
        out
    }
}

/// The level, the attributes and the runes held.
type LevelState = (i32, Vec<(&'static str, i32)>, i32);

fn level_state() -> Option<LevelState> {
    Some((crate::player::level().ok()?, crate::player::stats().ok()?, crate::player::runes().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_are_named_as_their_files() {
        assert_eq!(map_name(0x3C2A_2400), "m60_42_36_00");
        assert_eq!(map_name(0x0A00_0000), "m10_00_00_00");
    }
}
