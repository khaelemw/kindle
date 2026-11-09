//! Installing Kindle into the game's folder from an unzipped release: finding Dark Souls III
//! through Steam, and copying `dinput8.dll` and `kindle\` into its `Game` folder.

use std::path::{Path, PathBuf};

use crate::GAME_EXE;

/// Steam's id for Dark Souls III.
const APP_ID: &str = "374320";
/// The game's own folder under a Steam library, when its manifest doesn't say.
const INSTALL_DIR: &str = "DARK SOULS III";
/// Where a `dinput8.dll` that isn't Kindle's is kept when Kindle's replaces it.
const FOREIGN_DINPUT8: &str = "dinput8.dll.before-kindle";

/// The release a launcher was started from: the folder that holds `dinput8.dll` and
/// `kindle\kindle.dll` (the launcher may sit in it or in its `kindle\`). None for a launcher
/// already in a game's folder, or one with no release beside it.
pub fn payload(exe_dir: &Path) -> Option<PathBuf> {
    if installed_in_game(exe_dir) {
        return None;
    }
    let is_payload = |root: &Path| root.join("dinput8.dll").is_file() && root.join("kindle").join("kindle.dll").is_file();
    if is_payload(exe_dir) {
        return Some(exe_dir.to_path_buf());
    }
    let parent = exe_dir.parent()?;
    (exe_dir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("kindle")) && is_payload(parent)).then(|| parent.to_path_buf())
}

/// Whether `kindle_dir` is the `kindle` folder of a game's folder.
pub fn installed_in_game(kindle_dir: &Path) -> bool {
    kindle_dir.parent().is_some_and(|game| game.join(GAME_EXE).is_file())
}

/// The game's `Game` folder for a folder the user picked: the folder itself, its `Game`, or
/// `Game` of the game's folder in a Steam library.
pub fn game_dir_for(picked: &Path) -> Option<PathBuf> {
    [picked.to_path_buf(), picked.join("Game"), picked.join(INSTALL_DIR).join("Game"), picked.join("steamapps").join("common").join(INSTALL_DIR).join("Game")]
        .into_iter()
        .find(|d| d.join(GAME_EXE).is_file())
}

/// Dark Souls III's `Game` folder, found through Steam: its install folder (the registry), then
/// each of its libraries (libraryfolders.vdf) for the game's manifest or folder.
pub fn find_game() -> Option<PathBuf> {
    let steam = steam_dir()?;
    let mut libraries = vec![steam.clone()];
    if let Ok(text) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(library_paths(&text));
    }
    libraries.iter().find_map(|lib| {
        let apps = lib.join("steamapps");
        let dir = std::fs::read_to_string(apps.join(format!("appmanifest_{APP_ID}.acf")))
            .ok()
            .and_then(|m| vdf_value(&m, "installdir"))
            .unwrap_or_else(|| INSTALL_DIR.into());
        let game = apps.join("common").join(dir).join("Game");
        game.join(GAME_EXE).is_file().then_some(game)
    })
}

/// Steam's install folder, from the registry (the user's key, then the machine's).
fn steam_dir() -> Option<PathBuf> {
    use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::{HSTRING, PCWSTR};
    let read = |root: HKEY, key: &str, value: &str| -> Option<PathBuf> {
        let (key, value) = (HSTRING::from(key), HSTRING::from(value));
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        let ok =
            unsafe { RegGetValueW(root, PCWSTR(key.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) };
        if ok.is_err() {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]));
        path.is_dir().then_some(path)
    };
    read(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .or_else(|| read(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))
        .or_else(|| read(HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"))
}

/// The `"path"` of each library in Steam's libraryfolders.vdf.
fn library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines().filter_map(|line| vdf_pair(line).filter(|(k, _)| k.eq_ignore_ascii_case("path")).map(|(_, v)| PathBuf::from(v))).collect()
}

/// The first value of `key` in a VDF text.
fn vdf_value(vdf: &str, key: &str) -> Option<String> {
    vdf.lines().filter_map(vdf_pair).find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

/// A `"key"  "value"` line, with the value's `\\` unescaped.
fn vdf_pair(line: &str) -> Option<(String, String)> {
    let mut parts = line.trim().split('"').filter(|s| !s.trim().is_empty());
    let key = parts.next()?.to_string();
    let value = parts.next()?.replace("\\\\", "\\");
    Some((key, value))
}

/// Whether Kindle is already in a `Game` folder.
pub fn is_installed(game_dir: &Path) -> bool {
    game_dir.join("kindle").join("kindle.dll").is_file()
}

/// What an install did.
pub struct Installed {
    /// The installed `kindle` folder.
    pub kindle_dir: PathBuf,
    /// Files copied.
    pub files: usize,
    /// A `dinput8.dll` that wasn't Kindle's, kept under this name.
    pub kept_dinput8: Option<PathBuf>,
}

/// Copies the release at `payload` into `game_dir` (the folder with the game's exe):
/// `dinput8.dll` beside the game, and `kindle\`. What the user has there stays: `kindle.toml`
/// and everything in `data\` (mod settings, the mods switched off) are copied only when missing,
/// and mods the release doesn't have are left alone. A `dinput8.dll` that isn't Kindle's is kept
/// as dinput8.dll.before-kindle. Not while the game runs (its files are in use).
pub fn install(payload: &Path, game_dir: &Path) -> Result<Installed, String> {
    if !game_dir.join(GAME_EXE).is_file() {
        return Err(format!("{GAME_EXE} isn't in {}", game_dir.display()));
    }
    if crate::game_process().is_some() {
        return Err("Close Dark Souls III first.".into());
    }
    let target = game_dir.join("kindle");
    let source = payload.join("kindle");
    if same_folder(&source, &target) {
        return Err("This is the installed copy.".into());
    }
    let dinput8 = game_dir.join("dinput8.dll");
    let mut kept_dinput8 = None;
    if dinput8.is_file() && !is_kindles_dinput8(&dinput8) {
        let keep = game_dir.join(FOREIGN_DINPUT8);
        if !keep.exists() {
            std::fs::rename(&dinput8, &keep).map_err(|e| format!("{}: {e}", dinput8.display()))?;
            kept_dinput8 = Some(keep);
        }
    }
    let mut files = 0;
    copy_file(&payload.join("dinput8.dll"), &dinput8)?;
    files += 1;
    copy_tree(&source, &target, &source, &mut files)?;
    Ok(Installed { kindle_dir: target, files, kept_dinput8 })
}

/// Whether a dinput8.dll is Kindle's (it names kindle.dll).
fn is_kindles_dinput8(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else { return false };
    let wide: Vec<u8> = "kindle.dll".encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    bytes.windows(10).any(|w| w.eq_ignore_ascii_case(b"kindle.dll")) || bytes.windows(wide.len()).any(|w| w.eq_ignore_ascii_case(&wide))
}

fn same_folder(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether a file of the release (its path under `kindle\`) is the user's once installed.
fn users_own(relative: &Path) -> bool {
    let first = relative.components().next().map(|c| c.as_os_str().to_string_lossy().to_lowercase());
    matches!(first.as_deref(), Some("data")) || relative.as_os_str().eq_ignore_ascii_case("kindle.toml")
}

fn copy_tree(dir: &Path, target: &Path, root: &Path, files: &mut usize) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let relative = from.strip_prefix(root).unwrap_or(&from);
        let to = target.join(relative);
        if from.is_dir() {
            copy_tree(&from, target, root, files)?;
        } else if !(users_own(relative) && to.exists()) {
            copy_file(&from, &to)?;
            *files += 1;
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::copy(from, to).map(|_| ()).map_err(|e| format!("{}: {e}", to.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kindle-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn steam_library_paths_are_read() {
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"374320\"\t\t\"25000\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]);
        assert_eq!(vdf_value("\"AppState\"\n{\n\t\"installdir\"\t\t\"DARK SOULS III\"\n}", "installdir").as_deref(), Some("DARK SOULS III"));
    }

    #[test]
    fn the_release_is_found_beside_either_launcher() {
        let root = temp("payload");
        write(&root.join("dinput8.dll"), "kindle.dll");
        write(&root.join("kindle").join("kindle.dll"), "");
        assert_eq!(payload(&root), Some(root.clone()));
        assert_eq!(payload(&root.join("kindle")), Some(root.clone()));
        // Once in a game's folder it's the installed copy.
        write(&root.join(GAME_EXE), "");
        assert_eq!(payload(&root.join("kindle")), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_folder_picked_anywhere_in_the_game_is_its_game_folder() {
        let lib = temp("pick");
        let game = lib.join("steamapps").join("common").join(INSTALL_DIR).join("Game");
        write(&game.join(GAME_EXE), "");
        for picked in [lib.clone(), lib.join("steamapps").join("common"), game.parent().unwrap().to_path_buf(), game.clone()] {
            assert_eq!(game_dir_for(&picked), Some(game.clone()), "{}", picked.display());
        }
        assert_eq!(game_dir_for(&lib.join("steamapps")), None);
        let _ = std::fs::remove_dir_all(lib);
    }

    #[test]
    fn an_install_keeps_what_is_the_users() {
        let base = temp("copy");
        let (release, game) = (base.join("release"), base.join("Game"));
        write(&release.join("dinput8.dll"), "loads kindle.dll");
        write(&release.join("kindle").join("kindle.dll"), "new");
        write(&release.join("kindle").join("kindle.toml"), "release");
        write(&release.join("kindle").join("data").join("disabled.json"), "[\"a\"]");
        write(&release.join("kindle").join("mods").join("a").join("main.lua"), "new");
        write(&game.join(GAME_EXE), "");
        write(&game.join("dinput8.dll"), "another mod's");
        write(&game.join("kindle").join("kindle.toml"), "mine");
        write(&game.join("kindle").join("data").join("disabled.json"), "[]");
        write(&game.join("kindle").join("mods").join("mine").join("main.lua"), "mine");

        let done = install(&release, &game).unwrap();
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
        assert_eq!(read(game.join("dinput8.dll")), "loads kindle.dll");
        assert_eq!(read(game.join(FOREIGN_DINPUT8)), "another mod's");
        assert_eq!(done.kept_dinput8, Some(game.join(FOREIGN_DINPUT8)));
        assert_eq!(read(game.join("kindle").join("kindle.dll")), "new");
        assert_eq!(read(game.join("kindle").join("kindle.toml")), "mine");
        assert_eq!(read(game.join("kindle").join("data").join("disabled.json")), "[]");
        assert_eq!(read(game.join("kindle").join("mods").join("a").join("main.lua")), "new");
        assert_eq!(read(game.join("kindle").join("mods").join("mine").join("main.lua")), "mine");

        // Again (an update): Kindle's own dinput8.dll is replaced, not kept.
        assert_eq!(install(&release, &game).unwrap().kept_dinput8, None);
        assert_eq!(read(game.join(FOREIGN_DINPUT8)), "another mod's");
        // The installed copy can't install over itself.
        assert!(install(&game, &game).is_err());
        let _ = std::fs::remove_dir_all(base);
    }
}
