//! The game's own icons (an item's `iconId`) as pictures for mods' layers and screens.
//!
//! Icon n is `/menu/knowledge/menu_knowledge_<n:05>.tpf.dcx` in Data1 (the exe's
//! `menu:/Knowledge/%s.tpf`, `MENU_Knowledge_%05d`): a texture file holding one 512 x 512 BC7
//! DDS. A mod that replaces the file (its `files/`) is read instead, as the game reads it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kindle_core::hud::{self, Image};

use crate::{archive, files};

/// The game's icon `id` as a picture, made once.
pub fn icon(id: u32) -> Result<Arc<Image>, String> {
    hud::cached_image(Path::new(&format!("game:icon/{id}")), std::time::UNIX_EPOCH, || {
        let path = format!("/menu/knowledge/menu_knowledge_{id:05}.tpf.dcx");
        let dcx = match files::replacement(&path) {
            Some(file) => std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?,
            None => {
                let dir = game_dir().ok_or("no game folder")?;
                archive::read_any(&dir, &path).map_err(|_| format!("the game has no icon {id}"))?.1
            }
        };
        let tpf = archive::undcx(&dcx).ok_or_else(|| format!("icon {id}: not a DCX file Kindle can read"))?;
        let dds = first_texture(&tpf).ok_or_else(|| format!("icon {id}: not a texture file Kindle can read"))?;
        kindle_core::dds::decode(dds).map_err(|e| format!("icon {id}: {e}"))
    })
}

/// A TPF's first texture: `TPF\0`, the count at +8, then 0x14-byte entries from +0x10 whose
/// first two u32 are the texture's offset and size (a DDS file on PC).
fn first_texture(tpf: &[u8]) -> Option<&[u8]> {
    let u32_at = |at: usize| tpf.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    if tpf.get(..4)? != b"TPF\0" || u32_at(8)? == 0 {
        return None;
    }
    let (offset, size) = (u32_at(0x10)?, u32_at(0x14)?);
    tpf.get(offset..offset.checked_add(size)?)
}

fn game_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_texture_file_gives_its_first_texture() {
        let mut tpf = b"TPF\0".to_vec();
        tpf.extend_from_slice(&4u32.to_le_bytes());
        tpf.extend_from_slice(&1u32.to_le_bytes());
        tpf.extend_from_slice(&[0, 3, 1, 0]);
        tpf.extend_from_slice(&0x24u32.to_le_bytes());
        tpf.extend_from_slice(&4u32.to_le_bytes());
        tpf.extend_from_slice(&[0u8; 12]);
        tpf.extend_from_slice(b"DDS ");
        assert_eq!(super::first_texture(&tpf), Some(&b"DDS "[..]));
        assert_eq!(super::first_texture(b"TPF\0\0\0\0\0\0\0\0\0"), None);
        assert_eq!(super::first_texture(b"BND4"), None);
    }
}
