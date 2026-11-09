//! `kindle` command-line tool.
//!
//!   kindle console [--port N]         interactive Lua console into the running game
//!   kindle run "<lua or :command>"    run one line and print the reply
//!   kindle run -f <file.lua>          run a Lua file and print the reply
//!   kindle new <id> [--template T] [--mods DIR]
//!                                     create a new mod (templates: menu, item, event, hud)
//!   kindle check <mod>                check a mod (manifest, syntax, API use, permissions)
//!   kindle pack <mod> [--out DIR]     check a mod and zip it for sharing
//!   kindle launch [profile]           start the game with Kindle (launcher.toml profiles)
//!
//! Run with no arguments (e.g. double-clicked) when launcher.toml exists, it starts the game.

mod check;
mod launch;
mod pack;

use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_PORT: u16 = 27850;
const END_OF_REPLY: &str = "\u{4}";
/// A newline inside a request (the protocol is one request per line).
const NEWLINE: char = '\u{1}';
/// The game's reply to unfinished code: send it again with more lines.
const INCOMPLETE: &str = "\u{5}";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port = flag_value(&args, "--port").and_then(|p| p.parse().ok()).unwrap_or(DEFAULT_PORT);
    let result = match args.first().map(String::as_str) {
        Some("console") => console(port),
        Some("run") => match (flag_value(&args, "-f"), positional(&args, 1)) {
            (Some(file), _) => std::fs::read_to_string(&file)
                .map_err(|e| format!("{file}: {e}"))
                .and_then(|code| run(port, &code.replace("\r\n", "\n").replace('\n', &NEWLINE.to_string()))),
            (None, Some(line)) => run(port, &line),
            (None, None) => Err("usage: kindle run \"<lua or :command>\" | kindle run -f <file.lua>".into()),
        },
        Some("new") => match positional(&args, 1) {
            Some(id) => new_mod(&id, flag_value(&args, "--template").as_deref(), flag_value(&args, "--mods").map(PathBuf::from)),
            None => Err(format!("usage: kindle new <mod-id> [--template {}] [--mods DIR]", template_names())),
        },
        Some("check") => match positional(&args, 1) {
            Some(m) => check_mod(&m).map(|_| ()),
            None => Err("usage: kindle check <mod folder or id>".into()),
        },
        Some("pack") => match positional(&args, 1) {
            Some(m) => pack_mod(&m, flag_value(&args, "--out").map(PathBuf::from)),
            None => Err("usage: kindle pack <mod folder or id> [--out DIR]".into()),
        },
        Some("launch") => launch_command(&args[1..]),
        None if kindle_dir().join("launcher.toml").is_file() => {
            // Double-clicked: start the game, and keep the window open if something's wrong.
            let result = launch::launch(&kindle_dir(), None);
            if let Err(e) = &result {
                eprintln!("kindle: {e}");
                eprint!("Press Enter to close.");
                let _ = io::stdin().lock().read_line(&mut String::new());
                return ExitCode::FAILURE;
            }
            return ExitCode::SUCCESS;
        }
        _ => {
            println!("{}", USAGE.trim());
            return ExitCode::SUCCESS;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("kindle: {e}");
            ExitCode::FAILURE
        }
    }
}

const USAGE: &str = r#"
kindle — Dark Souls III scripting runtime tools

  kindle console [--port N]        interactive Lua console into the running game
  kindle run "<lua or :command>"   run one line in the game and print the reply
  kindle run -f <file.lua>         run a whole Lua file in the game
  kindle new <mod-id> [--mods DIR] create a new mod (default DIR: .\mods next to kindle.exe)
  kindle check <mod>               check a mod: mod.toml, Lua syntax, API names, permissions
  kindle pack <mod> [--out DIR]    check a mod and zip it for sharing (<id>-<version>.zip)
  kindle launch [profile]          start the game with Kindle through Mod Engine 2
  kindle launch --setup            write launcher.toml (finds the game, Mod Engine 2, Seamless)
  kindle launch --list             show the launcher profiles
  kindle launch [profile] --direct start without Mod Engine 2 (Kindle loads the DLLs itself)
  kindle launch --import <name> <modengine config.toml>   add a profile from a Mod Engine config

<mod> is a mod folder, or the id of a mod in .\mods next to kindle.exe.

In the console, type Lua (e.g. flags.get(14000800)) or :help for commands.
"#;

/// A mod folder from a path, or a mod id in the mods folder next to kindle.exe.
fn mod_dir(arg: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(arg);
    if p.join("mod.toml").is_file() {
        return Ok(p);
    }
    let in_mods = default_mods_dir().join(arg);
    if in_mods.join("mod.toml").is_file() {
        return Ok(in_mods);
    }
    Err(format!("{arg}: no mod.toml there (give a mod folder, or the id of a mod in {})", default_mods_dir().display()))
}

/// Prints the check's findings; Err if there were errors.
fn check_mod(arg: &str) -> Result<check::Report, String> {
    let dir = mod_dir(arg)?;
    let report = check::check(&dir);
    for e in &report.errors {
        println!("error: {e}");
    }
    for w in &report.warnings {
        println!("warning: {w}");
    }
    let name = report.manifest.as_ref().map(|m| m.info.id.clone()).unwrap_or_else(|| dir.display().to_string());
    if report.errors.is_empty() {
        println!(
            "{name}: ok ({} Lua file(s){})",
            report.files.len(),
            match report.warnings.len() {
                0 => String::new(),
                n => format!(", {n} warning(s)"),
            }
        );
        Ok(report)
    } else {
        Err(format!("{name}: {} error(s)", report.errors.len()))
    }
}

fn pack_mod(arg: &str, out: Option<PathBuf>) -> Result<(), String> {
    let report = check_mod(arg)?;
    let dir = mod_dir(arg)?;
    let m = report.manifest.ok_or("mod.toml is unreadable")?;
    let out = out.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let (path, count) = pack::pack(&dir, &m.info.id, m.info.version.as_deref().unwrap_or(""), &out)?;
    println!("packed {count} file(s) into {}", path.display());
    println!("To install: unzip it into the game's Game\\kindle\\mods folder.");
    Ok(())
}

fn flag_value(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

/// The n-th argument that isn't a `--flag` or a flag's value.
fn positional(args: &[String], n: usize) -> Option<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            skip = true;
            continue;
        }
        out.push(a.clone());
    }
    out.get(n).cloned()
}

fn connect(port: u16) -> Result<(TcpStream, BufReader<TcpStream>), String> {
    let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| {
        format!("cannot reach the game on 127.0.0.1:{port} ({e}). Is DS3 running with Kindle loaded, and console = true in kindle\\kindle.toml?")
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
    let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    Ok((stream, reader))
}

fn exchange(stream: &mut TcpStream, reader: &mut BufReader<TcpStream>, line: &str) -> Result<String, String> {
    writeln!(stream, "{line}").map_err(|e| format!("connection lost: {e}"))?;
    let mut reply = Vec::new();
    loop {
        let mut l = String::new();
        let n = reader.read_line(&mut l).map_err(|e| format!("connection lost: {e}"))?;
        if n == 0 {
            return Err("connection closed by the game".into());
        }
        let l = l.trim_end_matches(['\r', '\n']);
        if l == END_OF_REPLY {
            break;
        }
        reply.push(l.to_string());
    }
    Ok(reply.join("\n"))
}

fn run(port: u16, line: &str) -> Result<(), String> {
    let (mut stream, mut reader) = connect(port)?;
    let reply = exchange(&mut stream, &mut reader, line)?;
    if !reply.is_empty() {
        println!("{reply}");
    }
    if reply == INCOMPLETE {
        return Err("the code is unfinished (an open function, if or table?)".into());
    }
    if reply.starts_with("error:") { Err("the command failed".into()) } else { Ok(()) }
}

fn console(port: u16) -> Result<(), String> {
    let (mut stream, mut reader) = connect(port)?;
    let status = exchange(&mut stream, &mut reader, ":status")?;
    println!("{status}\nType Lua, :help for commands, Ctrl+C to quit.");
    let stdin = io::stdin();
    // Lines of code that isn't finished yet (an open function, if or table).
    let mut pending: Vec<String> = Vec::new();
    loop {
        print!("{}", if pending.is_empty() { "kindle> " } else { "    ... " });
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Ok(());
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if pending.is_empty() {
            if line.trim().is_empty() {
                continue;
            }
            if matches!(line.trim(), "exit" | "quit" | ":q") {
                return Ok(());
            }
        }
        pending.push(line.to_string());
        let request = pending.join(&NEWLINE.to_string());
        let reply = exchange(&mut stream, &mut reader, &request)?;
        if reply == INCOMPLETE {
            continue;
        }
        pending.clear();
        if !reply.is_empty() {
            println!("{reply}");
        }
    }
}

// ── kindle new ──────────────────────────────────────────────────────────────

/// The folder kindle.exe is in (the game's `kindle` folder once installed).
fn kindle_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."))
}

fn launch_command(args: &[String]) -> Result<(), String> {
    let dir = kindle_dir();
    match args.first().map(String::as_str) {
        Some("--setup") => launch::setup(&dir),
        Some("--list") => launch::list(&dir),
        Some("--import") => match (args.get(1), args.get(2)) {
            (Some(name), Some(config)) => launch::import(&dir, name, Path::new(config)),
            _ => Err("usage: kindle launch --import <profile name> <modengine config.toml>".into()),
        },
        Some(flag) if flag.starts_with("--") && flag != "--direct" => Err(format!("unknown option {flag} (--setup, --list, --import, --direct)")),
        _ => {
            let direct = args.iter().any(|a| a == "--direct");
            let profile = args.iter().find(|a| !a.starts_with("--")).cloned();
            launch::launch_with(&dir, profile, direct)
        }
    }
}

fn default_mods_dir() -> PathBuf {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("mods"))).unwrap_or_else(|| PathBuf::from("mods"))
}

/// A starter mod: its name for --template, the modules it needs and its main.lua.
struct Template {
    name: &'static str,
    modules: &'static [&'static str],
    main: &'static str,
}

const TEMPLATES: &[Template] = &[
    Template { name: "menu", modules: &["ui", "player"], main: MENU_LUA },
    Template { name: "item", modules: &["params", "items", "text"], main: ITEM_LUA },
    Template { name: "event", modules: &["flags", "items", "ui"], main: EVENT_LUA },
    Template { name: "hud", modules: &["player", "ui"], main: HUD_LUA },
];

fn template_names() -> String {
    TEMPLATES.iter().map(|t| t.name).collect::<Vec<_>>().join("|")
}

fn new_mod(id: &str, template: Option<&str>, mods: Option<PathBuf>) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || id.is_empty() {
        return Err("mod id must be letters, digits, '-' or '_'".into());
    }
    let (modules, main): (&[&str], &str) = match template {
        None => (&["flags", "items", "player"], MAIN_LUA),
        Some(name) => TEMPLATES
            .iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .map(|t| (t.modules, t.main))
            .ok_or_else(|| format!("no template {name:?} (templates: {})", template_names().replace('|', ", ")))?,
    };
    let mods = mods.unwrap_or_else(default_mods_dir);
    let dir = mods.join(id);
    if dir.exists() {
        return Err(format!("{} already exists", dir.display()));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let name = id
        .split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");

    let modules = modules.iter().map(|m| format!("{m:?}")).collect::<Vec<_>>().join(", ");
    let manifest =
        MOD_TOML.replace("{id}", id).replace("{name}", &name).replace("{api}", &kindle_manifest::API_VERSION.to_string()).replace("{modules}", &modules);
    write(&dir.join("mod.toml"), &manifest)?;
    write(&dir.join("main.lua"), &main.replace("{name}", &name))?;

    // Point the Lua language server at Kindle's API stubs for autocomplete: the ones next to
    // kindle.exe, so mods kept outside the game folder get them too.
    let stubs = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("stubs")))
        .filter(|p| p.is_dir())
        .or_else(|| mods.parent().map(|k| k.join("stubs")).filter(|p| p.is_dir()));
    let library = stubs.map(|p| format!("[{:?}]", p.to_string_lossy())).unwrap_or_else(|| "[]".into());
    write(&dir.join(".luarc.json"), &LUARC.replace("{library}", &library))?;

    println!("created {}", dir.display());
    println!("Kindle loads it automatically if the game is running; edit main.lua and save to reload.");
    Ok(())
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("{}: {e}", path.display()))
}

const MOD_TOML: &str = r#"[mod]
id = "{id}"
name = "{name}"
version = "0.1.0"
author = ""
description = ""
# The Kindle API version this mod is written for.
api = {api}
# The games this mod is for, by id (ds3: Dark Souls III).
games = ["ds3"]

[permissions]
# Game modules this mod may use (kindle check says which ones it needs).
modules = [{modules}]
"#;

const MAIN_LUA: &str = r#"-- {name}
local log, flags, items, player = kindle.log, kindle.flags, kindle.items, kindle.player

kindle.on("player_loaded", function()
  log.info("Hello, %s!", player.name())
end)

-- Example: react to an event flag turning on.
-- flags.on_set(14000800, function()
--   items.give(items.goods(380), 1)
-- end)
"#;

const MENU_LUA: &str = r#"-- {name}: an option in every bonfire's menu that opens a list of its own.
-- The Handmaid's shop stock: ShopLineupParam rows 110000-119899.
local HANDMAID_FIRST, HANDMAID_LAST = 110000, 119899

kindle.ui.add_menu_option {
  text = "{name}",
  where = "bonfire", -- or "npc": the menus of NPCs that sell things
  on_select = function()
    kindle.ui.menu {
      { text = "Restore HP and FP", on_select = function() kindle.player.heal() end },
      { text = "Refill estus", on_select = function() kindle.player.refill_estus() end },
      { text = "Open the Handmaid's shop", on_select = function() kindle.ui.shop(HANDMAID_FIRST, HANDMAID_LAST) end },
    }
  end,
}
"#;

const ITEM_LUA: &str = r#"-- {name}: a brand-new goods item, a copy of Ember with its own id, name and description.
-- The id is yours to choose (above every existing goods id); keep it fixed: saves remember it,
-- and a save holding the item needs this mod when it loads.
local ID = 4000000
local row = kindle.params.add_row("EquipParamGoods", ID, "Ember") -- change fields on row
local item = kindle.items.goods(ID)
kindle.items.set_name(item, "{name}")
kindle.items.set_description(item, "A new item, made by a Kindle mod.")

-- Each character gets one, once.
kindle.on("player_loaded", function()
  if not kindle.store.get("given") then
    kindle.items.give(item)
    kindle.store.set("given", true)
  end
end)
"#;

const EVENT_LUA: &str = r#"-- {name}: react to the game's progress. Every boss and bonfire has a named flag
-- (kindle.flags.boss.*, kindle.flags.bonfire.*); search items from the console with
-- kindle.items.find("estus").
kindle.flags.on_set(kindle.flags.boss.iudex_gundyr, function()
  kindle.items.give("Ember") -- shows the game's "item received" box
end)

kindle.on("boss_defeated", function(kind)
  kindle.ui.toast(kind == "lord_of_cinder" and "A Lord of Cinder has fallen!" or "Boss down!")
end)
"#;

const HUD_LUA: &str = r#"-- {name}: a warning on screen when HP drops below a quarter.
local warned = false
kindle.every(0.5, function()
  if not kindle.player.loaded() then return end
  local low = kindle.player.hp() < kindle.player.max_hp() * 0.25
  if low and not warned then
    kindle.ui.toast("Low HP!", { color = "red" })
  end
  warned = low
end)
"#;

const LUARC: &str = r#"{
  "runtime": { "version": "Lua 5.4" },
  "workspace": { "library": {library} },
  "diagnostics": { "globals": ["kindle"] }
}
"#;
