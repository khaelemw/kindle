//! The docs as one HTML page (`docs/kindle-docs.html`): every guide and reference page from
//! `docs/`, one shown at a time, with a search over functions and guide sections. Links
//! between the Markdown files become links between pages, and headings get ids of the form
//! `page--heading` so anchors keep working.

use std::collections::HashMap;
use std::path::Path;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

const TEMPLATE: &str = include_str!("site.html");

pub struct Page {
    pub slug: String,
    /// Path under docs/, with forward slashes (e.g. "reference/core.md").
    pub file: String,
    pub title: String,
    pub group: &'static str,
}

/// Builds the page from the Markdown under `root`.
pub fn build(root: &Path, pages: &[Page], version: &str) -> Result<String, String> {
    let slugs: HashMap<&str, &str> = pages.iter().map(|p| (p.file.as_str(), p.slug.as_str())).collect();
    let mut articles = String::new();
    let mut search = Vec::new();
    for page in pages {
        let path = root.join(&page.file);
        let md = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (body, toc) = render(&md, page, &slugs);
        articles.push_str(&format!("<article class=\"page\" id=\"page-{0}\" data-slug=\"{0}\">{body}</article>\n", page.slug));
        // Reference pages: one entry per function; guides: one per section.
        let wanted = if page.file.starts_with("reference/") { 3 } else { 2 };
        for (level, text, id) in toc {
            if level == wanted {
                search.push(serde_json::json!({ "t": text, "p": page.title, "h": id }));
            }
        }
    }

    let mut nav = String::new();
    let mut group = "";
    for page in pages {
        if page.group != group {
            if !group.is_empty() {
                nav.push_str("</ul>\n");
            }
            group = page.group;
            nav.push_str(&format!("<p class=\"nav-group\">{group}</p><ul>\n"));
        }
        let class = if page.title.starts_with("kindle") { " class=\"mono\"" } else { "" };
        nav.push_str(&format!("<li><a href=\"#{0}\" data-nav=\"{0}\"{class}>{1}</a></li>\n", page.slug, escape(&page.title)));
    }
    nav.push_str("</ul>");

    let search = serde_json::to_string(&search).map_err(|e| e.to_string())?;
    Ok(TEMPLATE
        .replace("{{NAV}}", &nav)
        .replace("{{PAGES}}", &articles)
        // Inside a <script>: keep "</" out of the JSON.
        .replace("{{SEARCH}}", &search.replace("</", "<\\/"))
        .replace("{{VERSION}}", version))
}

/// One page's HTML and its table of contents: (heading level, text, id).
fn render(md: &str, page: &Page, slugs: &HashMap<&str, &str>) -> (String, Vec<(u8, String, String)>) {
    let mut out: Vec<Event> = Vec::new();
    let mut toc = Vec::new();
    // Links that were dropped (to files outside the docs): their End is dropped too.
    let mut links: Vec<bool> = Vec::new();
    let mut heading: Option<(u8, Vec<Event>)> = None;
    let mut code: Option<(String, String)> = None;

    for event in Parser::new_ext(md, Options::ENABLE_TABLES) {
        // Code blocks: collect, then highlight.
        if let Some((lang, text)) = code.as_mut() {
            match event {
                Event::Text(t) => text.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    out.push(Event::Html(highlight(lang, text).into()));
                    code = None;
                }
                _ => {}
            }
            continue;
        }
        let event = match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => l.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code = Some((lang, String::new()));
                continue;
            }
            Event::Start(Tag::Link { dest_url, .. }) => match resolve_link(&dest_url, &page.file, slugs) {
                Some((url, external)) => {
                    links.push(true);
                    let attrs = if external { " target=\"_blank\" rel=\"noopener\"" } else { "" };
                    Event::Html(format!("<a href=\"{}\"{attrs}>", escape(&url)).into())
                }
                None => {
                    links.push(false);
                    continue;
                }
            },
            Event::End(TagEnd::Link) => {
                if links.pop() == Some(true) {
                    Event::Html("</a>".into())
                } else {
                    continue;
                }
            }
            Event::Start(Tag::Table(a)) => {
                out_push(&mut out, &mut heading, Event::Html("<div class=\"table\">".into()));
                Event::Start(Tag::Table(a))
            }
            Event::End(TagEnd::Table) => {
                out_push(&mut out, &mut heading, Event::End(TagEnd::Table));
                Event::Html("</div>".into())
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level_number(level), Vec::new()));
                continue;
            }
            Event::End(TagEnd::Heading(_)) => {
                let Some((level, inner)) = heading.take() else { continue };
                let text: String = inner
                    .iter()
                    .filter_map(|e| match e {
                        Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
                        _ => None,
                    })
                    .collect();
                let id = if level == 1 { page.slug.clone() } else { format!("{}--{}", page.slug, slugify(&text)) };
                if level == 2 || level == 3 {
                    toc.push((level, text, id.clone()));
                }
                out.push(Event::Html(format!("<h{level} id=\"{id}\">").into()));
                out.extend(inner);
                out.push(Event::Html(format!("</h{level}>\n").into()));
                continue;
            }
            other => other,
        };
        out_push(&mut out, &mut heading, event);
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    (html, toc)
}

/// Adds an event to the heading being collected, if any, else to the page.
fn out_push<'a>(out: &mut Vec<Event<'a>>, heading: &mut Option<(u8, Vec<Event<'a>>)>, e: Event<'a>) {
    match heading {
        Some((_, inner)) => inner.push(e),
        None => out.push(e),
    }
}

fn level_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// GitHub's heading anchors: lowercase, punctuation other than - and _ dropped, spaces to -.
pub fn slugify(text: &str) -> String {
    let s: String =
        text.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' ')).map(|c| if c == ' ' { '-' } else { c }).collect();
    s.trim_matches('-').to_string()
}

/// Where a Markdown link goes in the page: (url, external). None for repository files
/// outside the docs (the link is dropped, its text kept).
fn resolve_link(href: &str, current: &str, slugs: &HashMap<&str, &str>) -> Option<(String, bool)> {
    if href.contains("://") || href.starts_with("mailto:") {
        return Some((href.to_string(), true));
    }
    let (path, frag) = href.split_once('#').unwrap_or((href, ""));
    let slug = if path.is_empty() {
        *slugs.get(current)?
    } else {
        let dir = current.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        *slugs.get(normalize(dir, path).as_str())?
    };
    Some((if frag.is_empty() { format!("#{slug}") } else { format!("#{slug}--{frag}") }, false))
}

/// `dir/path` with `.` and `..` resolved.
fn normalize(dir: &str, path: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ── code highlighting ───────────────────────────────────────────────────────────────────
//
// Enough for the docs' Lua, TOML and shell snippets: comments, strings, numbers, keywords,
// and the name after `function`. Classes: k keyword, s string, c comment, m number/constant,
// nf function name.

const LUA_KEYWORDS: &[&str] =
    &["and", "break", "do", "else", "elseif", "end", "for", "function", "goto", "if", "in", "local", "not", "or", "repeat", "return", "then", "until", "while"];
const LUA_CONSTANTS: &[&str] = &["true", "false", "nil"];

fn highlight(lang: &str, code: &str) -> String {
    let body = match lang {
        "lua" => lua(code),
        "toml" | "powershell" | "ps1" | "bash" | "sh" => hash_comments(code),
        _ => escape(code),
    };
    format!("<pre class=\"code\"><code>{body}</code></pre>")
}

fn span(class: &str, text: &str) -> String {
    format!("<span class=\"{class}\">{}</span>", escape(text))
}

/// Length of a quoted string starting at `s` (which starts with its quote).
fn quoted_len(s: &str) -> usize {
    let quote = s.chars().next().unwrap_or('"');
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote || c == '\n' {
            return i + c.len_utf8();
        }
    }
    s.len()
}

fn number_len(s: &str) -> usize {
    let hex = s.starts_with("0x") || s.starts_with("0X");
    let start = if hex { 2 } else { 0 };
    start
        + s[start..]
            .char_indices()
            .find(|&(_, c)| !(c.is_ascii_hexdigit() && hex || c.is_ascii_digit() || c == '.' || c == '_'))
            .map(|(i, _)| i)
            .unwrap_or(s.len() - start)
}

fn lua(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    let mut after_function = false;
    while let Some(c) = rest.chars().next() {
        let (len, class) = if rest.starts_with("--[[") {
            (rest.find("]]").map(|i| i + 2).unwrap_or(rest.len()), "c")
        } else if rest.starts_with("--") {
            (rest.find('\n').unwrap_or(rest.len()), "c")
        } else if rest.starts_with("[[") {
            (rest.find("]]").map(|i| i + 2).unwrap_or(rest.len()), "s")
        } else if c == '"' || c == '\'' {
            (quoted_len(rest), "s")
        } else if c.is_ascii_digit() {
            (number_len(rest), "m")
        } else if c.is_alphabetic() || c == '_' {
            // A name, dotted/colon parts included when it follows `function`.
            let word_len = rest.find(|ch: char| !(ch.is_alphanumeric() || ch == '_')).unwrap_or(rest.len());
            let word = &rest[..word_len];
            if after_function {
                let len = rest.find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '.' | ':'))).unwrap_or(rest.len());
                after_function = false;
                (len, "nf")
            } else if LUA_KEYWORDS.contains(&word) {
                after_function = word == "function";
                (word_len, "k")
            } else if LUA_CONSTANTS.contains(&word) {
                (word_len, "m")
            } else {
                (word_len, "")
            }
        } else {
            if !c.is_whitespace() {
                after_function = false;
            }
            (c.len_utf8(), "")
        };
        let (text, tail) = rest.split_at(len.max(c.len_utf8()));
        out.push_str(&if class.is_empty() { escape(text) } else { span(class, text) });
        rest = tail;
    }
    out
}

/// TOML and shell: `#` comments, quoted strings, numbers and true/false.
fn hash_comments(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    let mut word_start = true;
    while let Some(c) = rest.chars().next() {
        let (len, class) = if c == '#' {
            (rest.find('\n').unwrap_or(rest.len()), "c")
        } else if c == '"' || c == '\'' {
            (quoted_len(rest), "s")
        } else if c.is_ascii_digit() && word_start {
            (number_len(rest), "m")
        } else if rest.starts_with("true") || rest.starts_with("false") {
            let len = if rest.starts_with("true") { 4 } else { 5 };
            let boundary = rest[len..].chars().next().is_none_or(|n| !n.is_alphanumeric());
            if word_start && boundary { (len, "m") } else { (c.len_utf8(), "") }
        } else {
            (c.len_utf8(), "")
        };
        let (text, tail) = rest.split_at(len.max(c.len_utf8()));
        word_start = !text.chars().last().is_some_and(|l| l.is_alphanumeric() || l == '_' || l == '-');
        out.push_str(&if class.is_empty() { escape(text) } else { span(class, text) });
        rest = tail;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_github() {
        assert_eq!(slugify("kindle.on(event, fn)"), "kindleonevent-fn");
        assert_eq!(slugify("Getting started"), "getting-started");
    }

    #[test]
    fn links_between_pages() {
        let slugs: HashMap<&str, &str> = [("README.md", "home"), ("reference/core.md", "core")].into();
        assert_eq!(resolve_link("../README.md#install", "reference/core.md", &slugs), Some(("#home--install".into(), false)));
        assert_eq!(resolve_link("#on", "reference/core.md", &slugs), Some(("#core--on".into(), false)));
        assert_eq!(resolve_link("../../dist/x.lua", "reference/core.md", &slugs), None);
        assert!(resolve_link("https://example.com", "README.md", &slugs).unwrap().1);
    }

    #[test]
    fn lua_highlighting() {
        let h = lua("local function f(x) -- hi\n  return \"a<b\", 0x1F, nil end");
        assert!(h.contains("<span class=\"k\">local</span>"));
        assert!(h.contains("<span class=\"nf\">f</span>"));
        assert!(h.contains("<span class=\"c\">-- hi</span>"));
        assert!(h.contains("<span class=\"s\">&quot;a&lt;b&quot;</span>"));
        assert!(h.contains("<span class=\"m\">0x1F</span>"));
        assert!(h.contains("<span class=\"m\">nil</span>"));
    }
}
