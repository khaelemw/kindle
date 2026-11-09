//! `kindle pack`: checks a mod and zips it for sharing, as `<id>-<version>.zip` holding the
//! mod's folder (so it unzips straight into `kindle\mods`). Editor files (`.luarc.json`),
//! hidden files and earlier zips stay out.

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

fn files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.ends_with(".zip") {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            files(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// Writes the zip; returns its path and how many files went in.
pub fn pack(dir: &Path, id: &str, version: &str, out_dir: &Path) -> Result<(PathBuf, usize), String> {
    let mut list = Vec::new();
    files(dir, &mut list).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = if version.is_empty() { format!("{id}.zip") } else { format!("{id}-{version}.zip") };
    let path = out_dir.join(name);
    let file = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for f in &list {
        let rel = f.strip_prefix(dir).unwrap_or(f).to_string_lossy().replace('\\', "/");
        let data = std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?;
        zip.start_file(format!("{id}/{rel}"), options).map_err(|e| e.to_string())?;
        zip.write_all(&data).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok((path, list.len()))
}
