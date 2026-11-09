//! `dinput8.dll` for the game's folder: Kindle loaded by the game itself, so the game can be
//! started from Steam's Play button (and on Linux through Proton, with the launch option
//! `WINEDLLOVERRIDES="dinput8=n,b" %command%`) instead of through Kindle Launcher. The game
//! imports DINPUT8.dll, so Windows (or Wine, with the override) loads this one from the game's
//! folder as the game starts; it passes DirectInput8Create on to the system's own dinput8.dll
//! and loads Kindle's DLL for that game from `kindle\` beside itself (`GAMES`), on a thread of
//! its own (not while the game's DLLs are still being loaded). Only in those games' exes, and
//! never in a game Easy Anti-Cheat started (Elden Ring from Steam's Play button: Kindle runs in
//! it started without, offline). Started through Kindle Launcher as well, the launcher's load
//! finds Kindle already there (the same file).

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::OnceLock;

use windows::Win32::Foundation::{CloseHandle, E_FAIL, HINSTANCE, HMODULE};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS};
use windows::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleFileNameW, GetProcAddress, LoadLibraryW};
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::{BOOL, GUID, HRESULT, PCWSTR, s};

const DLL_PROCESS_ATTACH: u32 = 1;

/// A game that imports dinput8.dll and that Kindle runs in.
struct Game {
    /// Its exe's file name, in lower case.
    exe: &'static str,
    /// Kindle's DLL for it, in `kindle\`.
    dll: &'static str,
    /// Easy Anti-Cheat's launcher, which starts the game with anti-cheat on: Kindle never loads in
    /// a game it started, nor in one whose starter can't be told (it has exited).
    anti_cheat: Option<&'static str>,
}

#[cfg(feature = "ds3")]
const DS3: Game = Game { exe: "darksoulsiii.exe", dll: "kindle.dll", anti_cheat: None };
#[cfg(feature = "er")]
const ER: Game = Game { exe: "eldenring.exe", dll: "kindle_er.dll", anti_cheat: Some("start_protected_game.exe") };

/// The games this build is for (its features).
#[cfg(all(feature = "ds3", feature = "er"))]
const GAMES: &[Game] = &[DS3, ER];
#[cfg(all(feature = "ds3", not(feature = "er")))]
const GAMES: &[Game] = &[DS3];
#[cfg(all(feature = "er", not(feature = "ds3")))]
const GAMES: &[Game] = &[ER];
#[cfg(not(any(feature = "ds3", feature = "er")))]
const GAMES: &[Game] = &[];

/// The game whose exe is named `exe` (None in a program that isn't one of them).
fn game(exe: &str) -> Option<&'static Game> {
    let exe = exe.to_lowercase();
    GAMES.iter().find(|g| g.exe == exe)
}

/// Kindle's DLL for the game `exe` started by the program `parent` (its exe's file name, if it's
/// still running): None in a program that isn't one of the games, or in a game anti-cheat may
/// have started.
fn kindle_dll(exe: &str, parent: Option<&str>) -> Option<&'static str> {
    let game = game(exe)?;
    let maybe_anti_cheat = game.anti_cheat.is_some_and(|ac| parent.is_none_or(|p| p.eq_ignore_ascii_case(ac)));
    (!maybe_anti_cheat).then_some(game.dll)
}

/// The file name of the exe of the process that started this one (None if it has exited).
fn parent_exe() -> Option<String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    let name = |e: &PROCESSENTRY32W| String::from_utf16_lossy(&e.szExeFile[..e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len())]);
    let mut processes = Vec::new();
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            processes.push((entry.th32ProcessID, entry.th32ParentProcessID, name(&entry)));
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }
    let _ = unsafe { CloseHandle(snapshot) };
    let me = unsafe { GetCurrentProcessId() };
    let parent = processes.iter().find(|p| p.0 == me)?.1;
    processes.into_iter().find(|p| p.0 == parent).map(|p| p.2)
}

type Create = unsafe extern "system" fn(HINSTANCE, u32, *const GUID, *mut *mut c_void, *mut c_void) -> HRESULT;

/// The system dinput8.dll's DirectInput8Create (0 if it couldn't be loaded).
fn real_create() -> usize {
    static REAL: OnceLock<usize> = OnceLock::new();
    *REAL.get_or_init(|| unsafe {
        let mut dir = [0u16; 520];
        let n = GetSystemDirectoryW(Some(&mut dir)) as usize;
        if n == 0 || n >= dir.len() {
            return 0;
        }
        let mut path: Vec<u16> = dir[..n].to_vec();
        path.extend(r"\dinput8.dll".encode_utf16());
        path.push(0);
        let Ok(module) = LoadLibraryW(PCWSTR(path.as_ptr())) else { return 0 };
        GetProcAddress(module, s!("DirectInput8Create")).map_or(0, |f| f as usize)
    })
}

/// # Safety
/// Called by the game as the system's DirectInput8Create.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DirectInput8Create(instance: HINSTANCE, version: u32, riid: *const GUID, out: *mut *mut c_void, outer: *mut c_void) -> HRESULT {
    let f = real_create();
    if f == 0 {
        return E_FAIL;
    }
    unsafe { std::mem::transmute::<usize, Create>(f)(instance, version, riid, out, outer) }
}

fn module_path(module: Option<HMODULE>) -> Option<PathBuf> {
    let mut buf = [0u16; 1024];
    let n = unsafe { GetModuleFileNameW(module, &mut buf) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

fn load_kindle(this: PathBuf, exe: String) {
    let Some(name) = kindle_dll(&exe, parent_exe().as_deref()) else { return };
    let Some(dll) = this.parent().map(|d| d.join("kindle").join(name)) else { return };
    if !dll.is_file() {
        return;
    }
    let wide: Vec<u16> = dll.as_os_str().encode_wide().chain([0]).collect();
    let _ = unsafe { LoadLibraryW(PCWSTR(wide.as_ptr())) };
}

/// # Safety
/// The DLL entry point, called by the loader.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllMain(module: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        let module = HMODULE(module.0);
        let _ = unsafe { DisableThreadLibraryCalls(module) };
        let exe = module_path(None).and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).filter(|n| game(n).is_some());
        if let Some(exe) = exe
            && let Some(this) = module_path(Some(module))
        {
            // The thread starts once the loader is done with the game's DLLs.
            let _ = std::thread::Builder::new().name("kindle-proxy".into()).spawn(move || load_kindle(this, exe));
        }
    }
    BOOL(1)
}

#[cfg(test)]
mod tests {
    use super::{kindle_dll, parent_exe};

    #[test]
    fn the_starter_is_found() {
        // The test runner (cargo), alive while its tests run.
        assert!(parent_exe().is_some_and(|p| p.to_lowercase().ends_with(".exe")));
    }

    #[test]
    #[cfg(feature = "ds3")]
    fn dark_souls_iii_gets_kindle() {
        assert_eq!(kindle_dll("DarkSoulsIII.exe", None), Some("kindle.dll"));
        assert_eq!(kindle_dll("DarkSoulsIII.exe", Some("steam.exe")), Some("kindle.dll"));
    }

    #[test]
    #[cfg(feature = "er")]
    fn elden_ring_gets_its_own_kindle() {
        assert_eq!(kindle_dll("eldenring.exe", Some("kindle-launcher.exe")), Some("kindle_er.dll"));
        assert_eq!(kindle_dll("ELDENRING.EXE", Some("explorer.exe")), Some("kindle_er.dll"));
    }

    #[test]
    #[cfg(not(feature = "er"))]
    fn a_build_without_elden_ring_leaves_it_alone() {
        assert_eq!(kindle_dll("eldenring.exe", Some("kindle-launcher.exe")), None);
    }

    #[test]
    fn never_in_a_game_anti_cheat_may_have_started() {
        assert_eq!(kindle_dll("eldenring.exe", Some("start_protected_game.exe")), None);
        assert_eq!(kindle_dll("eldenring.exe", Some("Start_Protected_Game.exe")), None);
        // Whatever started it has exited: it may have been anti-cheat's launcher.
        assert_eq!(kindle_dll("eldenring.exe", None), None);
    }

    #[test]
    fn other_programs_get_none() {
        // Elden Ring's Easy Anti-Cheat launcher itself, and programs that happen to load dinput8.dll from the folder.
        assert_eq!(kindle_dll("start_protected_game.exe", Some("steam.exe")), None);
        assert_eq!(kindle_dll("eldenring.exe.bak", None), None);
        assert_eq!(kindle_dll("", None), None);
    }
}
