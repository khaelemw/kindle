//! Save safety. Mods can change a character for good (items, flags, stats), so Kindle:
//! - copies the game's save files to `<kindle>/backups/<date time>/` when it starts loading
//!   mods, if they changed since the newest backup, keeping the newest `runtime.save_backups`
//!   (each backup also lists the mods that were installed, in `mods.txt`);
//! - remembers which mods have run with each character (`<kindle>/data/characters.json`),
//!   shown in the overlay's Mods tab.
//!
//! Saves are `%APPDATA%\DarkSoulsIII\<Steam id>\DS3*.sl2` (`.co2` with Seamless Co-op).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

const SAVE_EXTENSIONS: &[&str] = &["sl2", "co2"];

/// The game's save folders (one per Steam account).
fn save_dirs() -> Vec<PathBuf> {
    let Some(appdata) = std::env::var_os("APPDATA") else { return Vec::new() };
    let root = PathBuf::from(appdata).join("DarkSoulsIII");
    let Ok(entries) = std::fs::read_dir(&root) else { return Vec::new() };
    entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect()
}

/// (path relative to the backup folder, source path) of every save file.
fn save_files() -> Vec<(PathBuf, PathBuf)> {
    let mut out = Vec::new();
    for dir in save_dirs() {
        let Some(account) = dir.file_name().map(PathBuf::from) else { continue };
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for path in entries.flatten().map(|e| e.path()) {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            if path.is_file()
                && SAVE_EXTENSIONS.contains(&ext.as_str())
                && let Some(name) = path.file_name()
            {
                out.push((account.join(name), path));
            }
        }
    }
    out.sort();
    out
}

/// Local time as `2026-10-03_19-40-12` (sorts by age).
pub(crate) fn timestamp() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Backup folders, oldest first.
fn backups(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs
}

fn same_contents(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => {}
        _ => return false,
    }
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

/// Backs the saves up (on a background thread) unless they match the newest backup, then
/// keeps only the newest `keep` backups. `mods` is written to the backup's `mods.txt`.
pub fn backup(kindle_dir: &Path, keep: usize, mods: Vec<String>) {
    if keep == 0 {
        return;
    }
    let root = kindle_dir.join("backups");
    let _ = std::thread::Builder::new().name("kindle-save-backup".into()).spawn(move || backup_now(&root, keep, &mods));
}

fn backup_now(root: &Path, keep: usize, mods: &[String]) {
    let files = save_files();
    if files.is_empty() {
        crate::info!("save backup: no saves found");
        return;
    }
    if let Some(newest) = backups(root).last()
        && files.iter().all(|(rel, src)| same_contents(src, &newest.join(rel)))
    {
        crate::info!("save backup: saves unchanged since {}", newest.display());
        return;
    }
    let dir = root.join(timestamp());
    for (rel, src) in &files {
        let dst = dir.join(rel);
        let copied = dst.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok()) && std::fs::copy(src, &dst).is_ok();
        if !copied {
            crate::warn!("save backup: could not copy {}", src.display());
            return;
        }
    }
    let mut list = String::from("Mods installed when this backup was made:\r\n");
    for m in mods {
        list.push_str(m);
        list.push_str("\r\n");
    }
    let _ = std::fs::write(dir.join("mods.txt"), list);
    crate::info!("save backup: {} file(s) copied to {}", files.len(), dir.display());

    let all = backups(root);
    for old in all.iter().take(all.len().saturating_sub(keep)) {
        if let Err(e) = std::fs::remove_dir_all(old) {
            crate::warn!("save backup: could not remove old backup {} ({e})", old.display());
        }
    }
}

// ── which mods each character has been played with ───────────────────────────────────

/// Records that `mods` (id -> version) ran with the character in `slot`, and returns every
/// mod that has run with it (sorted "id version"). A different character in the slot
/// starts a new record.
pub fn record_character(kindle_dir: &Path, slot: i32, character: &str, mods: &BTreeMap<String, String>) -> Vec<String> {
    let path = kindle_dir.join("data").join("characters.json");
    let mut all: Map<String, Value> =
        std::fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let key = format!("slot{slot}");
    let same = all.get(&key).and_then(|e| e.get("character")).and_then(Value::as_str) == Some(character);
    let mut seen: Map<String, Value> =
        if same { all.get(&key).and_then(|e| e.get("mods")).and_then(Value::as_object).cloned().unwrap_or_default() } else { Map::new() };
    let before = seen.clone();
    for (id, version) in mods {
        seen.insert(id.clone(), Value::String(version.clone()));
    }
    if !same || seen != before {
        let mut entry = Map::new();
        entry.insert("character".into(), Value::String(character.into()));
        entry.insert("mods".into(), Value::Object(seen.clone()));
        all.insert(key, Value::Object(entry));
        let written = path.parent().is_some_and(|p| std::fs::create_dir_all(p).is_ok())
            && serde_json::to_string_pretty(&Value::Object(all)).is_ok_and(|s| std::fs::write(&path, s).is_ok());
        if !written {
            crate::warn!("could not write {}", path.display());
        }
    }
    seen.iter().map(|(id, v)| format!("{id} {}", v.as_str().unwrap_or(""))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_record_accumulates_and_resets_for_a_new_character() {
        let dir = std::env::temp_dir().join(format!("kindle-saves-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let one = BTreeMap::from([("a".to_string(), "1.0".to_string())]);
        let two = BTreeMap::from([("b".to_string(), "0.2".to_string())]);
        assert_eq!(record_character(&dir, 0, "Ash", &one), ["a 1.0"]);
        assert_eq!(record_character(&dir, 0, "Ash", &two), ["a 1.0", "b 0.2"]);
        assert_eq!(record_character(&dir, 0, "Other", &two), ["b 0.2"]);
        assert_eq!(record_character(&dir, 1, "Ash", &one), ["a 1.0"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timestamps_sort_by_time() {
        let t = timestamp();
        assert_eq!(t.len(), 19);
        assert_eq!(&t[4..5], "-");
        assert_eq!(&t[10..11], "_");
    }
}
