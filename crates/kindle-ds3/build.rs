//! Embeds every DS3 paramdef XML into the DLL so scripts get by-name param access
//! without shipping loose files.

use std::{env, fs, path::Path};

fn main() {
    // (Cargo's value as the script runs: a script built for one checkout can run for another.)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let defs_dir = Path::new(&manifest_dir).join("../../paramdefs/darksouls3");
    println!("cargo:rerun-if-changed={}", defs_dir.display());

    let mut entries: Vec<_> = fs::read_dir(&defs_dir)
        .expect("paramdefs/darksouls3 missing")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "xml"))
        .collect();
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
