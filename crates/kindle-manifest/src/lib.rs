//! `mod.toml`: a Kindle mod's manifest (identity, permissions, dependencies, settings).
//! Shared by the runtime, which loads mods, and the CLI, which checks and packs them.

use std::path::Path;

use serde::Deserialize;

mod toml_line;
pub use toml_line::set_toml_value;

#[derive(Debug, Clone, Deserialize)]
pub struct ModManifest {
    #[serde(rename = "mod")]
    pub info: ModInfo,
    #[serde(default)]
    pub permissions: Permissions,
    /// `[settings.<name>]` tables: options players change in the overlay, in file order.
    #[serde(default, deserialize_with = "settings_from_toml")]
    pub settings: Vec<Setting>,
}

/// A mod setting declared in mod.toml.
#[derive(Debug, Clone)]
pub struct Setting {
    pub name: String,
    pub label: String,
    pub description: String,
    pub kind: SettingKind,
    pub default: SettingValue,
}

#[derive(Debug, Clone)]
pub enum SettingKind {
    Bool,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Choice(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Choice(String),
}

impl Setting {
    /// `value` as this setting's type: numbers convert between int and float, ranges clamp,
    /// choices must be listed.
    pub fn check(&self, value: SettingValue) -> Result<SettingValue, String> {
        use SettingValue as V;
        match (&self.kind, value) {
            (SettingKind::Bool, V::Bool(b)) => Ok(V::Bool(b)),
            (SettingKind::Int { min, max }, V::Int(i)) => Ok(V::Int(i.clamp(*min, *max))),
            (SettingKind::Int { min, max }, V::Float(f)) if f.fract() == 0.0 => Ok(V::Int((f as i64).clamp(*min, *max))),
            (SettingKind::Float { min, max }, V::Float(f)) if f.is_finite() => Ok(V::Float(f.clamp(*min, *max))),
            (SettingKind::Float { min, max }, V::Int(i)) => Ok(V::Float((i as f64).clamp(*min, *max))),
            (SettingKind::Choice(list), V::Choice(c)) if list.contains(&c) => Ok(V::Choice(c)),
            (SettingKind::Choice(list), V::Choice(c)) => Err(format!("{c:?} isn't one of {}", list.join(", "))),
            (kind, v) => Err(format!("{v:?} doesn't fit a {} setting", kind.name())),
        }
    }
}

impl SettingKind {
    pub fn name(&self) -> &'static str {
        match self {
            SettingKind::Bool => "bool",
            SettingKind::Int { .. } => "int",
            SettingKind::Float { .. } => "float",
            SettingKind::Choice(_) => "choice",
        }
    }
}

impl SettingValue {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            SettingValue::Bool(b) => serde_json::Value::Bool(*b),
            SettingValue::Int(i) => serde_json::Value::from(*i),
            SettingValue::Float(f) => serde_json::Number::from_f64(*f).map(serde_json::Value::Number).unwrap_or(serde_json::Value::Null),
            SettingValue::Choice(c) => serde_json::Value::String(c.clone()),
        }
    }

    pub fn from_json(v: &serde_json::Value) -> Option<SettingValue> {
        Some(match v {
            serde_json::Value::Bool(b) => SettingValue::Bool(*b),
            serde_json::Value::Number(n) => match n.as_i64() {
                Some(i) => SettingValue::Int(i),
                None => SettingValue::Float(n.as_f64()?),
            },
            serde_json::Value::String(s) => SettingValue::Choice(s.clone()),
            _ => return None,
        })
    }

    fn from_toml(v: &toml::Value) -> Option<SettingValue> {
        Some(match v {
            toml::Value::Boolean(b) => SettingValue::Bool(*b),
            toml::Value::Integer(i) => SettingValue::Int(*i),
            toml::Value::Float(f) => SettingValue::Float(*f),
            toml::Value::String(s) => SettingValue::Choice(s.clone()),
            _ => return None,
        })
    }
}

fn settings_from_toml<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Setting>, D::Error> {
    use serde::de::Error;
    let table = toml::Table::deserialize(d)?;
    table.iter().map(|(name, v)| parse_setting(name, v).map_err(|e| D::Error::custom(format!("settings.{name}: {e}")))).collect()
}

fn parse_setting(name: &str, v: &toml::Value) -> Result<Setting, String> {
    let t = v.as_table().ok_or("must be a table, e.g. [settings.my_option]")?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("names must be letters, digits or '_'".into());
    }
    let ty = t.get("type").and_then(|v| v.as_str()).ok_or("needs a type: \"bool\", \"int\", \"float\" or \"choice\"")?;
    let num = |key: &str| t.get(key).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)));
    let kind = match ty {
        "bool" => SettingKind::Bool,
        "int" => SettingKind::Int { min: num("min").map(|f| f as i64).unwrap_or(i64::MIN), max: num("max").map(|f| f as i64).unwrap_or(i64::MAX) },
        "float" => SettingKind::Float { min: num("min").unwrap_or(f64::MIN), max: num("max").unwrap_or(f64::MAX) },
        "choice" => {
            let list: Vec<String> =
                t.get("choices").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect()).unwrap_or_default();
            if list.is_empty() {
                return Err("a choice setting needs choices = [\"...\", ...]".into());
            }
            SettingKind::Choice(list)
        }
        other => return Err(format!("unknown type {other:?} (bool, int, float or choice)")),
    };
    let raw = t.get("default").ok_or("needs a default")?;
    let mut setting = Setting {
        name: name.to_string(),
        label: t.get("label").and_then(|v| v.as_str()).unwrap_or(name).to_string(),
        description: t.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        kind,
        default: SettingValue::Bool(false),
    };
    let default = SettingValue::from_toml(raw).ok_or("the default must be a boolean, number or string")?;
    setting.default = setting.check(default).map_err(|e| format!("default: {e}"))?;
    Ok(setting)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModInfo {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Mods that must be running first (loaded before this one).
    #[serde(default)]
    pub depends: Vec<String>,
    /// Mods this one can't run alongside.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// The games the mod is for, by id (GAMES); without it, Dark Souls III (DEFAULT_GAME). A game this
    /// Kindle doesn't know is allowed (a newer Kindle may run in it).
    pub games: Option<Vec<String>>,
    /// The Kindle API version the mod was written for (see API_VERSION).
    pub api: Option<u32>,
}

fn yes() -> bool {
    true
}

impl ModInfo {
    /// Whether the mod is for the game with this id: one its `games` lists, or Dark Souls III if it
    /// lists none.
    pub fn for_game(&self, game: &str) -> bool {
        match &self.games {
            Some(games) => games.iter().any(|g| g == game),
            None => game == DEFAULT_GAME,
        }
    }

    /// The games the mod is for, by name ("Dark Souls III, Elden Ring").
    pub fn game_names(&self) -> String {
        match &self.games {
            Some(games) => games.iter().map(|g| game_name(g)).collect::<Vec<_>>().join(", "),
            None => game_name(DEFAULT_GAME).to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Permissions {
    /// Which `kindle.*` game modules the mod may use: flags, items, player, world, params, text, ui, debug.
    #[serde(default)]
    pub modules: Vec<String>,
}

/// The Lua API's version. It goes up when a change could break existing mods (a function
/// removed or behaving differently); additions don't change it. A mod declares the version
/// it was written for (`api` in mod.toml) and won't load on a Kindle with an older one.
pub const API_VERSION: u32 = 1;

/// Game modules a mod can request. `log`, events and timers are always available.
/// `debug` (read-only memory inspection) is meant for reverse-engineering tools.
pub const GAME_MODULES: &[&str] = &["flags", "items", "player", "world", "params", "maps", "text", "ui", "hks", "events", "debug", "game", "net", "input"];

/// The games Kindle runs in: the id a mod's `games` names each by, and its name.
pub const GAMES: &[(&str, &str)] = &[("ds3", "Dark Souls III"), ("er", "Elden Ring")];

/// The game a mod is for when its mod.toml names none: the mods written before Kindle ran in a
/// second game are all Dark Souls III's.
pub const DEFAULT_GAME: &str = "ds3";

/// A game's name from its id (the id itself, for a game this Kindle doesn't know).
pub fn game_name(id: &str) -> &str {
    GAMES.iter().find(|(g, _)| *g == id).map_or(id, |&(_, name)| name)
}

impl ModManifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let m: Self = toml::from_str(&s).map_err(|e| format!("mod.toml: {e}"))?;
        if m.info.id.is_empty() || !m.info.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("mod.toml: id {:?} must be letters, digits, '-' or '_'", m.info.id));
        }
        if let Some(api) = m.info.api
            && api > API_VERSION
        {
            return Err(format!("mod.toml: this mod needs Kindle API {api}, but this Kindle has API {API_VERSION}: update Kindle"));
        }
        if let Some(games) = &m.info.games {
            if games.is_empty() {
                return Err("mod.toml: games = [] names no game: list the games the mod is for, or leave games out for Dark Souls III".into());
            }
            for game in games {
                if game.is_empty() || !game.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
                    let ids: Vec<&str> = GAMES.iter().map(|(g, _)| *g).collect();
                    return Err(format!("mod.toml: {game:?} in games isn't a game's id (lowercase letters and digits: {})", ids.join(", ")));
                }
            }
        }
        for module in &m.permissions.modules {
            if !GAME_MODULES.contains(&module.as_str()) {
                return Err(format!("mod.toml: unknown module {module:?} in permissions (available: {})", GAME_MODULES.join(", ")));
            }
        }
        Ok(m)
    }

    pub fn allows(&self, module: &str) -> bool {
        self.permissions.modules.iter().any(|m| m == module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str, toml: &str) -> Result<ModManifest, String> {
        let dir = std::env::temp_dir().join(format!("kindle-manifest-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mod.toml");
        std::fs::write(&path, toml).unwrap();
        let m = ModManifest::load(&path);
        let _ = std::fs::remove_dir_all(dir);
        m
    }

    #[test]
    fn a_mod_without_games_is_dark_souls_iiis() {
        let m = load("none", "[mod]\nid = \"a\"\n").unwrap();
        assert!(m.info.for_game("ds3") && !m.info.for_game("er"));
        assert_eq!(m.info.game_names(), "Dark Souls III");
    }

    #[test]
    fn games_lists_the_games_a_mod_is_for() {
        let m = load("er", "[mod]\nid = \"a\"\ngames = [\"er\"]\n").unwrap();
        assert!(m.info.for_game("er") && !m.info.for_game("ds3"));
        // A game this Kindle doesn't know goes by its id.
        let m = load("three", "[mod]\nid = \"a\"\ngames = [\"ds3\", \"er\", \"nr\"]\n").unwrap();
        assert!(m.info.for_game("ds3") && m.info.for_game("er"));
        assert_eq!(m.info.game_names(), "Dark Souls III, Elden Ring, nr");
    }

    #[test]
    fn games_must_name_games() {
        let e = load("empty", "[mod]\nid = \"a\"\ngames = []\n").unwrap_err();
        assert!(e.contains("names no game"), "{e}");
        let e = load("name", "[mod]\nid = \"a\"\ngames = [\"Elden Ring\"]\n").unwrap_err();
        assert!(e.contains("isn't a game's id"), "{e}");
    }
}
