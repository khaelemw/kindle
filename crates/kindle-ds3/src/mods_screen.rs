//! The Mods screen: a full-screen menu in the game's style (opened from "Mods" in the game's
//! title and pause menus), for turning mods on and off and changing their settings with the
//! mouse, a controller or the keyboard. It draws with ImGui on the render thread, like the
//! overlay, and changes things through ui_bridge requests. The button guide shows the
//! controller's buttons or the keyboard's keys, whichever was used last, as the game does.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use hudhook::imgui::{DrawListMut, FontId, Key, MouseButton, Ui};

use crate::config::{SettingKind as K, SettingValue as V};
use crate::input_block;
use crate::ui_bridge::{self, ModRow, SettingRow, Snapshot, UiRequest};

// The game's menu colours: warm off-white text, muted gold highlights, near-black panels.
pub(crate) const TEXT: [f32; 4] = [0.93, 0.90, 0.82, 1.0];
pub(crate) const SOFT: [f32; 4] = [0.85, 0.82, 0.75, 1.0];
pub(crate) const MUTED: [f32; 4] = [0.62, 0.58, 0.50, 1.0];
pub(crate) const GOLD: [f32; 4] = [0.80, 0.68, 0.42, 1.0];
pub(crate) const PANEL: [f32; 4] = [0.04, 0.035, 0.03, 0.92];
pub(crate) const DIM: [f32; 4] = [0.0, 0.0, 0.0, 0.6];
pub(crate) const CLEAR: [f32; 4] = [0.0, 0.0, 0.0, 0.0];
// Xbox face buttons, as the game's PC prompts show them.
const XBOX_A: [f32; 4] = [0.36, 0.66, 0.24, 1.0];
const XBOX_B: [f32; 4] = [0.80, 0.22, 0.20, 1.0];

const REPEAT_DELAY: Duration = Duration::from_millis(380);
const REPEAT_RATE: Duration = Duration::from_millis(90);
/// How far the mouse must move after keyboard or controller use before hovering selects again.
const MOUSE_WAKE: f32 = 6.0;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Nav {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    /// The tab before or after (a screen with tabs): LB / RB, Q / E.
    PrevTab,
    NextTab,
}

#[derive(PartialEq)]
enum Focus {
    Mods,
    Settings,
}

/// What the player used last: the button guide matches it.
#[derive(PartialEq, Clone, Copy)]
pub(crate) enum Device {
    Pad,
    Keys,
}

pub struct ModsScreen {
    pub open: bool,
    focus: Focus,
    mod_index: usize,
    row: usize,
    /// The first mod and the first row shown: the lists scroll to keep the selection in view.
    mod_top: usize,
    row_top: usize,
    input: MenuInput,
    /// Values changed here but not in the snapshot yet: (mod, setting) -> value.
    edits: HashMap<(String, String), V>,
    enabled_edits: HashMap<String, bool>,
    /// The mod to show the settings of once the list has it (opened at one mod).
    pub open_at: Option<String>,
}

impl Default for ModsScreen {
    fn default() -> Self {
        Self {
            open: false,
            focus: Focus::Mods,
            mod_index: 0,
            row: 0,
            mod_top: 0,
            row_top: 0,
            input: MenuInput::default(),
            edits: HashMap::new(),
            enabled_edits: HashMap::new(),
            open_at: None,
        }
    }
}

/// One line on the right: the mod's on/off switch, then its settings.
enum Row<'a> {
    Enabled,
    Setting(&'a SettingRow),
}

fn rows(m: &ModRow) -> Vec<Row<'_>> {
    std::iter::once(Row::Enabled).chain(m.settings.iter().map(Row::Setting)).collect()
}

/// A setting's value moved one step left (-1) or right (+1).
fn step(kind: &K, value: &V, dir: i32) -> Option<V> {
    match (kind, value) {
        (K::Bool, V::Bool(b)) => Some(V::Bool(!b)),
        (K::Int { min, max }, V::Int(i)) => {
            let bounded = *min > i32::MIN as i64 && *max < i32::MAX as i64;
            let step = if bounded { ((max - min) / 20).max(1) } else { 1 };
            let v = (i + step * dir as i64).clamp(*min, *max);
            (v != *i).then_some(V::Int(v))
        }
        (K::Float { min, max }, V::Float(f)) => {
            let bounded = *min > f32::MIN as f64 && *max < f32::MAX as f64;
            let step = if bounded { (max - min) / 20.0 } else { 0.1 };
            let v = ((f + step * dir as f64) * 100.0).round() / 100.0;
            let v = v.clamp(*min, *max);
            ((v - f).abs() > f64::EPSILON).then_some(V::Float(v))
        }
        (K::Choice(list), V::Choice(c)) if !list.is_empty() => {
            let i = list.iter().position(|x| x == c).unwrap_or(0) as i32;
            let n = list.len() as i32;
            Some(V::Choice(list[((i + dir).rem_euclid(n)) as usize].clone()))
        }
        _ => None,
    }
}

fn show(value: &V) -> String {
    match value {
        V::Bool(true) => "On".into(),
        V::Bool(false) => "Off".into(),
        V::Int(i) => i.to_string(),
        V::Float(f) => format!("{f:.2}").trim_end_matches('0').trim_end_matches('.').to_string(),
        V::Choice(c) => c.clone(),
    }
}

/// How far along its range a number is (0..1), for the bar under it.
fn fraction(kind: &K, value: &V) -> Option<f32> {
    match (kind, value) {
        (K::Int { min, max }, V::Int(i)) if *min > i32::MIN as i64 && *max < i32::MAX as i64 && max > min => Some((*i - min) as f32 / (max - min) as f32),
        (K::Float { min, max }, V::Float(f)) if *min > f32::MIN as f64 && *max < f32::MAX as f64 && max > min => Some(((f - min) / (max - min)) as f32),
        _ => None,
    }
}

pub(crate) type Rect = ([f32; 2], [f32; 2]);

pub(crate) fn inside(r: Rect, p: [f32; 2]) -> bool {
    p[0] >= r.0[0] && p[0] < r.1[0] && p[1] >= r.0[1] && p[1] < r.1[1]
}

/// Where everything goes this frame (with the body font pushed).
struct Layout {
    panel: Rect,
    pad: f32,
    line_h: f32,
    top: f32,
    split: f32,
    /// Lowest y a list line may start at (above the guide).
    bottom: f32,
    /// Where values are centred, and the arrows' half-distance from there.
    value_x: f32,
    arrow_dx: f32,
}

impl Layout {
    fn new(ui: &Ui) -> Self {
        let [w, h] = ui.io().display_size;
        let (pw, ph) = ((w * 0.72).min(1500.0), (h * 0.72).min(860.0));
        let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
        let pad = (pw * 0.035).max(20.0);
        let line_h = ui.text_line_height() * 1.7;
        Self {
            panel: ([x0, y0], [x0 + pw, y0 + ph]),
            pad,
            line_h,
            top: y0 + pad * 0.7 + line_h * 1.9,
            split: x0 + pw * 0.32,
            // Above the help line and the button guide (as on mods' own screens).
            bottom: y0 + ph - line_h * 3.3,
            value_x: x0 + pw - pad - pw * 0.1,
            arrow_dx: pw * 0.1,
        }
    }

    fn mod_rect(&self, i: usize) -> Rect {
        let y = self.top + i as f32 * self.line_h;
        ([self.panel.0[0] + self.pad * 0.5, y - 4.0], [self.split - self.pad * 0.5, y + self.line_h - 8.0])
    }

    fn row_y(&self, i: usize) -> f32 {
        self.top + self.line_h * (1.9 + i as f32)
    }

    fn row_rect(&self, i: usize) -> Rect {
        let y = self.row_y(i);
        ([self.split + self.pad * 0.5, y - 4.0], [self.panel.1[0] - self.pad * 0.5, y + self.line_h - 8.0])
    }

    /// How many mods the list shows at once.
    fn mods_fit(&self) -> usize {
        (0..).take_while(|&i| self.mod_rect(i).1[1] <= self.bottom + self.line_h).count().max(1)
    }

    /// How many rows the selected mod's side shows at once.
    fn rows_fit(&self) -> usize {
        (0..).take_while(|&i| self.row_y(i) <= self.bottom).count().max(1)
    }

    /// The clickable areas of a row's left and right arrows.
    fn arrows(&self, i: usize) -> (Rect, Rect) {
        let y = self.row_y(i);
        let s = self.line_h * 0.6;
        let left = self.value_x - self.arrow_dx;
        let right = self.value_x + self.arrow_dx;
        (([left - s, y - 4.0], [left + s, y + self.line_h - 8.0]), ([right - s, y - 4.0], [right + s, y + self.line_h - 8.0]))
    }
}

/// Small gold triangles at the top and bottom of a list's area ([x0, x1], [top, bottom]) when it
/// has more above or below.
fn more_markers(dl: &DrawListMut, l: &Layout, x: [f32; 2], y: [f32; 2], above: bool, below: bool) {
    let (cx, s) = ((x[0] + x[1]) / 2.0, l.line_h * 0.12);
    let c = [GOLD[0], GOLD[1], GOLD[2], 0.8];
    if above {
        dl.add_triangle([cx - s * 1.5, y[0] - 2.0], [cx + s * 1.5, y[0] - 2.0], [cx, y[0] - 2.0 - s * 1.5], c).filled(true).build();
    }
    if below {
        dl.add_triangle([cx - s * 1.5, y[1] + 2.0], [cx + s * 1.5, y[1] + 2.0], [cx, y[1] + 2.0 + s * 1.5], c).filled(true).build();
    }
}

// ── button guide glyphs ─────────────────────────────────────────────────────────────────

/// A filled round face button with its letter, as the game's controller prompts.
fn face_button(ui: &Ui, dl: &DrawListMut, at: [f32; 2], size: f32, color: [f32; 4], letter: &str) -> f32 {
    let r = size * 0.5;
    let c = [at[0] + r, at[1] + r];
    dl.add_circle(c, r, color).filled(true).num_segments(24).build();
    let t = ui.calc_text_size(letter);
    dl.add_text([c[0] - t[0] / 2.0, c[1] - t[1] / 2.0], [1.0, 1.0, 1.0, 1.0], letter);
    size
}

/// A D-pad: a light cross.
fn dpad(dl: &DrawListMut, at: [f32; 2], size: f32) -> f32 {
    let (t, c) = (size * 0.34, [at[0] + size / 2.0, at[1] + size / 2.0]);
    let col = [0.82, 0.80, 0.76, 1.0];
    dl.add_rect([c[0] - t / 2.0, at[1]], [c[0] + t / 2.0, at[1] + size], col).filled(true).rounding(2.0).build();
    dl.add_rect([at[0], c[1] - t / 2.0], [at[0] + size, c[1] + t / 2.0], col).filled(true).rounding(2.0).build();
    size
}

#[derive(Clone, Copy)]
pub(crate) enum Arrows {
    UpDown,
    LeftRight,
}

/// A keyboard key: an outlined cap with a label, or with arrows drawn in it.
fn key_cap(ui: &Ui, dl: &DrawListMut, at: [f32; 2], size: f32, label: Option<&str>, arrows: Option<Arrows>) -> f32 {
    let w = match label {
        Some(l) => ui.calc_text_size(l)[0] + size * 0.6,
        None => size * 1.4,
    };
    let col = [0.82, 0.80, 0.76, 1.0];
    dl.add_rect(at, [at[0] + w, at[1] + size], col).rounding(3.0).thickness(1.5).build();
    if let Some(l) = label {
        let t = ui.calc_text_size(l);
        dl.add_text([at[0] + (w - t[0]) / 2.0, at[1] + (size - t[1]) / 2.0], col, l);
    }
    if let Some(a) = arrows {
        let (cx, cy, s) = (at[0] + w / 2.0, at[1] + size / 2.0, size * 0.18);
        match a {
            Arrows::UpDown => {
                dl.add_triangle([cx - s, cy - s * 0.4], [cx + s, cy - s * 0.4], [cx, cy - s * 1.6], col).filled(true).build();
                dl.add_triangle([cx - s, cy + s * 0.4], [cx + s, cy + s * 0.4], [cx, cy + s * 1.6], col).filled(true).build();
            }
            Arrows::LeftRight => {
                dl.add_triangle([cx - s * 0.4, cy - s], [cx - s * 0.4, cy + s], [cx - s * 1.6, cy], col).filled(true).build();
                dl.add_triangle([cx + s * 0.4, cy - s], [cx + s * 0.4, cy + s], [cx + s * 1.6, cy], col).filled(true).build();
            }
        }
    }
    w
}

/// One prompt of the guide.
pub(crate) enum Glyph {
    PadA,
    PadB,
    Dpad,
    Key(&'static str),
    KeyArrows(Arrows),
}

fn draw_glyph(ui: &Ui, dl: &DrawListMut, g: &Glyph, at: [f32; 2], size: f32) -> f32 {
    match g {
        Glyph::PadA => face_button(ui, dl, at, size, XBOX_A, "A"),
        Glyph::PadB => face_button(ui, dl, at, size, XBOX_B, "B"),
        Glyph::Dpad => dpad(dl, at, size),
        Glyph::Key(l) => key_cap(ui, dl, at, size, Some(l), None),
        Glyph::KeyArrows(a) => key_cap(ui, dl, at, size, None, Some(*a)),
    }
}

/// The guide, right-aligned at `right`: each prompt is a glyph, then ": label".
pub(crate) fn guide(ui: &Ui, dl: &DrawListMut, items: &[(Glyph, &str)], right: f32, y: f32, size: f32) {
    let gap = size * 1.2;
    // Measure by drawing off-screen first would cost a pass; widths are cheap to work out.
    let widths: Vec<f32> = items
        .iter()
        .map(|(g, label)| {
            let gw = match g {
                Glyph::PadA | Glyph::PadB | Glyph::Dpad => size,
                Glyph::Key(l) => ui.calc_text_size(l)[0] + size * 0.6,
                Glyph::KeyArrows(_) => size * 1.4,
            };
            gw + ui.calc_text_size(format!(": {label}"))[0] + size * 0.15
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + gap * (items.len().saturating_sub(1)) as f32;
    let mut x = right - total;
    let text_h = ui.text_line_height();
    for ((g, label), w) in items.iter().zip(widths) {
        let gw = draw_glyph(ui, dl, g, [x, y + (text_h - size) / 2.0], size);
        dl.add_text([x + gw + size * 0.15, y], SOFT, format!(": {label}"));
        x += w + gap;
    }
}

/// Which device the player is using, and what it takes to read it as the game's menus do: the
/// controller's buttons last frame, a held direction's next repeat, and where the mouse rests.
pub(crate) struct MenuInput {
    pub(crate) device: Device,
    pad_last: u16,
    repeat: Option<(u16, Instant)>,
    mouse_last: [f32; 2],
    /// Where the mouse was when the keyboard or controller was last used; hover is ignored
    /// until it moves away from there.
    mouse_rest: Option<[f32; 2]>,
}

impl Default for MenuInput {
    fn default() -> Self {
        Self { device: Device::Keys, pad_last: 0, repeat: None, mouse_last: [0.0; 2], mouse_rest: None }
    }
}

impl MenuInput {
    /// The keys and controller buttons pressed this frame (a held direction repeats), and a right
    /// click as Back.
    pub(crate) fn navs(&mut self, ui: &Ui) -> Vec<Nav> {
        let mut out = Vec::new();
        for (key, nav) in [(Key::UpArrow, Nav::Up), (Key::DownArrow, Nav::Down), (Key::LeftArrow, Nav::Left), (Key::RightArrow, Nav::Right)] {
            if ui.is_key_pressed(key) {
                out.push(nav);
            }
        }
        for key in [Key::Enter, Key::Space, Key::KeypadEnter] {
            if ui.is_key_pressed_no_repeat(key) {
                out.push(Nav::Accept);
            }
        }
        for key in [Key::Escape, Key::Backspace] {
            if ui.is_key_pressed_no_repeat(key) {
                out.push(Nav::Back);
            }
        }
        for (key, nav) in [(Key::Q, Nav::PrevTab), (Key::E, Nav::NextTab)] {
            if ui.is_key_pressed_no_repeat(key) {
                out.push(nav);
            }
        }
        if !out.is_empty() {
            self.device = Device::Keys;
        }
        // Controller (the game's own reads are blanked while this is open).
        let pad = input_block::PAD_RAW.load(Ordering::Relaxed);
        let pressed = pad & !self.pad_last;
        self.pad_last = pad;
        if pressed != 0 {
            self.device = Device::Pad;
        }
        let dirs =
            [(input_block::PAD_UP, Nav::Up), (input_block::PAD_DOWN, Nav::Down), (input_block::PAD_LEFT, Nav::Left), (input_block::PAD_RIGHT, Nav::Right)];
        let now = Instant::now();
        for (bit, nav) in dirs {
            if pressed & bit != 0 {
                out.push(nav);
                self.repeat = Some((bit, now + REPEAT_DELAY));
            }
        }
        if let Some((bit, at)) = self.repeat {
            if pad & bit == 0 {
                self.repeat = None;
            } else if now >= at {
                out.extend(dirs.iter().filter(|(b, _)| *b == bit).map(|(_, n)| *n));
                self.repeat = Some((bit, now + REPEAT_RATE));
            }
        }
        if pressed & input_block::PAD_A != 0 {
            out.push(Nav::Accept);
        }
        if pressed & input_block::PAD_B != 0 {
            out.push(Nav::Back);
        }
        if pressed & input_block::PAD_LB != 0 {
            out.push(Nav::PrevTab);
        }
        if pressed & input_block::PAD_RB != 0 {
            out.push(Nav::NextTab);
        }
        if ui.is_mouse_clicked(MouseButton::Right) {
            self.device = Device::Keys;
            out.push(Nav::Back);
        }
        out
    }

    /// The keyboard or controller was used: hover waits until the mouse really moves.
    pub(crate) fn rest_mouse(&mut self, ui: &Ui) {
        self.mouse_rest = Some(ui.io().mouse_pos);
    }

    /// The mouse's position if it moved (past its rest) or clicked this frame, and whether it
    /// clicked; None when it did neither, or there's no mouse.
    pub(crate) fn mouse(&mut self, ui: &Ui) -> Option<([f32; 2], bool)> {
        let pos = ui.io().mouse_pos;
        // ImGui reports a huge negative position when it has no mouse.
        if pos[0] < -1e6 || pos[1] < -1e6 {
            return None;
        }
        let clicked = ui.is_mouse_clicked(MouseButton::Left);
        let moved = match self.mouse_rest {
            Some(rest) => {
                let far = (pos[0] - rest[0]).abs() > MOUSE_WAKE || (pos[1] - rest[1]).abs() > MOUSE_WAKE;
                if far {
                    self.mouse_rest = None;
                }
                far
            }
            None => pos != self.mouse_last,
        };
        self.mouse_last = pos;
        if !moved && !clicked {
            return None;
        }
        self.device = Device::Keys;
        Some((pos, clicked))
    }
}

impl ModsScreen {
    /// Hover selects; a click opens a mod or changes a value (its arrows step down or up).
    fn mouse(&mut self, ui: &Ui, layout: &Layout, mods: &[&ModRow], lines: &[Row<'_>]) {
        let Some((pos, clicked)) = self.input.mouse(ui) else { return };
        // The wheel moves the selection of the list under the pointer (which scrolls it).
        let wheel = ui.io().mouse_wheel;
        if wheel != 0.0 {
            let down = wheel < 0.0;
            if pos[0] < layout.split {
                self.focus = Focus::Mods;
                let next = if down { (self.mod_index + 1).min(mods.len().saturating_sub(1)) } else { self.mod_index.saturating_sub(1) };
                if next != self.mod_index {
                    (self.mod_index, self.row) = (next, 0);
                }
            } else {
                self.focus = Focus::Settings;
                self.row = if down { (self.row + 1).min(lines.len().saturating_sub(1)) } else { self.row.saturating_sub(1) };
            }
            return;
        }
        for slot in 0..layout.mods_fit() {
            let i = self.mod_top + slot;
            if i >= mods.len() {
                break;
            }
            if inside(layout.mod_rect(slot), pos) {
                if i != self.mod_index {
                    self.mod_index = i;
                    self.row = 0;
                }
                self.focus = Focus::Mods;
                if clicked {
                    self.focus = Focus::Settings;
                }
                return;
            }
        }
        for slot in 0..layout.rows_fit() {
            let i = self.row_top + slot;
            let Some(line) = lines.get(i) else { break };
            if inside(layout.row_rect(slot), pos) {
                self.focus = Focus::Settings;
                self.row = i;
                if clicked && let Some(m) = mods.get(self.mod_index) {
                    let (left, _) = layout.arrows(slot);
                    let dir = if inside(left, pos) { -1 } else { 1 };
                    self.change(m, line, dir);
                }
                return;
            }
        }
    }

    /// Draws the screen and handles its input; closes itself on Back from the mod list.
    pub fn draw(&mut self, ui: &Ui, snap: &Snapshot, title_font: Option<FontId>, body_font: Option<FontId>) {
        let mods: Vec<&ModRow> = snap.mods.iter().collect();
        if let Some(i) = self.open_at.as_ref().and_then(|id| mods.iter().position(|m| m.id == *id)) {
            (self.mod_index, self.row, self.focus) = (i, 0, Focus::Settings);
            self.open_at = None;
        }
        self.mod_index = self.mod_index.min(mods.len().saturating_sub(1));
        let current = mods.get(self.mod_index).copied();
        let lines = current.map(rows).unwrap_or_default();
        self.row = self.row.min(lines.len().saturating_sub(1));

        let _f = body_font.map(|f| ui.push_font(f));
        let layout = Layout::new(ui);
        let navs = self.input.navs(ui);
        if !navs.is_empty() {
            self.input.rest_mouse(ui);
        }
        for nav in navs {
            match (&self.focus, nav) {
                (Focus::Mods, Nav::Up) => self.mod_index = self.mod_index.saturating_sub(1),
                (Focus::Mods, Nav::Down) if self.mod_index + 1 < mods.len() => self.mod_index += 1,
                (Focus::Mods, Nav::Accept | Nav::Right) if current.is_some() => {
                    self.focus = Focus::Settings;
                    self.row = 0;
                }
                (Focus::Mods, Nav::Back) => self.open = false,
                (Focus::Settings, Nav::Up) => self.row = self.row.saturating_sub(1),
                (Focus::Settings, Nav::Down) if self.row + 1 < lines.len() => self.row += 1,
                (Focus::Settings, Nav::Back) => self.focus = Focus::Mods,
                (Focus::Settings, Nav::Left | Nav::Right | Nav::Accept) => {
                    let dir = if nav == Nav::Left { -1 } else { 1 };
                    if let (Some(m), Some(line)) = (current, lines.get(self.row)) {
                        self.change(m, line, dir);
                    }
                }
                _ => {}
            }
        }
        if !self.open {
            return;
        }
        self.scroll(&layout, mods.len(), lines.len());
        self.mouse(ui, &layout, &mods, &lines);
        self.scroll(&layout, mods.len(), lines.len());
        self.render(ui, &layout, &mods, &lines, title_font);
    }

    /// Keeps the selected mod and row in view.
    fn scroll(&mut self, l: &Layout, mods: usize, rows: usize) {
        fn keep(top: &mut usize, sel: usize, fit: usize, len: usize) {
            if sel < *top {
                *top = sel;
            } else if sel >= *top + fit {
                *top = sel + 1 - fit;
            }
            *top = (*top).min(len.saturating_sub(fit));
        }
        keep(&mut self.mod_top, self.mod_index, l.mods_fit(), mods);
        keep(&mut self.row_top, self.row, l.rows_fit(), rows);
    }

    fn change(&mut self, m: &ModRow, line: &Row<'_>, dir: i32) {
        match line {
            Row::Enabled => {
                if m.disabled_in_toml {
                    return;
                }
                let on = !self.enabled_edits.get(&m.id).copied().unwrap_or(!m.disabled);
                self.enabled_edits.insert(m.id.clone(), on);
                ui_bridge::send(UiRequest::SetEnabled(m.id.clone(), on));
            }
            Row::Setting(s) => {
                let key = (m.id.clone(), s.name.clone());
                let value = self.edits.get(&key).cloned().unwrap_or_else(|| s.value.clone());
                if let Some(v) = step(&s.kind, &value, dir) {
                    self.edits.insert(key, v.clone());
                    ui_bridge::send(UiRequest::SetSetting(m.id.clone(), s.name.clone(), v));
                }
            }
        }
    }

    fn render(&mut self, ui: &Ui, l: &Layout, mods: &[&ModRow], lines: &[Row<'_>], title_font: Option<FontId>) {
        let [w, h] = ui.io().display_size;
        let dl = ui.get_foreground_draw_list();
        dl.add_rect([0.0, 0.0], [w, h], DIM).filled(true).build();
        let ([x0, y0], [x1, y1]) = l.panel;
        dl.add_rect([x0, y0], [x1, y1], PANEL).filled(true).build();
        // Thin gold rules fading at the ends, top and bottom, as on the game's menus.
        for y in [y0, y1] {
            let mid = (x0 + x1) / 2.0;
            dl.add_rect_filled_multicolor([x0, y], [mid, y + 1.5], CLEAR, GOLD, GOLD, CLEAR);
            dl.add_rect_filled_multicolor([mid, y], [x1, y + 1.5], GOLD, CLEAR, CLEAR, GOLD);
        }
        {
            let _f = title_font.map(|f| ui.push_font(f));
            dl.add_text([x0 + l.pad, y0 + l.pad * 0.7], TEXT, "Mods");
        }
        dl.add_line([l.split, l.top], [l.split, y1 - l.line_h * 2.4], [GOLD[0], GOLD[1], GOLD[2], 0.35]).build();

        if mods.is_empty() {
            dl.add_text([x0 + l.pad, l.top], MUTED, "No mods installed.");
        }
        // Mod list (from mod_top), with a marker where more are above or below.
        let fit = l.mods_fit();
        let (list_top, list_bottom) = (l.mod_rect(0).0[1], l.mod_rect(fit - 1).1[1]);
        more_markers(&dl, l, [x0 + l.pad * 0.5, l.split - l.pad * 0.5], [list_top, list_bottom], self.mod_top > 0, self.mod_top + fit < mods.len());
        for (slot, m) in mods.iter().skip(self.mod_top).take(fit).enumerate() {
            let i = self.mod_top + slot;
            let r = l.mod_rect(slot);
            let selected = i == self.mod_index;
            if selected {
                let hl = if self.focus == Focus::Mods { 0.28 } else { 0.12 };
                let c = [GOLD[0], GOLD[1], GOLD[2], hl];
                dl.add_rect_filled_multicolor(r.0, r.1, c, CLEAR, CLEAR, c);
            }
            let on = self.enabled_edits.get(&m.id).copied().unwrap_or(!m.disabled);
            let color = if !on || m.failed {
                MUTED
            } else if selected {
                TEXT
            } else {
                SOFT
            };
            // A long name stops at the column's edge.
            dl.with_clip_rect_intersect([x0, r.0[1]], [l.split - l.pad * 0.3, r.1[1]], || {
                dl.add_text([x0 + l.pad, r.0[1] + 4.0], color, &m.name);
            });
        }

        // The selected mod: name, details, then its rows.
        let rx = l.split + l.pad;
        let mut help = String::new();
        if let Some(m) = mods.get(self.mod_index) {
            let detail = [m.version.as_str(), m.author.as_str()].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join("  \u{00B7}  ");
            dl.add_text([rx, l.top], GOLD, &m.name);
            dl.add_text([rx, l.top + l.line_h * 0.75], MUTED, format!("{detail}   {}", m.status));
            let fit = l.rows_fit();
            let (rows_top, rows_bottom) = (l.row_rect(0).0[1], l.row_rect(fit - 1).1[1]);
            more_markers(&dl, l, [l.split + l.pad * 0.5, x1 - l.pad * 0.5], [rows_top, rows_bottom], self.row_top > 0, self.row_top + fit < lines.len());
            for (slot, line) in lines.iter().skip(self.row_top).take(fit).enumerate() {
                let i = self.row_top + slot;
                let y = l.row_y(slot);
                let selected = self.focus == Focus::Settings && i == self.row;
                let (label, value, frac, desc) = match line {
                    Row::Enabled => {
                        let on = self.enabled_edits.get(&m.id).copied().unwrap_or(!m.disabled);
                        let value = if m.disabled_in_toml { "Off (mod.toml)".to_string() } else { show(&V::Bool(on)) };
                        ("Enabled".to_string(), value, None, "Turn the mod on or off (remembered).".to_string())
                    }
                    Row::Setting(s) => {
                        let key = (m.id.clone(), s.name.clone());
                        if self.edits.get(&key) == Some(&s.value) {
                            self.edits.remove(&key);
                        }
                        let v = self.edits.get(&key).cloned().unwrap_or_else(|| s.value.clone());
                        (s.label.clone(), show(&v), fraction(&s.kind, &v), s.description.clone())
                    }
                };
                let r = l.row_rect(slot);
                if selected {
                    let c = [GOLD[0], GOLD[1], GOLD[2], 0.25];
                    dl.add_rect_filled_multicolor(r.0, r.1, c, CLEAR, CLEAR, c);
                    help = desc;
                }
                dl.add_text([rx, y], if selected { TEXT } else { SOFT }, &label);
                let tw = ui.calc_text_size(&value)[0];
                dl.add_text([l.value_x - tw / 2.0, y], if selected { TEXT } else { GOLD }, &value);
                if selected {
                    // Arrows either side of the value (clickable).
                    let (left, right) = l.arrows(slot);
                    let s = l.line_h * 0.16;
                    let cy = (left.0[1] + left.1[1]) / 2.0;
                    let lx = (left.0[0] + left.1[0]) / 2.0;
                    let rxa = (right.0[0] + right.1[0]) / 2.0;
                    dl.add_triangle([lx + s, cy - s * 1.3], [lx + s, cy + s * 1.3], [lx - s, cy], GOLD).filled(true).build();
                    dl.add_triangle([rxa - s, cy - s * 1.3], [rxa - s, cy + s * 1.3], [rxa + s, cy], GOLD).filled(true).build();
                }
                if let Some(f) = frac {
                    let (bx0, bx1, by) = (l.value_x - l.arrow_dx * 0.7, l.value_x + l.arrow_dx * 0.7, y + l.line_h * 0.62);
                    dl.add_rect([bx0, by], [bx1, by + 2.0], [1.0, 1.0, 1.0, 0.15]).filled(true).build();
                    dl.add_rect([bx0, by], [bx0 + (bx1 - bx0) * f.clamp(0.0, 1.0), by + 2.0], GOLD).filled(true).build();
                }
            }
            if self.focus == Focus::Mods {
                help = if m.settings.is_empty() { "This mod has no settings.".to_string() } else { "Select to change its settings.".to_string() };
            }
        }

        // The help line on its own line, the button guide under it (as on mods' own screens).
        let hy = y1 - l.line_h * 2.25;
        let gy = y1 - l.line_h * 1.2;
        dl.add_line([x0 + l.pad, hy - 6.0], [x1 - l.pad, hy - 6.0], [GOLD[0], GOLD[1], GOLD[2], 0.25]).build();
        dl.add_text([x0 + l.pad, hy], MUTED, &help);
        let size = ui.text_line_height() * 0.95;
        let pad_ui = self.input.device == Device::Pad;
        let items: Vec<(Glyph, &str)> = match (&self.focus, pad_ui) {
            (Focus::Mods, true) => vec![(Glyph::Dpad, "Select"), (Glyph::PadA, "OK"), (Glyph::PadB, "Close")],
            (Focus::Mods, false) => {
                vec![(Glyph::KeyArrows(Arrows::UpDown), "Select"), (Glyph::Key("Enter"), "OK"), (Glyph::Key("Esc"), "Close")]
            }
            (Focus::Settings, true) => vec![(Glyph::Dpad, "Change"), (Glyph::PadB, "Back")],
            (Focus::Settings, false) => {
                vec![(Glyph::KeyArrows(Arrows::UpDown), "Select"), (Glyph::KeyArrows(Arrows::LeftRight), "Change"), (Glyph::Key("Esc"), "Back")]
            }
        };
        guide(ui, &dl, &items, x1 - l.pad, gy, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_stay_in_range() {
        let k = K::Int { min: 0, max: 100 };
        assert_eq!(step(&k, &V::Int(0), -1), None);
        assert_eq!(step(&k, &V::Int(0), 1), Some(V::Int(5)));
        assert_eq!(step(&k, &V::Int(98), 1), Some(V::Int(100)));
        let c = K::Choice(vec!["a".into(), "b".into()]);
        assert_eq!(step(&c, &V::Choice("b".into()), 1), Some(V::Choice("a".into())));
        assert_eq!(step(&K::Bool, &V::Bool(true), -1), Some(V::Bool(false)));
        let f = K::Float { min: 0.0, max: 1.0 };
        assert_eq!(step(&f, &V::Float(0.5), 1), Some(V::Float(0.55)));
    }

    #[test]
    fn values_read_well() {
        assert_eq!(show(&V::Float(0.5)), "0.5");
        assert_eq!(show(&V::Float(2.0)), "2");
        assert_eq!(show(&V::Bool(true)), "On");
    }

    #[test]
    fn hit_testing() {
        let r: Rect = ([10.0, 10.0], [20.0, 20.0]);
        assert!(inside(r, [15.0, 15.0]));
        assert!(!inside(r, [25.0, 15.0]));
    }
}
