//! Kindle's own view of Elden Ring 2.7.0.0: the one build it runs in, and where the game keeps the
//! managers Kindle reads (singletons.rs finds their statics once, from the exe's code).

use std::collections::HashMap;
use std::sync::OnceLock;

use kindle_core::memory;

use crate::singletons::{self, Image};

/// The build Kindle supports: 2.7.0.0 (1.17 on the title screen).
pub fn supported_version() -> Result<String, String> {
    match memory::exe_version() {
        Some(v) if v == "2.7.0.0" => Ok(v),
        Some(v) => Err(format!("this eldenring.exe is version {v}; Kindle needs 2.7.0.0 (1.17)")),
        None => Err("couldn't read eldenring.exe's version".into()),
    }
}

/// The managers Kindle reads, by class: logged at start, so a game build that moved one says so there.
pub const MANAGERS: &[&str] = &[
    "CS::WorldChrMan",
    "CS::MapItemMan",
    "CS::CSEventFlagMan",
    "CS::CSEventFlagUsageParamManager",
    "CS::SoloParamRepository",
    "CS::CSSessionManager",
    "CS::CSNetMan",
    "CS::CSTask",
    "CS::CSTaskGroup",
];

/// Every singleton's static in the game (class -> its address), found the first time it's asked for.
fn statics() -> &'static HashMap<String, usize> {
    static STATICS: OnceLock<HashMap<String, usize>> = OnceLock::new();
    STATICS.get_or_init(|| {
        let (Some(image), Some((base, _))) = (Image::of_game(), memory::game_module()) else { return HashMap::new() };
        let found = singletons::find(&image);
        kindle_core::info!("singletons: {} found in the game's code", found.len());
        found.into_iter().map(|s| (s.class, base + s.rva)).collect()
    })
}

/// The address of the static that holds `class`'s instance ("CS::WorldChrMan"), if the game has one.
pub fn static_of(class: &str) -> Option<usize> {
    statics().get(class).copied()
}

/// `class`'s instance (None until the game makes it).
pub fn instance(class: &str) -> Option<usize> {
    memory::read_u64(static_of(class)?).map(|p| p as usize).filter(|&p| p != 0)
}

/// Logs where each of the managers Kindle reads is, and warns of any not found.
pub fn log_managers() {
    let base = memory::game_module().map_or(0, |(b, _)| b);
    for class in MANAGERS {
        match static_of(class) {
            Some(at) => kindle_core::info!("{class}: static at {}+{:#x}", kindle_core::bindings::exe(), at - base),
            None => kindle_core::warn!("{class}: not found in this game build"),
        }
    }
}
