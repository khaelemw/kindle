//! No official online play while Kindle runs (the game's terms don't allow modded play online).
//! Kindle sets the game's own launch setting (Network settings > Launch Setting) to Play Offline
//! whenever it isn't in safe mode: Kindle's own changes (its sessions, hooks and archives) are
//! there with no mod on (a tester's game went online with a build that shipped none, 2026-10-14);
//! it used to do so only with a mod or the randomizer on. Seamless Co-op isn't affected: it has
//! networking of its own.
//!
//! The setting is the options object's +0x21 (GameDataMan +0x58; 0 Play Online, 1 Play
//! Offline). The object exists about 6 s into the game's start and the saved options are
//! copied into it just after (a write right away was overwritten), so Kindle waits a moment.
//! Nothing reads +0x21 after that (watched from then through the title, Continue and loading a
//! save), and after "press any button" the game saves the options with the character data. So
//! the game decides how to start as it loads them, before Kindle can see them: Kindle's change
//! is saved and takes effect from the next start. With mods on Kindle keeps the setting offline
//! all session (switching it back in the menu doesn't last), says so when it changes it and
//! asks for a restart, and remembers the player's own choice (`data\online.json`) to put back
//! on a start with no mods. Nothing else in the options is read (the matching password is next
//! to it).
//!
//! The launch setting doesn't stop the game logging in to FromSoftware's server after "press
//! any button" (seen 2026-10-06: "Logging in to the DARK SOULS III game server", then a failure
//! dialog, with the setting on Play Offline; nothing reads it then). So with mods on Kindle also
//! refuses the game's own name lookups (its `getaddrinfo` import): the server can't be found, so
//! nothing is sent to it. Addresses and "localhost" still resolve; Kindle's co-op uses sockets of
//! its own.
//!
//! Verified 2026-10-06: the setting is switched and kept in the game's memory both ways (also
//! after the game loads its options again at "press any button"). Not verified: a start that
//! would go online staying offline, and the change reaching the save; the test install never
//! goes online and always starts offline (Seamless Co-op is installed there).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const GAME_DATA_MAN: &str = "48 8b 15 ?? ?? ?? ?? 32 c0";
const OPTIONS_AT: usize = 0x58;
const LAUNCH_SETTING_AT: usize = 0x21;
const ONLINE: u8 = 0;
const OFFLINE: u8 = 1;
/// How long to wait for the game's options.
const WAIT: Duration = Duration::from_secs(120);
/// After the options appear: the saved ones are copied in by then.
const SETTLE: Duration = Duration::from_secs(2);
/// How often the setting is checked and kept.
const KEEP_EVERY: Duration = Duration::from_secs(1);
/// How long a restored setting is kept after the start.
const RESTORE_FOR: Duration = Duration::from_secs(300);

fn remembered(kindle_dir: &Path) -> PathBuf {
    kindle_dir.join("data").join("online.json")
}

/// Watches for the game's options (on a thread of its own) and sets the launch setting:
/// offline when `modded`, else the player's own choice if Kindle changed it before.
pub fn start(kindle_dir: &Path, modded: bool) {
    if modded {
        block_lookups();
        // The game then doesn't try FromSoftware's login at all (p2p swaps the import).
        crate::p2p::STEAM_OFFLINE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let kindle_dir = kindle_dir.to_path_buf();
    let _ = std::thread::Builder::new().name("kindle-offline".into()).spawn(move || {
        let _ = std::panic::catch_unwind(|| run(&kindle_dir, modded));
    });
}

/// The game's `getaddrinfo`, before Kindle's took its import slot.
static GETADDRINFO: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
/// WSAHOST_NOT_FOUND.
const HOST_NOT_FOUND: i32 = 11001;

/// Points the game's `getaddrinfo` import at Kindle's, which refuses names (not addresses).
fn block_lookups() {
    let Some(&slot) = crate::memory::game_imports("WS2_32.dll").get("getaddrinfo") else {
        return crate::warn!("online: the game's name lookup wasn't found; its server login isn't blocked");
    };
    let Some(original) = crate::memory::read_u64(slot).filter(|&f| f != 0) else { return };
    if GETADDRINFO.set(original as usize).is_err() {
        return;
    }
    if !unsafe { crate::hooks::write_ptr(slot, getaddrinfo as *const () as usize) } {
        crate::warn!("online: couldn't block the game's server lookups");
    }
}

type GetAddrInfo = unsafe extern "system" fn(*const u8, *const u8, *const std::ffi::c_void, *mut *mut std::ffi::c_void) -> i32;

/// The game's name lookups while mods are on: addresses and "localhost" go through, names
/// (FromSoftware's servers) are refused.
unsafe extern "system" fn getaddrinfo(node: *const u8, service: *const u8, hints: *const std::ffi::c_void, out: *mut *mut std::ffi::c_void) -> i32 {
    let name = (!node.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(node.cast()) }.to_string_lossy().into_owned());
    let allowed = name.as_deref().is_none_or(|n| n.eq_ignore_ascii_case("localhost") || n.parse::<std::net::IpAddr>().is_ok());
    if !allowed {
        static LOGGED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let n = name.unwrap_or_default();
        let mut logged = LOGGED.lock().unwrap_or_else(|e| e.into_inner());
        if !logged.contains(&n) {
            crate::info!("online: the game looked up {n}; refused (mods are on: no FromSoftware servers)");
            logged.push(n);
        }
        // No result, as the system's own sets it before anything else.
        if !out.is_null() {
            unsafe { *out = std::ptr::null_mut() };
        }
        return HOST_NOT_FOUND;
    }
    let original: GetAddrInfo = unsafe { std::mem::transmute(*GETADDRINFO.get().expect("set before the slot was swapped")) };
    unsafe { original(node, service, hints, out) }
}

/// Where the launch setting is now (the game loads its options again as it checks the save,
/// so it's looked up each time).
fn launch_setting(stat: usize) -> Option<usize> {
    let gdm = crate::memory::read_u64(stat).filter(|&p| p != 0)?;
    let options = crate::memory::read_u64(gdm as usize + OPTIONS_AT).filter(|&p| p != 0)?;
    Some(options as usize + LAUNCH_SETTING_AT)
}

/// GameDataMan's static, once the game's options exist.
fn wait_for_options() -> Option<usize> {
    let started = Instant::now();
    let mut stat = None;
    while started.elapsed() < WAIT {
        if stat.is_none() {
            let all = crate::patterns::find_all(GAME_DATA_MAN);
            stat = (all.len() == 1).then(|| crate::patterns::rip_target(all[0] + 3, all[0] + 7)).flatten();
        }
        if let Some(s) = stat
            && launch_setting(s).is_some()
        {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(if stat.is_some() { 1 } else { 50 }));
    }
    None
}

fn run(kindle_dir: &Path, modded: bool) {
    let file = remembered(kindle_dir);
    let restore = !modded
        && std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["launch_setting"].as_u64())
            == Some(ONLINE as u64);
    if !modded && !restore {
        let _ = std::fs::remove_file(&file);
        return;
    }
    let Some(stat) = wait_for_options() else {
        crate::warn!("online: the game's launch setting wasn't found; online play isn't turned off");
        return;
    };
    std::thread::sleep(SETTLE);
    let (want, wrong) = if modded { (OFFLINE, ONLINE) } else { (ONLINE, OFFLINE) };
    let started = Instant::now();
    let mut told = false;
    let mut first = true;
    // With mods: all session (the player switching it back would be saved with the character).
    // Restoring: long enough to outlast the game loading its options again and saving them.
    while modded || started.elapsed() < RESTORE_FOR {
        if let Some(at) = launch_setting(stat)
            && let Some(now) = crate::memory::read::<u8>(at)
        {
            if first && now == want && modded {
                crate::info!("online: the game starts offline (mods are on)");
            }
            first = false;
            if now == wrong {
                unsafe { std::ptr::write_volatile(at as *mut u8, want) };
                if !told {
                    told = true;
                    if modded {
                        let _ = std::fs::create_dir_all(file.parent().unwrap_or(kindle_dir));
                        if !file.exists() {
                            let _ = std::fs::write(&file, format!("{{ \"launch_setting\": {now} }}\n"));
                        }
                        crate::warn!("online: mods are on, so the game's launch setting is now Play Offline (restart the game for it to apply)");
                        crate::ui_bridge::push_notice(
                            crate::ui_bridge::NoticeKind::Banner,
                            "Mods are on: online play is now off. Restart the game to play offline.".into(),
                            [1.0, 0.55, 0.45, 1.0],
                            12.0,
                        );
                    } else {
                        crate::info!("online: no mods are on, so the game's launch setting is back to Play Online (from the next start)");
                    }
                }
            }
        }
        std::thread::sleep(KEEP_EVERY);
    }
    let _ = std::fs::remove_file(&file);
}
