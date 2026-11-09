//! Generates the API reference in `docs/reference` from `dist/stubs/kindle.lua`.
//!
//! The stubs are the single source of truth: the editor reads them for autocomplete and this
//! turns the same annotations into Markdown, one page per module plus a page of types, so the
//! two can't drift apart. It then builds `docs/kindle-docs.html`: the guides and the
//! reference as one searchable page. Run from the repository root: `cargo run -p kindle-docs`.

mod site;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

const STUBS: &str = "dist/stubs/kindle.lua";
const OUT: &str = "docs/reference";
const SITE: &str = "docs/kindle-docs.html";

/// Modules in the order the reference lists them.
const ORDER: &[&str] = &[
    "kindle",
    "kindle.log",
    "kindle.settings",
    "kindle.store",
    "kindle.mods",
    "kindle.json",
    "kindle.flags",
    "kindle.items",
    "kindle.text",
    "kindle.ui",
    "kindle.player",
    "kindle.world",
    "kindle.events",
    "kindle.hks",
    "kindle.params",
    "kindle.maps",
    "kindle.game",
    "kindle.net",
    "kindle.input",
    "kindle.debug",
];

/// Names Kindle generates from the player's game (in kindle-game.lua), not in the stubs.
const GENERATED: &[(&str, &str)] = &[
    ("kindle.goods_name", "A goods item's name, e.g. \"Ember\"."),
    ("kindle.weapon_name", "A weapon's name (shields, catalysts and ammunition included), e.g. \"Uchigatana\". Add \" +N\" for an upgrade level."),
    ("kindle.armor_name", "An armor piece's name."),
    ("kindle.ring_name", "A ring's name."),
    ("kindle.item_name", "Any item's name."),
    ("kindle.animation_name", "An animation event the player's behaviour accepts, e.g. \"W_Rolling\"."),
    ("kindle.bonfire_name", "A bonfire's name, e.g. \"Firelink Shrine\"."),
    ("kindle.param_table", "A param table's name, e.g. \"EquipParamWeapon\". Each table also has a row type of the same name listing its fields."),
];

#[derive(Default)]
struct Param {
    name: String,
    ty: String,
    desc: String,
}

#[derive(Default)]
struct Function {
    name: String,
    doc: Vec<String>,
    params: Vec<Param>,
    returns: Vec<Param>,
    overloads: Vec<String>,
}

#[derive(Default)]
struct Class {
    name: String,
    doc: Vec<String>,
    permission: Option<String>,
    fields: Vec<Param>,
}

struct Alias {
    name: String,
    ty: String,
    doc: Vec<String>,
}

/// A documented value, e.g. `kindle.on_unload` or `kindle.hks.env`.
struct Value {
    name: String,
    ty: String,
    doc: Vec<String>,
}

#[derive(Default)]
struct Api {
    modules: BTreeMap<String, Class>,
    classes: Vec<Class>,
    aliases: Vec<Alias>,
    functions: Vec<Function>,
    values: Vec<Value>,
}

/// Splits a type expression off the front of `s`: balanced brackets, string literals and
/// `fun(...): ret` are one type. Returns (type, rest).
fn split_type(s: &str) -> (String, &str) {
    let s = s.trim_start();
    let b = s.as_bytes();
    let (mut i, mut depth, mut in_str) = (0, 0i32, false);
    while i < b.len() {
        let c = b[i];
        if in_str {
            in_str = c != b'"';
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'(' | b'{' | b'[' | b'<' => depth += 1,
            // An unbalanced closer ends the type (e.g. the `)` closing an overload).
            b')' | b'}' | b']' | b'>' if depth == 0 => break,
            b')' | b'}' | b']' | b'>' => depth -= 1,
            b':' if depth == 0 && s[..i].starts_with("fun") => {
                i += 1;
                while i < b.len() && b[i] == b' ' {
                    i += 1;
                }
                continue;
            }
            b' ' | b'\t' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    (s[..i].to_string(), s[i..].trim_start())
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// `---@return` may list several values: `number x, number y`.
fn parse_returns(mut rest: &str) -> Vec<Param> {
    let mut out = Vec::new();
    loop {
        let (ty, after) = split_type(rest);
        let (word, tail) = after.split_once(char::is_whitespace).unwrap_or((after, ""));
        let word_clean = word.trim_end_matches(',');
        let (name, mut desc) = if is_ident(word_clean) { (word_clean.to_string(), tail.trim()) } else { (String::new(), after.trim()) };
        let more = word.ends_with(',') || desc.starts_with(", ");
        if desc.starts_with(", ") {
            desc = &desc[2..];
        }
        if more && !desc.is_empty() {
            out.push(Param { name, ty, desc: String::new() });
            rest = desc;
            continue;
        }
        out.push(Param { name, ty, desc: desc.to_string() });
        return out;
    }
}

/// `name type desc` (params and fields; a `?` on the name marks it optional).
fn parse_named(rest: &str) -> Param {
    let (name, after) = rest.trim().split_once(char::is_whitespace).unwrap_or((rest.trim(), ""));
    let (mut ty, desc) = split_type(after);
    let mut name = name.to_string();
    if let Some(n) = name.strip_suffix('?') {
        name = n.to_string();
        if !ty.ends_with('?') {
            ty.push('?');
        }
    }
    Param { name, ty, desc: desc.to_string() }
}

#[derive(Default)]
struct Block {
    text: Vec<String>,
    class: Option<Class>,
    ty: Option<String>,
    func: Function,
}

fn parse(src: &str) -> Api {
    let mut api = Api::default();
    let mut block = Block::default();
    for raw in src.lines() {
        let line = raw.trim_end();
        if line.starts_with("---@meta") {
            continue;
        }
        if let Some(tag) = line.strip_prefix("---@") {
            let (kind, rest) = tag.split_once(' ').unwrap_or((tag, ""));
            match kind {
                "alias" => {
                    let (name, after) = rest.split_once(' ').unwrap_or((rest, ""));
                    let (ty, desc) = split_type(after);
                    let mut doc = std::mem::take(&mut block.text);
                    if !desc.is_empty() {
                        doc.insert(0, desc.to_string());
                    }
                    api.aliases.push(Alias { name: name.to_string(), ty, doc });
                }
                "class" => {
                    let (name, inline) = rest.split_once(' ').unwrap_or((rest, ""));
                    let mut doc = std::mem::take(&mut block.text);
                    let mut inline = inline.trim().to_string();
                    let mut permission = None;
                    if let Some(start) = inline.find("(permission: \"") {
                        let tail = &inline[start + 14..];
                        if let Some(end) = tail.find("\")") {
                            permission = Some(tail[..end].to_string());
                            inline = format!("{}{}", &inline[..start], &tail[end + 2..]).trim().to_string();
                        }
                    }
                    if !inline.is_empty() {
                        doc.push(inline);
                    }
                    block.class = Some(Class { name: name.to_string(), doc, permission, fields: Vec::new() });
                }
                "field" => {
                    if let Some(c) = &mut block.class {
                        c.fields.push(parse_named(rest));
                    }
                }
                "param" => block.func.params.push(parse_named(rest)),
                "return" => block.func.returns.extend(parse_returns(rest)),
                "overload" => block.func.overloads.push(rest.trim().to_string()),
                "type" => block.ty = Some(rest.trim().to_string()),
                _ => {} // generic, meta: not shown
            }
            continue;
        }
        if let Some(text) = line.strip_prefix("---") {
            let text = text.strip_prefix(' ').unwrap_or(text);
            match &mut block.class {
                // Lines after a class tag (before its fields) continue its description.
                Some(c) if c.fields.is_empty() => c.doc.push(text.to_string()),
                _ => block.text.push(text.to_string()),
            }
            continue;
        }
        if line.starts_with("--") {
            continue;
        }
        if line.trim().is_empty() {
            if let Some(c) = block.class.take() {
                api.classes.push(c);
            }
            block = Block::default();
            continue;
        }
        if let Some(sig) = line.strip_prefix("function ") {
            let name = sig.split('(').next().unwrap_or("").trim().to_string();
            let mut f = std::mem::take(&mut block.func);
            f.name = name;
            f.doc = std::mem::take(&mut block.text);
            api.functions.push(f);
            if let Some(c) = block.class.take() {
                api.classes.push(c);
            }
            block = Block::default();
            continue;
        }
        if let Some((name, _)) = line.split_once(" = ") {
            let name = name.trim().to_string();
            match (block.class.take(), block.ty.take()) {
                (Some(c), Some(ty)) => {
                    api.values.push(Value { name, ty, doc: std::mem::take(&mut block.text) });
                    api.classes.push(c);
                }
                (Some(c), None) => {
                    api.modules.insert(name, c);
                }
                (None, Some(ty)) => api.values.push(Value { name, ty, doc: std::mem::take(&mut block.text) }),
                (None, None) => {}
            }
            block = Block::default();
        }
    }
    api
}

// ── rendering ────────────────────────────────────────────────────────────────

/// Doc lines as Markdown: lines join into paragraphs; empty lines break paragraphs; `* `
/// lines start list items (indented lines continue them).
fn prose(lines: &[String]) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for l in lines {
        let t = l.trim();
        // Fenced code blocks pass through line by line.
        if t.starts_with("```") || in_code {
            if t.starts_with("```") {
                in_code = !in_code;
            }
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str("\n\n");
            }
            out.push_str(l.trim_end());
            out.push('\n');
            if !in_code {
                out.push('\n');
            }
            continue;
        }
        if t.is_empty() {
            out.push_str("\n\n");
        } else if t.starts_with("* ") || t.starts_with("- ") {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("- ");
            out.push_str(&t[2..]);
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(t);
        }
    }
    out.trim().to_string()
}

/// The first sentence, for summaries.
fn summary(lines: &[String]) -> String {
    let p = prose(lines);
    let first = p.lines().next().unwrap_or("");
    match first.find(". ") {
        Some(i) => first[..=i].to_string(),
        None => first.to_string(),
    }
}

fn page_of(module: &str) -> String {
    match module.strip_prefix("kindle.") {
        Some(m) => format!("{m}.md"),
        None => "core.md".into(),
    }
}

/// A type in inline code, each documented type in it linked. `in_table` escapes the union
/// bars that would otherwise split a table cell.
fn type_link(ty: &str, types: &[String], in_table: bool) -> String {
    // Unions of plain names link each member; anything structured stays one code span.
    let simple = !ty.contains(['(', '{', '<', ' ']);
    let parts: Vec<&str> = if simple { ty.split('|').collect() } else { vec![ty] };
    let linked: Vec<String> = parts
        .iter()
        .map(|part| {
            let base = part.trim_end_matches('?').trim_end_matches("[]");
            if types.iter().any(|t| t == base) {
                format!("[`{part}`](types.md#{})", anchor(base))
            } else if in_table {
                format!("`{}`", part.replace('|', "\\|"))
            } else {
                format!("`{part}`")
            }
        })
        .collect();
    linked.join(if in_table { " \\| " } else { " | " })
}

fn anchor(name: &str) -> String {
    name.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect()
}

fn signature(f: &Function) -> String {
    let params: Vec<String> = f.params.iter().map(|p| format!("{}: {}", p.name, p.ty)).collect();
    let rets: Vec<String> = f.returns.iter().map(|r| r.ty.clone()).collect();
    let ret = if rets.is_empty() { String::new() } else { format!(" -> {}", rets.join(", ")) };
    format!("{}({}){}", f.name, params.join(", "), ret)
}

/// `kindle.on`'s overloads as an event table: (events, handler).
fn event_rows(f: &Function) -> Vec<(String, String)> {
    f.overloads
        .iter()
        .filter_map(|o| {
            let events = o.split_once("event: ")?.1;
            let (events, _) = split_type(events);
            let (handler, _) = split_type(o.split_once("fn: ")?.1);
            Some((events.trim_end_matches(',').replace('"', "").replace('|', ", "), handler))
        })
        .collect()
}

fn render_function(out: &mut String, f: &Function, types: &[String]) {
    let short = f.name.rsplit('.').next().unwrap_or(&f.name);
    let _ = writeln!(out, "### {short}\n");
    let _ = writeln!(out, "```lua\n{}\n```\n", signature(f));
    let doc = prose(&f.doc);
    if !doc.is_empty() {
        let _ = writeln!(out, "{doc}\n");
    }
    if !f.params.is_empty() {
        out.push_str("**Parameters**\n\n");
        for p in &f.params {
            let desc = if p.desc.is_empty() { String::new() } else { format!(": {}", p.desc) };
            let _ = writeln!(out, "- `{}` {}{desc}", p.name, type_link(&p.ty, types, false));
        }
        out.push('\n');
    }
    if !f.returns.is_empty() {
        out.push_str("**Returns**\n\n");
        for r in &f.returns {
            let name = if r.name.is_empty() { String::new() } else { format!(" `{}`", r.name) };
            let desc = if r.desc.is_empty() { String::new() } else { format!(": {}", r.desc) };
            let _ = writeln!(out, "-{name} {}{desc}", type_link(&r.ty, types, false));
        }
        out.push('\n');
    }
    if f.name == "kindle.on" {
        let rows = event_rows(f);
        if !rows.is_empty() {
            out.push_str("**Handlers by event**\n\n| Event | Handler |\n|---|---|\n");
            for (events, handler) in rows {
                let _ = writeln!(out, "| `{}` | `{}` |", events.replace('|', "\\|"), handler.replace('|', "\\|"));
            }
            out.push('\n');
        }
    }
}

fn render_fields(out: &mut String, fields: &[Param], types: &[String]) {
    out.push_str("| Field | Type | Description |\n|---|---|---|\n");
    for p in fields {
        let _ = writeln!(out, "| `{}` | {} | {} |", p.name, type_link(&p.ty, types, true), p.desc.replace('|', "\\|"));
    }
    out.push('\n');
}

const HEADER: &str = "<!-- Generated from dist/stubs/kindle.lua by `cargo run -p kindle-docs`. Don't edit. -->\n\n";

fn render_module(name: &str, class: &Class, api: &Api, types: &[String]) -> String {
    let mut out = String::from(HEADER);
    let _ = writeln!(out, "# {name}\n");
    if let Some(p) = &class.permission {
        let _ = writeln!(out, "> Needs the `\"{p}\"` permission in `mod.toml`.\n");
    }
    let doc = prose(&class.doc);
    if !doc.is_empty() {
        let _ = writeln!(out, "{doc}\n");
    }
    let module_of = |n: &str| n.rsplit_once('.').map(|(m, _)| m.to_string()).unwrap_or_default();
    if !class.fields.is_empty() {
        out.push_str("## Fields\n\n");
        render_fields(&mut out, &class.fields, types);
    }
    let values: Vec<&Value> = api.values.iter().filter(|v| module_of(&v.name) == name).collect();
    let functions: Vec<&Function> = api.functions.iter().filter(|f| module_of(&f.name) == name).collect();
    if !functions.is_empty() {
        out.push_str("## Functions\n\n");
        for f in &functions {
            let short = f.name.rsplit('.').next().unwrap_or(&f.name);
            let s = summary(&f.doc);
            let _ = writeln!(out, "- [`{short}`](#{}){}", anchor(short), if s.is_empty() { String::new() } else { format!(": {s}") });
        }
        out.push('\n');
    }
    for v in &values {
        let short = v.name.rsplit('.').next().unwrap_or(&v.name);
        let _ = writeln!(out, "### {short}\n");
        let _ = writeln!(out, "`{}`: {}\n", v.name, type_link(&v.ty, types, false));
        let doc = prose(&v.doc);
        if !doc.is_empty() {
            let _ = writeln!(out, "{doc}\n");
        }
        // A value typed by a class (e.g. hks.env) lists that class's fields here.
        if let Some(c) = api.classes.iter().find(|c| c.name == v.ty) {
            render_fields(&mut out, &c.fields, types);
        }
    }
    for f in functions {
        render_function(&mut out, f, types);
    }
    out
}

fn render_types(api: &Api, types: &[String]) -> String {
    let mut out = String::from(HEADER);
    out.push_str("# Types\n\nData that functions take and return, and the names of things the API accepts.\n\n");
    out.push_str("## Tables\n\n");
    let value_types: Vec<&str> = api.values.iter().map(|v| v.ty.as_str()).collect();
    for c in api.classes.iter().filter(|c| !value_types.contains(&c.name.as_str())) {
        let _ = writeln!(out, "### {}\n", c.name);
        let doc = prose(&c.doc);
        if !doc.is_empty() {
            let _ = writeln!(out, "{doc}\n");
        }
        if !c.fields.is_empty() {
            render_fields(&mut out, &c.fields, types);
        }
    }
    out.push_str("## Aliases\n\n");
    for a in &api.aliases {
        let _ = writeln!(out, "### {}\n", a.name);
        let doc = prose(&a.doc);
        if !doc.is_empty() {
            let _ = writeln!(out, "{doc}\n");
        }
        let _ = writeln!(out, "```lua\n{}\n```\n", a.ty.replace('|', " | "));
    }
    out.push_str(
        "## Names from your game\n\nKindle writes these to `kindle\\stubs\\kindle-game.lua` from your own game the first time you \
         load into the world, so your editor can complete them (in your game's language).\n\n",
    );
    for (name, doc) in GENERATED {
        let _ = writeln!(out, "### {name}\n\n{doc}\n");
    }
    out
}

fn render_index(api: &Api) -> String {
    let mut out = String::from(HEADER);
    out.push_str(
        "# API reference\n\nEverything a mod can call. Most modules need a permission in the mod's `mod.toml` \
         (`[permissions] modules = [...]`); the core functions and `log` are always available.\n\n\
         | Module | Permission | |\n|---|---|---|\n",
    );
    for name in ORDER {
        let Some(c) = api.modules.get(*name) else { continue };
        let perm = c.permission.as_deref().map(|p| format!("`\"{p}\"`")).unwrap_or_else(|| "always".into());
        let s = summary(&c.doc);
        let _ = writeln!(out, "| [`{name}`]({}) | {perm} | {s} |", page_of(name));
    }
    out.push_str("\nAlso: [Types](types.md), the tables and names functions take and return.\n");
    out
}

fn main() -> Result<(), String> {
    let src = std::fs::read_to_string(STUBS).map_err(|e| format!("{STUBS}: {e} (run from the repository root)"))?;
    let api = parse(&src);
    // Classes that only type a value (e.g. hks.env) are shown with the value, not on the types page.
    let value_types: Vec<&str> = api.values.iter().map(|v| v.ty.as_str()).collect();
    let mut types: Vec<String> = api.classes.iter().filter(|c| !value_types.contains(&c.name.as_str())).map(|c| c.name.clone()).collect();
    types.extend(api.aliases.iter().map(|a| a.name.clone()));
    types.extend(GENERATED.iter().map(|(n, _)| n.to_string()));

    let out = Path::new(OUT);
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let write = |file: &str, text: String| std::fs::write(out.join(file), text).map_err(|e| format!("{file}: {e}"));
    write("README.md", render_index(&api))?;
    write("types.md", render_types(&api, &types))?;
    let mut pages = 2;
    for name in ORDER {
        if let Some(c) = api.modules.get(*name) {
            write(&page_of(name), render_module(name, c, &api, &types))?;
            pages += 1;
        }
    }
    let missing: Vec<&String> = api.modules.keys().filter(|m| !ORDER.contains(&m.as_str())).collect();
    if !missing.is_empty() {
        return Err(format!("modules missing from ORDER: {missing:?}"));
    }
    println!("{pages} pages, {} functions, {} types", api.functions.len(), types.len());

    // The single-page site: guides, then the reference in the same order.
    let page = |slug: &str, file: &str, title: &str, group| site::Page { slug: slug.into(), file: file.into(), title: title.into(), group };
    let mut site_pages = vec![
        page("home", "README.md", "Overview", "Guides"),
        page("getting-started", "getting-started.md", "Getting started", "Guides"),
        page("events", "events.md", "Events and hooks", "Guides"),
        page("cookbook", "cookbook.md", "Cookbook", "Guides"),
        page("mod-tours", "mod-tours.md", "Mod tours", "Guides"),
        page("mod-options", "mod-options.md", "Ways to do it", "Guides"),
        page("mod-errors", "mod-errors.md", "When a mod goes wrong", "Guides"),
        page("reference", "reference/README.md", "All modules", "Reference"),
    ];
    for name in ORDER {
        let file = page_of(name);
        let slug = file.trim_end_matches(".md");
        // "events" is taken by the guide.
        let slug = if slug == "events" { "module-events" } else { slug };
        site_pages.push(page(slug, &format!("reference/{file}"), name, "Reference"));
    }
    site_pages.push(page("types", "reference/types.md", "Types", "Reference"));
    let html = site::build(Path::new("docs"), &site_pages, env!("CARGO_PKG_VERSION"))?;
    std::fs::write(SITE, &html).map_err(|e| format!("{SITE}: {e}"))?;
    println!("{SITE}: {} pages, {} KB", site_pages.len(), html.len() / 1024);
    Ok(())
}
