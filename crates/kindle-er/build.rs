//! Embeds Elden Ring's paramdef XMLs into the DLL so scripts get by-name param access without
//! shipping loose files: those in paramdefs/eldenring, or in the folder KINDLE_ER_PARAMDEFS names
//! (a local build's own copy). With neither, none: Kindle-ER then maps no tables.

use std::{env, fs, path::Path, path::PathBuf};

fn main() {
    // (Cargo's value as the script runs: a script built for one checkout can run for another.)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    println!("cargo:rerun-if-env-changed=KINDLE_ER_PARAMDEFS");
    let defs_dir = env::var_os("KINDLE_ER_PARAMDEFS").map(PathBuf::from).unwrap_or_else(|| Path::new(&manifest_dir).join("../../paramdefs/eldenring"));
    // (A folder that isn't there yet: its parent, so adding it is seen.)
    let watched = if defs_dir.is_dir() { defs_dir.clone() } else { defs_dir.parent().map(Path::to_path_buf).unwrap_or_else(|| defs_dir.clone()) };
    println!("cargo:rerun-if-changed={}", watched.display());

    let mut entries: Vec<_> = fs::read_dir(&defs_dir)
        .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "xml")).collect())
        .unwrap_or_default();
    entries.sort();

    let mut out = String::from("pub static PARAMDEFS: &[(&str, &str)] = &[\n");
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy();
        let abs = fs::canonicalize(&path).unwrap();
        out += &format!("    ({name:?}, include_str!({:?})),\n", abs.to_string_lossy());
    }
    out += "];\n";

    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("paramdefs.rs"), out).unwrap();
}
