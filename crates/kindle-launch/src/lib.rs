//! Starting Dark Souls III with Kindle, shared by `kindle launch` and the launcher window
//! (kindle-launcher): named profiles in `launcher.toml` (next to kindle.exe) instead of
//! hand-written Mod Engine configs and .bat files. Kindle's own DLL is always loaded (last); a
//! profile adds other DLLs (Seamless Co-op, a crash fix) and file mods (folders of replacement
//! game files, e.g. a randomizer).
//!
//! A profile with `loader = "direct"` starts the game without Mod Engine 2 (Kindle loads the
//! DLLs itself). File mods need Mod Engine 2 (Kindle mods replace files with their own `files`
//! folder instead).

pub mod inject;
pub mod install;
mod mods;

pub use kindle_manifest::{Setting, SettingKind, SettingValue};
pub use mods::{KindleMod, ModSetting, kindle_mods, set_setting};

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE: &str = "launcher.toml";
pub const GAME_EXE: &str = "DarkSoulsIII.exe";
const MODENGINE_EXE: &str = "modengine2_launcher.exe";
/// Mod Engine 2's launcher is a console program; it gets no console window of its own.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Launcher {
    /// DarkSoulsIII.exe.
    pub game: PathBuf,
    /// Mod Engine 2's modengine2_launcher.exe.
    pub modengine: PathBuf,
    /// Profile used when none is given and there's no one to ask.
    pub default: String,
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Profile {
    /// DLLs loaded before Kindle, in order.
    pub dlls: Vec<PathBuf>,
    /// DLLs from `dlls` that are switched off (kept so they can be switched back on).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dlls_off: Vec<PathBuf>,
    /// The Kindle mods (ids) a start runs; the other installed ones are off. None in a
    /// launcher.toml from before profiles had them: filled with the mods running now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kindle_mods: Option<Vec<String>>,
    /// Folders of replacement game files (Mod Engine 2's mod loader), highest priority first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_mods: Vec<FileMod>,
    /// Let file mods ship loose param files.
    #[serde(skip_serializing_if = "is_false")]
    pub loose_params: bool,
    /// "modengine" (default) or "direct": Kindle starts the game and loads the DLLs itself.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub loader: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileMod {
    pub name: String,
    pub path: PathBuf,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Profile {
    /// The DLLs that are switched on, in order.
    pub fn active_dlls(&self) -> Vec<PathBuf> {
        self.dlls.iter().filter(|d| !self.dlls_off.contains(d)).cloned().collect()
    }

    pub fn set_dll_on(&mut self, dll: &Path, on: bool) {
        self.dlls_off.retain(|d| d != dll);
        if !on {
            self.dlls_off.push(dll.to_path_buf());
        }
    }

    pub fn direct(&self) -> bool {
        self.loader.eq_ignore_ascii_case("direct")
    }

    pub fn active_file_mods(&self) -> impl Iterator<Item = &FileMod> {
        self.file_mods.iter().filter(|m| m.enabled)
    }
}

/// A readable name for a DLL mod.
pub fn dll_name(dll: &Path) -> String {
    let file = dll.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
    match file.to_ascii_lowercase().as_str() {
        "ds3sc.dll" => "Seamless Co-op".into(),
        "randomizercrashfix.dll" => "Randomizer crash fix".into(),
        _ => dll.file_stem().map(|f| f.to_string_lossy().to_string()).unwrap_or(file),
    }
}

pub fn load(kindle_dir: &Path) -> Result<Launcher, String> {
    let path = kindle_dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|_| format!("no {} yet: run `kindle launch --setup` first", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// launcher.toml brought up to date: once a game started from a profile has closed, the
/// profile gets the Kindle mods that were running (so mods switched on or off in game count),
/// and a profile without a Kindle mod list gets the mods running now.
pub fn load_synced(kindle_dir: &Path) -> Result<Launcher, String> {
    let mut l = load(kindle_dir)?;
    let mut changed = false;
    if !game_running()
        && let Some(name) = mods::take_session(kindle_dir)
        && let Some(p) = l.profiles.get_mut(&name)
    {
        p.kindle_mods = Some(mods::after_session(kindle_dir, p.kindle_mods.as_deref().unwrap_or_default()));
        changed = true;
    }
    for p in l.profiles.values_mut().filter(|p| p.kindle_mods.is_none()) {
        p.kindle_mods = Some(mods::running_ids(kindle_dir));
        changed = true;
    }
    if changed {
        save(kindle_dir, &l)?;
    }
    Ok(l)
}

pub fn save(kindle_dir: &Path, l: &Launcher) -> Result<PathBuf, String> {
    let path = kindle_dir.join(FILE);
    let body = toml::to_string_pretty(l).map_err(|e| e.to_string())?;
    let text = format!("# Kindle launcher profiles (Kindle Launcher.exe, or kindle launch [profile]).\n\n{body}");
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Folders (up to `depth` levels below `root`) containing a file named `name`.
fn find_file(root: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    let candidate = root.join(name);
    if candidate.is_file() {
        return Some(candidate);
    }
    if depth == 0 {
        return None;
    }
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    dirs.iter().find_map(|d| find_file(d, name, depth - 1))
}

/// Whether Dark Souls III is running.
pub fn game_running() -> bool {
    game_process().is_some()
}

/// Dark Souls III's process id, if it's running.
pub fn game_process() -> Option<u32> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut found = None;
    let mut more = unsafe { Process32FirstW(snap, &mut entry) }.is_ok();
    while more {
        let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
        if String::from_utf16_lossy(&entry.szExeFile[..len]).eq_ignore_ascii_case(GAME_EXE) {
            found = Some(entry.th32ProcessID);
            break;
        }
        more = unsafe { Process32NextW(snap, &mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snap) };
    found
}

/// Whether the game has opened its window (it can hang before then, e.g. when started again
/// right after being closed).
pub fn game_window_open(pid: u32) -> bool {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};
    use windows::core::BOOL;
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> BOOL {
        let (want, found) = unsafe { &mut *(data.0 as *mut (u32, bool)) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == *want && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            *found = true;
            return false.into();
        }
        true.into()
    }
    let mut state = (pid, false);
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut state as *mut (u32, bool) as isize)) };
    state.1
}

/// Ends the game's process (for a start that hung).
pub fn stop_game(pid: u32) -> Result<(), String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
    let process = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.map_err(|e| e.to_string())?;
    let result = unsafe { TerminateProcess(process, 1) }.map_err(|e| e.to_string());
    let _ = unsafe { CloseHandle(process) };
    result
}

/// Finds the game, Mod Engine 2, Seamless Co-op and a crash fix, and writes a "default"
/// profile (keeping any other profiles). Returns the launcher and notes for the user.
pub fn setup(kindle_dir: &Path) -> Result<(Launcher, Vec<String>), String> {
    let mut l = load(kindle_dir).unwrap_or_default();
    let mut notes = Vec::new();
    let game_dir = kindle_dir.parent().ok_or("kindle.exe isn't inside the game folder")?;
    let game = game_dir.join(GAME_EXE);
    if !game.is_file() {
        return Err(format!("{} not found: install Kindle into the game folder (Game\\kindle)", game.display()));
    }
    l.game = game;
    let search_root = game_dir.parent().unwrap_or(game_dir);
    if !l.modengine.is_file() {
        match find_file(search_root, MODENGINE_EXE, 5) {
            Some(me2) => l.modengine = me2,
            None => {
                notes.push(format!("{MODENGINE_EXE} not found under {}: the game will start without Mod Engine 2 (file mods need it)", search_root.display()))
            }
        }
    }
    let mut dlls = Vec::new();
    if let Some(fix) = find_file(search_root, "RandomizerCrashFix.dll", 5) {
        dlls.push(fix);
    }
    let seamless = game_dir.join("SeamlessCoop").join("ds3sc.dll");
    if seamless.is_file() {
        dlls.push(seamless);
    }
    let loader = if l.modengine.is_file() { String::new() } else { "direct".into() };
    let default = Profile { dlls, loader, ..Profile::default() };
    l.profiles.insert("default".into(), default);
    if l.default.is_empty() {
        l.default = "default".into();
    }
    let path = save(kindle_dir, &l)?;
    notes.insert(0, format!("wrote {}", path.display()));
    Ok((l, notes))
}

/// Adds a profile from an existing Mod Engine 2 config (its DLLs other than Kindle's, and its
/// file mods).
pub fn import(kindle_dir: &Path, name: &str, config: &Path) -> Result<PathBuf, String> {
    let mut l = load(kindle_dir)?;
    let text = std::fs::read_to_string(config).map_err(|e| format!("{}: {e}", config.display()))?;
    let t: toml::Table = toml::from_str(&text).map_err(|e| format!("{}: {e}", config.display()))?;
    let dlls: Vec<PathBuf> = t
        .get("modengine")
        .and_then(|m| m.get("external_dlls"))
        .and_then(|d| d.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).map(PathBuf::from).collect())
        .unwrap_or_default();
    let dlls: Vec<PathBuf> = dlls.into_iter().filter(|p| !p.file_name().is_some_and(|f| f.eq_ignore_ascii_case("kindle.dll"))).collect();
    let loader = t.get("extension").and_then(|e| e.get("mod_loader"));
    let file_mods = loader
        .and_then(|m| m.get("mods"))
        .and_then(|m| m.as_array())
        .map(|a| {
            a.iter()
                .filter(|m| m.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
                .filter_map(|m| Some(FileMod { name: m.get("name")?.as_str()?.to_string(), path: PathBuf::from(m.get("path")?.as_str()?), enabled: true }))
                .collect()
        })
        .unwrap_or_default();
    let loose_params = loader.and_then(|m| m.get("loose_params")).and_then(|v| v.as_bool()).unwrap_or(false);
    l.profiles.insert(name.to_string(), Profile { dlls, file_mods, loose_params, ..Profile::default() });
    save(kindle_dir, &l)
}

/// Mod Engine 2's config for a profile, with Kindle's DLL last.
pub fn modengine_config(p: &Profile, kindle_dll: &Path) -> Result<String, String> {
    let mut dlls: Vec<toml::Value> = p.active_dlls().iter().map(|d| toml::Value::String(d.display().to_string())).collect();
    dlls.push(toml::Value::String(kindle_dll.display().to_string()));
    let mut modengine = toml::Table::new();
    modengine.insert("debug".into(), false.into());
    modengine.insert("external_dlls".into(), toml::Value::Array(dlls));
    let mods: Vec<toml::Value> = p
        .active_file_mods()
        .map(|m| {
            let mut t = toml::Table::new();
            t.insert("enabled".into(), true.into());
            t.insert("name".into(), m.name.clone().into());
            t.insert("path".into(), m.path.display().to_string().into());
            toml::Value::Table(t)
        })
        .collect();
    let mut loader = toml::Table::new();
    loader.insert("enabled".into(), true.into());
    loader.insert("loose_params".into(), p.loose_params.into());
    loader.insert("mods".into(), toml::Value::Array(mods));
    let mut extension = toml::Table::new();
    extension.insert("mod_loader".into(), toml::Value::Table(loader));
    let mut root = toml::Table::new();
    root.insert("modengine".into(), toml::Value::Table(modengine));
    root.insert("extension".into(), toml::Value::Table(extension));
    let body = toml::to_string(&root).map_err(|e| e.to_string())?;
    Ok(format!("# Written by the Kindle launcher from launcher.toml; edit that instead.\n{body}"))
}

pub enum Started {
    /// Started directly (the game's process id).
    Direct(u32),
    /// Mod Engine 2 is starting it.
    ModEngine,
}

/// Writes the steps of a direct start to `kindle\logs\launch.log` (a fresh one each start), for
/// when the game doesn't start and Kindle wrote nothing.
fn launch_log(kindle_dir: &Path) -> impl FnMut(&str) {
    use std::io::Write;
    let dir = kindle_dir.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let mut file = std::fs::File::create(dir.join("launch.log")).ok();
    let started = std::time::Instant::now();
    move |line: &str| {
        if let Some(f) = file.as_mut() {
            let _ = writeln!(f, "[{:>8.3}] {line}", started.elapsed().as_secs_f64());
        }
    }
}

/// The game versions Kindle runs on (as kindle-ds3's ds3.rs checks them).
const SUPPORTED: [&str; 2] = ["1.15.2.0", "1.15.2.1"];

/// The file version of `exe` ("1.15.2.0"), if it has one.
pub fn game_version(exe: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW};
    use windows::core::{HSTRING, w};
    let file = HSTRING::from(exe.as_os_str());
    let size = unsafe { GetFileVersionInfoSizeW(&file, None) };
    if size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    unsafe { GetFileVersionInfoW(&file, None, size, data.as_mut_ptr().cast()) }.ok()?;
    let mut info: *mut std::ffi::c_void = std::ptr::null_mut();
    let mut len = 0u32;
    if !unsafe { VerQueryValueW(data.as_ptr().cast(), w!("\\"), &mut info, &mut len) }.as_bool() || info.is_null() {
        return None;
    }
    let v = unsafe { &*(info as *const VS_FIXEDFILEINFO) };
    Some(format!("{}.{}.{}.{}", v.dwFileVersionMS >> 16, v.dwFileVersionMS & 0xFFFF, v.dwFileVersionLS >> 16, v.dwFileVersionLS & 0xFFFF))
}

/// Ok when Kindle runs on this game, else what to tell the player.
pub fn check_version(exe: &Path) -> Result<String, String> {
    match game_version(exe) {
        Some(v) if SUPPORTED.contains(&v.as_str()) => Ok(v),
        Some(v) => Err(format!("This Dark Souls III is version {}. Kindle needs 1.15.2: update the game.", v.strip_suffix(".0").unwrap_or(&v))),
        None => Err(format!("Couldn't read the version of {}.", exe.display())),
    }
}

/// What to do when the game is set to run as administrator and the launcher isn't.
pub(crate) const ADMIN: &str = "Dark Souls III is set to run as administrator. Untick \"Run this program as an administrator\" \
    (right-click DarkSoulsIII.exe > Properties > Compatibility), or start Kindle Launcher as administrator.";

/// Whether Windows' compatibility setting "Run this program as an administrator" is on for
/// `exe` (for this user or for all users).
fn runs_as_admin(exe: &Path) -> bool {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
    use windows::core::{HSTRING, w};
    let name = HSTRING::from(exe.as_os_str());
    [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].into_iter().any(|hive| {
        let mut buf = [0u16; 512];
        let mut size = (buf.len() * 2) as u32;
        let key = w!("Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers");
        let found = unsafe { RegGetValueW(hive, key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)) };
        found.is_ok() && String::from_utf16_lossy(&buf[..(size as usize / 2).min(buf.len())]).to_uppercase().contains("RUNASADMIN")
    })
}

/// Whether this program runs as administrator.
fn elevated() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin() }.as_bool()
}

/// Starts the game with profile `name` (directly when `force_direct` or the profile says so).
pub fn start(kindle_dir: &Path, l: &Launcher, name: &str, force_direct: bool) -> Result<Started, String> {
    let p = l.profiles.get(name).ok_or_else(|| format!("no profile {name:?} (profiles: {})", l.profiles.keys().cloned().collect::<Vec<_>>().join(", ")))?;
    if game_running() {
        return Err("Dark Souls III is already running: close it first".into());
    }
    check_version(&l.game)?;
    // An admin program can't be started (with DLLs) by one that isn't: Mod Engine 2 fails
    // without a word, Kindle's own start with "elevation required".
    if runs_as_admin(&l.game) && !elevated() {
        return Err(ADMIN.into());
    }
    let kindle_dll = kindle_dir.join("kindle.dll");
    let dlls = p.active_dlls();
    // The profile's Kindle mods run; mods switched on or off in game go back into it.
    if let Some(ids) = &p.kindle_mods {
        mods::apply_profile(kindle_dir, ids)?;
    }
    mods::begin_session(kindle_dir, name)?;
    if force_direct || p.direct() {
        if p.active_file_mods().next().is_some() {
            return Err(format!("profile {name:?} has file mods, which need Mod Engine 2"));
        }
        let missing: Vec<String> = [&l.game, &kindle_dll].into_iter().chain(&dlls).filter(|p| !p.is_file()).map(|p| p.display().to_string()).collect();
        if !missing.is_empty() {
            return Err(format!("profile {name:?}: not found:\n  {}", missing.join("\n  ")));
        }
        let mut dlls = dlls;
        dlls.push(kindle_dll);
        return inject::start(&l.game, &dlls, &mut launch_log(kindle_dir)).map(Started::Direct);
    }
    let mut missing: Vec<String> = Vec::new();
    for path in [&l.game, &l.modengine, &kindle_dll].into_iter().chain(&dlls) {
        if !path.is_file() {
            missing.push(path.display().to_string());
        }
    }
    missing.extend(p.active_file_mods().filter(|m| !m.path.is_dir()).map(|m| m.path.display().to_string()));
    if !missing.is_empty() {
        return Err(format!("profile {name:?}: not found:\n  {}", missing.join("\n  ")));
    }
    let config_dir = kindle_dir.join("launch");
    std::fs::create_dir_all(&config_dir).map_err(|e| format!("{}: {e}", config_dir.display()))?;
    let config = config_dir.join(format!("{name}.toml"));
    std::fs::write(&config, modengine_config(p, &kindle_dll)?).map_err(|e| format!("{}: {e}", config.display()))?;
    let modengine_dir = l.modengine.parent().ok_or("modengine path has no folder")?;
    std::process::Command::new(&l.modengine)
        .current_dir(modengine_dir)
        .creation_flags(CREATE_NO_WINDOW)
        .arg("-t")
        .arg("ds3")
        .arg("-p")
        .arg(&l.game)
        .arg("-c")
        .arg(&config)
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", l.modengine.display()))?;
    Ok(Started::ModEngine)
}

/// Whether kindle.toml turns the developer overlay on (`[overlay] developer`, off if unset).
pub fn developer_overlay(kindle_dir: &Path) -> bool {
    let text = std::fs::read_to_string(kindle_dir.join("kindle.toml")).unwrap_or_default();
    let config: toml::Table = toml::from_str(&text).unwrap_or_default();
    config.get("overlay").and_then(|o| o.get("developer")).and_then(toml::Value::as_bool).unwrap_or(false)
}

/// Turns the developer overlay on or off in kindle.toml (only that line changes).
pub fn set_developer_overlay(kindle_dir: &Path, on: bool) -> Result<(), String> {
    let path = kindle_dir.join("kindle.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let text = kindle_manifest::set_toml_value(&text, "overlay", "developer", if on { "true" } else { "false" });
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modengine_config_lists_kindle_last_and_skips_what_is_off() {
        let p = Profile {
            dlls: vec![PathBuf::from(r"C:\x\fix.dll"), PathBuf::from(r"C:\x\off.dll")],
            dlls_off: vec![PathBuf::from(r"C:\x\off.dll")],
            file_mods: vec![
                FileMod { name: "rando".into(), path: PathBuf::from(r"C:\x\rando"), enabled: true },
                FileMod { name: "old".into(), path: PathBuf::from(r"C:\x\old"), enabled: false },
            ],
            ..Profile::default()
        };
        let text = modengine_config(&p, Path::new(r"C:\game\kindle\kindle.dll")).unwrap();
        let t: toml::Table = toml::from_str(&text).unwrap();
        let dlls = t["modengine"]["external_dlls"].as_array().unwrap();
        assert_eq!(dlls.len(), 2);
        assert_eq!(dlls[0].as_str(), Some(r"C:\x\fix.dll"));
        assert_eq!(dlls[1].as_str(), Some(r"C:\game\kindle\kindle.dll"));
        let mods = t["extension"]["mod_loader"]["mods"].as_array().unwrap();
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0]["path"].as_str(), Some(r"C:\x\rando"));
    }

    #[test]
    fn import_reads_a_modengine_config() {
        let dir = std::env::temp_dir().join(format!("kindle-launch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        save(&dir, &Launcher::default()).unwrap();
        let me2 = dir.join("me2.toml");
        std::fs::write(
            &me2,
            "[modengine]\nexternal_dlls = ['C:\\a\\fix.dll', 'C:\\g\\kindle\\kindle.dll']\n\
             [extension.mod_loader]\nenabled = true\nloose_params = false\n\
             mods = [{ enabled = true, name = \"randomizer\", path = 'C:\\r' }]\n",
        )
        .unwrap();
        import(&dir, "rando", &me2).unwrap();
        let l = load(&dir).unwrap();
        let p = &l.profiles["rando"];
        assert_eq!(p.dlls, vec![PathBuf::from(r"C:\a\fix.dll")], "Kindle's own DLL isn't imported");
        assert_eq!(p.file_mods[0].name, "randomizer");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn switching_a_dll_off_round_trips() {
        let mut p = Profile { dlls: vec![PathBuf::from("a.dll"), PathBuf::from("b.dll")], ..Profile::default() };
        p.set_dll_on(Path::new("a.dll"), false);
        assert_eq!(p.active_dlls(), vec![PathBuf::from("b.dll")]);
        let l = Launcher { profiles: [("x".to_string(), p)].into(), ..Launcher::default() };
        let back: Launcher = toml::from_str(&toml::to_string_pretty(&l).unwrap()).unwrap();
        assert_eq!(back.profiles["x"].active_dlls(), vec![PathBuf::from("b.dll")]);
        let mut p = back.profiles["x"].clone();
        p.set_dll_on(Path::new("a.dll"), true);
        assert_eq!(p.active_dlls().len(), 2);
        assert_eq!(dll_name(Path::new(r"C:\g\SeamlessCoop\ds3sc.dll")), "Seamless Co-op");
    }

    #[test]
    fn developer_overlay_round_trips() {
        let dir = std::env::temp_dir().join(format!("kindle-developer-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("kindle.toml"),
            "[overlay]
# The key.
toggle_key = \"F1\"
",
        )
        .unwrap();
        assert!(!developer_overlay(&dir));
        set_developer_overlay(&dir, true).unwrap();
        assert!(developer_overlay(&dir));
        let text = std::fs::read_to_string(dir.join("kindle.toml")).unwrap();
        assert!(text.starts_with(
            "[overlay]
# The key.
toggle_key = \"F1\"
developer = true"
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
