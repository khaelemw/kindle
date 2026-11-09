//! Dark Souls III's side of mods' changes to its event scripts (kindle_core::event_patches): a
//! map's event file read from the game's archives (archive.rs) and unpacked, and the changed file
//! packed again as the game's (DFLT DCX) for files.rs to serve.

use std::path::Path;

pub use kindle_core::event_patches::*;

/// Reads the maps' event files from the game's archives from now on.
pub fn install(game_dir: &Path) {
    static GAME_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let _ = GAME_DIR.set(game_dir.to_path_buf());
    kindle_core::event_patches::set_reader(|map| {
        let game_dir = GAME_DIR.get().ok_or("the game's folder isn't known")?;
        let (_, dcx) = crate::archive::read_any(game_dir, &format!("/event/{map}.emevd.dcx"))?;
        crate::archive::undcx(&dcx).ok_or_else(|| "not a DCX file Kindle can read".into())
    });
}

/// The game's event file `original` (DCX) with the patches for its map, or None without any.
pub fn patched(rel: &str, original: &[u8]) -> Result<Option<Vec<u8>>, String> {
    if version(rel).is_none() {
        return Ok(None);
    }
    let raw = crate::archive::undcx(original).ok_or("not a DCX file Kindle can read")?;
    let Some(made) = kindle_core::event_patches::patched(rel, &raw)? else { return Ok(None) };
    crate::archive::dcx(original, &made, original.len()).map(Some).ok_or_else(|| "couldn't write the event file".into())
}
