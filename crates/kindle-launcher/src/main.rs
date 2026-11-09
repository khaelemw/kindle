//! Kindle Launcher: a window that starts Dark Souls III with Kindle. It manages the profiles in
//! launcher.toml (kindle-launch, shared with `kindle launch`): each profile's Kindle mods
//! (picked from the installed ones), DLL mods (Seamless Co-op, a crash fix) and file mods.
//! Kindle mods' settings are changed through the same files as the game's Mods screen.

#![windows_subsystem = "windows"]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Layout, Margin, RichText, Stroke, Vec2};
use kindle_launch::{FileMod, KindleMod, Launcher, Profile, Setting, SettingKind, SettingValue, Started};

const BG: Color32 = Color32::from_rgb(14, 13, 11);
const PANEL: Color32 = Color32::from_rgb(22, 20, 17);
const ROW: Color32 = Color32::from_rgb(29, 26, 22);
const LINE: Color32 = Color32::from_rgb(52, 46, 38);
/// Checkbox boxes, slider tracks, buttons.
const WIDGET: Color32 = Color32::from_rgb(44, 40, 34);
const TEXT: Color32 = Color32::from_rgb(216, 208, 192);
const DIM: Color32 = Color32::from_rgb(140, 132, 118);
const GOLD: Color32 = Color32::from_rgb(200, 161, 101);
const EMBER: Color32 = Color32::from_rgb(224, 128, 52);
const BAD: Color32 = Color32::from_rgb(214, 92, 74);
const GOOD: Color32 = Color32::from_rgb(132, 180, 96);

/// How often the game's process and the mods folder are looked at again.
const POLL: Duration = Duration::from_secs(1);
/// How long a start waits for the game's window.
const START_TIMEOUT: Duration = Duration::from_secs(60);

fn main() -> eframe::Result {
    let kindle_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kindle Launcher")
            .with_inner_size([900.0, 600.0])
            .with_min_inner_size([680.0, 440.0])
            .with_icon(Arc::new(ember_icon(64))),
        ..Default::default()
    };
    eframe::run_native("Kindle Launcher", options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx, kindle_dir)))))
}

struct App {
    kindle_dir: PathBuf,
    /// launcher.toml, or why it couldn't be read.
    launcher: Result<Launcher, String>,
    profile: String,
    mods: Vec<KindleMod>,
    /// The game's process id while it runs.
    game: Option<u32>,
    last_poll: Instant,
    /// The last thing that happened (error = true for failures).
    message: Option<(String, bool)>,
    /// A start in progress (it can block while DLLs load).
    starting: Option<JoinHandle<Result<Started, String>>>,
    /// When the game was started, until its window opens.
    launched: Option<Instant>,
    /// The game's process is up but never opened its window.
    stuck: bool,
    /// A profile name being typed (new, or renaming the selected one).
    naming: Option<Naming>,
    confirm_delete: bool,
    /// The Add mods list is open: the installed mods ticked so far.
    adding: Option<BTreeSet<String>>,
    /// kindle.toml's `[overlay] developer`.
    developer: bool,
    /// The game's version, or why Kindle can't run on it.
    version: Option<Result<String, String>>,
    /// Started from an unzipped release (not a game's folder): installing it.
    install: Option<Install>,
}

/// The install screen: the release and the game's folder it goes into.
struct Install {
    payload: PathBuf,
    /// The game's `Game` folder (found through Steam, or picked).
    game_dir: Option<PathBuf>,
}

struct Naming {
    rename: bool,
    text: String,
}

impl App {
    fn new(ctx: &egui::Context, kindle_dir: PathBuf) -> Self {
        style(ctx);
        let install = kindle_launch::install::payload(&kindle_dir).map(|payload| Install { payload, game_dir: kindle_launch::install::find_game() });
        let launcher = kindle_launch::load_synced(&kindle_dir);
        let profile = launcher.as_ref().map(first_profile).unwrap_or_default();
        Self {
            mods: kindle_launch::kindle_mods(&kindle_dir),
            developer: kindle_launch::developer_overlay(&kindle_dir),
            version: launcher.as_ref().ok().map(|l| kindle_launch::check_version(&l.game)),
            kindle_dir,
            launcher,
            profile,
            game: kindle_launch::game_process(),
            last_poll: Instant::now(),
            message: None,
            starting: None,
            launched: None,
            stuck: false,
            naming: None,
            confirm_delete: false,
            adding: None,
            install,
        }
    }

    /// Installs the release into the picked game folder, then manages the installed copy.
    fn install(&mut self) {
        let Some(Install { payload, game_dir: Some(game_dir) }) = &self.install else { return };
        match kindle_launch::install::install(payload, game_dir) {
            Ok(done) => {
                let kindle_dir = done.kindle_dir;
                if kindle_launch::load(&kindle_dir).is_err()
                    && let Err(e) = kindle_launch::setup(&kindle_dir)
                {
                    self.message = Some((e, true));
                }
                let mut text = format!("Installed into {}.", game_dir.display());
                if let Some(kept) = &done.kept_dinput8 {
                    text.push_str(&format!(" The dinput8.dll that was there is now {}.", kept.file_name().unwrap_or_default().to_string_lossy()));
                }
                *self = App::new_installed(self, kindle_dir);
                self.message = Some((text, false));
            }
            Err(e) => self.message = Some((e, true)),
        }
    }

    /// This window, now for the copy installed in `kindle_dir`.
    fn new_installed(old: &App, kindle_dir: PathBuf) -> App {
        let launcher = kindle_launch::load_synced(&kindle_dir);
        App {
            profile: launcher.as_ref().map(first_profile).unwrap_or_default(),
            mods: kindle_launch::kindle_mods(&kindle_dir),
            developer: kindle_launch::developer_overlay(&kindle_dir),
            version: launcher.as_ref().ok().map(|l| kindle_launch::check_version(&l.game)),
            kindle_dir,
            launcher,
            game: old.game,
            last_poll: Instant::now(),
            message: None,
            starting: None,
            launched: None,
            stuck: false,
            naming: None,
            confirm_delete: false,
            adding: None,
            install: None,
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if self.last_poll.elapsed() < POLL {
            return;
        }
        self.last_poll = Instant::now();
        if let Some(h) = self.starting.take_if(|h| h.is_finished()) {
            match h.join().unwrap_or_else(|_| Err("the launcher crashed while starting the game".into())) {
                Ok(_) => self.launched = Some(Instant::now()),
                Err(e) => self.message = Some((e, true)),
            }
        }
        // Checked again each time (the game may be updated while the launcher is open).
        self.version = self.launcher.as_ref().ok().map(|l| kindle_launch::check_version(&l.game));
        let was_running = self.game.is_some();
        self.game = kindle_launch::game_process();
        if was_running && self.game.is_none() {
            // Mods switched on or off in game go back into the profile; Kindle's settings may
            // have changed in game too.
            self.launcher = kindle_launch::load_synced(&self.kindle_dir);
            self.developer = kindle_launch::developer_overlay(&self.kindle_dir);
            if let Ok(l) = &self.launcher
                && !l.profiles.contains_key(&self.profile)
            {
                self.profile = first_profile(l);
            }
        }
        if let Some(t) = self.launched {
            if self.game.is_some_and(kindle_launch::game_window_open) {
                self.launched = None;
                self.message = None;
            } else if t.elapsed() > START_TIMEOUT {
                self.launched = None;
                self.stuck = self.game.is_some();
                let text = if self.stuck { "Dark Souls III is stuck starting." } else { "Dark Souls III didn't start." };
                self.message = Some((text.into(), true));
            }
        }
        if self.game.is_none() {
            self.stuck = false;
        }
        // Not while a slider is held, so the value being dragged isn't replaced.
        if !ctx.input(|i| i.pointer.any_down()) {
            self.mods = kindle_launch::kindle_mods(&self.kindle_dir);
        }
    }

    fn save(&mut self) {
        if let Ok(l) = &self.launcher
            && let Err(e) = kindle_launch::save(&self.kindle_dir, l)
        {
            self.message = Some((e, true));
        }
    }

    fn launch(&mut self) {
        let Ok(l) = &mut self.launcher else { return };
        l.default = self.profile.clone();
        let (l, name, dir) = (l.clone(), self.profile.clone(), self.kindle_dir.clone());
        self.save();
        self.message = None;
        self.starting = Some(std::thread::spawn(move || kindle_launch::start(&dir, &l, &name, false)));
    }

    fn busy(&self) -> bool {
        self.starting.is_some() || self.launched.is_some()
    }

    fn profile_mut(&mut self) -> Option<&mut Profile> {
        self.launcher.as_mut().ok()?.profiles.get_mut(&self.profile)
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll(&ui.ctx().clone());
        ui.ctx().request_repaint_after(POLL);

        egui::Panel::top("title")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(24, 16)))
            .show_separator_line(false)
            .show(ui, |ui| self.title_bar(ui));
        egui::Panel::bottom("actions")
            .frame(egui::Frame::new().fill(PANEL).inner_margin(Margin::symmetric(24, 14)))
            .show_separator_line(false)
            .show(ui, |ui| self.action_bar(ui));
        if self.install.is_none() && self.launcher.is_ok() {
            egui::Panel::left("profiles")
                .frame(egui::Frame::new().fill(BG).inner_margin(Margin { left: 24, right: 12, top: 20, bottom: 12 }))
                .resizable(false)
                .exact_size(250.0)
                .show_separator_line(false)
                .show(ui, |ui| self.profiles(ui));
        }
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::Frame::new().fill(BG).inner_margin(Margin { left: 12, right: 24, top: 20, bottom: 12 }).show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                if self.install.is_some() {
                    self.install_screen(ui);
                } else if self.launcher.is_ok() {
                    self.mod_lists(ui);
                } else {
                    self.setup_screen(ui);
                }
            });
        });
    }
}

// ── panels ──────────────────────────────────────────────────────────────────

impl App {
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new("KINDLE").font(serif(34.0)).color(GOLD));
                match &self.version {
                    Some(Ok(v)) => ui.label(RichText::new(format!("Dark Souls III {}", v.strip_suffix(".0").unwrap_or(v))).color(DIM)),
                    Some(Err(e)) => ui.label(RichText::new(e).color(BAD)),
                    None => ui.label(RichText::new("Dark Souls III").color(DIM)),
                };
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let status = if self.busy() {
                    Some((EMBER, "Starting"))
                } else if self.stuck {
                    Some((BAD, "Stuck"))
                } else if self.game.is_some() {
                    Some((GOOD, "Running"))
                } else {
                    None
                };
                if let Some((dot, text)) = status {
                    ui.label(RichText::new(text).color(TEXT));
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 4.5, dot);
                }
            });
        });
    }

    fn action_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(install) = &self.install {
            let (ready, update) = (install.game_dir.is_some(), install.game_dir.as_deref().is_some_and(kindle_launch::install::is_installed));
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if big_button(ui, if update { "Update" } else { "Install" }, ready && self.game.is_none()).clicked() {
                        self.install();
                    }
                    ui.add_space(16.0);
                    if let Some((text, error)) = &self.message {
                        ui.add(egui::Label::new(RichText::new(text).color(if *error { BAD } else { TEXT })).wrap());
                    } else if self.game.is_some() {
                        ui.label(RichText::new("Close Dark Souls III to install.").color(TEXT));
                    }
                });
            });
            return;
        }
        ui.horizontal(|ui| {
            if ui.button("Mods folder").clicked() {
                open(&self.kindle_dir.join("mods"));
            }
            if ui.button("Logs").clicked() {
                open(&self.kindle_dir.join("logs"));
            }
            let docs = self.kindle_dir.join("docs.html");
            if docs.is_file() && ui.button("Docs").clicked() {
                open(&docs);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.stuck
                    && let Some(pid) = self.game
                {
                    if big_button(ui, "Close game", true).clicked() {
                        match kindle_launch::stop_game(pid) {
                            Ok(()) => self.message = None,
                            Err(e) => self.message = Some((e, true)),
                        }
                    }
                } else {
                    let ready = self.launcher.as_ref().is_ok_and(|l| l.profiles.contains_key(&self.profile)) && !matches!(self.version, Some(Err(_)));
                    let label = if self.busy() {
                        "Starting…"
                    } else if self.game.is_some() {
                        "Running"
                    } else {
                        "Launch"
                    };
                    if big_button(ui, label, ready && self.game.is_none() && !self.busy()).clicked() {
                        self.launch();
                    }
                }
                ui.add_space(16.0);
                if let Some((text, error)) = &self.message {
                    ui.add(egui::Label::new(RichText::new(text).color(if *error { BAD } else { TEXT })).wrap());
                }
            });
        });
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Profiles", |_| {});
        let Ok(l) = &mut self.launcher else { return };
        let names: Vec<String> = l.profiles.keys().cloned().collect();
        let mut picked = None;
        for name in &names {
            let selected = *name == self.profile;
            let r = card(ui, selected, |ui| {
                ui.label(RichText::new(name).color(if selected { GOLD } else { TEXT }).strong());
            });
            if r.interact(egui::Sense::click()).clicked() && !selected {
                picked = Some(name.clone());
            }
        }
        if let Some(name) = picked {
            self.profile = name;
            self.confirm_delete = false;
            self.adding = None;
            l.default = self.profile.clone();
            self.save();
        }

        let mut commit = None;
        if let Some(n) = &mut self.naming {
            let r = ui.add(egui::TextEdit::singleline(&mut n.text).hint_text("Profile name").desired_width(f32::INFINITY));
            r.request_focus();
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                commit = Some(n.text.trim().to_string());
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.naming = None;
            }
        } else {
            ui.horizontal(|ui| {
                if ui.small_button("New").clicked() {
                    self.naming = Some(Naming { rename: false, text: String::new() });
                    self.confirm_delete = false;
                }
                if ui.small_button("Rename").clicked() {
                    self.naming = Some(Naming { rename: true, text: self.profile.clone() });
                    self.confirm_delete = false;
                }
                let last = names.len() <= 1;
                let label = if self.confirm_delete { RichText::new("Delete?").color(BAD) } else { RichText::new("Delete") };
                if ui.add_enabled(!last, egui::Button::new(label).small()).clicked() {
                    if self.confirm_delete {
                        self.delete_profile();
                    } else {
                        self.confirm_delete = true;
                    }
                }
            });
        }
        if let Some(name) = commit {
            self.name_profile(name);
        }

        ui.add_space(14.0);
        let Ok(l) = &mut self.launcher else { return };
        let has_modengine = l.modengine.is_file();
        let mut changed = false;
        if let Some(p) = l.profiles.get_mut(&self.profile) {
            let needs_modengine = p.active_file_mods().next().is_some();
            let mut use_modengine = (!p.direct() || needs_modengine) && has_modengine;
            let r = ui.add_enabled(has_modengine && !needs_modengine, egui::Checkbox::new(&mut use_modengine, "Mod Engine 2"));
            let r = if !has_modengine {
                r.on_disabled_hover_text("Not found")
            } else if needs_modengine {
                r.on_disabled_hover_text("File mods need it")
            } else {
                r
            };
            if r.changed() {
                p.loader = if use_modengine { String::new() } else { "direct".into() };
                changed = true;
            }
        }
        if changed {
            self.save();
        }

        ui.add_space(14.0);
        heading(ui, "Kindle", |_| {});
        let running = self.game.is_some();
        let r = ui.add_enabled(!running, egui::Checkbox::new(&mut self.developer, "Developer overlay"));
        let r = if running { r.on_disabled_hover_text("Close the game to change this, or use Mods > Kindle in game") } else { r };
        if r.on_hover_text("F1 in game opens the developer overlay: console, log, game state and Find.").changed()
            && let Err(e) = kindle_launch::set_developer_overlay(&self.kindle_dir, self.developer)
        {
            self.message = Some((e, true));
        }
    }

    fn name_profile(&mut self, name: String) {
        let Some(n) = self.naming.take() else { return };
        let Ok(l) = &mut self.launcher else { return };
        if name.is_empty() {
            return;
        }
        if name != self.profile && l.profiles.contains_key(&name) {
            self.message = Some((format!("There's already a profile called {name}."), true));
            self.naming = Some(n);
            return;
        }
        self.message = None;
        if n.rename {
            if let Some(p) = l.profiles.remove(&self.profile) {
                l.profiles.insert(name.clone(), p);
            }
        } else {
            l.profiles.insert(name.clone(), Profile { kindle_mods: Some(Vec::new()), ..Profile::default() });
        }
        self.profile = name;
        l.default = self.profile.clone();
        self.save();
    }

    fn delete_profile(&mut self) {
        self.confirm_delete = false;
        let Ok(l) = &mut self.launcher else { return };
        l.profiles.remove(&self.profile);
        self.profile = first_profile(l);
        l.default = self.profile.clone();
        self.save();
    }

    fn mod_lists(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            self.kindle_mods(ui);
            ui.add_space(18.0);
            self.dll_mods(ui);
            ui.add_space(18.0);
            self.file_mods(ui);
        });
    }

    fn kindle_mods(&mut self, ui: &mut egui::Ui) {
        let running = self.game.is_some();
        let mut open_picker = false;
        let can_add = !running && self.adding.is_none();
        heading(ui, "Kindle mods", |ui| {
            open_picker = ui.add_enabled(can_add, egui::Button::new("Add mods").small()).clicked();
        });
        if open_picker {
            self.adding = Some(BTreeSet::new());
        }
        if running {
            ui.label(RichText::new("Close the game to change these.").small().color(DIM));
            ui.add_space(4.0);
        }
        let ids: Vec<String> = self.profile_mut().and_then(|p| p.kindle_mods.clone()).unwrap_or_default();
        if self.adding.is_some() {
            self.mod_picker(ui, &ids);
        }
        let mut remove = None;
        let mut commits: Vec<(String, Setting, SettingValue)> = Vec::new();
        for id in &ids {
            let Some(m) = self.mods.iter_mut().find(|m| m.id == *id) else {
                card(ui, false, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(id).color(TEXT).strong());
                        ui.label(RichText::new("Not installed").small().color(BAD));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(!running, egui::Button::new("Remove").small()).clicked() {
                                remove = Some(id.clone());
                            }
                        });
                    });
                });
                continue;
            };
            card(ui, false, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&m.name).color(TEXT).strong());
                    if !m.version.is_empty() {
                        ui.label(RichText::new(&m.version).color(DIM));
                    }
                    if !m.author.is_empty() {
                        ui.label(RichText::new(format!("by {}", m.author)).color(DIM));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.add_enabled(!running, egui::Button::new("Remove").small()).clicked() {
                            remove = Some(id.clone());
                        }
                    });
                });
                if let Some(e) = &m.error {
                    ui.label(RichText::new(e).small().color(BAD));
                } else if m.off_in_toml {
                    ui.label(RichText::new("Off in its mod.toml").small().color(DIM));
                } else if let Some(games) = &m.other_game {
                    ui.label(RichText::new(format!("For {games}, not Dark Souls III (games in its mod.toml)")).small().color(DIM));
                }
                if !m.description.is_empty() {
                    ui.label(RichText::new(&m.description).color(DIM));
                }
                if !m.settings.is_empty() {
                    ui.add_space(4.0);
                    ui.add_enabled_ui(!running, |ui| {
                        egui::Grid::new(("settings", &m.id)).num_columns(2).spacing([18.0, 6.0]).show(ui, |ui| {
                            for s in &mut m.settings {
                                let label = ui.label(RichText::new(&s.decl.label).color(TEXT));
                                if !s.decl.description.is_empty() {
                                    label.on_hover_text(&s.decl.description);
                                }
                                if setting_control(ui, (&m.id, &s.decl.name), &s.decl.kind, &mut s.value) {
                                    commits.push((m.id.clone(), s.decl.clone(), s.value.clone()));
                                }
                                ui.end_row();
                            }
                        });
                    });
                }
            });
        }
        if ids.is_empty() && self.adding.is_none() {
            ui.label(RichText::new("None").color(DIM));
        }
        if let Some(id) = remove
            && let Some(p) = self.profile_mut()
        {
            p.kindle_mods.get_or_insert_default().retain(|m| *m != id);
            self.save();
        }
        for (id, decl, value) in commits {
            if let Err(e) = kindle_launch::set_setting(&self.kindle_dir, &id, &decl, value) {
                self.message = Some((e, true));
            }
        }
    }

    /// The installed mods that aren't in the profile, to tick and add.
    fn mod_picker(&mut self, ui: &mut egui::Ui, ids: &[String]) {
        let Some(picked) = &mut self.adding else { return };
        let choices: Vec<&KindleMod> = self.mods.iter().filter(|m| !ids.contains(&m.id)).collect();
        let mut done = None;
        egui::Frame::new().fill(ROW).stroke(Stroke::new(1.0, GOLD)).corner_radius(CornerRadius::same(3)).inner_margin(Margin::symmetric(12, 9)).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                if choices.is_empty() {
                    ui.label(RichText::new("Every installed mod is already in this profile.").color(DIM));
                }
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for m in &choices {
                        let usable = m.error.is_none() && !m.off_in_toml && m.other_game.is_none();
                        let mut on = picked.contains(&m.id);
                        let text = if m.version.is_empty() { m.name.clone() } else { format!("{}  {}", m.name, m.version) };
                        if ui.add_enabled(usable, egui::Checkbox::new(&mut on, text)).changed() {
                            if on {
                                picked.insert(m.id.clone());
                            } else {
                                picked.remove(&m.id);
                            }
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let label = if picked.is_empty() { "Add".to_string() } else { format!("Add {}", picked.len()) };
                    if ui.add_enabled(!picked.is_empty(), egui::Button::new(label)).clicked() {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                    let usable: Vec<&String> =
                        choices.iter().filter(|m| m.error.is_none() && !m.off_in_toml && m.other_game.is_none()).map(|m| &m.id).collect();
                    if usable.len() > 1 && ui.button("All").clicked() {
                        picked.extend(usable.into_iter().cloned());
                    }
                });
            },
        );
        ui.add_space(6.0);
        match done {
            Some(true) => {
                let picked = self.adding.take().unwrap_or_default();
                // In the mods folder's order.
                let new: Vec<String> = self.mods.iter().filter(|m| picked.contains(&m.id)).map(|m| m.id.clone()).collect();
                if let Some(p) = self.profile_mut() {
                    p.kindle_mods.get_or_insert_default().extend(new);
                    self.save();
                }
            }
            Some(false) => self.adding = None,
            None => {}
        }
    }

    fn dll_mods(&mut self, ui: &mut egui::Ui) {
        let mut add = false;
        heading(ui, "DLL mods", |ui| add = ui.small_button("Add DLL").clicked());
        if add
            && let Some(path) = pick_path(false)
            && let Some(p) = self.profile_mut()
            && !p.dlls.contains(&path)
        {
            p.dlls.push(path);
            self.save();
        }
        let Some(p) = self.profile_mut() else { return };
        let mut changed = false;
        let mut remove = None;
        for dll in p.dlls.clone() {
            let mut on = !p.dlls_off.contains(&dll);
            let r = mod_row(ui, &mut on, &kindle_launch::dll_name(&dll), &dll, dll.is_file());
            if r.toggled {
                p.set_dll_on(&dll, on);
                changed = true;
            }
            if r.remove {
                remove = Some(dll);
            }
        }
        if let Some(dll) = remove {
            p.dlls.retain(|d| *d != dll);
            p.dlls_off.retain(|d| *d != dll);
            changed = true;
        }
        if p.dlls.is_empty() {
            ui.label(RichText::new("None").color(DIM));
        }
        if changed {
            self.save();
        }
    }

    fn file_mods(&mut self, ui: &mut egui::Ui) {
        let mut add = false;
        heading(ui, "File mods", |ui| add = ui.small_button("Add folder").clicked());
        if add
            && let Some(path) = pick_path(true)
            && let Some(p) = self.profile_mut()
            && !p.file_mods.iter().any(|m| m.path == path)
        {
            let name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            p.file_mods.push(FileMod { name, path, enabled: true });
            self.save();
        }
        let Some(p) = self.profile_mut() else { return };
        let mut changed = false;
        let mut remove = None;
        for (i, m) in p.file_mods.iter_mut().enumerate() {
            let r = mod_row(ui, &mut m.enabled, &m.name, &m.path, m.path.is_dir());
            changed |= r.toggled;
            if r.remove {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            p.file_mods.remove(i);
            changed = true;
        }
        if p.file_mods.is_empty() {
            ui.label(RichText::new("None").color(DIM));
        }
        if changed {
            self.save();
        }
    }

    fn install_screen(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Install", |_| {});
        ui.label(RichText::new("Kindle goes into Dark Souls III's Game folder: dinput8.dll beside the game, and the kindle folder.").color(TEXT));
        ui.add_space(10.0);
        let Some(install) = &mut self.install else { return };
        let mut browse = false;
        card(ui, true, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Dark Souls III").color(TEXT).strong());
                    match &install.game_dir {
                        Some(dir) => ui.label(RichText::new(dir.display().to_string()).small().color(DIM)),
                        None => ui.label(RichText::new("Not found. Pick the game's folder.").small().color(BAD)),
                    };
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| browse = ui.button("Browse…").clicked());
            });
        });
        if let Some(dir) = &install.game_dir
            && kindle_launch::install::is_installed(dir)
        {
            ui.label(RichText::new("Kindle is installed there. Update replaces Kindle's files and keeps your settings and your own mods.").color(DIM));
            ui.add_space(6.0);
            if ui.button("Use the installed copy").clicked() {
                let kindle_dir = dir.join("kindle");
                *self = App::new_installed(self, kindle_dir);
                return;
            }
        }
        if browse && let Some(picked) = pick_path(true) {
            match kindle_launch::install::game_dir_for(&picked) {
                Some(dir) => {
                    install.game_dir = Some(dir);
                    self.message = None;
                }
                None => self.message = Some((format!("{} isn't in that folder.", kindle_launch::GAME_EXE), true)),
            }
        }
    }

    fn setup_screen(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Set up", |_| {});
        if ui.button("Find Dark Souls III").clicked() {
            match kindle_launch::setup(&self.kindle_dir) {
                Ok((l, notes)) => {
                    self.profile = first_profile(&l);
                    self.launcher = Ok(l);
                    self.message = notes.get(1).map(|n| (n.clone(), false));
                }
                Err(e) => self.message = Some((e, true)),
            }
        }
    }
}

struct RowAction {
    toggled: bool,
    remove: bool,
}

/// A DLL or file mod: switch, name, path, and a Remove button.
fn mod_row(ui: &mut egui::Ui, on: &mut bool, name: &str, path: &Path, exists: bool) -> RowAction {
    let mut action = RowAction { toggled: false, remove: false };
    card(ui, false, |ui| {
        ui.horizontal(|ui| {
            action.toggled = ui.add(egui::Checkbox::without_text(on)).changed();
            ui.vertical(|ui| {
                ui.label(RichText::new(name).color(TEXT).strong());
                ui.label(RichText::new(path.display().to_string()).small().color(DIM));
                if !exists {
                    ui.label(RichText::new("Not found").small().color(BAD));
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                action.remove = ui.small_button("Remove").clicked();
            });
        });
    });
    action
}

/// A setting's control; true when a new value should be saved (sliders: when let go).
fn setting_control(ui: &mut egui::Ui, id: (&str, &str), kind: &SettingKind, value: &mut SettingValue) -> bool {
    fn bounded<T: PartialOrd>(min: T, max: T, lo: T, hi: T) -> bool {
        min > lo && max < hi
    }
    let slid = |r: egui::Response| r.drag_stopped() || (r.changed() && !r.dragged());
    match (kind, value) {
        (SettingKind::Bool, SettingValue::Bool(b)) => ui.add(egui::Checkbox::without_text(b)).changed(),
        (SettingKind::Int { min, max }, SettingValue::Int(i)) => {
            if bounded(*min, *max, i64::MIN, i64::MAX) {
                slid(ui.add(egui::Slider::new(i, *min..=*max)))
            } else {
                slid(ui.add(egui::DragValue::new(i)))
            }
        }
        (SettingKind::Float { min, max }, SettingValue::Float(f)) => {
            if bounded(*min, *max, f64::MIN, f64::MAX) {
                slid(ui.add(egui::Slider::new(f, *min..=*max)))
            } else {
                slid(ui.add(egui::DragValue::new(f).speed(0.1)))
            }
        }
        (SettingKind::Choice(list), SettingValue::Choice(c)) => {
            let before = c.clone();
            egui::ComboBox::from_id_salt(id).selected_text(c.as_str()).show_ui(ui, |ui| {
                for o in list {
                    ui.selectable_value(c, o.clone(), o);
                }
            });
            *c != before
        }
        _ => {
            ui.label(RichText::new("?").color(DIM));
            false
        }
    }
}

/// Windows' own Open dialog: a .dll, or a folder.
fn pick_path(folder: bool) -> Option<PathBuf> {
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    use windows::core::w;
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let d: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        if folder {
            d.SetOptions(d.GetOptions().ok()? | FOS_PICKFOLDERS).ok()?;
        } else {
            d.SetFileTypes(&[COMDLG_FILTERSPEC { pszName: w!("DLL"), pszSpec: w!("*.dll") }]).ok()?;
        }
        d.Show(Some(GetActiveWindow())).ok()?;
        let name = d.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = name.to_string().ok();
        CoTaskMemFree(Some(name.0 as _));
        path.map(PathBuf::from)
    }
}

// ── look ────────────────────────────────────────────────────────────────────

fn first_profile(l: &Launcher) -> String {
    if l.profiles.contains_key(&l.default) { l.default.clone() } else { l.profiles.keys().next().cloned().unwrap_or_default() }
}

fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

/// A section title with a rule under it; `right` adds buttons at its right end.
fn heading(ui: &mut egui::Ui, text: &str, right: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).font(serif(21.0)).color(GOLD));
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, LINE));
    ui.add_space(6.0);
}

/// A row with a dark fill (gold edge when selected).
fn card(ui: &mut egui::Ui, selected: bool, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let r = egui::Frame::new()
        .fill(ROW)
        .stroke(Stroke::new(1.0, if selected { GOLD } else { LINE }))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .response;
    ui.add_space(6.0);
    r
}

fn big_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(RichText::new(label).font(serif(22.0)).color(if enabled { BG } else { DIM }))
        .fill(if enabled { GOLD } else { ROW })
        .min_size(Vec2::new(170.0, 44.0))
        .corner_radius(CornerRadius::same(3));
    ui.add_enabled(enabled, button)
}

fn style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let windows_fonts = std::env::var("WINDIR").map(|w| PathBuf::from(w).join("Fonts")).unwrap_or_default();
    let load = |names: &[&str]| names.iter().find_map(|n| std::fs::read(windows_fonts.join(n)).ok());
    if let Some(ui_font) = load(&["segoeui.ttf"]) {
        fonts.font_data.insert("ui".into(), Arc::new(FontData::from_owned(ui_font)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    let mut serif_family = Vec::new();
    if let Some(serif_font) = load(&["GARA.TTF", "georgia.ttf", "times.ttf"]) {
        fonts.font_data.insert("serif".into(), Arc::new(FontData::from_owned(serif_font)));
        serif_family.push("serif".into());
    }
    serif_family.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts.families.insert(FontFamily::Name("serif".into()), serif_family);
    ctx.set_fonts(fonts);

    ctx.global_style_mut(|s| {
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(15.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(12.5));
        s.spacing.item_spacing = Vec2::new(8.0, 4.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.spacing.slider_width = 180.0;
        let v = &mut s.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = PANEL;
        v.extreme_bg_color = BG;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = EMBER.gamma_multiply(0.6);
        v.selection.stroke = Stroke::new(1.0, GOLD);
        v.hyperlink_color = GOLD;
        v.slider_trailing_fill = true;
        let w = &mut v.widgets;
        for (state, fill) in [(&mut w.inactive, WIDGET), (&mut w.hovered, Color32::from_rgb(56, 50, 41)), (&mut w.active, Color32::from_rgb(68, 60, 49))] {
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.bg_stroke = Stroke::new(1.0, LINE);
            state.corner_radius = CornerRadius::same(3);
        }
        w.hovered.bg_stroke = Stroke::new(1.0, GOLD);
        w.inactive.fg_stroke = Stroke::new(1.5, GOLD);
        w.hovered.fg_stroke = Stroke::new(1.5, GOLD);
        w.active.fg_stroke = Stroke::new(1.5, GOLD);
        w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    });
}

/// The window icon: a glowing ember.
fn ember_icon(size: u32) -> egui::IconData {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = ((x as f32 - c) / c, (y as f32 - c) / c);
            let d = (dx * dx + dy * dy).sqrt();
            let glow = (1.0 - d).clamp(0.0, 1.0);
            let core = (1.0 - d * 2.2).clamp(0.0, 1.0);
            let r = 200.0 + 55.0 * core;
            let g = 90.0 + 150.0 * core;
            let b = 30.0 + 140.0 * core * core;
            rgba.extend([r as u8, g as u8, b as u8, (glow.powf(0.8) * 255.0) as u8]);
        }
    }
    egui::IconData { rgba, width: size, height: size }
}

/// Opens a folder in Explorer, or a file with its program.
fn open(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}
