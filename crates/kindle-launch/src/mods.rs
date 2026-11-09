//! The Kindle mods installed next to kindle.exe, which of them a start runs, and their
//! settings: the same files the in-game Mods screen writes (`data/disabled.json`, and
//! `data/<mod id>/settings.json`).
//!
//! A start runs its profile's Kindle mods (the other installed ones are off), and remembers
//! which profile it was (`data/launched.json`). Mods switched on or off in game go back into
//! that profile once the game has closed (see `load_synced`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kindle_manifest::{ModManifest, Setting, SettingValue};
use serde_json::{Map, Value};

/// The game Kindle runs in when this starts it (a mod's `games` in mod.toml).
const GAME: &str = "ds3";

pub struct KindleMod {
    pub folder: PathBuf,
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    /// Switched off in the launcher or the game.
    pub disabled: bool,
    /// `enabled = false` in its mod.toml (only editing that turns it on).
    pub off_in_toml: bool,
    /// The games it's for, when its mod.toml's `games` leaves out Dark Souls III (it doesn't run).
    pub other_game: Option<String>,
    pub settings: Vec<ModSetting>,
    /// Its mod.toml couldn't be read.
    pub error: Option<String>,
}

pub struct ModSetting {
    pub decl: Setting,
    /// The saved value if it still fits the declaration, else the default.
    pub value: SettingValue,
}

fn settings_path(kindle_dir: &Path, id: &str) -> PathBuf {
    kindle_dir.join("data").join(id).join("settings.json")
}

/// settings.json's document and its saved values.
fn read_settings(path: &Path) -> (Map<String, Value>, Map<String, Value>) {
    let mut doc = match std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        Some(Value::Object(o)) => o,
        _ => Map::new(),
    };
    let values = match doc.remove("values") {
        Some(Value::Object(v)) => v,
        _ => Map::new(),
    };
    (doc, values)
}

fn mod_settings(kindle_dir: &Path, id: &str, decls: Vec<Setting>) -> Vec<ModSetting> {
    let (_, saved) = read_settings(&settings_path(kindle_dir, id));
    decls
        .into_iter()
        .map(|decl| {
            let value = saved.get(&decl.name).and_then(SettingValue::from_json).and_then(|v| decl.check(v).ok()).unwrap_or_else(|| decl.default.clone());
            ModSetting { decl, value }
        })
        .collect()
}

/// Changes a mod's setting (checked against its declaration). Only while the game is closed:
/// a running game keeps its own copy and writes it back.
pub fn set_setting(kindle_dir: &Path, id: &str, decl: &Setting, value: SettingValue) -> Result<(), String> {
    let value = decl.check(value)?;
    let path = settings_path(kindle_dir, id);
    let (mut doc, mut values) = read_settings(&path);
    values.insert(decl.name.clone(), value.to_json());
    doc.insert("values".into(), Value::Object(values));
    let text = serde_json::to_string_pretty(&Value::Object(doc)).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
}

fn disabled_path(kindle_dir: &Path) -> PathBuf {
    kindle_dir.join("data").join("disabled.json")
}

fn read_disabled(kindle_dir: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(disabled_path(kindle_dir))
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

/// Every folder in `kindle/mods`, by folder name.
pub fn kindle_mods(kindle_dir: &Path) -> Vec<KindleMod> {
    let disabled = read_disabled(kindle_dir);
    let Ok(entries) = std::fs::read_dir(kindle_dir.join("mods")) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs.into_iter()
        .map(|folder| {
            let fallback = folder.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            match ModManifest::load(&folder.join("mod.toml")) {
                Ok(m) => KindleMod {
                    disabled: disabled.contains(&m.info.id),
                    name: m.info.name.clone().unwrap_or_else(|| m.info.id.clone()),
                    version: m.info.version.clone().unwrap_or_default(),
                    author: m.info.author.clone().unwrap_or_default(),
                    description: m.info.description.clone().unwrap_or_default(),
                    off_in_toml: !m.info.enabled,
                    other_game: (!m.info.for_game(GAME)).then(|| m.info.game_names()),
                    settings: mod_settings(kindle_dir, &m.info.id, m.settings),
                    id: m.info.id,
                    error: None,
                    folder,
                },
                Err(e) => KindleMod {
                    folder,
                    id: fallback.clone(),
                    name: fallback,
                    version: String::new(),
                    author: String::new(),
                    description: String::new(),
                    disabled: false,
                    off_in_toml: false,
                    other_game: None,
                    settings: Vec::new(),
                    error: Some(e),
                },
            }
        })
        .collect()
}

/// The ids of the installed mods that run now (not switched off, not off in mod.toml, not for
/// another game).
pub(crate) fn running_ids(kindle_dir: &Path) -> Vec<String> {
    kindle_mods(kindle_dir).into_iter().filter(|m| m.error.is_none() && !m.disabled && !m.off_in_toml && m.other_game.is_none()).map(|m| m.id).collect()
}

/// A profile's mod list after a game session: the session's running mods, plus the profile's
/// mods that aren't installed any more (kept in case they come back).
pub(crate) fn after_session(kindle_dir: &Path, profile: &[String]) -> Vec<String> {
    let installed: BTreeSet<String> = kindle_mods(kindle_dir).into_iter().filter(|m| m.error.is_none()).map(|m| m.id).collect();
    let running = running_ids(kindle_dir);
    let mut out: Vec<String> = profile.iter().filter(|id| !installed.contains(*id) || running.contains(id)).cloned().collect();
    out.extend(running.into_iter().filter(|id| !profile.contains(id)));
    out
}

/// Switches on exactly `ids` of the installed mods, for the next start.
pub(crate) fn apply_profile(kindle_dir: &Path, ids: &[String]) -> Result<(), String> {
    let disabled: BTreeSet<String> = kindle_mods(kindle_dir).into_iter().filter(|m| m.error.is_none() && !ids.contains(&m.id)).map(|m| m.id).collect();
    write_disabled(kindle_dir, &disabled)
}

fn session_path(kindle_dir: &Path) -> PathBuf {
    kindle_dir.join("data").join("launched.json")
}

/// Remembers that the game was started with `profile`.
pub(crate) fn begin_session(kindle_dir: &Path, profile: &str) -> Result<(), String> {
    let path = session_path(kindle_dir);
    let text = serde_json::json!({ "profile": profile }).to_string();
    path.parent().map(std::fs::create_dir_all).transpose().and_then(|_| std::fs::write(&path, text)).map_err(|e| format!("{}: {e}", path.display()))
}

/// The profile of the last start, if not yet taken (the file is removed).
pub(crate) fn take_session(kindle_dir: &Path) -> Option<String> {
    let path = session_path(kindle_dir);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    serde_json::from_str::<Value>(&text).ok()?.get("profile")?.as_str().map(String::from)
}

fn write_disabled(kindle_dir: &Path, disabled: &BTreeSet<String>) -> Result<(), String> {
    let path = disabled_path(kindle_dir);
    let list: Vec<&String> = disabled.iter().collect();
    path.parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| std::fs::write(&path, serde_json::to_string_pretty(&list).unwrap_or_default()))
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_mods_and_remembers_what_is_off() {
        let dir = std::env::temp_dir().join(format!("kindle-mods-test-{}", std::process::id()));
        let m = dir.join("mods").join("demo");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("mod.toml"), "[mod]\nid = \"demo\"\nname = \"Demo\"\nversion = \"1.0\"\n").unwrap();
        std::fs::create_dir_all(dir.join("mods").join("broken")).unwrap();
        let mods = kindle_mods(&dir);
        assert_eq!(mods.len(), 2);
        assert!(mods[0].error.is_some(), "a folder without mod.toml is listed with its error");
        assert_eq!((mods[1].name.as_str(), mods[1].disabled), ("Demo", false));
        apply_profile(&dir, &[]).unwrap();
        assert!(kindle_mods(&dir)[1].disabled);
        apply_profile(&dir, &["demo".into()]).unwrap();
        assert!(!kindle_mods(&dir)[1].disabled);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_mod_for_another_game_is_listed_but_does_not_run() {
        let dir = std::env::temp_dir().join(format!("kindle-games-test-{}", std::process::id()));
        for (id, games) in [("ds3-mod", ""), ("er-mod", "games = [\"er\"]\n")] {
            let m = dir.join("mods").join(id);
            std::fs::create_dir_all(&m).unwrap();
            std::fs::write(m.join("mod.toml"), format!("[mod]\nid = \"{id}\"\n{games}")).unwrap();
        }
        let mods = kindle_mods(&dir);
        assert_eq!((mods[0].other_game.as_deref(), mods[1].other_game.as_deref()), (None, Some("Elden Ring")));
        assert_eq!(running_ids(&dir), vec!["ds3-mod".to_string()]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_session_s_changes_go_back_into_the_profile() {
        let dir = std::env::temp_dir().join(format!("kindle-session-test-{}", std::process::id()));
        for id in ["a", "b"] {
            let m = dir.join("mods").join(id);
            std::fs::create_dir_all(&m).unwrap();
            std::fs::write(m.join("mod.toml"), format!("[mod]\nid = \"{id}\"\n")).unwrap();
        }
        let profile = vec!["a".to_string(), "gone".to_string()];
        apply_profile(&dir, &profile).unwrap();
        assert_eq!(running_ids(&dir), vec!["a".to_string()]);
        // In game: a switched off, b switched on.
        write_disabled(&dir, &["a".to_string()].into()).unwrap();
        assert_eq!(after_session(&dir, &profile), vec!["gone".to_string(), "b".to_string()]);
        begin_session(&dir, "rando").unwrap();
        assert_eq!(take_session(&dir).as_deref(), Some("rando"));
        assert_eq!(take_session(&dir), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn settings_read_and_write_the_game_s_file() {
        let dir = std::env::temp_dir().join(format!("kindle-settings-test-{}", std::process::id()));
        let m = dir.join("mods").join("demo");
        std::fs::create_dir_all(&m).unwrap();
        std::fs::write(m.join("mod.toml"), "[mod]\nid = \"demo\"\n[settings.speed]\ntype = \"int\"\ndefault = 5\nmin = 1\nmax = 10\n").unwrap();
        let s = &kindle_mods(&dir)[0].settings[0];
        assert_eq!(s.value, SettingValue::Int(5));
        set_setting(&dir, "demo", &s.decl, SettingValue::Int(50)).unwrap();
        assert_eq!(kindle_mods(&dir)[0].settings[0].value, SettingValue::Int(10), "clamped to the range");
        let text = std::fs::read_to_string(dir.join("data").join("demo").join("settings.json")).unwrap();
        assert!(text.contains("\"values\""), "the runtime's store layout");
        let _ = std::fs::remove_dir_all(dir);
    }
}
