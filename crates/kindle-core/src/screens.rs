//! Mods' own screens (kindle.ui.screen): full-screen menus in the game's style that a mod builds
//! from rows (buttons, switches, choices, sliders, lines of text), drawn by the game's overlay on
//! the render thread and worked with the controller, the keyboard or the mouse.
//!
//! The mod's Lua runs on the game's thread and the screen is drawn on the render thread, so the
//! two only share data here: a mod opens or changes a screen (a [`Screen`] put on the stack, the
//! newest on top), and the overlay sends back what the player did ([`ScreenEvent`]s), which the
//! game's frame hands to the mod's Lua. A value the player changes is changed on the stack at
//! once ([`change`]), so a slow handler never makes a row lag and a screen keeps its values while
//! a sub-screen is open over it.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Table, Value};

use crate::lock;

/// The most rows a screen holds (more scroll), its tabs' together.
pub const MAX_ROWS: usize = 200;
/// The most tabs a screen has.
pub const MAX_TABS: usize = 8;

/// What a row is, with its value.
#[derive(Clone, Debug, PartialEq)]
pub enum RowKind {
    /// Does something when picked (on_select), or opens a sub-screen. `value` is shown on the right.
    Button { value: Option<String> },
    /// On or off.
    Toggle(bool),
    /// One of a list (index into `choices`).
    Choice { choices: Vec<String>, index: usize },
    /// A number in a range, moved by `step`.
    Slider { min: f64, max: f64, step: f64, value: f64 },
    /// A line of text the player types (up to `max` characters).
    Input { value: String, max: usize },
    /// A key, controller button or mouse button the player picks by pressing it (its name, as
    /// kindle.input and the key_pressed event name it).
    Key { value: String },
    /// A line of text (not selectable): greyed, or with `value` shown on the right (a stat, a count).
    Info { value: Option<String> },
    /// A section's heading (not selectable).
    Header,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub text: String,
    /// A picture left of the text, at the text's height (kindle_core::hud's pictures).
    pub icon: Option<std::sync::Arc<crate::hud::Image>>,
    /// Shown at the bottom while the row is selected.
    pub help: String,
    pub kind: RowKind,
    /// Shown greyed and can't be changed or picked.
    pub disabled: bool,
    /// Shown in the screen's details pane while the row is selected.
    pub details: Option<Details>,
}

/// What a details pane shows for the row selected (as the game's menus show an item's picture
/// and description beside the list).
#[derive(Clone, Debug, PartialEq)]
pub struct Details {
    pub title: String,
    pub text: String,
    pub image: Option<std::sync::Arc<crate::hud::Image>>,
}

impl Row {
    /// Whether the cursor can stop on it.
    pub fn selectable(&self) -> bool {
        !matches!(self.kind, RowKind::Info { .. } | RowKind::Header)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// The mod that opened it.
    pub owner: String,
    pub id: u64,
    pub title: String,
    /// A line under the title.
    pub subtitle: String,
    pub rows: Vec<Row>,
    /// Back closes it (false: only the mod closes it, e.g. a choice that must be made).
    pub closable: bool,
    /// Its tabs, in order (none: one list): each shows `rows[start..]` up to the next tab's start.
    pub tabs: Vec<Tab>,
    /// The tab (0-based) shown when it opens, or that a mod's update turns to.
    pub tab: Option<usize>,
    /// Bumped on every change, so the overlay knows its own edits are now the mod's.
    pub version: u64,
}

/// One tab of a screen: its title and its first row in the screen's rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub title: String,
    pub start: usize,
}

impl Screen {
    /// Whether any row has details (the screen then has a details pane).
    pub fn has_details(&self) -> bool {
        self.rows.iter().any(|r| r.details.is_some())
    }

    /// The rows tab `tab` shows (all of them when it has no tabs).
    pub fn tab_rows(&self, tab: usize) -> std::ops::Range<usize> {
        if self.tabs.is_empty() {
            return 0..self.rows.len();
        }
        let tab = tab.min(self.tabs.len() - 1);
        let end = self.tabs.get(tab + 1).map_or(self.rows.len(), |t| t.start);
        self.tabs[tab].start..end
    }
}

/// What the player did on a screen.
#[derive(Clone, Debug, PartialEq)]
pub enum ScreenEvent {
    /// A button was picked (row index, 0-based).
    Selected(usize),
    /// A switch, choice or slider changed: the row and its new value (a switch 0/1, a choice its
    /// 0-based index, a slider the number).
    Changed(usize, f64),
    /// The player typed a new text into an input row (and kept it).
    Text(usize, String),
    /// The player turned to another tab (0-based).
    Tab(usize),
    /// The player backed out of it.
    Closed,
}

/// The screens open, oldest first: the last is the one shown.
static STACK: Mutex<Vec<Screen>> = Mutex::new(Vec::new());
/// (owner, screen id, event), oldest first.
static EVENTS: Mutex<Vec<(String, u64, ScreenEvent)>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static VERSION: AtomicU64 = AtomicU64::new(1);

/// Opens a screen on top of the others (a sub-screen goes back to the one under it); returns its id.
pub fn open(mut screen: Screen) -> u64 {
    screen.id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    screen.version = VERSION.fetch_add(1, Ordering::Relaxed);
    let id = screen.id;
    lock(&STACK).push(screen);
    id
}

/// Replaces an open screen's title, rows and so on (it stays where it is in the stack).
pub fn update(owner: &str, id: u64, mut screen: Screen) -> Result<(), String> {
    let mut stack = lock(&STACK);
    let Some(at) = stack.iter().position(|s| s.id == id && s.owner == owner) else { return Err("that screen isn't open".into()) };
    screen.id = id;
    screen.version = VERSION.fetch_add(1, Ordering::Relaxed);
    stack[at] = screen;
    Ok(())
}

/// Closes one of a mod's screens (and any opened above it, which belonged to it).
pub fn close(owner: &str, id: u64) -> bool {
    let mut stack = lock(&STACK);
    let Some(at) = stack.iter().position(|s| s.id == id && s.owner == owner) else { return false };
    stack.truncate(at);
    true
}

/// Closes every screen a mod has open (it unloaded).
pub fn close_owner(owner: &str) {
    lock(&STACK).retain(|s| s.owner != owner);
}

/// The open screens' ids, oldest first (the last is shown).
pub fn ids() -> Vec<u64> {
    lock(&STACK).iter().map(|s| s.id).collect()
}

/// An open screen as it is now.
pub fn get(id: u64) -> Option<Screen> {
    lock(&STACK).iter().find(|s| s.id == id).cloned()
}

pub fn is_open(id: u64) -> bool {
    lock(&STACK).iter().any(|s| s.id == id)
}

/// The screen on top, if any (for the overlay).
pub fn top() -> Option<Screen> {
    lock(&STACK).last().cloned()
}

/// Whether any mod screen is open (the game's input is then the screen's).
pub fn any_open() -> bool {
    !lock(&STACK).is_empty()
}

/// From the overlay: the player did something on screen `id`. Backing out also takes the screen
/// off the stack at once.
pub fn report(id: u64, event: ScreenEvent) {
    let owner = {
        let mut stack = lock(&STACK);
        let Some(at) = stack.iter().position(|s| s.id == id) else { return };
        let owner = stack[at].owner.clone();
        if event == ScreenEvent::Closed {
            stack.truncate(at);
        }
        owner
    };
    lock(&EVENTS).push((owner, id, event));
}

/// From the overlay: the player changed row `row` of screen `id` (a switch, a choice, a slider)
/// to `kind`. The screen keeps it (a sub-screen opened and closed on top of it shows the change
/// still), and the mod hears of it as Changed with `value`.
pub fn change(id: u64, row: usize, kind: RowKind, value: f64) {
    let owner = {
        let mut stack = lock(&STACK);
        let Some(screen) = stack.iter_mut().find(|s| s.id == id) else { return };
        let Some(r) = screen.rows.get_mut(row) else { return };
        r.kind = kind;
        screen.owner.clone()
    };
    lock(&EVENTS).push((owner, id, ScreenEvent::Changed(row, value)));
}

/// From the overlay: the player typed `text` into input row `row` of screen `id` and kept it, or
/// pressed the key named `text` for key row `row`.
pub fn set_text(id: u64, row: usize, text: String) {
    let owner = {
        let mut stack = lock(&STACK);
        let Some(screen) = stack.iter_mut().find(|s| s.id == id) else { return };
        let Some(r) = screen.rows.get_mut(row) else { return };
        let (RowKind::Input { value, .. } | RowKind::Key { value }) = &mut r.kind else { return };
        *value = text.clone();
        screen.owner.clone()
    };
    lock(&EVENTS).push((owner, id, ScreenEvent::Text(row, text)));
}

/// What the player did since the last frame, for the game's frame to hand to the mods.
pub fn take_events() -> Vec<(String, u64, ScreenEvent)> {
    std::mem::take(&mut *lock(&EVENTS))
}

/// A slider's value moved `dir` steps, kept in range and on its steps.
pub fn slide(min: f64, max: f64, step: f64, value: f64, dir: i32) -> f64 {
    let step = if step > 0.0 { step } else { ((max - min) / 20.0).max(f64::EPSILON) };
    let steps = ((value - min) / step).round() + dir as f64;
    let v = (min + steps * step).clamp(min, max);
    // Steps like 0.1 add up to 0.30000000000000004: keep what the step has places for.
    let places = format!("{step}").split('.').nth(1).map_or(0, |d| d.len()) as i32;
    let scale = 10f64.powi(places.min(9));
    (v * scale).round() / scale
}

/// A slider's value as shown: whole numbers without a point, others with the step's places.
pub fn show_number(value: f64, step: f64) -> String {
    if value.fract() == 0.0 && step.fract() == 0.0 {
        return format!("{}", value as i64);
    }
    let places = format!("{step}").split('.').nth(1).map_or(2, |d| d.len()).clamp(1, 6);
    format!("{value:.places$}")
}

/// Reads a screen from a mod's table: {title, subtitle?, closable?, rows = {...}}, or tabs =
/// {{title, rows = {...}}, ...} (a tab's rows follow the tab before's) with tab? (1-based). Each row:
/// {text, help?, disabled?} and one of: `header = true`, `info = true`, `toggle = bool`,
/// `choices = {...}` with `value` (1-based), `slider = {min, max, step?}` with `value`, `input =
/// "text"` with `max?`, `key = "f6"` (a key the player picks), or a
/// button (`value` the text shown on its right); any row may have `icon = "file.png"`, found by
/// `images` (a PNG in the mod's folder). Callbacks stay in Lua (the prelude keeps them).
pub fn from_lua(owner: &str, t: &Table, images: &crate::hud::Images<'_>) -> Result<Screen, String> {
    let title: String = t.get::<Option<String>>("title").map_err(|e| e.to_string())?.unwrap_or_default();
    let subtitle: String = t.get::<Option<String>>("subtitle").map_err(|e| e.to_string())?.unwrap_or_default();
    let closable = t.get::<Option<bool>>("closable").map_err(|e| e.to_string())?.unwrap_or(true);
    let mut rows = Vec::new();
    let mut tabs = Vec::new();
    let read_rows = |rows_t: Table, rows: &mut Vec<Row>, tab: &str| -> Result<(), String> {
        for (i, r) in rows_t.sequence_values::<Table>().enumerate() {
            let n = i + 1;
            let r = r.map_err(|_| format!("{tab}row {n} isn't a table"))?;
            if rows.len() >= MAX_ROWS {
                return Err(format!("more than {MAX_ROWS} rows"));
            }
            rows.push(row_from_lua(&r, images).map_err(|e| format!("{tab}row {n}: {e}"))?);
        }
        Ok(())
    };
    match t.get::<Option<Table>>("tabs").map_err(|_| "tabs must be a list of {title, rows}".to_string())? {
        Some(tabs_t) => {
            for (i, tab) in tabs_t.sequence_values::<Table>().enumerate() {
                let n = i + 1;
                let tab = tab.map_err(|_| format!("tab {n} isn't a table"))?;
                let title: String = tab.get::<Option<String>>("title").ok().flatten().ok_or_else(|| format!("tab {n} needs a title"))?;
                let rows_t: Table = tab.get("rows").map_err(|_| format!("tab {n} needs rows = {{...}}"))?;
                tabs.push(Tab { title, start: rows.len() });
                read_rows(rows_t, &mut rows, &format!("tab {n}, "))?;
            }
            if tabs.is_empty() {
                return Err("tabs is empty".into());
            }
            if tabs.len() > MAX_TABS {
                return Err(format!("more than {MAX_TABS} tabs"));
            }
        }
        None => read_rows(t.get("rows").map_err(|_| "a screen needs rows = {...} (or tabs = {...})".to_string())?, &mut rows, "")?,
    }
    let tab = match t.get::<Option<i64>>("tab").map_err(|_| "tab must be a tab's number".to_string())? {
        Some(n) if n >= 1 && (n as usize) <= tabs.len() => Some(n as usize - 1),
        Some(n) => return Err(format!("tab {n}: the screen has {} tabs", tabs.len())),
        None => None,
    };
    Ok(Screen { owner: owner.into(), id: 0, title, subtitle, rows, closable, tabs, tab, version: 0 })
}

fn row_from_lua(r: &Table, images: &crate::hud::Images<'_>) -> Result<Row, String> {
    let get_str = |k: &str| r.get::<Option<String>>(k).map_err(|_| format!("{k} must be text"));
    let help = get_str("help")?.unwrap_or_default();
    let icon = match r.get::<Option<String>>("icon").map_err(|_| "icon must be a file's path in the mod's folder or the game's icon id".to_string())? {
        Some(path) => Some(images(&path).map_err(|e| format!("icon {path:?}: {e}"))?),
        None => None,
    };
    let disabled = r.get::<Option<bool>>("disabled").map_err(|_| "disabled must be true or false".to_string())?.unwrap_or(false);
    let details = match r.get::<Option<Table>>("details").map_err(|_| "details must be a table {title, text, image}".to_string())? {
        Some(d) => {
            let get = |k: &str| d.get::<Option<String>>(k).map_err(|_| format!("details' {k} must be text"));
            let image = match get("image")? {
                Some(path) => Some(images(&path).map_err(|e| format!("details' image {path:?}: {e}"))?),
                None => None,
            };
            Some(Details { title: get("title")?.unwrap_or_default(), text: get("text")?.unwrap_or_default(), image })
        }
        None => None,
    };
    if let Some(text) = get_str("header")? {
        return Ok(Row { text, icon, help, kind: RowKind::Header, disabled, details });
    }
    let text = get_str("text")?.ok_or("it needs text (or header = \"...\")")?;
    let value: Value = r.get("value").unwrap_or(Value::Nil);
    let shown = |value: Value| -> Result<Option<String>, String> {
        Ok(match value {
            Value::Nil => None,
            Value::String(s) => Some(s.to_str().map_err(|e| e.to_string())?.to_string()),
            Value::Integer(i) => Some(i.to_string()),
            Value::Number(f) => Some(format!("{f}")),
            Value::Boolean(b) => Some(if b { "On".into() } else { "Off".into() }),
            _ => return Err("value must be text or a number".into()),
        })
    };
    let kind = if r.get::<Option<bool>>("info").ok().flatten() == Some(true) {
        RowKind::Info { value: shown(value)? }
    } else if let Ok(Some(on)) = r.get::<Option<bool>>("toggle") {
        RowKind::Toggle(on)
    } else if let Ok(Some(choices)) = r.get::<Option<Table>>("choices") {
        let choices: Vec<String> = choices.sequence_values::<String>().collect::<Result<_, _>>().map_err(|_| "choices must be a list of text")?;
        if choices.is_empty() {
            return Err("choices is empty".into());
        }
        let index = match value {
            Value::Integer(i) => (i.max(1) as usize - 1).min(choices.len() - 1),
            Value::Number(f) => (f.max(1.0) as usize - 1).min(choices.len() - 1),
            Value::String(s) => {
                let s = s.to_str().map_err(|e| e.to_string())?.to_string();
                choices.iter().position(|c| *c == s).unwrap_or(0)
            }
            _ => 0,
        };
        RowKind::Choice { choices, index }
    } else if let Some(name) = get_str("key")? {
        RowKind::Key { value: name.to_ascii_lowercase() }
    } else if let Some(text) = get_str("input")? {
        let max = r.get::<Option<i64>>("max").ok().flatten().unwrap_or(64).clamp(1, 256) as usize;
        RowKind::Input { value: text.chars().take(max).collect(), max }
    } else if let Ok(Some(s)) = r.get::<Option<Table>>("slider") {
        let min: f64 = s.get::<Option<f64>>("min").ok().flatten().ok_or("slider needs min")?;
        let max: f64 = s.get::<Option<f64>>("max").ok().flatten().ok_or("slider needs max")?;
        if max.partial_cmp(&min) != Some(std::cmp::Ordering::Greater) {
            return Err("slider's max must be above its min".into());
        }
        let step = s.get::<Option<f64>>("step").ok().flatten().filter(|s| *s > 0.0).unwrap_or((max - min) / 20.0);
        let v = match value {
            Value::Integer(i) => i as f64,
            Value::Number(f) if f.is_finite() => f,
            _ => min,
        };
        RowKind::Slider { min, max, step, value: v.clamp(min, max) }
    } else {
        RowKind::Button { value: shown(value)? }
    };
    Ok(Row { text, icon, help, kind, disabled, details })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(owner: &str, title: &str) -> Screen {
        Screen {
            owner: owner.into(),
            id: 0,
            title: title.into(),
            subtitle: String::new(),
            rows: Vec::new(),
            closable: true,
            tabs: Vec::new(),
            tab: None,
            version: 0,
        }
    }

    /// Screens stack, back closes the top one and reports it, and a mod's screens go with it.
    #[test]
    fn stack_and_events() {
        let a = open(screen("test-screens-a", "A"));
        let b = open(screen("test-screens-a", "B"));
        let c = open(screen("test-screens-c", "C"));
        assert!(is_open(a) && is_open(b) && is_open(c));
        report(c, ScreenEvent::Closed);
        assert!(!is_open(c) && is_open(b));
        report(b, ScreenEvent::Changed(2, 1.0));
        let ours: Vec<_> = take_events().into_iter().filter(|(o, ..)| o.starts_with("test-screens")).collect();
        assert_eq!(ours, [("test-screens-c".into(), c, ScreenEvent::Closed), ("test-screens-a".into(), b, ScreenEvent::Changed(2, 1.0))]);
        assert!(update("test-screens-a", b, screen("test-screens-a", "B2")).is_ok());
        assert!(update("test-screens-c", b, screen("test-screens-c", "x")).is_err());
        assert!(close("test-screens-a", a));
        assert!(!is_open(a) && !is_open(b));
        let d = open(screen("test-screens-d", "D"));
        close_owner("test-screens-d");
        assert!(!is_open(d));
    }

    #[test]
    fn sliders_keep_to_their_steps() {
        assert_eq!(slide(0.0, 1.0, 0.1, 0.2, 1), 0.3);
        assert_eq!(slide(0.0, 1.0, 0.1, 1.0, 1), 1.0);
        assert_eq!(slide(1.0, 5.0, 1.0, 1.0, -1), 1.0);
        assert_eq!(slide(0.5, 3.0, 0.25, 0.5, 3), 1.25);
        assert_eq!(show_number(3.0, 1.0), "3");
        assert_eq!(show_number(1.5, 0.1), "1.5");
        assert_eq!(show_number(1.25, 0.25), "1.25");
    }

    #[test]
    fn rows_from_lua() {
        let lua = mlua::Lua::new();
        let t: Table = lua
            .load(
                r#"return { title = "Hard Mode", rows = {
                    { header = "Enemies" },
                    { text = "Damage", slider = { min = 0.5, max = 3, step = 0.5 }, value = 1.5 },
                    { text = "Mode", choices = { "Easy", "Hard" }, value = 2 },
                    { text = "Mode by name", choices = { "Easy", "Hard" }, value = "Hard" },
                    { text = "Permadeath", toggle = false, help = "One life." },
                    { text = "Reset", value = 3 },
                    { text = "Read me", info = true },
                    { text = "Vigor", value = 12, info = true },
                    { text = "Name", input = "Ashen One", max = 4 },
                    { text = "Heal", key = "F6", icon = "art/flask.png" },
                } }"#,
            )
            .eval()
            .unwrap();
        let flask = std::sync::Arc::new(crate::hud::Image { id: 7, width: 2, height: 2, rgba: vec![0; 16] });
        let s = from_lua("m", &t, &|p: &str| if p == "art/flask.png" { Ok(flask.clone()) } else { Err("missing".into()) }).unwrap();
        assert_eq!(s.title, "Hard Mode");
        assert!(s.rows[9].icon.as_ref().is_some_and(|i| i.id == 7) && s.rows[8].icon.is_none());
        let detailed: Table = lua
            .load(r#"return { rows = { { text = "Flask", details = { title = "Estus", text = "Heals.", image = "art/flask.png" } }, { text = "x" } } }"#)
            .eval()
            .unwrap();
        let d = from_lua("m", &detailed, &|p: &str| if p == "art/flask.png" { Ok(flask.clone()) } else { Err("missing".into()) }).unwrap();
        let shown = d.rows[0].details.as_ref().unwrap();
        assert_eq!((shown.title.as_str(), shown.text.as_str(), shown.image.as_ref().map(|i| i.id)), ("Estus", "Heals.", Some(7)));
        assert!(d.has_details() && d.rows[1].details.is_none() && !s.has_details());
        let kinds: Vec<_> = s.rows.iter().map(|r| r.kind.clone()).collect();
        assert_eq!(
            kinds,
            [
                RowKind::Header,
                RowKind::Slider { min: 0.5, max: 3.0, step: 0.5, value: 1.5 },
                RowKind::Choice { choices: vec!["Easy".into(), "Hard".into()], index: 1 },
                RowKind::Choice { choices: vec!["Easy".into(), "Hard".into()], index: 1 },
                RowKind::Toggle(false),
                RowKind::Button { value: Some("3".into()) },
                RowKind::Info { value: None },
                RowKind::Info { value: Some("12".into()) },
                RowKind::Input { value: "Ashe".into(), max: 4 },
                RowKind::Key { value: "f6".into() },
            ]
        );
        assert_eq!(s.rows[4].help, "One life.");
        assert!(!s.rows[0].selectable() && s.rows[1].selectable());
        let tabbed: Table = lua
            .load(r#"return { title = "T", tab = 2, tabs = { { title = "A", rows = { { text = "a1" }, { text = "a2" } } }, { title = "B", rows = { { text = "b1" } } } } }"#)
            .eval()
            .unwrap();
        let s = from_lua("m", &tabbed, &|_: &str| Err("no pictures".to_string())).unwrap();
        assert_eq!((s.rows.len(), s.tabs[1].start, s.tab), (3, 2, Some(1)));
        assert_eq!((s.tab_rows(0), s.tab_rows(1), s.tab_rows(9)), (0..2, 2..3, 2..3));
        let bad_tab: Table = lua.load(r#"return { tabs = { { title = "A", rows = { { text = "a", slider = {} } } } } }"#).eval().unwrap();
        assert!(from_lua("m", &bad_tab, &|_: &str| Err("x".to_string())).unwrap_err().contains("tab 1, row 1"));
        let bad: Table = lua.load(r#"return { rows = { { text = "x", slider = { min = 2, max = 1 } } } }"#).eval().unwrap();
        assert!(from_lua("m", &bad, &|_: &str| Err("no pictures".to_string())).unwrap_err().contains("row 1"));
    }
}
