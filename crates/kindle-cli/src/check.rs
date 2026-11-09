//! `kindle check`: validates a mod before it's shared. Reads mod.toml with the same rules
//! the runtime uses, compiles every Lua file (syntax), and checks each use of the Kindle API
//! against the API stubs of the game it's for (Elden Ring's for a mod only for Elden Ring, else Dark
//! Souls III's): unknown modules and functions (with a suggestion) and modules used without their
//! permission.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use kindle_manifest::ModManifest;

/// The API, as the editor sees it: Dark Souls III's and Elden Ring's (each in the builds for it).
#[cfg(feature = "ds3")]
const STUBS: &str = include_str!("../../../dist/stubs/kindle.lua");
#[cfg(feature = "er")]
const STUBS_ER: &str = include_str!("../../../dist/stubs-er/kindle.lua");

/// The stubs a mod is checked against: Elden Ring's for a mod only for Elden Ring, else Dark Souls III's
/// (a mod without `games` is Dark Souls III's, as the runtime takes it). Err when this build has
/// no game the mod is for.
fn stubs_for(manifest: Option<&ModManifest>) -> Result<&'static str, String> {
    let er_only = matches!(manifest.and_then(|m| m.info.games.as_ref()), Some(games) if !games.is_empty() && games.iter().all(|g| g == "er"));
    if er_only {
        #[cfg(feature = "er")]
        return Ok(STUBS_ER);
        #[cfg(not(feature = "er"))]
        return Err("mod.toml: this mod is for Elden Ring only, and this kindle.exe is Kindle for Dark Souls III".into());
    }
    #[cfg(feature = "ds3")]
    return Ok(STUBS);
    #[cfg(not(feature = "ds3"))]
    return Err("mod.toml: this mod is for Dark Souls III, and this kindle.exe is Kindle for Elden Ring".into());
}

pub struct Report {
    pub manifest: Option<ModManifest>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub files: Vec<PathBuf>,
}

/// Modules (`kindle.items`) → (permission if one is needed, members), and functions that
/// need a second permission ("items.set_name" → "text", from "needs \"text\"" in its docs).
struct Api {
    modules: BTreeMap<String, (Option<String>, BTreeSet<String>)>,
    extra: HashMap<String, String>,
}

/// `needs "x"` / `Needs the "x" permission` in a function's doc lines.
fn needed_permission(doc: &[&str]) -> Option<String> {
    doc.iter().find_map(|l| {
        let lower = l.to_lowercase();
        let i = lower.find("needs \"").map(|i| i + 7).or_else(|| lower.find("needs the \"").map(|i| i + 11))?;
        let rest = &l[i..];
        rest.split_once('"').map(|(p, _)| p.to_string())
    })
}

impl Api {
    fn parse(stubs: &str) -> Api {
        let mut modules: BTreeMap<String, (Option<String>, BTreeSet<String>)> = BTreeMap::new();
        let mut extra = HashMap::new();
        let mut doc: Vec<&str> = Vec::new();
        // The class being declared, for its fields (`kindle` itself: "", a module: its name).
        let mut class: Option<String> = None;
        for line in stubs.lines() {
            if line.starts_with("---@class kindle ") || line == "---@class kindle" {
                class = Some(String::new());
            } else if let Some(rest) = line.strip_prefix("---@class kindle.") {
                let name = rest.split_whitespace().next().unwrap_or("");
                class = (!name.contains('.')).then(|| name.to_string());
            } else if let (Some(c), Some(field)) = (&class, line.strip_prefix("---@field ")) {
                let name = field.split_whitespace().next().unwrap_or("").trim_end_matches('?');
                modules.entry(c.clone()).or_default().1.insert(name.to_string());
            } else if !line.starts_with("---") {
                class = None;
            }
            if let Some(rest) = line.strip_prefix("---@class kindle.") {
                let name = rest.split_whitespace().next().unwrap_or("");
                let permission = rest.split_once("(permission: \"").and_then(|(_, t)| t.split_once('"')).map(|(p, _)| p.to_string());
                if !name.contains('.') {
                    let e = modules.entry(name.to_string()).or_default();
                    if permission.is_some() {
                        e.0 = permission;
                    }
                }
            }
            let decl = line.strip_prefix("function kindle.").map(|r| r.split('(').next().unwrap_or(""));
            if let Some(path) = decl
                && let Some(p) = needed_permission(&doc)
            {
                extra.insert(path.to_string(), p);
            }
            if line.starts_with("---") && !line.starts_with("---@") {
                doc.push(line);
            } else if !line.starts_with("---@") {
                doc.clear();
            }
            let field = line.strip_prefix("kindle.").and_then(|r| r.split_once(" = ")).map(|(n, _)| n);
            if let Some(path) = decl.or(field) {
                match path.split_once('.') {
                    Some((module, member)) => {
                        modules.entry(module.to_string()).or_default().1.insert(member.to_string());
                    }
                    None => {
                        modules.entry(String::new()).or_default().1.insert(path.to_string());
                    }
                }
            }
        }
        Api { modules, extra }
    }

    /// Top-level names under `kindle` (functions and modules).
    fn is_top(&self, name: &str) -> bool {
        self.modules.contains_key(name) || self.modules.get("").is_some_and(|(_, m)| m.contains(name))
    }
}

/// Split `a: T, b?: {c: U, d: V}` at its top-level commas.
fn top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            _ if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// The stubs' types for checking tables written into calls: each function's first parameter's
/// type ("world.spawn" -> "kindle.spawn_opts"), and each class's fields with their types.
struct Types {
    calls: HashMap<String, String>,
    classes: HashMap<String, Vec<(String, String)>>,
}

fn option_types(stubs: &str) -> Types {
    let mut classes: HashMap<String, Vec<(String, String)>> = HashMap::new();
    let mut class: Option<String> = None;
    for line in stubs.lines() {
        if let Some(rest) = line.strip_prefix("---@class ") {
            class = rest.split_whitespace().next().map(str::to_string);
        } else if let (Some(c), Some(field)) = (&class, line.strip_prefix("---@field ")) {
            let (name, rest) = field.split_once(' ').unwrap_or((field, ""));
            classes.entry(c.clone()).or_default().push((name.trim_end_matches('?').to_string(), type_part(rest).to_string()));
        } else if !line.starts_with("---") {
            class = None;
        }
    }
    let mut calls = HashMap::new();
    let mut first_param: Option<&str> = None;
    for line in stubs.lines() {
        if let Some(rest) = line.strip_prefix("---@param ") {
            if first_param.is_none() {
                first_param = Some(rest.split_once(' ').map(|(_, t)| type_part(t)).unwrap_or(""));
            }
            continue;
        }
        if line.starts_with("---") {
            continue;
        }
        let ty = first_param.take();
        let Some(path) = line.strip_prefix("function kindle.").map(|r| r.split('(').next().unwrap_or("")) else { continue };
        if let Some(ty) = ty {
            calls.insert(path.to_string(), ty.to_string());
        }
    }
    Types { calls, classes }
}

/// The type at the start of a `---@param`'s or `---@field`'s text (its description after it).
fn type_part(s: &str) -> &str {
    let s = s.trim_start();
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' | '(' | '[' | '<' => depth += 1,
            '}' | ')' | ']' | '>' => depth -= 1,
            ' ' if depth == 0 => {
                // `fun(a): b` and `x | y` carry spaces inside the type.
                let before = s[..i].trim_end();
                let after = s[i..].trim_start();
                if before.ends_with(':') || before.ends_with('|') || after.starts_with('|') || after.starts_with(':') {
                    continue;
                }
                return &s[..i];
            }
            _ => {}
        }
    }
    s
}

impl Types {
    /// A type's alternatives (`a|b?`), each trimmed.
    fn alternatives(ty: &str) -> Vec<&str> {
        top_level(ty.trim().trim_end_matches('?'), '|').into_iter().map(|t| t.trim().trim_end_matches('?')).collect()
    }

    /// The fields a table of this type can have, with their types; None when it isn't known to
    /// be a table (or one alternative is a table Kindle can't list, as `table`).
    fn fields(&self, ty: &str) -> Option<Vec<(String, String)>> {
        let mut out = Vec::new();
        let mut any = false;
        for alt in Self::alternatives(ty) {
            if let Some(inner) = alt.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
                any = true;
                for f in top_level(inner, ',') {
                    let (k, t) = f.split_once(':').unwrap_or((f, ""));
                    out.push((k.trim().trim_end_matches('?').to_string(), t.trim().to_string()));
                }
            } else if let Some(fields) = self.classes.get(alt) {
                any = true;
                out.extend(fields.iter().cloned());
            } else if alt == "table" || alt.starts_with("table<") || alt == "any" {
                return None;
            }
        }
        any.then_some(out)
    }

    /// The element type of a list type (`x[]`), if it is one.
    fn element(ty: &str) -> Option<String> {
        Self::alternatives(ty).into_iter().find_map(|alt| {
            let el = alt.strip_suffix("[]")?;
            Some(el.trim_start_matches('(').trim_end_matches(')').to_string())
        })
    }

    /// Walks a table's path (field names, "[]" for a list's element) from a call's type; the
    /// type there, if the stubs tell it.
    fn at(&self, ty: &str, path: &[String]) -> Option<String> {
        let mut ty = ty.to_string();
        for step in path {
            ty = if step == "[]" { Self::element(&ty)? } else { self.fields(&ty)?.into_iter().find(|(k, _)| k == step)?.1 };
        }
        Some(ty)
    }
}

/// Tables written straight into a call of `call` (`call{...}` or `call({...})`): (line, the
/// path to the key: the tables it's nested in by key, "[]" for a list's element, then the key),
/// for each key set by name. `code` is code_only's (no strings or comments).
fn table_keys(code: &str, call: &str) -> Vec<(usize, Vec<String>)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = code[from..].find(call).map(|a| a + from) {
        from = at + call.len();
        if code[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':') {
            continue;
        }
        let rest = code[from..].trim_start();
        let rest = rest.strip_prefix('(').map(str::trim_start).unwrap_or(rest);
        if !rest.starts_with('{') {
            continue;
        }
        let open = code.len() - rest.len();
        scan_table(code, open + 1, &mut Vec::new(), &mut out);
    }
    out
}

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// A function's body from `at` (just after its `function`): where it ends.
fn skip_function(code: &str, mut at: usize) -> usize {
    let bytes = code.as_bytes();
    let mut level = 1;
    while level > 0 && at < bytes.len() {
        let len = bytes[at..].iter().take_while(|c| is_word(**c)).count();
        if len == 0 {
            at += 1;
            continue;
        }
        let starts_word = !(is_word(bytes[at - 1]) || bytes[at - 1] == b'.');
        match &code[at..at + len] {
            "function" | "if" | "do" if starts_word => level += 1,
            "end" if starts_word => level -= 1,
            _ => {}
        }
        at += len;
    }
    at
}

/// The table whose body starts at `at` (after its `{`): its keys into `out` under `path`, and
/// the tables in it, by key or as list elements. Returns where it ends (after its `}`).
fn scan_table(code: &str, mut at: usize, path: &mut Vec<String>, out: &mut Vec<(usize, Vec<String>)>) -> usize {
    let bytes = code.as_bytes();
    // At an element's start: a `{` there is a list element's table.
    let mut element_start = true;
    while at < bytes.len() {
        let c = bytes[at];
        match c {
            b'}' => return at + 1,
            b',' | b';' => {
                element_start = true;
                at += 1;
            }
            b'{' => {
                if element_start {
                    path.push("[]".into());
                    at = scan_table(code, at + 1, path, out);
                    path.pop();
                } else {
                    at = skip_brackets(code, at);
                }
                element_start = false;
            }
            b'(' | b'[' => {
                at = skip_brackets(code, at);
                element_start = false;
            }
            _ if c.is_ascii_whitespace() => at += 1,
            _ if (c.is_ascii_alphabetic() || c == b'_') && !(at > 0 && (is_word(bytes[at - 1]) || bytes[at - 1] == b'.')) => {
                let end = at + bytes[at..].iter().take_while(|c| is_word(**c)).count();
                let word = &code[at..end];
                if word == "function" {
                    at = skip_function(code, end);
                    element_start = false;
                    continue;
                }
                let after = code[end..].trim_start();
                if element_start && after.starts_with('=') && !after.starts_with("==") {
                    let line = code[..at].matches('\n').count() + 1;
                    path.push(word.to_string());
                    out.push((line, path.clone()));
                    let value = after[1..].trim_start();
                    let value_at = code.len() - value.len();
                    if value.starts_with('{') {
                        at = scan_table(code, value_at + 1, path, out);
                    } else {
                        at = value_at;
                    }
                    path.pop();
                    element_start = false;
                    continue;
                }
                at = end;
                element_start = false;
            }
            _ => {
                at += 1;
                element_start = false;
            }
        }
    }
    at
}

/// From an opening bracket at `at`: just after its match.
fn skip_brackets(code: &str, mut at: usize) -> usize {
    let bytes = code.as_bytes();
    let mut depth = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return at + 1;
                }
            }
            _ => {}
        }
        at += 1;
    }
    at
}

/// The closest known name, for "did you mean".
/// Event names (the stubs' `kindle.event` alias, which the runtime's tests keep in step
/// with what `kindle.on` accepts).
fn events(stubs: &str) -> Vec<String> {
    stubs
        .lines()
        .find_map(|l| l.strip_prefix("---@alias kindle.event "))
        .map(|a| a.split('|').map(|e| e.trim().trim_matches('"').to_string()).collect())
        .unwrap_or_default()
}

/// `kindle.on("name", ...)` calls with a literal event name: (line, name). Comments are
/// skipped.
fn event_uses(src: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let mut rest = line.split("--").next().unwrap_or("");
        while let Some(at) = rest.find("kindle.on(") {
            let after = rest[at + "kindle.on(".len()..].trim_start();
            if let Some(q) = after.chars().next().filter(|c| *c == '"' || *c == '\'')
                && let Some(end) = after[1..].find(q)
            {
                out.push((i + 1, after[1..1 + end].to_string()));
            }
            rest = &rest[at + 1..];
        }
    }
    out
}

fn suggest<'a>(word: &str, known: impl Iterator<Item = &'a String>) -> Option<&'a String> {
    fn distance(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut cur = vec![i + 1];
            for (j, cb) in b.iter().enumerate() {
                cur.push((prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
            }
            prev = cur;
        }
        prev[b.len()]
    }
    known.map(|k| (distance(word, k), k)).filter(|(d, _)| *d <= 3).min_by_key(|(d, _)| *d).map(|(_, k)| k)
}

fn lua_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let p = e.path();
        if p.is_dir() {
            lua_files(&p, out);
        } else if name.ends_with(".lua") {
            out.push(p);
        }
    }
}

/// `local a, b = kindle.x, kindle.y` aliases in a file: alias → module.
fn aliases(src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in src.lines() {
        let Some(rest) = line.trim_start().strip_prefix("local ") else { continue };
        let Some((names, values)) = rest.split_once('=') else { continue };
        let names: Vec<&str> = names.split(',').map(str::trim).collect();
        let values: Vec<&str> = values.split("--").next().unwrap_or("").split(',').map(str::trim).collect();
        for (n, v) in names.iter().zip(values) {
            if let Some(module) = v.strip_prefix("kindle.")
                && !module.is_empty()
                && module.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                out.insert(n.to_string(), module.to_string());
            }
        }
    }
    out
}

/// Calls of `name` with a literal text as the first argument: `name("x")`, `name ("x")`, `name "x"`
/// (either quote): (line, the text). Comments are skipped; a name inside a longer one isn't
/// (`my_require "x"` isn't `require`).
fn literal_calls(src: &str, name: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let code = line.split("--").next().unwrap_or("");
        let mut from = 0;
        while let Some(at) = code[from..].find(name).map(|a| a + from) {
            from = at + name.len();
            let before = code[..at].chars().next_back();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':') {
                continue;
            }
            let mut rest = code[from..].trim_start();
            if let Some(r) = rest.strip_prefix('(') {
                rest = r.trim_start();
            }
            if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'')
                && let Some(end) = rest[1..].find(q).filter(|&end| !rest[2 + end..].trim_start().starts_with(".."))
            {
                out.push((i + 1, rest[1..1 + end].to_string()));
            }
        }
    }
    out
}

/// Fields of a table set to a literal text: `key = "x"` (a layer's image, a screen row's icon):
/// (line, the text). Comments are skipped.
fn literal_fields(src: &str, key: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let code = line.split("--").next().unwrap_or("");
        let mut from = 0;
        while let Some(at) = code[from..].find(key).map(|a| a + from) {
            from = at + key.len();
            let before = code[..at].chars().next_back();
            if before.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '"' || c == '\'') {
                continue;
            }
            let Some(rest) = code[from..].trim_start().strip_prefix('=') else { continue };
            let rest = rest.trim_start();
            if rest.starts_with('=') {
                continue;
            }
            if let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'')
                && let Some(end) = rest[1..].find(q).filter(|&end| !rest[2 + end..].trim_start().starts_with(".."))
            {
                out.push((i + 1, rest[1..1 + end].to_string()));
            }
        }
    }
    out
}

/// Code with comments and string contents blanked out (same length per line), so API
/// lookups only see code.
fn code_only(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '-' && chars.get(i + 1) == Some(&'-') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != c && chars[i] != '\n' {
                if chars[i] == '\\' {
                    out.push(' ');
                    i += 1;
                }
                if i < chars.len() {
                    out.push(' ');
                    i += 1;
                }
            }
            // The closing quote (else it would open another string over the rest of the line).
            if chars.get(i) == Some(&c) {
                out.push(c);
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Every `<name>.<member>` in the code where name is `kindle` or an alias: (line, module path).
fn uses(code: &str, aliases: &HashMap<String, String>) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (n, line) in code.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.') {
                i += 1;
            }
            if i > start {
                let preceded = start > 0 && (bytes[start - 1] == b'.' || bytes[start - 1] == b':');
                let word = &line[start..i];
                if !preceded {
                    let mut parts = word.split('.');
                    let head = parts.next().unwrap_or("");
                    let rest: Vec<&str> = parts.filter(|p| !p.is_empty()).collect();
                    if head == "kindle" && !rest.is_empty() {
                        out.push((n + 1, rest.join(".")));
                    } else if let Some(module) = aliases.get(head) {
                        let mut path = vec![module.as_str()];
                        path.extend(rest.iter().take(1));
                        out.push((n + 1, path.join(".")));
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    out
}

/// Each param table's fields, from the `stubs\kindle-game.lua` Kindle writes from the player's own
/// game (next to kindle.exe, or next to the mods folder the mod is in): table → fields. None
/// where there's no such file (Kindle hasn't run the game yet).
fn param_fields(dir: &Path) -> Option<HashMap<String, Vec<String>>> {
    let beside_exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("stubs")));
    let beside_mods = dir.canonicalize().ok().and_then(|d| d.parent()?.parent().map(|k| k.join("stubs")));
    let src = [beside_exe, beside_mods].into_iter().flatten().find_map(|s| std::fs::read_to_string(s.join("kindle-game.lua")).ok())?;
    Some(parse_param_fields(&src))
}

/// `---@class <table> : kindle.param_row` and the `---@field` lines under it.
fn parse_param_fields(src: &str) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("---@class ") {
            current = rest.split_once(" : kindle.param_row").map(|(name, _)| name.trim().to_string());
            if let Some(name) = &current {
                out.entry(name.clone()).or_default();
            }
        } else if let Some(rest) = line.strip_prefix("---@field ") {
            if let (Some(table), Some(field)) = (&current, rest.split_whitespace().next()) {
                out.entry(table.clone()).or_default().push(field.to_string());
            }
        } else {
            current = None;
        }
    }
    out
}

/// The param calls whose first argument is a table: their names in the file, with `kindle.params`'s
/// aliases.
fn param_calls(aliases: &HashMap<String, String>) -> Vec<(String, bool)> {
    const TABLE_FIRST: [&str; 7] = ["get", "add_row", "add_rows", "copy", "reset", "fields", "rows"];
    let heads = std::iter::once("kindle.params".to_string()).chain(aliases.iter().filter(|(_, m)| *m == "params").map(|(a, _)| a.clone()));
    heads.flat_map(|h| TABLE_FIRST.map(|f| (format!("{h}.{f}"), f == "get" || f == "add_row"))).collect()
}

/// Rows a file keeps in a local from one table: `local row = params.get("T", ...)` (an `or
/// params.add_row("T", ...)` too): name → table. A name set from anything else, or from two
/// tables, isn't taken: its fields can't be known from the text.
fn row_locals(src: &str, calls: &[(String, bool)]) -> HashMap<String, String> {
    let mut tables: HashMap<String, Option<String>> = HashMap::new();
    for line in src.lines() {
        let code = line.split("--").next().unwrap_or("");
        let Some((lhs, rhs)) = code.split_once('=') else { continue };
        if rhs.starts_with('=') || lhs.ends_with(['~', '<', '>', '=']) {
            continue;
        }
        let name = lhs.trim().trim_start_matches("local ").trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let found: BTreeSet<String> = calls.iter().filter(|(_, row)| *row).flat_map(|(call, _)| literal_calls(rhs, call)).map(|(_, t)| t).collect();
        let entry = tables.entry(name.to_string()).or_insert_with(|| found.iter().next().cloned());
        if found.len() != 1 || entry.as_ref() != found.iter().next() {
            *entry = None;
        }
    }
    let bound = bound_names(src);
    tables.into_iter().filter(|(name, _)| !bound.contains(name)).filter_map(|(name, t)| Some((name, t?))).collect()
}

/// Names a file binds other than by `=`: functions' parameters and loop variables (the same name
/// there may hold another table's row).
fn bound_names(src: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut add = |list: &str| out.extend(list.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()));
    for line in src.lines() {
        let code = line.split("--").next().unwrap_or("");
        let mut from = 0;
        while let Some(at) = code[from..].find("function").map(|a| a + from) {
            from = at + "function".len();
            if let Some(open) = code[from..].find('(')
                && let Some(close) = code[from + open..].find(')')
            {
                add(&code[from + open + 1..from + open + close]);
            }
        }
        if let Some(rest) = code.trim_start().strip_prefix("for ") {
            add(rest.split(" in ").next().unwrap_or("").split('=').next().unwrap_or(""));
        }
    }
    out
}

/// `<name>.<field>` in the code: (line, field). Not `<name>:<method>`, nor `x.<name>.<field>`.
fn field_uses(code: &str, name: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in code.lines().enumerate() {
        let mut from = 0;
        while let Some(at) = line[from..].find(name).map(|a| a + from) {
            from = at + name.len();
            if line[..at].chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == ':') {
                continue;
            }
            let Some(rest) = line[from..].strip_prefix('.') else { continue };
            let field: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if !field.is_empty() {
                out.push((i + 1, field));
            }
        }
    }
    out
}

pub fn check(dir: &Path) -> Report {
    let mut r = Report { manifest: None, errors: Vec::new(), warnings: Vec::new(), files: Vec::new() };
    match ModManifest::load(&dir.join("mod.toml")) {
        Ok(m) => r.manifest = Some(m),
        Err(e) => r.errors.push(e),
    }
    // A mod of game files only (files\ and no main.lua) runs nothing, as the runtime starts it.
    if !dir.join("main.lua").is_file() && !dir.join("files").is_dir() {
        r.errors.push("main.lua is missing".into());
    }
    lua_files(dir, &mut r.files);

    let stubs = match stubs_for(r.manifest.as_ref()) {
        Ok(stubs) => stubs,
        Err(e) => {
            r.errors.push(e);
            return r;
        }
    };
    let api = Api::parse(stubs);
    let lua = mlua::Lua::new();
    if let Some(m) = &r.manifest
        && m.info.api.is_none()
    {
        r.warnings.push(format!("mod.toml: no api version; add `api = {}` under [mod] (the Kindle API this mod is written for)", kindle_manifest::API_VERSION));
    }
    if let Some(games) = r.manifest.as_ref().and_then(|m| m.info.games.as_ref()) {
        let known: Vec<String> = kindle_manifest::GAMES.iter().map(|(g, _)| g.to_string()).collect();
        for game in games.iter().filter(|g| !known.contains(g)) {
            let hint = suggest(game, known.iter()).map(|s| format!(" (did you mean \"{s}\"?)")).unwrap_or_default();
            let ids = kindle_manifest::GAMES.iter().map(|(g, name)| format!("{g}: {name}")).collect::<Vec<_>>().join(", ");
            r.warnings.push(format!("mod.toml: \"{game}\" in games isn't a game this Kindle runs in ({ids}){hint}"));
        }
    }
    let param_fields = param_fields(dir);
    let options = option_types(stubs);
    let mut used_modules = BTreeSet::new();
    for file in &r.files {
        let rel = file.strip_prefix(dir).unwrap_or(file).display().to_string().replace('\\', "/");
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                r.errors.push(format!("{rel}: {e}"));
                continue;
            }
        };
        if let Err(e) = lua.load(&src).set_name(format!("@{rel}")).into_function() {
            let msg = e.to_string();
            r.errors.push(msg.lines().next().unwrap_or(&msg).trim_start_matches("syntax error: ").to_string());
            continue;
        }
        let known_events = events(stubs);
        for (line, event) in event_uses(&src) {
            if !known_events.contains(&event) {
                let hint = suggest(&event, known_events.iter()).map(|s| format!(" (did you mean \"{s}\"?)")).unwrap_or_default();
                r.errors.push(format!("{rel}:{line}: unknown event \"{event}\"{hint}"));
            }
        }
        // Names and files the mod gives as literal text: its settings (mod.toml), the Lua files it
        // requires and the pictures its layers and screens show (its folder).
        let file_aliases = aliases(&src);
        let settings_calls = ["kindle.settings.get", "kindle.settings.set"]
            .into_iter()
            .map(String::from)
            .chain(file_aliases.iter().filter(|(_, m)| *m == "settings").flat_map(|(a, _)| [format!("{a}.get"), format!("{a}.set")]));
        if let Some(m) = &r.manifest {
            let declared: Vec<String> = m.settings.iter().map(|s| s.name.clone()).collect();
            for call in settings_calls {
                for (line, name) in literal_calls(&src, &call) {
                    if !declared.contains(&name) {
                        let hint = suggest(&name, declared.iter()).map(|s| format!(" (did you mean \"{s}\"?)")).unwrap_or_default();
                        r.errors.push(format!("{rel}:{line}: no setting \"{name}\" in mod.toml (declare it under [settings.{name}]){hint}"));
                    }
                }
            }
        }
        for (line, name) in literal_calls(&src, "require") {
            let path = format!("{}.lua", name.replace('.', "/"));
            if !dir.join(&path).is_file() {
                r.errors.push(format!("{rel}:{line}: require \"{name}\": no {path} in the mod's folder"));
            }
        }
        for key in ["image", "icon"] {
            // A name without an extension is one of the game's own (char_make's page icons).
            for (line, path) in literal_fields(&src, key).into_iter().filter(|(_, p)| p.contains('.')) {
                if !path.to_ascii_lowercase().ends_with(".png") {
                    r.warnings.push(format!("{rel}:{line}: {key} \"{path}\" isn't a .png file (pictures are PNG files)"));
                } else if !dir.join(&path).is_file() {
                    r.errors.push(format!("{rel}:{line}: {key} \"{path}\": no such file in the mod's folder"));
                }
            }
        }
        let code = code_only(&src);
        // Param tables and fields, by the player's own game's names (when Kindle has written them).
        if let Some(fields) = &param_fields {
            let calls = param_calls(&file_aliases);
            for (call, _) in &calls {
                for (line, table) in literal_calls(&src, call) {
                    if !fields.contains_key(&table) {
                        let hint = suggest(&table, fields.keys()).map(|s| format!(" (did you mean \"{s}\"?)")).unwrap_or_default();
                        r.errors.push(format!("{rel}:{line}: no param table \"{table}\"{hint}"));
                    }
                }
            }
            for (name, table) in row_locals(&src, &calls) {
                let Some(known) = fields.get(&table) else { continue };
                for (line, field) in field_uses(&code, &name).into_iter().filter(|(_, f)| !known.contains(f)) {
                    let hint = suggest(&field, known.iter()).map(|s| format!(" (did you mean {name}.{s}?)")).unwrap_or_default();
                    r.errors.push(format!("{rel}:{line}: {table} has no field \"{field}\"{hint}"));
                }
            }
        }
        // Keys of an options table written into the call (`kindle.world.spawn{enemy = ...}`).
        // Nested ones too (a screen's rows, a row's slider), as far as the stubs' types go.
        for (path, ty) in &options.calls {
            let (module, member) = path.split_once('.').unwrap_or(("", path));
            let calls =
                std::iter::once(format!("kindle.{path}")).chain(file_aliases.iter().filter(|(_, m)| *m == module).map(|(a, _)| format!("{a}.{member}")));
            for call in calls {
                for (line, keys) in table_keys(&code, &call) {
                    let (key, within) = keys.split_last().expect("a key");
                    let Some(table) = options.at(ty, within) else { continue };
                    let Some(fields) = options.fields(&table) else { continue };
                    if fields.iter().any(|(k, _)| k == key) {
                        continue;
                    }
                    let names: Vec<&String> = fields.iter().map(|(k, _)| k).collect();
                    let hint = suggest(key, names.into_iter()).map(|s| format!(" (did you mean {s}?)")).unwrap_or_default();
                    if within.is_empty() {
                        r.errors.push(format!("{rel}:{line}: kindle.{path} has no option \"{key}\"{hint}"));
                    } else {
                        let place = within.join(".").replace(".[]", "[]");
                        r.errors.push(format!("{rel}:{line}: kindle.{path}'s {place} has no option \"{key}\"{hint}"));
                    }
                }
            }
        }
        for (line, path) in uses(&code, &aliases(&code)) {
            let mut parts = path.split('.');
            let first = parts.next().unwrap_or("");
            let member = parts.next();
            if !api.is_top(first) {
                let hint = suggest(first, api.modules.keys().chain(api.modules.get("").map(|m| m.1.iter()).into_iter().flatten()))
                    .map(|s| format!(" (did you mean kindle.{s}?)"))
                    .unwrap_or_default();
                r.errors.push(format!("{rel}:{line}: kindle.{first} doesn't exist{hint}"));
                continue;
            }
            let Some((permission, members)) = api.modules.get(first) else { continue };
            used_modules.insert(first.to_string());
            if let Some(m) = member
                && !members.contains(m)
            {
                let hint = suggest(m, members.iter()).map(|s| format!(" (did you mean {first}.{s}?)")).unwrap_or_default();
                r.errors.push(format!("{rel}:{line}: kindle.{first}.{m} doesn't exist{hint}"));
            }
            let extra = member.and_then(|m| api.extra.get(&format!("{first}.{m}")));
            if let Some(p) = extra {
                used_modules.insert(p.clone());
            }
            for (what, p) in [(format!("kindle.{first}"), permission.as_ref()), (format!("kindle.{path}"), extra)] {
                if let (Some(p), Some(manifest)) = (p, &r.manifest)
                    && !manifest.allows(p)
                {
                    let msg = format!("{what} needs the \"{p}\" permission: add it to modules in mod.toml");
                    if !r.errors.contains(&msg) {
                        r.errors.push(msg);
                    }
                }
            }
        }
    }
    if let Some(m) = &r.manifest {
        for module in &m.permissions.modules {
            let needed_by: Vec<&String> = api.modules.iter().filter(|(_, (p, _))| p.as_deref() == Some(module.as_str())).map(|(name, _)| name).collect();
            if !needed_by.iter().any(|name| used_modules.contains(*name)) {
                r.warnings.push(format!("permission \"{module}\" is listed but the mod never uses kindle.{module}"));
            }
        }
    }
    r
}

// Most of these check Dark Souls III mods: an Elden Ring-only build runs none of them.
#[cfg(all(test, feature = "ds3"))]
mod tests {
    use super::*;

    #[test]
    fn code_after_a_string_is_still_code() {
        let code = code_only(
            "f { text = \"a\", go = function() kindle.player.heal() end } -- kindle.items
",
        );
        assert!(code.contains("kindle.player.heal()"));
        assert!(!code.contains("kindle.items"));
        assert_eq!(uses(&code, &HashMap::new()), vec![(1, "player.heal".to_string())]);
    }

    fn temp_mod(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kindle-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        for (name, body) in files {
            let p = dir.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        dir
    }

    #[test]
    fn finds_options_an_api_table_does_not_have() {
        let dir = temp_mod(&[
            (
                "mod.toml",
                "[mod]
id = \"t\"
api = 1
[permissions]
modules = [\"world\", \"player\", \"ui\"]
",
            ),
            (
                "main.lua",
                "local world = kindle.world
                 world.spawn{ enemy = \"wolf\", x = 1, y = 2, z = 3,
                   on_spawn = function(c) local angel = 1; if c then aangle = 2 end end,
                   ai_enbled = false }
                 kindle.player.set_camera({ x = 1, y = 2, z = 3, look_at = { x = 0, y = 0, zz = 0 }, fvo = 50 })
                 kindle.ui.confirm { title = \"a = b\", text = \"t\", on_yes = function() end }
                 local t = { anything = 1 }
                 kindle.ui.screen{ title = \"s\", tabs = { { title = \"a\", rows = {
                   { text = \"v\", slider = { min = 0, max = 1, setp = 0.1 }, on_change = function(v) local x = { y = 1 } end },
                   { text = \"w\", toogle = true } } } } }
",
            ),
        ]);
        let all = check(&dir).errors.join(
            "
",
        );
        assert!(all.contains("main.lua:4: kindle.world.spawn has no option \"ai_enbled\" (did you mean ai_enabled?)"), "{all}");
        assert!(all.contains("main.lua:5: kindle.player.set_camera has no option \"fvo\" (did you mean fov?)"), "{all}");
        assert!(all.contains("main.lua:5: kindle.player.set_camera's look_at has no option \"zz\" (did you mean z?)"), "{all}");
        assert!(all.contains("main.lua:9: kindle.ui.screen's tabs[].rows[].slider has no option \"setp\" (did you mean step?)"), "{all}");
        assert!(all.contains("main.lua:10: kindle.ui.screen's tabs[].rows[] has no option \"toogle\" (did you mean toggle?)"), "{all}");
        assert!(!all.contains("angel") && !all.contains("aangle") && !all.contains("anything") && !all.contains("\"y\""), "{all}");
        assert_eq!(all.matches("has no option").count(), 5, "{all}");
    }

    #[test]
    fn finds_param_tables_and_fields_the_game_does_not_have() {
        let root = temp_mod(&[
            (
                "kindle/stubs/kindle-game.lua",
                "--- A row of EquipParamWeapon (EQUIP_PARAM_WEAPON_ST).\n---@class EquipParamWeapon : kindle.param_row\n---@field weight number\n---@field sortId integer\n\n\
                 ---@class SpEffectParam : kindle.param_row\n---@field effectEndurance number\n\n---@class kindle.other\n---@field nope integer\n",
            ),
            ("kindle/mods/t/mod.toml", "[mod]\nid = \"t\"\napi = 1\n[permissions]\nmodules = [\"params\"]\n"),
            (
                "kindle/mods/t/main.lua",
                "local params = kindle.params\n\
                 local row = params.get(\"EquipParamWeapon\", 1) or params.add_row(\"EquipParamWeapon\", 9, 1)\n\
                 row.wieght = 0\n\
                 row.weight = row.sortId\n\
                 local fx = kindle.params.get(\"SpEffectParm\", 2)\n\
                 local both = params.get(\"SpEffectParam\", 3)\n\
                 both = params.get(\"EquipParamWeapon\", 3)\n\
                 both.anything = 1\n\
                 local function f(row) return row.other end\n\
                 local s = \"row.inastring\"\n",
            ),
        ]);
        let r = check(&root.join("kindle/mods/t"));
        let all = r.errors.join("\n");
        assert!(all.contains("main.lua:5: no param table \"SpEffectParm\" (did you mean \"SpEffectParam\"?)"), "{all}");
        assert!(r.errors.iter().all(|e| !e.contains("other") && !e.contains("anything") && !e.contains("inastring")), "{all}");
        // `row` is bound as a function's parameter too, so its fields aren't checked.
        assert!(!all.contains("wieght"), "{all}");

        let p = root.join("kindle/mods/t/main.lua");
        std::fs::write(&p, std::fs::read_to_string(&p).unwrap().replace("local function f(row) return row.other end\n", "")).unwrap();
        let all = check(&root.join("kindle/mods/t")).errors.join("\n");
        assert!(all.contains("main.lua:3: EquipParamWeapon has no field \"wieght\" (did you mean row.weight?)"), "{all}");
        assert!(!all.contains("sortId") && !all.contains("\"weight\""), "{all}");
        assert_eq!(parse_param_fields("---@class kindle.other\n---@field nope integer\n").len(), 0);
    }

    #[test]
    fn finds_api_mistakes_and_missing_permissions() {
        let dir = temp_mod(&[
            ("mod.toml", "[mod]\nid = \"t\"\n[permissions]\nmodules = [\"player\", \"params\"]\n"),
            (
                "main.lua",
                "local items, player = kindle.items, kindle.player\n\
                 items.give(\"Ember\")\n\
                 player.hpp()\n\
                 kindle.ui.toast(\"hi\") -- kindle.nothing in a comment is fine\n\
                 local s = \"kindle.fake.call\"\n\
                 kindle.log.info(\"ok\")\n\
                 kindle.on(\"frame\", function() end)\n\
                 items.set_name(\"Ember\", \"Spark\")\n\
                 kindle.on('player_loded', function() end)\n\
                 -- kindle.on(\"in_a_comment\", f)\n",
            ),
        ]);
        let r = check(&dir);
        let all = r.errors.join("\n");
        assert!(all.contains("kindle.items needs the \"items\" permission"), "{all}");
        assert!(all.contains("main.lua:3: kindle.player.hpp doesn't exist (did you mean player.hp?)"), "{all}");
        assert!(all.contains("kindle.ui needs the \"ui\" permission"), "{all}");
        assert!(all.contains("kindle.items.set_name needs the \"text\" permission"), "{all}");
        assert!(all.contains("main.lua:9: unknown event \"player_loded\" (did you mean \"player_loaded\"?)"), "{all}");
        assert!(!all.contains("in_a_comment") && !all.contains("\"frame\""), "{all}");
        assert!(!all.contains("fake") && !all.contains("nothing"), "{all}");
        assert!(!all.contains("kindle.log") && !all.contains("kindle.on"), "{all}");
        assert!(r.warnings.iter().any(|w| w.contains("\"params\"")), "{:?}", r.warnings);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn finds_settings_requires_and_pictures_that_are_not_there() {
        let dir = temp_mod(&[
            (
                "mod.toml",
                "[mod]\nid = \"t\"\napi = 1\n[permissions]\nmodules = [\"ui\"]\n[settings.speed]\nlabel = \"Speed\"\ntype = \"int\"\ndefault = 1\nmin = 1\nmax = 3\n",
            ),
            ("lib/util.lua", "return {}\n"),
            ("art/crest.png", "not checked here"),
            (
                "main.lua",
                "local util = require(\"lib.util\")\n\
                 local gone = require \"missing\"\n\
                 local settings = kindle.settings\n\
                 local a = kindle.settings.get(\"speed\")\n\
                 local b = settings.get('sped')\n\
                 kindle.ui.layer { { image = \"art/crest.png\" }, { image = \"art/gone.png\" } }\n\
                 kindle.ui.screen { title = \"x\", rows = { { text = \"a\", icon = \"art/crest.jpg\" }, { text = \"b\", icon = \"skulls\" } } }\n\
                 local my_require = function() end\n\
                 my_require \"not_a_require\"\n\
                 local built = require(\"lib.\" .. \"util\")\n\
                 -- require(\"in_a_comment\")\n",
            ),
        ]);
        let r = check(&dir);
        let all = r.errors.join("\n");
        assert!(all.contains("main.lua:2: require \"missing\": no missing.lua"), "{all}");
        assert!(all.contains("main.lua:5: no setting \"sped\" in mod.toml") && all.contains("did you mean \"speed\""), "{all}");
        assert!(all.contains("main.lua:6: image \"art/gone.png\": no such file"), "{all}");
        assert!(!all.contains("lib.util") && !all.contains("\"speed\" in") && !all.contains("crest.png"), "{all}");
        assert!(!all.contains("not_a_require") && !all.contains("in_a_comment") && !all.contains("\"lib.\""), "{all}");
        assert!(r.warnings.iter().any(|w| w.contains("main.lua:7: icon \"art/crest.jpg\" isn't a .png")), "{:?}", r.warnings);
        assert!(!r.warnings.iter().any(|w| w.contains("skulls")), "{:?}", r.warnings);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn warns_about_games_it_does_not_know() {
        let dir = temp_mod(&[("mod.toml", "[mod]\nid = \"t\"\napi = 1\ngames = [\"ds3\", \"ds\"]\n"), ("main.lua", "kindle.log.info(\"hi\")\n")]);
        let r = check(&dir);
        assert!(r.errors.is_empty(), "{:?}", r.errors);
        assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
        assert!(r.warnings[0].contains("\"ds\" in games") && r.warnings[0].contains("did you mean \"ds3\""), "{:?}", r.warnings);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[cfg(feature = "er")]
    fn mods_for_elden_ring_are_checked_against_its_api() {
        let er = temp_mod(&[
            (
                "mod.toml",
                "[mod]
id = \"t\"
api = 1
games = [\"er\"]
[permissions]
modules = [\"player\", \"world\"]
",
            ),
            (
                "main.lua",
                "local player = kindle.player
                 kindle.on(\"map_changed\", function(new, old) kindle.log.info(new) end)
                 player.add_runes(player.level())
                 kindle.world.spawn({})
                 kindle.on(\"bonfire_rested\", function() end)
",
            ),
        ]);
        let r = check(&er);
        let all = r.errors.join(
            "
",
        );
        assert!(all.contains("main.lua:4: kindle.world.spawn doesn't exist"), "{all}");
        assert!(all.contains("main.lua:5: unknown event \"bonfire_rested\""), "{all}");
        assert_eq!(r.errors.len(), 2, "{all}");
        // The same lines for Dark Souls III: its events and functions instead.
        std::fs::write(
            er.join("mod.toml"),
            "[mod]
id = \"t\"
api = 1
[permissions]
modules = [\"player\", \"world\"]
",
        )
        .unwrap();
        let all = check(&er).errors.join(
            "
",
        );
        assert!(all.contains("unknown event \"map_changed\"") && all.contains("kindle.player.add_runes doesn't exist"), "{all}");
        assert!(!all.contains("spawn") && !all.contains("bonfire_rested"), "{all}");
        let _ = std::fs::remove_dir_all(er);
    }

    #[test]
    fn a_mod_of_files_only_needs_no_main_lua() {
        let dir = temp_mod(&[
            (
                "mod.toml",
                "[mod]
id = \"t\"
api = 1
games = [\"ds3\"]
",
            ),
            ("files/menu/hi/x.dcx", ""),
        ]);
        assert!(check(&dir).errors.is_empty());
        std::fs::remove_dir_all(dir.join("files")).unwrap();
        assert!(check(&dir).errors.iter().any(|e| e == "main.lua is missing"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reports_syntax_errors_with_the_file() {
        let dir = temp_mod(&[("mod.toml", "[mod]\nid = \"t\"\n"), ("main.lua", "local x = \n"), ("lib/util.lua", "return {}\n")]);
        let r = check(&dir);
        assert!(r.errors.iter().any(|e| e.contains("main.lua:")), "{:?}", r.errors);
        assert_eq!(r.files.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
