//! Mods' own screens (kindle.ui.screen, kindle_core::screens) drawn in the game's menu style, as
//! the Mods screen is: a panel with the screen's title, its rows (buttons, switches, choices,
//! sliders, text the player types, headings and lines of text), the selected row's help and the
//! button guide. Worked
//! with the controller, the keyboard or the mouse (mods_screen::MenuInput); what the player does
//! goes back to the mod through kindle_core::screens::report, on the render thread like the
//! rest of the overlay.

use std::collections::HashMap;

use hudhook::imgui::{FontId, Key, Ui};
use kindle_core::screens::{self, Row, RowKind, Screen, ScreenEvent};

use crate::mods_screen::{Arrows, CLEAR, DIM, Device, GOLD, Glyph, MUTED, MenuInput, Nav, PANEL, Rect, SOFT, TEXT, guide, inside};

/// How long a key row waits for a key before it gives up (a controller has no Esc).
const BIND_SECONDS: f64 = 6.0;

#[derive(Default)]
pub struct ModScreens {
    input: MenuInput,
    /// The selected row and the first row shown, per screen (a sub-screen's parent keeps its own),
    /// counted in the tab shown.
    rows: HashMap<u64, (usize, usize)>,
    /// The tab shown per screen, and the tab the mod last asked for (an update asking for the
    /// same one again leaves the player's choice alone).
    tabs: HashMap<u64, (usize, Option<usize>)>,
    /// An input row being typed into: its screen, its row and the text so far.
    editing: Option<(u64, usize, String)>,
    /// A key row waiting for a key: its screen, its row, what was held when it started (not
    /// taken until let go) and when it started (it gives up after BIND_SECONDS).
    binding: Option<(u64, usize, Vec<&'static str>, f64)>,
    /// The key just bound, and whether it has been let go: the screen takes no input until a
    /// frame after it is (the overlay sees a press a frame after the game's read does, so a
    /// right click bound was also Back, and Enter bound picked the row again).
    after_bind: Option<(&'static str, bool)>,
}

/// Where everything goes this frame (with the body font pushed).
struct Layout {
    panel: Rect,
    pad: f32,
    line_h: f32,
    /// The first row's y, and how many rows fit.
    top: f32,
    visible: usize,
    value_x: f32,
    arrow_dx: f32,
    /// The tab bar's text line (a screen with tabs).
    tab_y: Option<f32>,
    /// The subtitle's line.
    subtitle_y: f32,
    /// The list's right edge (the details pane, if any, is right of it).
    list_x1: f32,
}

impl Layout {
    fn new(ui: &Ui, rows: usize, subtitle: bool, tabs: bool, details: bool) -> Self {
        let [w, h] = ui.io().display_size;
        let line_h = ui.text_line_height() * 1.45;
        // A details pane makes the panel wider; the list keeps the left part.
        let pw = if details { (w * 0.8).min(1700.0) } else { (w * 0.56).min(1200.0) };
        let lw = if details { pw * 0.58 } else { pw };
        let tabs_h = if tabs { line_h * 1.5 } else { 0.0 };
        let head = line_h * if subtitle { 2.6 } else { 1.9 } + tabs_h;
        let foot = line_h * 2.6;
        let max_h = (h * 0.86).min(950.0);
        let want = head + foot + line_h * (rows.max(1) as f32 + 0.6);
        // A details pane needs room for its picture and text, however few the rows.
        let ph = want.clamp((line_h * if details { 15.0 } else { 8.0 }).min(max_h), max_h);
        let (x0, y0) = ((w - pw) / 2.0, (h - ph) / 2.0);
        let pad = (lw * 0.04).max(20.0);
        let top = y0 + pad * 0.5 + head;
        let visible = (((y0 + ph - foot) - top) / line_h).floor().max(1.0) as usize;
        Self {
            panel: ([x0, y0], [x0 + pw, y0 + ph]),
            pad,
            line_h,
            top,
            visible,
            value_x: x0 + lw - pad - lw * 0.14,
            arrow_dx: lw * 0.12,
            list_x1: x0 + lw,
            tab_y: tabs.then_some(top - tabs_h + line_h * 0.1),
            subtitle_y: top - tabs_h - line_h * 0.95,
        }
    }

    /// Each tab's title: where it's drawn and the box a click picks it in.
    fn tab_boxes(&self, ui: &Ui, screen: &Screen) -> Vec<([f32; 2], Rect)> {
        let Some(y) = self.tab_y else { return Vec::new() };
        let gap = self.line_h * 0.9;
        let mut x = self.panel.0[0] + self.pad;
        let mut out = Vec::new();
        for tab in &screen.tabs {
            let tw = ui.calc_text_size(&tab.title)[0];
            out.push(([x, y], ([x - gap * 0.4, y - 4.0], [x + tw + gap * 0.4, y + self.line_h - 4.0])));
            x += tw + gap;
        }
        out
    }

    fn row_y(&self, slot: usize) -> f32 {
        self.top + slot as f32 * self.line_h
    }

    fn row_rect(&self, slot: usize) -> Rect {
        let y = self.row_y(slot);
        ([self.panel.0[0] + self.pad * 0.5, y - 4.0], [self.list_x1 - self.pad * 0.5, y + self.line_h - 8.0])
    }

    fn arrows(&self, slot: usize) -> (Rect, Rect) {
        let y = self.row_y(slot);
        let s = self.line_h * 0.6;
        let (left, right) = (self.value_x - self.arrow_dx, self.value_x + self.arrow_dx);
        (([left - s, y - 4.0], [left + s, y + self.line_h - 8.0]), ([right - s, y - 4.0], [right + s, y + self.line_h - 8.0]))
    }
}

/// A row's value as shown, and how far along its range it is (sliders).
fn value_of(kind: &RowKind) -> (Option<String>, Option<f32>) {
    match kind {
        RowKind::Button { value } => (value.clone(), None),
        RowKind::Toggle(on) => (Some(if *on { "On" } else { "Off" }.into()), None),
        RowKind::Choice { choices, index } => (choices.get(*index).cloned(), None),
        RowKind::Slider { min, max, step, value } => (Some(screens::show_number(*value, *step)), Some(((value - min) / (max - min)) as f32)),
        RowKind::Info { value } => (value.clone(), None),
        RowKind::Key { value } => (Some(key_label(value)), None),
        RowKind::Header | RowKind::Input { .. } => (None, None),
    }
}

/// A key's name as shown: "f6" F6, "pad_y" Pad Y, "mouse_middle" Mouse Middle, "" none.
fn key_label(name: &str) -> String {
    if name.is_empty() {
        return "None".into();
    }
    name.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The row's value moved `dir` (-1, 1; Accept is 1): the new kind and the value reported.
fn change(kind: &RowKind, dir: i32) -> Option<(RowKind, f64)> {
    match kind {
        RowKind::Toggle(on) => Some((RowKind::Toggle(!on), if *on { 0.0 } else { 1.0 })),
        RowKind::Choice { choices, index } => {
            let n = choices.len() as i32;
            let i = (*index as i32 + dir).rem_euclid(n) as usize;
            Some((RowKind::Choice { choices: choices.clone(), index: i }, i as f64))
        }
        RowKind::Slider { min, max, step, value } => {
            let v = screens::slide(*min, *max, *step, *value, dir);
            (v != *value).then_some((RowKind::Slider { min: *min, max: *max, step: *step, value: v }, v))
        }
        _ => None,
    }
}

/// `text` broken into lines no wider than `width` (at spaces; a word wider than that stands
/// alone), keeping its own line breaks.
fn wrap(ui: &Ui, text: &str, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && ui.calc_text_size(&candidate)[0] > width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// Whether left and right change the row (a button only picks).
fn steps(kind: &RowKind) -> bool {
    matches!(kind, RowKind::Toggle(_) | RowKind::Choice { .. } | RowKind::Slider { .. })
}

impl ModScreens {
    /// Draws the top mod screen, if any, and handles its input. Returns whether one is open.
    pub fn draw(&mut self, ui: &Ui, title_font: Option<FontId>, body_font: Option<FontId>) -> bool {
        let Some(screen) = screens::top() else {
            self.rows.clear();
            return false;
        };
        // The game's own System screen shows it (native_mods.rs), or will.
        if !crate::native_mods::drawn_by_kindle(screen.id) {
            return false;
        }
        self.rows.retain(|id, _| screens::is_open(*id));
        self.tabs.retain(|id, _| screens::is_open(*id));
        let tab_count = screen.tabs.len().max(1);
        let tab = {
            let (tab, asked) = self.tabs.entry(screen.id).or_insert((screen.tab.unwrap_or(0), screen.tab));
            if *asked != screen.tab {
                // The mod asked for another tab (an update).
                if let Some(t) = screen.tab {
                    *tab = t;
                    self.rows.remove(&screen.id);
                }
                *asked = screen.tab;
            }
            *tab = (*tab).min(tab_count - 1);
            *tab
        };
        let base = screen.tab_rows(tab).start;
        let rows = &screen.rows[screen.tab_rows(tab)];
        let first = rows.iter().position(Row::selectable);
        // Just opened: its first choice is brought into view.
        let mut moved = !self.rows.contains_key(&screen.id);
        let (mut sel, mut scroll) = *self.rows.entry(screen.id).or_insert((first.unwrap_or(0), 0));
        if !rows.get(sel).is_some_and(Row::selectable) {
            sel = first.unwrap_or(0);
        }

        let _f = body_font.map(|f| ui.push_font(f));
        // As tall as the longest tab, so turning tabs doesn't move the panel.
        let most_rows = (0..tab_count).map(|t| screen.tab_rows(t).len()).max().unwrap_or(0);
        let layout = Layout::new(ui, most_rows, !screen.subtitle.is_empty(), !screen.tabs.is_empty(), screen.has_details());
        let all = &screen.rows;
        if self.editing.as_ref().is_some_and(|(id, row, _)| *id != screen.id || !matches!(all.get(*row).map(|r| &r.kind), Some(RowKind::Input { .. }))) {
            self.editing = None;
        }
        if self.binding.as_ref().is_some_and(|(id, row, ..)| *id != screen.id || !matches!(all.get(*row).map(|r| &r.kind), Some(RowKind::Key { .. }))) {
            self.binding = None;
        }
        if let Some((name, released)) = self.after_bind.as_mut() {
            let held = crate::input_block::input_named(name).is_some_and(crate::input_block::input_down);
            if !held {
                if *released {
                    self.after_bind = None;
                } else {
                    *released = true;
                }
            }
        }
        if self.editing.is_some() || self.binding.is_some() || self.after_bind.is_some() {
            if self.editing.is_some() {
                self.type_text(ui, all);
            } else if self.binding.is_some() {
                self.catch_key(ui);
            } else {
                // Just bound: what the overlay reads of that press is dropped.
                let _ = self.input.navs(ui);
            }
            self.rows.insert(screen.id, (sel, scroll));
            let screen = screens::top().unwrap_or(screen);
            let rows = &screen.rows[screen.tab_rows(tab)];
            self.render(ui, &layout, &screen, rows, base, tab, sel, scroll, title_font);
            return true;
        }
        let navs = self.input.navs(ui);
        if !navs.is_empty() {
            self.input.rest_mouse(ui);
        }
        let move_sel = |sel: usize, dir: i32| -> usize {
            let mut i = sel as i32;
            loop {
                i += dir;
                if i < 0 || i as usize >= rows.len() {
                    return sel;
                }
                if rows[i as usize].selectable() {
                    return i as usize;
                }
            }
        };
        let max_scroll = rows.len().saturating_sub(layout.visible);
        for nav in navs {
            match nav {
                Nav::Up | Nav::Down => {
                    let dir = if nav == Nav::Up { -1 } else { 1 };
                    let next = move_sel(sel, dir);
                    if next != sel {
                        sel = next;
                        moved = true;
                    } else if dir < 0 {
                        // Nothing more to pick that way: the lines of text there come into view.
                        scroll = scroll.saturating_sub(1);
                    } else {
                        scroll = (scroll + 1).min(max_scroll);
                    }
                }
                Nav::Left | Nav::Right | Nav::Accept => self.act(ui, &screen, rows, base, sel, nav),
                Nav::PrevTab | Nav::NextTab if tab_count > 1 => {
                    let dir = if nav == Nav::PrevTab { tab_count - 1 } else { 1 };
                    self.turn_tab(&screen, (tab + dir) % tab_count);
                    return true;
                }
                Nav::PrevTab | Nav::NextTab => {}
                Nav::Back if screen.closable => {
                    screens::report(screen.id, ScreenEvent::Closed);
                    return true;
                }
                Nav::Back => {}
            }
        }
        // The mouse wheel scrolls the list (hover then picks from what's shown).
        let wheel = ui.io().mouse_wheel;
        if wheel != 0.0 {
            scroll = (scroll as i64 - wheel.signum() as i64).clamp(0, max_scroll as i64) as usize;
        }
        // A new selection is kept in view, with its section's heading when there's room.
        if moved {
            if sel < scroll {
                scroll = sel.saturating_sub(1);
            } else if sel >= scroll + layout.visible {
                scroll = sel + 1 - layout.visible;
            }
            if scroll > 0 && rows.get(scroll - 1).is_some_and(|r| matches!(r.kind, RowKind::Header)) && sel < scroll + layout.visible - 1 {
                scroll -= 1;
            }
        }
        let scroll = scroll.min(max_scroll);
        if let Some((pos, clicked)) = self.input.mouse(ui) {
            if clicked
                && let Some(t) = layout.tab_boxes(ui, &screen).iter().position(|(_, b)| inside(*b, pos))
                && t != tab
            {
                self.turn_tab(&screen, t);
                return true;
            }
            for slot in 0..layout.visible {
                let i = scroll + slot;
                let Some(r) = rows.get(i) else { break };
                if r.selectable() && inside(layout.row_rect(slot), pos) {
                    sel = i;
                    if clicked {
                        let (left, _) = layout.arrows(slot);
                        let nav = if !steps(&r.kind) {
                            Nav::Accept
                        } else if inside(left, pos) {
                            Nav::Left
                        } else {
                            Nav::Right
                        };
                        self.act(ui, &screen, rows, base, sel, nav);
                    }
                    break;
                }
            }
        }
        self.rows.insert(screen.id, (sel, scroll));
        // What act() changed shows this frame.
        let screen = screens::top().unwrap_or(screen);
        let rows = &screen.rows[screen.tab_rows(tab)];
        self.render(ui, &layout, &screen, rows, base, tab, sel, scroll, title_font);
        true
    }

    /// The player turned to tab `to`: its list starts at the top, and the mod hears of it.
    fn turn_tab(&mut self, screen: &Screen, to: usize) {
        if let Some((tab, _)) = self.tabs.get_mut(&screen.id) {
            *tab = to;
        }
        self.rows.remove(&screen.id);
        screens::report(screen.id, ScreenEvent::Tab(to));
    }

    /// Typing into an input row: characters, Backspace, Enter keeps the text, Esc leaves it as it was.
    fn type_text(&mut self, ui: &Ui, rows: &[Row]) {
        let Some((id, row, text)) = self.editing.as_mut() else { return };
        let max = match rows.get(*row).map(|r| &r.kind) {
            Some(RowKind::Input { max, .. }) => *max,
            _ => 0,
        };
        for c in ui.io().input_queue_characters() {
            if !c.is_control() && text.chars().count() < max {
                text.push(c);
            }
        }
        if ui.is_key_pressed(Key::Backspace) {
            text.pop();
        }
        if ui.is_key_pressed_no_repeat(Key::Escape) {
            self.editing = None;
        } else if ui.is_key_pressed_no_repeat(Key::Enter) || ui.is_key_pressed_no_repeat(Key::KeypadEnter) {
            let (id, row, text) = (*id, *row, std::mem::take(text));
            self.editing = None;
            screens::set_text(id, row, text);
        }
    }

    /// Waiting on a key row: the first key or button pressed (not one already held when it
    /// started) is kept; Esc, or nothing for BIND_SECONDS, leaves it as it was.
    fn catch_key(&mut self, ui: &Ui) {
        let Some((id, row, held, since)) = self.binding.as_mut() else { return };
        let down = crate::input_block::inputs_down();
        // Let go: it counts if pressed again.
        held.retain(|name| down.contains(name));
        if ui.is_key_pressed_no_repeat(Key::Escape) || ui.time() - *since > BIND_SECONDS {
            self.binding = None;
            return;
        }
        if let Some(&name) = down.iter().find(|&&name| name != "esc" && !held.contains(&name)) {
            let (id, row) = (*id, *row);
            self.binding = None;
            self.after_bind = Some((name, false));
            screens::set_text(id, row, name.to_string());
        }
    }

    /// `rows` are the tab's, which start at the screen's row `base`.
    fn act(&mut self, ui: &Ui, screen: &Screen, rows: &[Row], base: usize, sel: usize, nav: Nav) {
        let Some(r) = rows.get(sel) else { return };
        if r.disabled || !r.selectable() {
            return;
        }
        let row = base + sel;
        match (&r.kind, nav) {
            (RowKind::Button { .. }, Nav::Accept) => screens::report(screen.id, ScreenEvent::Selected(row)),
            (RowKind::Input { value, .. }, Nav::Accept) => self.editing = Some((screen.id, row, value.clone())),
            (RowKind::Key { .. }, Nav::Accept) => self.binding = Some((screen.id, row, crate::input_block::inputs_down(), ui.time())),
            (kind, _) if steps(kind) => {
                let dir = if nav == Nav::Left { -1 } else { 1 };
                if let Some((new, value)) = change(kind, dir) {
                    screens::change(screen.id, row, new, value);
                }
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render(&self, ui: &Ui, l: &Layout, screen: &Screen, rows: &[Row], base: usize, tab: usize, sel: usize, scroll: usize, title_font: Option<FontId>) {
        let [w, h] = ui.io().display_size;
        let dl = ui.get_foreground_draw_list();
        dl.add_rect([0.0, 0.0], [w, h], DIM).filled(true).build();
        let ([x0, y0], [x1, y1]) = l.panel;
        dl.add_rect([x0, y0], [x1, y1], PANEL).filled(true).build();
        for y in [y0, y1] {
            let mid = (x0 + x1) / 2.0;
            dl.add_rect_filled_multicolor([x0, y], [mid, y + 1.5], CLEAR, GOLD, GOLD, CLEAR);
            dl.add_rect_filled_multicolor([mid, y], [x1, y + 1.5], GOLD, CLEAR, CLEAR, GOLD);
        }
        {
            let _f = title_font.map(|f| ui.push_font(f));
            dl.add_text([x0 + l.pad, y0 + l.pad * 0.7], TEXT, &screen.title);
        }
        if !screen.subtitle.is_empty() {
            dl.add_text([x0 + l.pad, l.subtitle_y], MUTED, &screen.subtitle);
        }
        // The tabs: the one shown bright and underlined, a line under them all.
        if let Some(ty) = l.tab_y {
            let boxes = l.tab_boxes(ui, screen);
            let ly = ty + l.line_h - 2.0;
            dl.add_line([x0 + l.pad, ly], [x1 - l.pad, ly], [GOLD[0], GOLD[1], GOLD[2], 0.25]).build();
            dl.with_clip_rect_intersect([x0 + l.pad * 0.5, ty - l.line_h], [x1 - l.pad * 0.5, ly + 4.0], || {
                for (i, (t, (at, b))) in screen.tabs.iter().zip(&boxes).enumerate() {
                    let shown = i == tab;
                    dl.add_text(*at, if shown { TEXT } else { MUTED }, &t.title);
                    if shown {
                        dl.add_rect([b.0[0], ly - 1.0], [b.1[0], ly + 2.0], GOLD).filled(true).build();
                    }
                }
            });
        }
        let rx = x0 + l.pad;
        let mut help = String::new();
        let mut details = None;
        for slot in 0..l.visible {
            let i = scroll + slot;
            let Some(r) = rows.get(i) else { break };
            let y = l.row_y(slot);
            let selected = i == sel && r.selectable();
            if selected {
                let rr = l.row_rect(slot);
                let c = [GOLD[0], GOLD[1], GOLD[2], 0.25];
                dl.add_rect_filled_multicolor(rr.0, rr.1, c, CLEAR, CLEAR, c);
                help = r.help.clone();
                details = r.details.as_ref();
            }
            match r.kind {
                RowKind::Header => {
                    dl.add_text([rx, y + l.line_h * 0.15], GOLD, &r.text);
                    let ly = y + l.line_h * 0.85;
                    dl.add_line([rx, ly], [l.list_x1 - l.pad, ly], [GOLD[0], GOLD[1], GOLD[2], 0.3]).build();
                    continue;
                }
                RowKind::Info { value: None } => {
                    dl.add_text([rx, y], MUTED, &r.text);
                    continue;
                }
                _ => {}
            }
            let text_color = if r.disabled {
                MUTED
            } else if selected {
                TEXT
            } else {
                SOFT
            };
            // An icon at the text's height, its shape kept; the text after it.
            let mut tx = rx;
            if let Some(icon) = &r.icon
                && let Some(tex) = crate::hud_draw::texture(icon.id)
            {
                let ih = l.line_h * 0.8;
                let iw = ih * icon.width as f32 / icon.height as f32;
                let tint = if r.disabled { [1.0, 1.0, 1.0, 0.45] } else { [1.0; 4] };
                dl.add_image(tex, [tx, y - l.line_h * 0.12], [tx + iw, y - l.line_h * 0.12 + ih]).col(tint).build();
                tx += iw + l.line_h * 0.3;
            }
            dl.add_text([tx, y], text_color, &r.text);
            let (mut value, frac) = value_of(&r.kind);
            if self.binding.as_ref().is_some_and(|(id, row, ..)| *id == screen.id && *row == base + i) {
                // Waiting for the key: blinks.
                value = Some(if ui.time().fract() < 0.6 { "Press a key".into() } else { String::new() });
            }
            if let Some(v) = &value {
                let tw = ui.calc_text_size(v)[0];
                let color = if r.disabled {
                    MUTED
                } else if selected {
                    TEXT
                } else {
                    GOLD
                };
                dl.add_text([l.value_x - tw / 2.0, y], color, v);
            }
            if selected && steps(&r.kind) && !r.disabled {
                let (left, right) = l.arrows(slot);
                let s = l.line_h * 0.16;
                let cy = (left.0[1] + left.1[1]) / 2.0;
                let lx = (left.0[0] + left.1[0]) / 2.0;
                let rxa = (right.0[0] + right.1[0]) / 2.0;
                dl.add_triangle([lx + s, cy - s * 1.3], [lx + s, cy + s * 1.3], [lx - s, cy], GOLD).filled(true).build();
                dl.add_triangle([rxa - s, cy - s * 1.3], [rxa - s, cy + s * 1.3], [rxa + s, cy], GOLD).filled(true).build();
            }
            if let RowKind::Input { value, .. } = &r.kind {
                // A field on the right: the text (being typed, with a caret), clipped to it.
                let typing = self.editing.as_ref().filter(|(id, row, _)| *id == screen.id && *row == base + i).map(|(_, _, t)| t);
                let (fx0, fx1) = (l.value_x - l.arrow_dx * 1.15, l.value_x + l.arrow_dx * 1.15);
                let (fy0, fy1) = (y - 3.0, y + l.line_h - 9.0);
                let edge = if typing.is_some() { GOLD } else { [GOLD[0], GOLD[1], GOLD[2], 0.35] };
                dl.add_rect([fx0, fy0], [fx1, fy1], [0.0, 0.0, 0.0, 0.35]).filled(true).build();
                dl.add_rect([fx0, fy0], [fx1, fy1], edge).build();
                let shown = typing.unwrap_or(value);
                let tw = ui.calc_text_size(shown)[0];
                // The end of a long text stays in view while it's typed.
                let tx = if typing.is_some() { (fx0 + 8.0).min(fx1 - 12.0 - tw) } else { fx0 + 8.0 };
                let color = if r.disabled { MUTED } else { TEXT };
                dl.with_clip_rect_intersect([fx0 + 2.0, fy0], [fx1 - 2.0, fy1], || {
                    dl.add_text([tx, y], color, shown);
                    if typing.is_some() && ui.time().fract() < 0.55 {
                        let cx = tx + tw + 2.0;
                        dl.add_line([cx, fy0 + 4.0], [cx, fy1 - 4.0], TEXT).build();
                    }
                });
            }
            if let Some(f) = frac {
                let (bx0, bx1, by) = (l.value_x - l.arrow_dx * 0.7, l.value_x + l.arrow_dx * 0.7, y + l.line_h * 0.62);
                dl.add_rect([bx0, by], [bx1, by + 2.0], [1.0, 1.0, 1.0, 0.15]).filled(true).build();
                dl.add_rect([bx0, by], [bx0 + (bx1 - bx0) * f.clamp(0.0, 1.0), by + 2.0], GOLD).filled(true).build();
            }
        }
        // More rows above or below: small gold marks at the list's edge.
        let mark_x = l.list_x1 - l.pad * 0.6;
        let s = l.line_h * 0.14;
        if scroll > 0 {
            let y = l.row_y(0);
            dl.add_triangle([mark_x - s, y + s * 1.6], [mark_x + s, y + s * 1.6], [mark_x, y], GOLD).filled(true).build();
        }
        if scroll + l.visible < rows.len() {
            let y = l.row_y(l.visible) - 10.0;
            dl.add_triangle([mark_x - s, y - s * 1.6], [mark_x + s, y - s * 1.6], [mark_x, y], GOLD).filled(true).build();
        }

        // The details pane: the selected row's picture, title and text, right of the list.
        if screen.has_details() {
            let (px0, px1) = (l.list_x1 + l.pad * 0.5, x1 - l.pad);
            let (py0, py1) = (l.top, y1 - l.line_h * 2.6);
            dl.add_line([l.list_x1, py0 - 4.0], [l.list_x1, py1], [GOLD[0], GOLD[1], GOLD[2], 0.25]).build();
            if let Some(d) = details {
                let mut y = py0;
                if let Some(image) = &d.image
                    && let Some(tex) = crate::hud_draw::texture(image.id)
                {
                    // As big as fits in the pane's width and half its height, its shape kept.
                    let (mw, mh) = (px1 - px0, (py1 - py0) * 0.5);
                    let scale = (mw / image.width as f32).min(mh / image.height as f32);
                    let (iw, ih) = (image.width as f32 * scale, image.height as f32 * scale);
                    let ix = px0 + (mw - iw) / 2.0;
                    dl.add_image(tex, [ix, y], [ix + iw, y + ih]).build();
                    y += ih + l.line_h * 0.4;
                }
                if !d.title.is_empty() {
                    dl.add_text([px0, y], GOLD, &d.title);
                    y += l.line_h * 1.1;
                }
                dl.with_clip_rect_intersect([px0, py0], [px1, py1], || {
                    for line in wrap(ui, &d.text, px1 - px0) {
                        if y > py1 {
                            break;
                        }
                        dl.add_text([px0, y], SOFT, &line);
                        y += ui.text_line_height() * 1.15;
                    }
                });
            }
        }

        // The selected row's help on its own line, the button guide under it.
        let hy = y1 - l.line_h * 2.25;
        let gy = y1 - l.line_h * 1.2;
        dl.add_line([x0 + l.pad, hy - 6.0], [x1 - l.pad, hy - 6.0], [GOLD[0], GOLD[1], GOLD[2], 0.25]).build();
        dl.add_text([x0 + l.pad, hy], MUTED, &help);
        let size = ui.text_line_height() * 0.95;
        let stepping = rows.get(sel).is_some_and(|r| steps(&r.kind));
        // A screen of only text and headings has nothing to pick: Back alone.
        let picks = rows.iter().any(Row::selectable);
        let mut items: Vec<(Glyph, &str)> = Vec::new();
        if self.editing.is_some() {
            items.push((Glyph::Key("Enter"), "OK"));
            items.push((Glyph::Key("Esc"), "Cancel"));
        } else if self.binding.is_some() {
            items.push((Glyph::Key("Esc"), "Cancel"));
        } else if self.input.device == Device::Pad {
            if picks {
                items.push((Glyph::Dpad, if stepping { "Change" } else { "Select" }));
            }
            if picks && !stepping {
                items.push((Glyph::PadA, "OK"));
            }
            if screen.tabs.len() > 1 {
                items.push((Glyph::Key("LB / RB"), "Tab"));
            }
            if screen.closable {
                items.push((Glyph::PadB, "Back"));
            }
        } else {
            if picks {
                items.push((Glyph::KeyArrows(Arrows::UpDown), "Select"));
            }
            if stepping {
                items.push((Glyph::KeyArrows(Arrows::LeftRight), "Change"));
            } else if picks {
                items.push((Glyph::Key("Enter"), "OK"));
            }
            if screen.tabs.len() > 1 {
                items.push((Glyph::Key("Q / E"), "Tab"));
            }
            if screen.closable {
                items.push((Glyph::Key("Esc"), "Back"));
            }
        }
        guide(ui, &dl, &items, x1 - l.pad, gy, size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_wrap_and_stop() {
        let choice = RowKind::Choice { choices: vec!["a".into(), "b".into(), "c".into()], index: 0 };
        assert_eq!(change(&choice, -1).unwrap().1, 2.0);
        assert_eq!(change(&RowKind::Toggle(false), 1).unwrap(), (RowKind::Toggle(true), 1.0));
        let slider = RowKind::Slider { min: 0.0, max: 1.0, step: 0.5, value: 1.0 };
        assert!(change(&slider, 1).is_none());
        assert_eq!(change(&slider, -1).unwrap().1, 0.5);
        assert!(change(&RowKind::Button { value: None }, 1).is_none());
        assert_eq!(value_of(&RowKind::Slider { min: 0.0, max: 2.0, step: 0.5, value: 1.0 }), (Some("1.0".into()), Some(0.5)));
    }
}
