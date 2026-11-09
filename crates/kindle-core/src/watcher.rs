//! Hot reload: watches `kindle\mods` and reports which mod folder changed. The runtime
//! debounces and reloads on the game thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use notify::{EventKind, RecursiveMode, Watcher};

pub fn start(mods_dir: &Path, to_game: Sender<String>) {
    let root: PathBuf = mods_dir.to_path_buf();
    let handler_root = root.clone();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
            return;
        }
        for path in &event.paths {
            let relevant = path.is_dir() || path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lua") || e.eq_ignore_ascii_case("toml")) || !path.exists();
            if !relevant {
                continue;
            }
            // First path component under mods\ is the mod's folder.
            if let Ok(rel) = path.strip_prefix(&handler_root)
                && let Some(first) = rel.components().next()
            {
                let _ = to_game.send(first.as_os_str().to_string_lossy().into_owned());
            }
        }
    });
    match watcher {
        Ok(mut w) => match w.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => {
                crate::info!("hot reload watching {}", root.display());
                // The watcher must live for the whole session.
                std::mem::forget(w);
            }
            Err(e) => crate::warn!("hot reload disabled: {e}"),
        },
        Err(e) => crate::warn!("hot reload disabled: {e}"),
    }
}
