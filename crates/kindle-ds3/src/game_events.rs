//! Game events derived by comparing game state between frames.
//!
//! Only the events some mod subscribes to are tracked; whole-world scans (characters,
//! inventory, effects) run every few frames. Baselines reset whenever the player leaves the
//! world, so loading a save never fires a burst of stale events.

use std::collections::{HashMap, HashSet};

use crate::chars::{self, Character};
use crate::{game, memory};

/// How often (in frames) the heavier scans run.
const SCAN_EVERY: u64 = 5;

pub const EVENTS: &[&str] = &[
    "player_died",
    "region_changed",
    "character_killed",
    "character_spawned",
    "item_gained",
    "item_lost",
    "effect_applied",
    "effect_expired",
    "level_up",
    "equipment_changed",
    "spells_changed",
    "souls_changed",
    "bonfire_lit",
    "menu_opened",
    "menu_closed",
    "ember_changed",
    "key_pressed",
];

pub enum Fired {
    PlayerDied,
    RegionChanged {
        new: i32,
        old: i32,
    },
    CharacterKilled(Character),
    CharacterSpawned(Character),
    ItemGained {
        item: i64,
        count: u32,
        name: Option<String>,
    },
    ItemLost {
        item: i64,
        count: u32,
        name: Option<String>,
    },
    EffectApplied(i32),
    EffectExpired(i32),
    /// The soul level rose: new and old level, the attributes that rose (name, new value), and
    /// the souls that went at the same time (the level's cost).
    LevelUp {
        level: i32,
        old: i32,
        raised: Vec<(&'static str, i32)>,
        souls: i32,
    },
    /// What a slot holds changed: the slot, the item now (None: emptied) and before.
    EquipmentChanged {
        slot: &'static str,
        item: Option<i64>,
        old: Option<i64>,
    },
    /// The attuned spells changed (at a bonfire's Attune Magic, or a ring that adds slots).
    SpellsChanged {
        spells: Vec<i64>,
        old: Vec<i64>,
    },
    /// The souls held changed: now and before.
    SoulsChanged {
        souls: i32,
        old: i32,
    },
    /// A bonfire was lit for the first time (its flag set).
    BonfireLit(crate::bonfire::Bonfire),
    /// One of the game's menus opened or closed: "esc" (the Esc menu) or "menu" (any other: a
    /// bonfire's, a talk's, a shop, the level up screen...).
    MenuOpened(&'static str),
    MenuClosed(&'static str),
    /// The player was embered or lost it.
    EmberChanged(bool),
}

impl Fired {
    pub fn name(&self) -> &'static str {
        match self {
            Fired::PlayerDied => "player_died",
            Fired::RegionChanged { .. } => "region_changed",
            Fired::CharacterKilled(_) => "character_killed",
            Fired::CharacterSpawned(_) => "character_spawned",
            Fired::ItemGained { .. } => "item_gained",
            Fired::ItemLost { .. } => "item_lost",
            Fired::EffectApplied(_) => "effect_applied",
            Fired::EffectExpired(_) => "effect_expired",
            Fired::LevelUp { .. } => "level_up",
            Fired::EquipmentChanged { .. } => "equipment_changed",
            Fired::SpellsChanged { .. } => "spells_changed",
            Fired::SoulsChanged { .. } => "souls_changed",
            Fired::BonfireLit(_) => "bonfire_lit",
            Fired::MenuOpened(_) => "menu_opened",
            Fired::MenuClosed(_) => "menu_closed",
            Fired::EmberChanged(_) => "ember_changed",
        }
    }
}

#[derive(Default)]
pub struct Tracker {
    player_alive: Option<bool>,
    region: Option<i32>,
    chr_hp: Option<HashMap<u32, i32>>,
    inventory: Option<HashMap<(&'static str, u32), u32>>,
    effects: Option<HashSet<i32>>,
    /// Soul level, attributes and souls held, last frame.
    level: Option<(i32, [i32; 9], i32)>,
    equipment: Option<HashMap<&'static str, i64>>,
    spells: Option<Vec<i64>>,
    souls: Option<i32>,
    /// The bonfires lit, by entity id.
    lit: Option<HashSet<i32>>,
    /// The menu open last frame, if any ("esc" or "menu").
    menu: Option<Option<&'static str>>,
    embered: Option<bool>,
}

/// The player's play region id (`PlayerIns + 0x1ABC`, from the Grand Archives CE table).
pub fn play_region() -> Option<i32> {
    let chr = game::player_chr_addr().ok()?;
    memory::read::<i32>(chr + 0x1ABC)
}

impl Tracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Compares current state with the last frame and returns the events that happened.
    /// `wants` says which events have subscribers.
    pub fn poll(&mut self, frame: u64, wants: &HashSet<&'static str>) -> Vec<Fired> {
        let mut out = Vec::new();
        let scan = frame.is_multiple_of(SCAN_EVERY);

        if wants.contains("player_died")
            && let Ok(d) = game::chr_data()
        {
            let alive = d.hp > 0;
            if self.player_alive == Some(true) && !alive {
                out.push(Fired::PlayerDied);
            }
            self.player_alive = Some(alive);
        }

        if wants.contains("region_changed")
            && let Some(r) = play_region()
        {
            if let Some(old) = self.region
                && old != r
            {
                out.push(Fired::RegionChanged { new: r, old });
            }
            self.region = Some(r);
        }

        let (killed, spawned) = (wants.contains("character_killed"), wants.contains("character_spawned"));
        if scan
            && (killed || spawned)
            && let Ok(all) = chars::all()
        {
            let mut now = HashMap::with_capacity(all.len());
            for c in all {
                if c.is_player || c.kind() == "helper" {
                    continue;
                }
                let before = self.chr_hp.as_ref().map(|m| m.get(&c.handle).copied());
                now.insert(c.handle, c.hp);
                // A character not there at the last scan (none on the first scan: the area's
                // characters as the player arrives aren't news).
                if spawned && before == Some(None) && c.hp > 0 {
                    out.push(Fired::CharacterSpawned(c));
                } else if killed && before.flatten().is_some_and(|hp| hp > 0) && c.hp <= 0 {
                    out.push(Fired::CharacterKilled(c));
                }
            }
            self.chr_hp = Some(now);
        }

        if scan
            && (wants.contains("item_gained") || wants.contains("item_lost"))
            && let Ok(items) = game::inventory()
        {
            let mut now: HashMap<(&'static str, u32), u32> = HashMap::new();
            for (cat, id, qty) in items {
                *now.entry((cat, id)).or_default() += qty;
            }
            if let Some(prev) = &self.inventory {
                let keys: HashSet<_> = prev.keys().chain(now.keys()).copied().collect();
                for key in keys {
                    let (a, b) = (prev.get(&key).copied().unwrap_or(0), now.get(&key).copied().unwrap_or(0));
                    if a == b {
                        continue;
                    }
                    let Some((item, name)) = item_of(key) else { continue };
                    if b > a {
                        out.push(Fired::ItemGained { item, count: b - a, name });
                    } else {
                        out.push(Fired::ItemLost { item, count: a - b, name });
                    }
                }
            }
            self.inventory = Some(now);
        }

        if scan
            && (wants.contains("effect_applied") || wants.contains("effect_expired"))
            && let Ok(list) = game::player_effects()
        {
            let now: HashSet<i32> = list.iter().filter(|e| e.duration < 0.0 || e.time_left > 0.0).map(|e| e.id).collect();
            if let Some(prev) = &self.effects {
                for id in now.difference(prev) {
                    out.push(Fired::EffectApplied(*id));
                }
                for id in prev.difference(&now) {
                    out.push(Fired::EffectExpired(*id));
                }
            }
            self.effects = Some(now);
        }

        if wants.contains("level_up")
            && let Some(now) = level_state()
        {
            if let Some((old, attrs, souls)) = self.level
                && now.0 > old
            {
                let raised = game::ATTRIBUTES.iter().enumerate().filter(|&(i, _)| now.1[i] > attrs[i]).map(|(i, (name, _))| (*name, now.1[i])).collect();
                out.push(Fired::LevelUp { level: now.0, old, raised, souls: (souls - now.2).max(0) });
            }
            self.level = Some(now);
        }

        if scan
            && wants.contains("equipment_changed")
            && let Ok(list) = game::equipment()
        {
            let now: HashMap<&'static str, i64> = list.into_iter().collect();
            if let Some(prev) = &self.equipment {
                for &(slot, _, _) in game::EQUIP_SLOTS {
                    let (old, item) = (prev.get(slot).copied(), now.get(slot).copied());
                    if old != item {
                        out.push(Fired::EquipmentChanged { slot, item, old });
                    }
                }
            }
            self.equipment = Some(now);
        }

        if scan
            && wants.contains("spells_changed")
            && let Ok(now) = game::spells()
        {
            if let Some(old) = &self.spells
                && *old != now
            {
                out.push(Fired::SpellsChanged { spells: now.clone(), old: old.clone() });
            }
            self.spells = Some(now);
        }

        if wants.contains("souls_changed")
            && let Ok(souls) = game::pgd_read_i32(0x74)
        {
            if let Some(old) = self.souls
                && old != souls
            {
                out.push(Fired::SoulsChanged { souls, old });
            }
            self.souls = Some(souls);
        }

        // Bonfires are lit rarely and reading them all reads their flags: twice a second.
        if frame.is_multiple_of(30)
            && wants.contains("bonfire_lit")
            && let Ok(all) = crate::bonfire::all()
        {
            let now: HashSet<i32> = all.iter().filter(|b| b.lit).map(|b| b.entity_id).collect();
            if let Some(prev) = &self.lit {
                for b in all.into_iter().filter(|b| b.lit && !prev.contains(&b.entity_id)) {
                    out.push(Fired::BonfireLit(b));
                }
            }
            self.lit = Some(now);
        }

        if wants.contains("menu_opened") || wants.contains("menu_closed") {
            let mut now = open_menu();
            // The Esc menu's request clears a few frames before its dialog closes: still the Esc menu.
            if self.menu == Some(Some("esc")) && now == Some("menu") {
                now = Some("esc");
            }
            if let Some(prev) = self.menu
                && prev != now
            {
                if let Some(kind) = prev {
                    out.push(Fired::MenuClosed(kind));
                }
                if let Some(kind) = now {
                    out.push(Fired::MenuOpened(kind));
                }
            }
            self.menu = Some(now);
        }

        if scan
            && wants.contains("ember_changed")
            && let Ok(now) = game::embered()
        {
            if let Some(old) = self.embered
                && old != now
            {
                out.push(Fired::EmberChanged(now));
            }
            self.embered = Some(now);
        }

        out
    }
}

/// Soul level, attributes (in game::ATTRIBUTES order) and souls held.
fn level_state() -> Option<(i32, [i32; 9], i32)> {
    let level = game::pgd_read_i32(0x70).ok()?;
    let mut attrs = [0; 9];
    for (i, (_, off)) in game::ATTRIBUTES.iter().enumerate() {
        attrs[i] = game::pgd_read_i32(*off).ok()?;
    }
    Some((level, attrs, game::pgd_read_i32(0x74).ok()?))
}

/// The game's menu open now: "esc" (the Esc menu), "menu" (any other dialog of its menu system), or
/// None.
fn open_menu() -> Option<&'static str> {
    if crate::native_ui::esc_menu_open() {
        Some("esc")
    } else if crate::native_ui::menu_open() {
        Some("menu")
    } else {
        None
    }
}

fn item_of((cat, id): (&'static str, u32)) -> Option<(i64, Option<String>)> {
    let kind = crate::text::Kind::parse(cat)?;
    let raw = game::make_item(kind.category(), id as i64).ok()?;
    let name = game::item_id(raw).ok().and_then(crate::lua_values::display_name);
    Some((raw, name))
}
