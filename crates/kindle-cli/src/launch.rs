//! `kindle launch`: starts the game with Kindle from the profiles in `launcher.toml` (see the
//! kindle-launch crate, shared with the Kindle Launcher window).
//!
//!   kindle launch [profile]        start the game (no profile: pick one, or the default)
//!   kindle launch --list           show the profiles
//!   kindle launch --setup          write launcher.toml from what's installed
//!   kindle launch --import <name> <modengine config.toml>   add a profile from a Mod Engine config
//!   kindle launch [profile] --direct   start without Mod Engine 2 (Kindle loads the DLLs)

use std::io::{BufRead, Write};
use std::path::Path;

use kindle_launch::{Launcher, Started};

pub fn setup(kindle_dir: &Path) -> Result<(), String> {
    let (_, notes) = kindle_launch::setup(kindle_dir)?;
    for n in notes {
        println!("{n}");
    }
    list(kindle_dir)
}

pub fn import(kindle_dir: &Path, name: &str, config: &Path) -> Result<(), String> {
    let path = kindle_launch::import(kindle_dir, name, config)?;
    println!("added profile {name:?} to {}", path.display());
    Ok(())
}

pub fn list(kindle_dir: &Path) -> Result<(), String> {
    let l = kindle_launch::load_synced(kindle_dir)?;
    for (name, p) in &l.profiles {
        let mark = if *name == l.default { " (default)" } else { "" };
        println!("{name}{mark}");
        for id in p.kindle_mods.iter().flatten() {
            println!("    kindle  {id}");
        }
        for d in &p.dlls {
            let off = if p.dlls_off.contains(d) { " (off)" } else { "" };
            println!("    dll  {}{off}", d.display());
        }
        for m in &p.file_mods {
            let off = if m.enabled { "" } else { " (off)" };
            println!("    mod  {} ({}){off}", m.name, m.path.display());
        }
    }
    Ok(())
}

/// Asks which profile to start when there's more than one.
fn pick(l: &Launcher) -> Result<String, String> {
    let names: Vec<&String> = l.profiles.keys().collect();
    match names.as_slice() {
        [] => Err(format!("{} has no profiles: run `kindle launch --setup`", kindle_launch::FILE)),
        [one] => Ok((*one).clone()),
        _ => {
            for (i, n) in names.iter().enumerate() {
                let mark = if **n == l.default { " (default)" } else { "" };
                println!("  {}) {n}{mark}", i + 1);
            }
            print!("Start which? [Enter = {}] ", l.default);
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line).map_err(|e| e.to_string())?;
            let line = line.trim();
            if line.is_empty() {
                return Ok(l.default.clone());
            }
            if let Ok(i) = line.parse::<usize>()
                && (1..=names.len()).contains(&i)
            {
                return Ok(names[i - 1].clone());
            }
            Ok(line.to_string())
        }
    }
}

/// `kindle launch [profile]`.
pub fn launch(kindle_dir: &Path, profile: Option<String>) -> Result<(), String> {
    launch_with(kindle_dir, profile, false)
}

/// `kindle launch [profile] [--direct]`.
pub fn launch_with(kindle_dir: &Path, profile: Option<String>, force_direct: bool) -> Result<(), String> {
    let l = kindle_launch::load_synced(kindle_dir)?;
    let name = match profile {
        Some(p) => p,
        None => pick(&l)?,
    };
    match kindle_launch::start(kindle_dir, &l, &name, force_direct)? {
        Started::Direct(pid) => println!("started Dark Souls III (process {pid}) with profile {name:?}, without Mod Engine 2"),
        Started::ModEngine => println!("starting Dark Souls III with profile {name:?}"),
    }
    Ok(())
}
