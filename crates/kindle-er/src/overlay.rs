//! In-game overlay (Dear ImGui via hudhook, DirectX 12), as Kindle for Dark Souls III's (its
//! overlay.rs, whose tabs and notices these are): mods' banners, toasts, prompts and status lines,
//! and the developer overlay (console, mods and their settings, player, nearby characters, flags,
//! params, log).
//!
//! Runs on the game's render thread. It never touches game memory or Lua: it draws the latest
//! [`Snapshot`](kindle_core::ui_bridge::Snapshot) and sends requests through
//! [`ui_bridge`](kindle_core::ui_bridge), which the runtime executes on the game's thread (frame.rs).
//! While it's open the game's input is kept from it (input_block.rs). Not yet: Dark Souls III's Mods
//! screen.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use hudhook::imgui::{self, Condition, Context, FontConfig, FontGlyphRanges, FontId, FontSource, Key, ProgressBar, TableFlags, Ui};
use hudhook::{ImguiRenderLoop, MessageFilter, RenderContext};

use kindle_core::console::ConsoleRequest;
use kindle_core::ui_bridge::{self, FindHit, NoticeKind, ParamField, Snapshot, UiRequest};

use crate::input_block;

const RED: [f32; 4] = [1.0, 0.45, 0.45, 1.0];
const YELLOW: [f32; 4] = [1.0, 0.85, 0.4, 1.0];
const GREEN: [f32; 4] = [0.55, 0.9, 0.55, 1.0];
const DIM: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const ACCENT: [f32; 4] = [1.0, 0.65, 0.3, 1.0];

pub fn parse_key(name: &str) -> Option<Key> {
    Some(match name.to_ascii_lowercase().as_str() {
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "insert" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "delete" => Key::Delete,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "backquote" | "`" | "graveaccent" => Key::GraveAccent,
        _ => return None,
    })
}

enum Line {
    Input(String),
    Output(String),
    Error(String),
}

/// Mutable UI state. Receivers aren't `Sync`, so the whole thing sits behind a Mutex to
/// satisfy hudhook's `Send + Sync` bound (only the render thread ever locks it).
#[derive(Default)]
struct State {
    // console
    lines: Vec<Line>,
    input: String,
    history: Vec<String>,
    focus_input: bool,
    scroll_to_bottom: bool,
    pending: Vec<Receiver<String>>,
    // flags
    flag_input: String,
    watched: Vec<u32>,
    // params
    table_idx: usize,
    row: i32,
    field_filter: String,
    fields: Option<Result<Vec<ParamField>, String>>,
    edits: HashMap<String, String>,
    row_pending: Option<Receiver<Result<Vec<ParamField>, String>>>,
    set_pending: Vec<(String, Receiver<Result<(), String>>)>,
    param_status: Option<(bool, String)>,
    // mod settings changed here but not yet in the snapshot: (mod id, setting) -> value
    setting_edits: HashMap<(String, String), kindle_core::config::SettingValue>,
    loaded_key: Option<(String, u64)>,
    // log
    log_autoscroll: bool,
    // nearby
    effect_input: String,
    // find
    find_query: String,
    /// The query the shown results are for, and the one waiting for results.
    find_shown: String,
    find_pending: Option<(String, Receiver<Vec<FindHit>>)>,
    find_hits: Vec<FindHit>,
}

/// FontId wraps a raw pointer into the font atlas, which lives as long as the ImGui
/// context; it's only ever used on the render thread.
#[derive(Clone, Copy)]
struct Font(FontId);
unsafe impl Send for Font {}
unsafe impl Sync for Font {}

/// Basic Latin + Latin-1/Extended + general punctuation (curly quotes, ellipsis, dashes).
static GLYPHS: [u32; 5] = [0x0020, 0x024F, 0x2000, 0x206F, 0];

fn load_font(names: &[&str]) -> Option<&'static [u8]> {
    let dir = std::env::var("WINDIR").map(|w| PathBuf::from(w).join("Fonts")).ok()?;
    names.iter().find_map(|n| std::fs::read(dir.join(n)).ok()).map(|d| &*Box::leak(d.into_boxed_slice()))
}

fn font_source(data: &'static [u8], px: f32) -> FontSource<'static> {
    FontSource::TtfData {
        data,
        size_pixels: px,
        config: Some(FontConfig { glyph_ranges: FontGlyphRanges::from_slice(&GLYPHS), oversample_h: 2, ..FontConfig::default() }),
    }
}

pub struct Overlay {
    banner_font: Option<Font>,
    toggle: Key,
    toggle_name: String,
    font_scale: f32,
    ini_path: PathBuf,
    visible: std::sync::atomic::AtomicBool,
    state: Mutex<State>,
}

impl Overlay {
    pub fn new(toggle: Key, toggle_name: String, font_scale: f32, ini_path: PathBuf) -> Self {
        Self {
            banner_font: None,
            toggle,
            toggle_name,
            font_scale,
            ini_path,
            visible: std::sync::atomic::AtomicBool::new(false),
            state: Mutex::new(State { log_autoscroll: true, ..Default::default() }),
        }
    }

    fn set_visible(&self, v: bool) {
        self.visible.store(v, Ordering::Relaxed);
        self.sync_input();
    }

    /// The game's input is blocked (and snapshots published) while the overlay is open.
    fn sync_input(&self) {
        let open = self.visible.load(Ordering::Relaxed);
        ui_bridge::OVERLAY_VISIBLE.store(open, Ordering::Relaxed);
        input_block::set_blocked(open);
    }
}

impl ImguiRenderLoop for Overlay {
    fn initialize<'a>(&'a mut self, ctx: &mut Context, _rc: &'a mut dyn RenderContext) {
        ctx.set_ini_filename(Some(self.ini_path.clone()));
        // Proper fonts instead of ImGui's pixel font: Segoe UI for the overlay, a serif
        // (Garamond, else Georgia/Times) for banners. Falls back to the default font.
        let ui_px = (14.0 * self.font_scale).round();
        let banner_px = (40.0 * self.font_scale).round();
        let ui_font = load_font(&["segoeui.ttf"]);
        let banner_font = load_font(&["GARA.TTF", "georgia.ttf", "times.ttf"]);
        {
            let fonts = ctx.fonts();
            match ui_font {
                Some(data) => {
                    fonts.add_font(&[font_source(data, ui_px)]);
                }
                None => {
                    fonts.add_font(&[FontSource::DefaultFontData { config: None }]);
                }
            }
            if let Some(data) = banner_font {
                self.banner_font = Some(Font(fonts.add_font(&[font_source(data, banner_px)])));
            }
        }
        ctx.io_mut().font_global_scale = if ui_font.is_some() { 1.0 } else { self.font_scale };
        // Only the title bar drags the window (clicking inside widgets shouldn't move it).
        ctx.io_mut().config_windows_move_from_title_bar_only = true;
        let style = ctx.style_mut();
        style.window_rounding = 6.0;
        style.frame_rounding = 3.0;
        input_block::install();
        ui_bridge::RENDERER_ACTIVE.store(true, Ordering::Relaxed);
        if ui_bridge::DEVELOPER.load(Ordering::Relaxed) {
            kindle_core::info!("overlay ready; press {} in game for the developer overlay", self.toggle_name);
        } else {
            kindle_core::info!("overlay ready");
        }
    }

    fn before_render<'a>(&'a mut self, ctx: &mut Context, _rc: &'a mut dyn RenderContext) {
        // The game hides the OS cursor; ImGui draws one while the overlay is open.
        ctx.io_mut().mouse_draw_cursor = self.visible.load(Ordering::Relaxed);
    }

    fn render(&mut self, ui: &mut Ui) {
        let fade = f32::from_bits(ui_bridge::SCREEN_FADE.load(std::sync::atomic::Ordering::Relaxed));
        if fade > 0.0 {
            let [w, h] = ui.io().display_size;
            ui.get_background_draw_list().add_rect([0.0, 0.0], [w, h], [0.0, 0.0, 0.0, fade.min(1.0)]).filled(true).build();
        }
        draw_notices(ui, self.banner_font);
        draw_raw_access(ui);

        let developer = ui_bridge::DEVELOPER.load(Ordering::Relaxed);
        if !developer && self.visible.load(Ordering::Relaxed) {
            self.set_visible(false);
        }
        if developer && ui.is_key_pressed_no_repeat(self.toggle) {
            let v = !self.visible.load(Ordering::Relaxed);
            self.set_visible(v);
            if v {
                self.state.lock().unwrap_or_else(|e| e.into_inner()).focus_input = true;
            }
        }
        if !self.visible.load(Ordering::Relaxed) {
            return;
        }

        let snap = ui_bridge::snapshot().unwrap_or_default();
        let mut open = true;
        let toggle_name = self.toggle_name.clone();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        poll_replies(&mut state);

        ui.window(format!("Kindle {}###kindle", env!("CARGO_PKG_VERSION")))
            .size([760.0, 520.0], Condition::FirstUseEver)
            .position([40.0, 40.0], Condition::FirstUseEver)
            .opened(&mut open)
            .build(|| {
                status_line(ui, &snap, &toggle_name);
                ui.separator();
                if let Some(_tabs) = ui.tab_bar("tabs") {
                    if let Some(_t) = ui.tab_item("Console") {
                        console_tab(ui, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Mods") {
                        mods_tab(ui, &snap, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Player") {
                        player_tab(ui, &snap);
                    }
                    if let Some(_t) = ui.tab_item("Nearby") {
                        nearby_tab(ui, &snap, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Find") {
                        find_tab(ui, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Flags") {
                        flags_tab(ui, &snap, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Params") {
                        params_tab(ui, &snap, &mut state);
                    }
                    if let Some(_t) = ui.tab_item("Log") {
                        log_tab(ui, &mut state);
                    }
                }
            });
        drop(state);
        if !open {
            self.set_visible(false);
        }
    }

    fn message_filter(&self, _io: &imgui::Io) -> MessageFilter {
        if self.visible.load(Ordering::Relaxed) {
            MessageFilter::InputKeyboard | MessageFilter::InputMouse | MessageFilter::InputRaw
        } else {
            MessageFilter::empty()
        }
    }
}

fn poll_replies(s: &mut State) {
    let mut done = Vec::new();
    for (i, rx) in s.pending.iter().enumerate() {
        match rx.try_recv() {
            Ok(reply) => {
                done.push(i);
                if !reply.is_empty() {
                    let line = if reply.starts_with("error:") { Line::Error(reply) } else { Line::Output(reply) };
                    s.lines.push(line);
                    s.scroll_to_bottom = true;
                }
            }
            Err(TryRecvError::Disconnected) => done.push(i),
            Err(TryRecvError::Empty) => {}
        }
    }
    for i in done.into_iter().rev() {
        s.pending.remove(i);
    }

    if let Some(rx) = &s.row_pending {
        match rx.try_recv() {
            Ok(result) => {
                if let Ok(fields) = &result {
                    s.edits = fields.iter().map(|f| (f.name.clone(), f.value.clone())).collect();
                }
                s.fields = Some(result);
                s.row_pending = None;
            }
            Err(TryRecvError::Disconnected) => s.row_pending = None,
            Err(TryRecvError::Empty) => {}
        }
    }

    let mut reload = false;
    s.set_pending.retain(|(field, rx)| match rx.try_recv() {
        Ok(Ok(())) => {
            reload = true;
            false
        }
        Ok(Err(e)) => {
            s.param_status = Some((false, format!("{field}: {e}")));
            false
        }
        Err(TryRecvError::Disconnected) => false,
        Err(TryRecvError::Empty) => true,
    });
    if reload && let Some((table, row)) = s.loaded_key.clone() {
        request_row(s, table, row);
    }
}

fn status_line(ui: &Ui, snap: &Snapshot, toggle: &str) {
    let running = snap.mods.iter().filter(|m| !m.failed).count();
    let failed = snap.mods.iter().filter(|m| m.failed).count();
    ui.text_colored(ACCENT, "Kindle");
    ui.same_line();
    ui.text_disabled(format!("frame {}  |  up {:.0}s  |", snap.frame, snap.uptime));
    ui.same_line();
    match &snap.player {
        Some(p) => ui.text(format!("{}  |", p.name)),
        None => ui.text_disabled("no character loaded  |"),
    }
    ui.same_line();
    if !snap.mods_loaded {
        ui.text_colored(YELLOW, "waiting for game data");
    } else if failed > 0 {
        ui.text_colored(RED, format!("{running} mods running, {failed} failed"));
    } else {
        ui.text_colored(GREEN, format!("{running} mods running"));
    }
    ui.same_line();
    ui.text_disabled(format!("|  {toggle} to close"));
}

// ── Console ─────────────────────────────────────────────────────────────────

fn console_tab(ui: &Ui, s: &mut State) {
    let footer = ui.frame_height_with_spacing() + 4.0;
    ui.child_window("console-scroll").size([0.0, -footer]).build(|| {
        if s.lines.is_empty() {
            ui.text_disabled("Type Lua and press Enter. It runs inside the game with every module available.");
            ui.text_disabled("Try: player.hp()   items.give('Smithing Stone [1]', 3)   player.warp('Church of Elleh')   :mods   :help");
        }
        for line in &s.lines {
            match line {
                Line::Input(t) => ui.text_colored(ACCENT, format!("> {t}")),
                Line::Output(t) => ui.text_wrapped(t),
                Line::Error(t) => ui.text_colored(RED, t),
            }
        }
        if s.scroll_to_bottom {
            ui.set_scroll_here_y_with_ratio(1.0);
            s.scroll_to_bottom = false;
        }
    });

    if s.focus_input {
        ui.set_keyboard_focus_here();
        s.focus_input = false;
    }
    ui.set_next_item_width(-120.0);
    let submitted = ui.input_text("##console-input", &mut s.input).enter_returns_true(true).hint("Lua or :command").build();
    ui.same_line();
    let run = ui.button("Run") || submitted;
    ui.same_line();
    if ui.button("Clear") {
        s.lines.clear();
    }
    if run {
        let line = s.input.trim().to_string();
        if !line.is_empty() {
            s.lines.push(Line::Input(line.clone()));
            s.history.push(line.clone());
            let (tx, rx) = mpsc::channel();
            ui_bridge::console(ConsoleRequest { line, reply: tx });
            s.pending.push(rx);
            s.scroll_to_bottom = true;
        }
        s.input.clear();
        s.focus_input = true;
    }
}

// ── Mods ────────────────────────────────────────────────────────────────────

fn mods_tab(ui: &Ui, snap: &Snapshot, s: &mut State) {
    if ui.button("Reload all") {
        ui_bridge::send(UiRequest::Reload(None));
    }
    ui.same_line();
    ui.text_disabled("Mods also reload automatically when you save their files.");
    if !snap.character_mods.is_empty() {
        ui.text_wrapped(format!("This character has been played with: {}", snap.character_mods.join(", ")));
        ui.text_disabled(r"Save backups are in the kindle\backups folder.");
    }
    if snap.safe_mode {
        ui.text_colored(RED, "Safe mode: no mods are loaded this session.");
        ui.text_disabled("Restart the game without holding Shift (and without safe_mode in kindle.toml) to load them.");
        return;
    }
    if snap.mods.is_empty() {
        ui.text_disabled("No mods installed. Create one with: kindle new my-mod");
        return;
    }
    let flags = TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::RESIZABLE;
    if let Some(_t) = ui.begin_table_with_flags("mods", 5, flags) {
        for h in ["Mod", "Version", "Modules", "Status", ""] {
            ui.table_setup_column(h);
        }
        ui.table_headers_row();
        for (i, m) in snap.mods.iter().enumerate() {
            let _id = ui.push_id_usize(i);
            ui.table_next_row();
            ui.table_next_column();
            ui.text(&m.name);
            if m.name != m.id {
                ui.text_disabled(&m.id);
            }
            ui.table_next_column();
            ui.text(&m.version);
            if !m.author.is_empty() {
                ui.text_disabled(&m.author);
            }
            ui.table_next_column();
            ui.text_wrapped(&m.modules);
            ui.table_next_column();
            if m.failed {
                ui.text_colored(RED, "failed");
                ui.text_wrapped(&m.status);
            } else if m.disabled {
                ui.text_disabled(&m.status);
                if m.disabled_in_toml {
                    ui.text_disabled("(enabled = false in mod.toml)");
                }
            } else {
                ui.text_colored(GREEN, &m.status);
                if m.status == "running" {
                    ui.text_disabled(format!("{:.2} ms/s", m.cpu_ms));
                    if ui.is_item_hovered() {
                        let mut tip = String::from("Time spent in this mod's Lua each second (a frame is ~16.7 ms at 60 fps).");
                        for (what, ms) in m.cpu_by_handler.iter().take(8) {
                            tip.push_str(&format!(
                                "
  {what}: {ms:.2} ms/s"
                            ));
                        }
                        ui.tooltip_text(tip);
                    }
                }
            }
            if m.reloads > 0 {
                ui.text_disabled(format!("reloaded {}x", m.reloads));
            }
            ui.table_next_column();
            if !m.disabled && ui.small_button("Reload") {
                ui_bridge::send(UiRequest::Reload(Some(m.id.clone())));
            }
            if m.disabled {
                if !m.disabled_in_toml && ui.small_button("Enable") {
                    ui_bridge::send(UiRequest::SetEnabled(m.id.clone(), true));
                }
            } else {
                if ui.small_button("Disable") {
                    ui_bridge::send(UiRequest::SetEnabled(m.id.clone(), false));
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Unload this mod and keep it off (until you enable it again).");
                }
            }
        }
    }
    mod_settings(ui, snap, s);
}

/// Each mod's declared settings, as controls. A change is sent to the runtime (which saves it
/// and tells the mod) and shown right away until the next snapshot carries it.
fn mod_settings(ui: &Ui, snap: &Snapshot, s: &mut State) {
    use kindle_core::config::{SettingKind as K, SettingValue as V};
    for m in snap.mods.iter().filter(|m| !m.settings.is_empty()) {
        ui.spacing();
        if !ui.collapsing_header(format!("{} settings##{}", m.name, m.id), imgui::TreeNodeFlags::DEFAULT_OPEN) {
            continue;
        }
        for row in &m.settings {
            let key = (m.id.clone(), row.name.clone());
            if s.setting_edits.get(&key) == Some(&row.value) {
                s.setting_edits.remove(&key);
            }
            let current = s.setting_edits.get(&key).cloned().unwrap_or_else(|| row.value.clone());
            let label = format!("{}##{}.{}", row.label, m.id, row.name);
            let changed = match (&row.kind, current) {
                (K::Bool, V::Bool(mut b)) => ui.checkbox(&label, &mut b).then_some(V::Bool(b)),
                (K::Int { min, max }, V::Int(i)) => {
                    let mut v = i.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    let edited = if *min > i32::MIN as i64 && *max < i32::MAX as i64 {
                        ui.slider(&label, *min as i32, *max as i32, &mut v)
                    } else {
                        ui.input_int(&label, &mut v).build()
                    };
                    edited.then_some(V::Int(v as i64))
                }
                (K::Float { min, max }, V::Float(f)) => {
                    let mut v = f as f32;
                    let edited = if *min > f32::MIN as f64 && *max < f32::MAX as f64 {
                        ui.slider(&label, *min as f32, *max as f32, &mut v)
                    } else {
                        ui.input_float(&label, &mut v).build()
                    };
                    // Through the shortest f32 text, so 0.807 is saved as 0.807, not 0.80699998.
                    edited.then(|| V::Float(v.to_string().parse().unwrap_or(v as f64)))
                }
                (K::Choice(list), V::Choice(c)) => {
                    let mut idx = list.iter().position(|x| *x == c).unwrap_or(0);
                    ui.combo_simple_string(&label, &mut idx, list).then(|| V::Choice(list[idx].clone()))
                }
                _ => None,
            };
            if !row.description.is_empty() && ui.is_item_hovered() {
                ui.tooltip_text(&row.description);
            }
            if let Some(v) = changed {
                s.setting_edits.insert(key, v.clone());
                ui_bridge::send(UiRequest::SetSetting(m.id.clone(), row.name.clone(), v));
            }
        }
    }
}

// ── Player ──────────────────────────────────────────────────────────────────

fn bar(ui: &Ui, label: &str, (cur, max): (i32, i32), color: [f32; 4]) {
    let frac = if max > 0 { cur as f32 / max as f32 } else { 0.0 };
    let _c = ui.push_style_color(imgui::StyleColor::PlotHistogram, color);
    ProgressBar::new(frac.clamp(0.0, 1.0)).overlay_text(format!("{label}  {cur} / {max}")).size([-1.0, 0.0]).build(ui);
}

fn player_tab(ui: &Ui, snap: &Snapshot) {
    let Some(p) = &snap.player else {
        ui.text_disabled("No character loaded. Load a save and this fills in.");
        return;
    };
    ui.text_colored(ACCENT, &p.name);
    bar(ui, "HP", p.hp, [0.75, 0.2, 0.2, 1.0]);
    bar(ui, "FP", p.fp, [0.25, 0.4, 0.85, 1.0]);
    bar(ui, "Stamina", p.stamina, [0.3, 0.7, 0.3, 1.0]);
    ui.separator();
    if let Some(_t) = ui.begin_table_with_flags("stats", 6, TableFlags::SIZING_STRETCH_SAME) {
        for (name, v) in p.stats {
            ui.table_next_column();
            ui.text_disabled(name);
            ui.table_next_column();
            ui.text(v.to_string());
        }
    }
    ui.separator();
    ui.text(format!("Inventory ({} entries)", snap.inventory.len()));
    ui.child_window("inventory").build(|| {
        let flags = TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::SCROLL_Y | TableFlags::RESIZABLE;
        if let Some(_t) = ui.begin_table_with_flags("inv", 4, flags) {
            ui.table_setup_column("Item");
            ui.table_setup_column("Category");
            ui.table_setup_column("Id");
            ui.table_setup_column("Quantity");
            ui.table_headers_row();
            for (cat, id, qty, name) in &snap.inventory {
                ui.table_next_row();
                ui.table_next_column();
                if name.is_empty() {
                    ui.text_disabled("?");
                } else {
                    ui.text(name);
                }
                ui.table_next_column();
                ui.text(*cat);
                ui.table_next_column();
                ui.text(id.to_string());
                ui.table_next_column();
                ui.text(qty.to_string());
            }
        }
    });
}

// ── Nearby characters ───────────────────────────────────────────────────────

fn nearby_tab(ui: &Ui, snap: &Snapshot, s: &mut State) {
    if snap.player.is_none() {
        ui.text_disabled("No character loaded.");
        return;
    }
    ui.set_next_item_width(140.0);
    ui.input_text("SpEffect id for 'Effect'", &mut s.effect_input).chars_decimal(true).build();
    let effect: Option<i64> = s.effect_input.trim().parse().ok();
    ui.text_disabled(format!("{} living characters within 40 m, nearest first.", snap.nearby.len()));
    ui.separator();
    ui.child_window("nearby").build(|| {
        let flags = TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::RESIZABLE | TableFlags::SCROLL_Y;
        if let Some(_t) = ui.begin_table_with_flags("nearby-table", 6, flags) {
            for h in ["Character", "Type", "Entity", "HP", "Dist", ""] {
                ui.table_setup_column(h);
            }
            ui.table_headers_row();
            for (i, c) in snap.nearby.iter().enumerate() {
                let _id = ui.push_id_usize(i);
                ui.table_next_row();
                ui.table_next_column();
                ui.text(&c.label);
                ui.table_next_column();
                match c.kind {
                    "enemy" => ui.text_colored(RED, "enemy"),
                    "npc" => ui.text_colored(GREEN, "npc"),
                    other => ui.text_disabled(other),
                }
                ui.table_next_column();
                match c.entity_id {
                    Some(e) => ui.text(e.to_string()),
                    None => ui.text_disabled("-"),
                }
                ui.table_next_column();
                let frac = if c.hp.1 > 0 { c.hp.0 as f32 / c.hp.1 as f32 } else { 0.0 };
                let _c = ui.push_style_color(imgui::StyleColor::PlotHistogram, [0.75, 0.2, 0.2, 1.0]);
                ProgressBar::new(frac.clamp(0.0, 1.0)).overlay_text(format!("{}/{}", c.hp.0, c.hp.1)).size([-1.0, 0.0]).build(ui);
                ui.table_next_column();
                ui.text(format!("{:.1} m", c.distance));
                ui.table_next_column();
                if ui.small_button("Kill") {
                    ui_bridge::send(UiRequest::SetChrHp(c.handle, 0));
                }
                ui.same_line();
                if ui.small_button("Heal") {
                    ui_bridge::send(UiRequest::SetChrHp(c.handle, i32::MAX));
                }
                if let Some(id) = effect {
                    ui.same_line();
                    if ui.small_button("Effect") {
                        ui_bridge::send(UiRequest::ChrEffect(c.handle, id));
                    }
                }
            }
        }
    });
}

// ── Flags ───────────────────────────────────────────────────────────────────

fn flags_tab(ui: &Ui, snap: &Snapshot, s: &mut State) {
    ui.set_next_item_width(200.0);
    let enter = ui.input_text("Event flag", &mut s.flag_input).chars_decimal(true).enter_returns_true(true).build();
    let parsed: Option<u32> = s.flag_input.trim().parse().ok();
    ui.same_line();
    if (ui.button("Watch") || enter)
        && let Some(f) = parsed
        && !s.watched.contains(&f)
    {
        s.watched.push(f);
        ui_bridge::set_watched_flags(s.watched.clone());
    }
    ui.text_disabled("Watched flags update live. Setting flags changes your save, so use with care.");
    ui.separator();

    let mut remove = None;
    let flags = TableFlags::BORDERS | TableFlags::ROW_BG;
    if let Some(_t) = ui.begin_table_with_flags("flags", 3, flags) {
        ui.table_setup_column("Flag");
        ui.table_setup_column("State");
        ui.table_setup_column("");
        ui.table_headers_row();
        for (i, f) in s.watched.iter().enumerate() {
            let _id = ui.push_id_usize(i);
            let value = snap.flags.iter().find(|(id, _)| id == f).and_then(|(_, v)| *v);
            ui.table_next_row();
            ui.table_next_column();
            ui.text(f.to_string());
            ui.table_next_column();
            match value {
                Some(true) => ui.text_colored(GREEN, "ON"),
                Some(false) => ui.text_colored(DIM, "off"),
                None => ui.text_disabled("?"),
            }
            ui.table_next_column();
            if ui.small_button("Set") {
                ui_bridge::send(UiRequest::SetFlag(*f, true));
            }
            ui.same_line();
            if ui.small_button("Clear") {
                ui_bridge::send(UiRequest::SetFlag(*f, false));
            }
            ui.same_line();
            if ui.small_button("Remove") {
                remove = Some(i);
            }
        }
    }
    if let Some(i) = remove {
        s.watched.remove(i);
        ui_bridge::set_watched_flags(s.watched.clone());
    }
}

// ── Params ──────────────────────────────────────────────────────────────────

// ── Find ────────────────────────────────────────────────────────────────────

fn find_tab(ui: &Ui, s: &mut State) {
    // Results for the latest query only; ask again once the previous answer is in.
    if let Some((query, rx)) = &s.find_pending {
        match rx.try_recv() {
            Ok(hits) => {
                s.find_shown = query.clone();
                s.find_hits = hits;
                s.find_pending = None;
            }
            Err(TryRecvError::Disconnected) => s.find_pending = None,
            Err(TryRecvError::Empty) => {}
        }
    }
    ui.set_next_item_width(360.0);
    ui.input_text("Find", &mut s.find_query).hint("item, grace, param table").build();
    let query = s.find_query.trim().to_string();
    if s.find_pending.is_none() && query != s.find_shown {
        let (tx, rx) = mpsc::channel();
        ui_bridge::send(UiRequest::Find { query: query.clone(), reply: tx });
        s.find_pending = Some((query.clone(), rx));
    }
    if query.chars().count() < 2 {
        ui.text_disabled("Type at least 2 letters. Click a Lua snippet to copy it.");
        return;
    }
    ui.text_disabled(format!("{} result(s). Click a Lua snippet to copy it.", s.find_hits.len()));
    ui.separator();
    ui.child_window("find-results").build(|| {
        let flags = TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::RESIZABLE | TableFlags::SCROLL_Y;
        if let Some(_t) = ui.begin_table_with_flags("hits", 4, flags) {
            ui.table_setup_column("Kind");
            ui.table_setup_column("Name");
            ui.table_setup_column("Id");
            ui.table_setup_column("Lua");
            ui.table_headers_row();
            for (i, h) in s.find_hits.iter().enumerate() {
                let _id = ui.push_id_usize(i);
                ui.table_next_row();
                ui.table_next_column();
                ui.text_disabled(h.kind);
                ui.table_next_column();
                ui.text(&h.name);
                if !h.note.is_empty() && ui.is_item_hovered() {
                    ui.tooltip_text(&h.note);
                }
                ui.table_next_column();
                if !h.id.is_empty() && ui.selectable(&h.id) {
                    ui.set_clipboard_text(&h.id);
                }
                ui.table_next_column();
                if ui.selectable(&h.lua) {
                    ui.set_clipboard_text(&h.lua);
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Click to copy");
                }
            }
        }
    });
}

fn request_row(s: &mut State, table: String, row: u64) {
    let (tx, rx) = mpsc::channel();
    ui_bridge::send(UiRequest::ParamRow { table: table.clone(), row, reply: tx });
    s.row_pending = Some(rx);
    s.loaded_key = Some((table, row));
}

fn params_tab(ui: &Ui, snap: &Snapshot, s: &mut State) {
    if snap.param_tables.is_empty() {
        ui.text_disabled("Params aren't loaded yet.");
        return;
    }
    s.table_idx = s.table_idx.min(snap.param_tables.len() - 1);
    ui.set_next_item_width(260.0);
    let changed_table = ui.combo_simple_string("Table", &mut s.table_idx, &snap.param_tables);
    ui.same_line();
    ui.set_next_item_width(140.0);
    let enter = ui.input_int("Row", &mut s.row).build();
    ui.same_line();
    if (ui.button("Load") || enter || changed_table) && s.row >= 0 {
        let table = snap.param_tables[s.table_idx].clone();
        s.param_status = None;
        request_row(s, table, s.row as u64);
    }
    ui.set_next_item_width(260.0);
    ui.input_text("Filter fields", &mut s.field_filter).build();
    ui.text_disabled("Edits here apply live and last until the game restarts (they are not tied to a mod).");
    if let Some((ok, msg)) = &s.param_status {
        ui.text_colored(if *ok { GREEN } else { RED }, msg);
    }
    ui.separator();

    let Some(fields) = &s.fields else {
        ui.text_disabled("Pick a table and row, then Load.");
        return;
    };
    let fields = match fields {
        Ok(f) => f,
        Err(e) => {
            ui.text_colored(RED, e);
            return;
        }
    };
    let (table, row) = s.loaded_key.clone().unwrap_or_default();
    let filter = s.field_filter.to_ascii_lowercase();
    let mut submit: Option<(String, String)> = None;

    ui.child_window("param-fields").build(|| {
        let flags = TableFlags::BORDERS | TableFlags::ROW_BG | TableFlags::RESIZABLE | TableFlags::SCROLL_Y;
        if let Some(_t) = ui.begin_table_with_flags("fields", 3, flags) {
            ui.table_setup_column("Field");
            ui.table_setup_column("Type");
            ui.table_setup_column("Value");
            ui.table_headers_row();
            for (i, f) in fields.iter().enumerate() {
                if !filter.is_empty() && !f.name.to_ascii_lowercase().contains(&filter) {
                    continue;
                }
                let _id = ui.push_id_usize(i);
                ui.table_next_row();
                ui.table_next_column();
                ui.text(&f.name);
                ui.table_next_column();
                ui.text_disabled(f.ty);
                ui.table_next_column();
                if f.writable {
                    let buf = s.edits.entry(f.name.clone()).or_insert_with(|| f.value.clone());
                    ui.set_next_item_width(-1.0);
                    if ui.input_text("##v", buf).enter_returns_true(true).build() {
                        submit = Some((f.name.clone(), buf.clone()));
                    }
                } else {
                    ui.text_wrapped(&f.value);
                }
            }
        }
    });

    if let Some((field, value)) = submit {
        let (tx, rx) = mpsc::channel();
        ui_bridge::send(UiRequest::ParamSet { table: table.clone(), row, field: field.clone(), value: value.clone(), reply: tx });
        s.set_pending.push((field.clone(), rx));
        s.param_status = Some((true, format!("{table}[{row}].{field} = {value}")));
    }
}

// ── Log ─────────────────────────────────────────────────────────────────────

fn log_tab(ui: &Ui, s: &mut State) {
    ui.checkbox("Auto-scroll", &mut s.log_autoscroll);
    ui.child_window("log-scroll").build(|| {
        for line in kindle_core::log::recent(300) {
            if line.contains(" ERROR ") {
                ui.text_colored(RED, &line);
            } else if line.contains(" WARN ") {
                ui.text_colored(YELLOW, &line);
            } else {
                ui.text(&line);
            }
        }
        if s.log_autoscroll && ui.scroll_y() >= ui.scroll_max_y() - 5.0 {
            ui.set_scroll_here_y_with_ratio(1.0);
        }
    });
}

// ── Notices (ui.banner / ui.toast), drawn even when the window is closed ─────

fn fade(age: f32, total: f32) -> f32 {
    let fade_in = (age / 0.25).min(1.0);
    let fade_out = ((total - age) / 0.6).clamp(0.0, 1.0);
    fade_in.min(fade_out)
}

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], c[3] * a]
}

/// While a mod has raw access (kindle.toml [developer]), a line at the top of the screen says so,
/// so a player is never in that mode without knowing.
fn draw_raw_access(ui: &Ui) {
    if !ui_bridge::RAW_ACCESS.load(Ordering::Relaxed) {
        return;
    }
    let text = "Developer mode: a mod has raw access (kindle.toml [developer])";
    let [w, _] = ui.io().display_size;
    let size = ui.calc_text_size(text);
    let dl = ui.get_foreground_draw_list();
    let (x, y) = (w * 0.5 - size[0] / 2.0, size[1] * 0.4);
    dl.add_rect([x - 8.0, y - 3.0], [x + size[0] + 8.0, y + size[1] + 3.0], [0.0, 0.0, 0.0, 0.7]).filled(true).build();
    dl.add_text([x, y], [1.0, 0.45, 0.35, 1.0], text);
}

/// The mods' status lines: a band each, stacked about the middle of the screen.
fn draw_status_lines(ui: &Ui, banner_font: Option<Font>) {
    let lines = ui_bridge::status_lines();
    if lines.is_empty() {
        return;
    }
    let [w, h] = ui.io().display_size;
    let _font = banner_font.map(|f| ui.push_font(f.0));
    let band = |text: &str, progress: Option<f32>| ui.calc_text_size(text)[1] * if progress.is_some() { 3.2 } else { 2.2 };
    let mut y0 = h * 0.5 - lines.iter().map(|(text, progress)| band(text, *progress)).sum::<f32>() / 2.0;
    let dark = [0.0, 0.0, 0.0, 0.78];
    let clear = [0.0, 0.0, 0.0, 0.0];
    let dl = ui.get_foreground_draw_list();
    for (text, progress) in &lines {
        let size = ui.calc_text_size(text);
        let y1 = y0 + band(text, *progress);
        let cy = (y0 + y1) / 2.0;
        dl.add_rect_filled_multicolor([0.0, y0], [w * 0.5, y1], clear, dark, dark, clear);
        dl.add_rect_filled_multicolor([w * 0.5, y0], [w, y1], dark, clear, clear, dark);
        let ty = if progress.is_some() { cy - size[1] * 0.85 } else { cy - size[1] / 2.0 };
        dl.add_text([w * 0.5 - size[0] / 2.0, ty], [1.0, 0.82, 0.5, 1.0], text);
        if let Some(p) = progress {
            let (bw, bh) = (w * 0.24, (size[1] * 0.22).max(4.0));
            let (x0, by) = (w * 0.5 - bw / 2.0, cy + size[1] * 0.45);
            dl.add_rect([x0, by], [x0 + bw, by + bh], [0.12, 0.10, 0.08, 0.9]).filled(true).build();
            dl.add_rect([x0, by], [x0 + bw * p.clamp(0.0, 1.0), by + bh], [1.0, 0.70, 0.30, 1.0]).filled(true).build();
        }
        y0 = y1;
    }
}

fn draw_prompt(ui: &Ui) {
    let Some(text) = ui_bridge::prompt() else { return };
    let [w, h] = ui.io().display_size;
    let size = ui.calc_text_size(&text);
    let (pw, ph) = (size[0] + 48.0, size[1] + 16.0);
    let (x0, y0) = (w * 0.5 - pw / 2.0, h * 0.84 - ph / 2.0);
    let dl = ui.get_foreground_draw_list();
    dl.add_rect([x0, y0], [x0 + pw, y0 + ph], [0.02, 0.02, 0.02, 0.80]).filled(true).build();
    dl.add_rect([x0, y0], [x0 + pw, y0 + ph], [0.45, 0.40, 0.32, 0.9]).build();
    dl.add_text([w * 0.5 - size[0] / 2.0, y0 + 8.0], [0.92, 0.89, 0.82, 1.0], &text);
}

fn draw_notices(ui: &Ui, banner_font: Option<Font>) {
    draw_status_lines(ui, banner_font);
    draw_prompt(ui);
    let notices = ui_bridge::notices();
    if notices.is_empty() {
        return;
    }
    let [w, h] = ui.io().display_size;

    for n in notices.iter().filter(|n| n.kind == NoticeKind::Banner) {
        let a = fade(n.created.elapsed().as_secs_f32(), n.seconds);
        let _font = banner_font.map(|f| ui.push_font(f.0));
        let size = ui.calc_text_size(&n.text);
        let cy = h * 0.5;
        let band = size[1] * 2.2;
        let (y0, y1) = (cy - band / 2.0, cy + band / 2.0);
        let dark = with_alpha([0.0, 0.0, 0.0, 0.78], a);
        let clear = [0.0, 0.0, 0.0, 0.0];
        let dl = ui.get_foreground_draw_list();
        // Dark band fading out towards the screen edges, like the game's own banners.
        dl.add_rect_filled_multicolor([0.0, y0], [w * 0.5, y1], clear, dark, dark, clear);
        dl.add_rect_filled_multicolor([w * 0.5, y0], [w, y1], dark, clear, clear, dark);
        dl.add_text([w * 0.5 - size[0] / 2.0, cy - size[1] / 2.0], with_alpha(n.color, a), &n.text);
    }

    let pad = 12.0;
    let mut y = 24.0;
    for n in notices.iter().filter(|n| n.kind == NoticeKind::Toast) {
        let a = fade(n.created.elapsed().as_secs_f32(), n.seconds);
        let size = ui.calc_text_size(&n.text);
        let (bw, bh) = (size[0] + pad * 2.0 + 6.0, size[1] + pad * 1.4);
        let x = w - bw - 24.0;
        let dl = ui.get_foreground_draw_list();
        dl.add_rect([x, y], [x + bw, y + bh], with_alpha([0.07, 0.06, 0.05, 0.9], a)).filled(true).rounding(6.0).build();
        dl.add_rect([x, y], [x + 4.0, y + bh], with_alpha(n.color, a)).filled(true).build();
        dl.add_text([x + pad + 6.0, y + pad * 0.7], with_alpha([0.95, 0.93, 0.88, 1.0], a), &n.text);
        y += bh + 8.0;
    }
}
