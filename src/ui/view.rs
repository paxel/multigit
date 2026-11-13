//! Drawing of the board: toolbar, cards, output panel, CLI, toasts, dialogs.

use crate::domain::board::SortKey;
use crate::domain::command::{Job, display, shell_name, shell_program};
use crate::domain::critters::{self, Troupe};
use crate::domain::favorites::{EMOJIS, Favorite, PALETTE, color_rgb, dark_text_on, favorites_of};
use crate::domain::kawaii;
use crate::domain::naming::{parent_line, title};
use crate::domain::output::{Run, SectionState};
use crate::domain::remote::remote_of;
use crate::domain::remote::{safe_url, short_url};
use crate::domain::repo::Snapshot;
use crate::domain::status::{Head, RepoState, Upstream};
use crate::domain::switch::{
    Choice, branch_choices, matching, new_branch_command, remote_choices, tag_choices,
};
use crate::ui::app::{App, Card, CopyFlash};
use crate::ui::cli::{CliAction, CliContext};
use crate::ui::critter;
use crate::ui::settings;
use crate::ui::style::{KAWAII_PINK, KAWAII_YELLOW};
use crate::ui::text_menu::{self, Picked, TextOf};
use egui::{Color32, RichText, Ui};
use std::path::{Path, PathBuf};

/// Content width of one card.
const CARD_WIDTH: f32 = 290.0;
/// Inner margin of a card frame.
const CARD_MARGIN: f32 = 8.0;
/// Border width of a marked card.
const MARKED_STROKE: f32 = 3.0;
/// Width a card takes on the board: content, margins and the widest frame stroke.
const CARD_OUTER_WIDTH: f32 = CARD_WIDTH + 2.0 * CARD_MARGIN + 2.0 * MARKED_STROKE;

/// Width of the filter field.
const FILTER_WIDTH: f32 = 160.0;
/// Room at the toolbar's right end for the sort button, the filter field and
/// its count.
const VIEW_CONTROLS_WIDTH: f32 = 380.0;

/// Id of the name field in the favorite dialog.
pub fn favorite_name_id() -> egui::Id {
    egui::Id::new("favorite-name")
}

/// Black or white, whichever reads on the palette color `rgb`. The text
/// carries its own color, so the theme's hover color cannot hide it.
fn text_on(rgb: [u8; 3]) -> Color32 {
    if dark_text_on(rgb) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// A button filled with a palette color; the picked or active one has a
/// thick border.
fn swatch(ui: &mut Ui, text: &str, rgb: [u8; 3], picked: bool, small: bool) -> egui::Response {
    let [r, g, b] = rgb;
    let mut text = RichText::new(text).color(text_on(rgb));
    if small {
        text = text.small();
    }
    let mut button = egui::Button::new(text).fill(Color32::from_rgb(r, g, b));
    if small {
        button = button.small();
    }
    if picked {
        let width = if small { 2.0 } else { 3.0 };
        button = button.stroke(egui::Stroke::new(width, ui.visuals().strong_text_color()));
    }
    ui.add(button)
}

fn favorite_button(ui: &mut Ui, favorite: &Favorite, active: bool) -> egui::Response {
    swatch(
        ui,
        &favorite.label(),
        color_rgb(&favorite.color),
        active,
        false,
    )
}

/// Id of the filter field.
pub fn filter_id() -> egui::Id {
    egui::Id::new("board-filter")
}

/// Something the user clicked that changes the board; applied after drawing.
enum BoardAction {
    /// From the right-click menu of this card: take its targets off the
    /// board for good, or hide them.
    Remove(PathBuf),
    Hide(PathBuf),
    Button(usize, PathBuf),
    Toggle(PathBuf),
    Terminal(PathBuf),
    /// Run these git arguments in this repository.
    Run(PathBuf, String),
    /// From the right-click menu of this card: put its targets into the
    /// favorite at this position, take them out, or make a new favorite.
    AddToFavorite(usize, PathBuf),
    RemoveFromFavorite(usize, PathBuf),
    NewFavorite(PathBuf),
    /// Open the read-only view of this repository.
    Open(PathBuf),
    /// A chip of the favorite at this position was clicked.
    ToggleFavorite(usize),
    /// The critter at this position was clicked.
    Pet(usize),
    /// A card text was clicked at this pointer position.
    Copy(String, egui::Pos2),
    /// The fetch error of this card was clicked at this pointer position.
    CopyFetchError(PathBuf, egui::Pos2),
}

/// How long the "Copied" animation lasts, and how far it floats up.
const COPY_FLASH_SECS: f64 = 0.6;
const COPY_FLASH_RISE: f32 = 20.0;

/// Progress of the "Copied" animation from 0 to 1; `None` once it is over.
fn flash_progress(started: f64, now: f64) -> Option<f32> {
    let t = (now - started) / COPY_FLASH_SECS;
    (0.0..1.0).contains(&t).then_some(t as f32)
}

/// Name of the shell `!` lines run in, for labels.
pub fn login_shell_name() -> String {
    shell_name(&shell_program(std::env::var_os("SHELL")))
}

/// Takes every Enter out of this frame's input while the dialog is open and
/// reports whether one was a fresh press. Key repeats do not count, so holding
/// the Enter that ran a line cannot confirm its dialog; taking them all also
/// keeps them from clicking a focused button.
fn take_fresh_enter(ui: &Ui) -> bool {
    ui.input_mut(|i| {
        let mut fresh = false;
        i.events.retain(|e| match e {
            egui::Event::Key {
                key: egui::Key::Enter,
                pressed,
                repeat,
                ..
            } => {
                fresh |= *pressed && !*repeat;
                false
            }
            _ => true,
        });
        fresh
    })
}

/// `1 repository`, `3 repositories`.
fn repositories(count: usize) -> String {
    if count == 1 {
        "1 repository".to_owned()
    } else {
        format!("{count} repositories")
    }
}

pub(super) fn ok_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x6c, 0xc6, 0x6c)
    } else {
        Color32::from_rgb(0x1a, 0x7f, 0x37)
    }
}

/// File picker for a sound of the user's own. Blocks until the dialog closes.
pub fn pick_sound_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Choose a sound")
        .add_filter("Sound", &["wav", "ogg", "mp3", "flac"])
        .pick_file()
}

/// Folder picker for (+). Blocks until the dialog closes.
fn pick_folder() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Add repository or folder with repositories")
        .pick_folder()
}

impl App {
    /// Draw the whole app into `ui` (the eframe root ui, or a test harness).
    pub fn show(&mut self, ui: &mut Ui) {
        self.handle_events();
        crate::ui::style::apply_theme(ui.ctx(), self.config.theme);
        // Taken before the command line sees the key, which keeps focus under the dialog.
        let enter_confirms = self.confirm.is_some() && take_fresh_enter(ui);
        if let Some(folder) = self.take_picked_folder() {
            self.add_folder(&folder);
        }
        self.take_picked_sound();
        self.pointer_held = ui.input(|i| i.pointer.any_down());
        let mut next = self.tick(std::time::Instant::now());
        if !self.toasts.is_empty() {
            // Repaint soon enough to take expired toasts down.
            next = next.min(std::time::Duration::from_millis(500));
        }
        ui.ctx()
            .request_repaint_after(next.max(std::time::Duration::from_millis(200)));
        // The Kawaii theme moves, but only while the window has focus and
        // for a few seconds after a repository changed: otherwise no frame
        // is asked for and the picture stands still.
        let (dt, focused) = ui.input(|i| (i.stable_dt, i.focused));
        if self.advance_kawaii(dt, focused) {
            let frame = std::time::Duration::from_secs_f32(1.0 / kawaii::FPS);
            ui.ctx().request_repaint_after(frame);
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("cli").show(ui, |ui| {
            ui.add_space(4.0);
            self.cli_line(ui);
            ui.add_space(4.0);
        });
        if !self.config.output_hidden {
            self.output_shown();
            egui::Panel::bottom("output")
                .resizable(true)
                .default_size(220.0)
                .min_size(60.0)
                .show(ui, |ui| self.output_panel(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| self.board(ui));

        self.toasts_overlay(ui);
        self.copy_flash_overlay(ui);
        self.confirm_dialog(ui, enter_confirms);
        self.favorite_dialog(ui);
        self.repo_view_dialog(ui);
        self.text_menu_overlay(ui);
        settings::show(self, ui.ctx());
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        // The filter keeps the right end; the buttons wrap in what is left.
        ui.horizontal_top(|ui| {
            let left = (ui.available_width() - VIEW_CONTROLS_WIDTH).max(200.0);
            let wrapping = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
            ui.allocate_ui_with_layout(egui::vec2(left, 0.0), wrapping, |ui| {
                self.toolbar_buttons(ui);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                self.filter_field(ui);
            });
        });
        self.favorites_line(ui);
        if let Some(error) = &self.config_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Config not loaded, changes are not saved: {error}"),
            );
        }
    }

    /// The filter field and, while it hides cards by its text, how many show;
    /// left of it the sort button. Drawn right to left.
    fn filter_field(&mut self, ui: &mut Ui) {
        if !self.filter.trim().is_empty() {
            let shown = self.shown_cards().len();
            ui.weak(format!("{shown} of {}", self.cards.len()));
        }
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .id(filter_id())
                .hint_text("Filter by name")
                .desired_width(FILTER_WIDTH)
                // As tall as the buttons beside it.
                .min_size(egui::vec2(0.0, ui.spacing().interact_size.y))
                .vertical_align(egui::Align::Center),
        );
        let button = format!("Sort: {}", self.sort.label());
        ui.menu_button(button, |ui| {
            for key in SortKey::ALL {
                if ui.selectable_label(self.sort == key, key.label()).clicked() {
                    self.sort = key;
                }
            }
        });
    }

    /// One button per favorite; no line without favorites. A click shows only
    /// that favorite's cards, a second click all again; a right click changes
    /// the favorite.
    fn favorites_line(&mut self, ui: &mut Ui) {
        if self.config.favorites.is_empty() {
            return;
        }
        enum Change {
            Toggle(usize),
            Edit(usize),
            Add(usize),
            Remove(usize),
            Delete(usize),
        }
        let marked = self.marked_repos();
        let mut change = None;
        ui.horizontal_wrapped(|ui| {
            for (i, favorite) in self.config.favorites.iter().enumerate() {
                let active = self.active_favorite == Some(i);
                let button = favorite_button(ui, favorite, active).on_hover_text(if active {
                    "Show all repositories again"
                } else {
                    "Show only this favorite's repositories"
                });
                if button.clicked() {
                    change = Some(Change::Toggle(i));
                }
                button.context_menu(|ui| {
                    ui.strong(favorite.label());
                    if ui.button("Edit\u{2026}").clicked() {
                        change = Some(Change::Edit(i));
                    }
                    let any = !marked.is_empty();
                    let add = format!("Add marked ({})", marked.len());
                    if ui.add_enabled(any, egui::Button::new(add)).clicked() {
                        change = Some(Change::Add(i));
                    }
                    let remove = format!("Remove marked ({})", marked.len());
                    if ui.add_enabled(any, egui::Button::new(remove)).clicked() {
                        change = Some(Change::Remove(i));
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        change = Some(Change::Delete(i));
                    }
                });
            }
        });
        match change {
            Some(Change::Toggle(i)) => self.toggle_favorite(i),
            Some(Change::Edit(i)) => self.edit_favorite(i),
            Some(Change::Add(i)) => self.add_to_favorite(i, &marked),
            Some(Change::Remove(i)) => self.remove_from_favorite(i, &marked),
            Some(Change::Delete(i)) => self.delete_favorite(i),
            None => {}
        }
    }

    /// The dialog that makes a favorite or changes one: name, color, emoji.
    fn favorite_dialog(&mut self, ui: &Ui) {
        let Some(dialog) = &mut self.favorite_dialog else {
            return;
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::new("favorite-dialog")).show(ui.ctx(), |ui| {
            ui.set_width(380.0);
            ui.heading(if dialog.editing.is_some() {
                "Edit favorite"
            } else {
                "New favorite"
            });
            if !dialog.repos.is_empty() {
                ui.weak(format!("with {}", repositories(dialog.repos.len())));
            }
            ui.add(
                egui::TextEdit::singleline(&mut dialog.draft.name)
                    .id(favorite_name_id())
                    .hint_text("Name")
                    .desired_width(f32::INFINITY),
            );
            ui.label("Color");
            ui.horizontal_wrapped(|ui| {
                for (name, rgb) in PALETTE {
                    let picked = dialog.draft.color == name;
                    if swatch(ui, name, rgb, picked, false).clicked() {
                        dialog.draft.color = name.to_owned();
                    }
                }
            });
            ui.label("Emoji");
            ui.horizontal_wrapped(|ui| {
                if ui
                    .selectable_label(dialog.draft.emoji.is_none(), "none")
                    .clicked()
                {
                    dialog.draft.emoji = None;
                }
                for emoji in EMOJIS {
                    let picked = dialog.draft.emoji.as_deref() == Some(emoji);
                    if ui.selectable_label(picked, emoji).clicked() {
                        dialog.draft.emoji = Some(emoji.to_owned());
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let save = egui::Button::new("Save");
                if ui.add_enabled(dialog.draft.is_valid(), save).clicked() {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        match decision {
            Some(true) => self.save_favorite(),
            Some(false) => self.favorite_dialog = None,
            None => {}
        }
    }

    fn toolbar_buttons(&mut self, ui: &mut Ui) {
        if ui
            .button(RichText::new(" + ").strong())
            .on_hover_text("Add a repository, or scan a folder for repositories")
            .clicked()
        {
            self.open_folder_picker(pick_folder);
        }
        ui.separator();
        ui.label("Mark:");
        if ui.button("all").clicked() {
            self.mark_all();
        }
        if ui.button("none").clicked() {
            self.mark_none();
        }
        if ui.button("dirty").clicked() {
            self.mark_dirty();
        }
        if ui
            .button("behind")
            .on_hover_text("Mark the repositories with unpulled commits")
            .clicked()
        {
            self.mark_behind();
        }
        if ui
            .button("ahead")
            .on_hover_text("Mark the repositories with unpushed commits")
            .clicked()
        {
            self.mark_ahead();
        }
        ui.separator();
        let marked = self.marked_repos().len();
        let any_marked = marked > 0;
        ui.label(format!("{marked} marked:"));
        let labels: Vec<String> = self
            .config
            .buttons
            .iter()
            .map(|b| b.label.clone())
            .collect();
        for (i, label) in labels.iter().enumerate() {
            if ui
                .add_enabled(any_marked, egui::Button::new(label))
                .on_hover_text("Run on all marked repositories")
                .clicked()
            {
                self.run_button_on_marked(i);
            }
        }
        ui.separator();
        if ui.button("Settings").clicked() {
            self.settings_open = true;
        }
    }

    fn board(&mut self, ui: &mut Ui) {
        if self.cards.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No repositories yet. Click + to add a repository or a folder.");
            });
            return;
        }
        if self.shown_cards().is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No repository matches the filter.");
            });
            return;
        }
        let mut actions = Vec::new();
        let gap = 10.0;
        let marked_shown = self.marked_repos();
        let kawaii = self.is_kawaii();
        // Room the Kawaii theme keeps around a card to move it in.
        let play = if kawaii {
            2.0 * (kawaii::WOBBLE + ui.spacing().item_spacing.x)
        } else {
            0.0
        };
        if kawaii {
            kawaii_background(ui, self.kawaii_clock);
            self.tend_critters();
        }
        let mut index = 0;
        // Where each card is, for the critters that sit on them.
        let mut places: Vec<(PathBuf, egui::Rect)> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Containers do not wrap inside `horizontal_wrapped`, so lay out
                // fixed rows: as many columns as fit the width.
                if kawaii {
                    // Room for a critter on the cards of the first row.
                    ui.add_space(critter::HEIGHT);
                }
                let card_width = CARD_OUTER_WIDTH + play;
                let columns =
                    (((ui.available_width() + gap) / (card_width + gap)).floor() as usize).max(1);
                for row in self.shown_cards().chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for (i, card) in row.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(gap);
                            }
                            let view = CardView {
                                card,
                                marked: self.marked.contains(&card.path),
                                busy: self.is_busy(&card.path),
                                buttons: &self.config.buttons,
                                favorites: &self.config.favorites,
                                marked_shown: &marked_shown,
                                kawaii,
                                active_favorite: self.active_favorite,
                            };
                            if kawaii {
                                let offset = self.card_offset(&card.path, index);
                                moved(ui, offset, |ui| {
                                    let shown = ui.scope(|ui| view.show(ui, &mut actions));
                                    places.push((card.path.clone(), shown.response.rect));
                                });
                            } else {
                                view.show(ui, &mut actions);
                            }
                            index += 1;
                        }
                    });
                    ui.add_space(gap);
                }
                if kawaii {
                    let asleep = self.critters.asleep(self.kawaii_clock);
                    critters(
                        ui,
                        &self.critters,
                        self.kawaii_clock,
                        asleep,
                        &places,
                        &mut actions,
                    );
                    confetti(ui, &self.bursts, self.kawaii_clock, &places);
                }
            });
        for action in actions {
            match action {
                BoardAction::Remove(p) => {
                    let targets = self.menu_targets(&p);
                    self.remove_cards(&targets);
                }
                BoardAction::Hide(p) => {
                    let targets = self.menu_targets(&p);
                    self.hide_cards(&targets);
                }
                BoardAction::Button(i, p) => self.run_button(i, &p),
                BoardAction::Terminal(p) => self.open_terminal(&p),
                BoardAction::Run(p, command) => self.start_run(&command, vec![p]),
                BoardAction::AddToFavorite(i, p) => {
                    let targets = self.menu_targets(&p);
                    self.add_to_favorite(i, &targets);
                }
                BoardAction::RemoveFromFavorite(i, p) => {
                    let targets = self.menu_targets(&p);
                    self.remove_from_favorite(i, &targets);
                }
                BoardAction::Open(p) => self.open_repo_view(&p),
                BoardAction::ToggleFavorite(i) => self.toggle_favorite(i),
                BoardAction::Pet(i) => self.pet_critter(i),
                BoardAction::NewFavorite(p) => {
                    let targets = self.menu_targets(&p);
                    self.new_favorite(targets);
                }
                BoardAction::Toggle(p) => self.toggle_mark(p),
                BoardAction::Copy(text, at) => self.copy(ui, text, at),
                BoardAction::CopyFetchError(path, at) => {
                    if let Some(output) = self.take_fetch_error(&path) {
                        self.copy(ui, output, at);
                    }
                }
            }
        }
    }

    /// Put `text` on the clipboard and start the "Copied" animation at `at`.
    pub(super) fn copy(&mut self, ui: &Ui, text: String, at: egui::Pos2) {
        ui.ctx().copy_text(text);
        self.copy_flash = Some(CopyFlash {
            at,
            started: ui.input(|i| i.time),
        });
    }

    fn cli_line(&mut self, ui: &mut Ui) {
        let values = self.repo_values();
        let cx = CliContext {
            enabled: !self.marked_repos().is_empty(),
            history: &self.history,
            vocab: &self.vocab,
            values: &values,
            auto_insert_single: self.config.auto_insert_single_match,
            shell: login_shell_name(),
        };
        let mut cli = std::mem::take(&mut self.cli);
        let unseen = self.unseen_outcome();
        let mut hidden = self.config.output_hidden;
        let actions = ui
            .horizontal(|ui| {
                output_toggle(ui, &mut hidden, unseen);
                cli.show(ui, &cx)
            })
            .inner;
        self.config.output_hidden = hidden;
        self.cli = cli;
        for action in actions {
            match action {
                CliAction::Run(line) => self.run_cli_line(&line),
                CliAction::NeedOptions(cmd) => self.request_options(&cmd),
            }
        }
    }

    fn output_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Output");
            if ui
                .add_enabled(!self.log.is_empty(), egui::Button::new("Clear"))
                .clicked()
            {
                self.log.clear();
            }
        });
        let mut opened = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for run in self.log.runs() {
                    run_ui(ui, run, &mut opened);
                }
            });
        for (run, repo) in opened {
            self.open_output(run, repo.as_deref());
        }
    }

    /// The right-click menu on a text, and what was picked in it.
    fn text_menu_overlay(&mut self, ui: &Ui) {
        match self.text_picked.take() {
            // egui copies what is marked; only the animation is left to do.
            Some(Picked::Copy(at)) => {
                self.copy_flash = Some(CopyFlash {
                    at,
                    started: ui.input(|i| i.time),
                });
            }
            Some(Picked::CopyAll(text, at)) => {
                if let Some(text) = self.text_of(&text) {
                    self.copy(ui, text, at);
                }
            }
            None => {}
        }
        // The text went away under the menu: the view closed, the panel
        // was folded away.
        let gone = self.text_menu.as_ref().is_some_and(|menu| match menu.text {
            TextOf::Message | TextOf::Pane => self.repo_view.is_none(),
            TextOf::Output(..) => self.config.output_hidden || self.repo_view.is_some(),
        });
        if gone {
            self.text_menu = None;
        }
        if let Some(menu) = &mut self.text_menu {
            text_menu::show(ui.ctx(), menu);
        }
    }

    /// "Copied" floating up from the pointer and fading out; repaints only
    /// while it runs.
    fn copy_flash_overlay(&mut self, ui: &Ui) {
        let Some(flash) = self.copy_flash else {
            return;
        };
        let Some(t) = flash_progress(flash.started, ui.input(|i| i.time)) else {
            self.copy_flash = None;
            return;
        };
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::new("copy-flash"),
        ));
        let color = ui.visuals().strong_text_color().gamma_multiply(1.0 - t);
        if self.is_kawaii() {
            // A burst of stars around the click.
            const SPARKS: usize = 6;
            for i in 0..SPARKS {
                let (x, y) = kawaii::spark(t, i, SPARKS);
                painter.text(
                    flash.at + egui::vec2(x, y),
                    egui::Align2::CENTER_CENTER,
                    STAR,
                    egui::FontId::proportional(12.0),
                    KAWAII_YELLOW.gamma_multiply(1.0 - t),
                );
            }
        }
        painter.text(
            flash.at - egui::vec2(0.0, 8.0 + COPY_FLASH_RISE * t),
            egui::Align2::CENTER_BOTTOM,
            "Copied",
            egui::TextStyle::Small.resolve(ui.style()),
            color,
        );
        ui.ctx().request_repaint();
    }

    fn toasts_overlay(&self, ui: &Ui) {
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("toasts"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 48.0))
            .show(ui.ctx(), |ui| {
                for toast in &self.toasts {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(&toast.text);
                    });
                }
            });
    }

    /// `enter` confirms, like a click on Run.
    fn confirm_dialog(&mut self, ui: &Ui, enter: bool) {
        let Some(pending) = &self.confirm else {
            return;
        };
        let names: Vec<String> = pending.repos.iter().map(|p| title(p)).collect();
        let (heading, question) = match &pending.job {
            Job::Shell(line) => (
                "Shell command",
                format!(
                    "Run `{line}` with {} in {}?",
                    login_shell_name(),
                    repositories(names.len())
                ),
            ),
            Job::Git(_) => (
                "Destructive command",
                format!(
                    "Run `{}` in {}? This can discard work.",
                    display(&pending.command),
                    repositories(names.len())
                ),
            ),
        };
        let mut decision = None;
        egui::Modal::new(egui::Id::new("confirm-destructive")).show(ui.ctx(), |ui| {
            ui.heading(heading);
            ui.label(question);
            ui.label(names.join(", "));
            ui.horizontal(|ui| {
                if ui.button("Run").clicked() || enter {
                    decision = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    decision = Some(false);
                }
            });
        });
        match decision {
            Some(true) => self.confirm_run(),
            Some(false) => self.cancel_run(),
            None => {}
        }
    }
}

/// Label of a card's terminal button.
pub const TERMINAL_LABEL: &str = ">_";

/// Mark behind "Output" while the hidden panel has an unseen run.
pub const OUTPUT_MARK: char = '\u{2022}';

/// The button that folds the output panel away and brings it back. While the
/// panel is hidden, a mark tells about the last command: running, then green
/// or red by its result.
fn output_toggle(ui: &mut Ui, hidden: &mut bool, unseen: Option<Option<bool>>) {
    let mut text = RichText::new("Output");
    let mut hover = if *hidden {
        "Show the output panel"
    } else {
        "Hide the output panel"
    };
    if let Some(outcome) = unseen.filter(|_| *hidden) {
        let (color, state) = match outcome {
            None => (ui.visuals().warn_fg_color, "Running. Show the output panel"),
            Some(true) => (ok_color(ui), "Done. Show the output panel"),
            Some(false) => (ui.visuals().error_fg_color, "Failed. Show the output panel"),
        };
        text = RichText::new(format!("Output {OUTPUT_MARK}")).color(color);
        hover = state;
    }
    if ui
        .selectable_label(!*hidden, text)
        .on_hover_text(hover)
        .clicked()
    {
        *hidden = !*hidden;
    }
}

/// Heart and star of the Kawaii theme.
pub const HEART: &str = "\u{2665}";
pub const STAR: &str = "\u{2605}";

/// The Kawaii theme's board: a pastel rainbow that drifts sideways, and
/// hearts and stars floating up it. Painted under the cards.
fn kawaii_background(ui: &Ui, clock: f64) {
    let rect = ui.clip_rect();
    let painter = ui.painter();
    const STRIPS: u32 = 24;
    let mut mesh = egui::Mesh::default();
    for i in 0..=STRIPS {
        let x = i as f32 / STRIPS as f32;
        let hue = kawaii::rainbow_hue(clock, x);
        let color: Color32 = egui::ecolor::Hsva::new(hue, 0.22, 1.0, 1.0).into();
        let at = egui::lerp(rect.left()..=rect.right(), x);
        mesh.colored_vertex(egui::pos2(at, rect.top()), color);
        mesh.colored_vertex(egui::pos2(at, rect.bottom()), color);
    }
    for i in 0..STRIPS {
        let a = 2 * i;
        mesh.add_triangle(a, a + 1, a + 2);
        mesh.add_triangle(a + 1, a + 2, a + 3);
    }
    painter.add(egui::Shape::mesh(mesh));
    for index in 0..kawaii::PARTICLES {
        let p = kawaii::particle(clock, index);
        let (glyph, color) = if p.heart {
            (HEART, KAWAII_PINK)
        } else {
            (STAR, KAWAII_YELLOW)
        };
        painter.text(
            rect.min + egui::vec2(p.x * rect.width(), p.y * rect.height()),
            egui::Align2::CENTER_CENTER,
            glyph,
            egui::FontId::proportional(p.size),
            color.gamma_multiply(0.8 * p.alpha),
        );
    }
}

/// The bursts of confetti: hearts and stars out of the top of their cards.
fn confetti(ui: &Ui, bursts: &[(PathBuf, f64)], clock: f64, places: &[(PathBuf, egui::Rect)]) {
    for (card, started) in bursts {
        let Some(t) = kawaii::burst(clock, *started) else {
            continue;
        };
        let Some((_, rect)) = places.iter().find(|(path, _)| path == card) else {
            continue;
        };
        for index in 0..kawaii::CONFETTI {
            let piece = kawaii::confetto(t, index);
            let (glyph, color) = if piece.heart {
                (HEART, KAWAII_PINK)
            } else {
                (STAR, KAWAII_YELLOW)
            };
            ui.painter().text(
                rect.center_top() + egui::vec2(piece.dx, piece.dy),
                egui::Align2::CENTER_CENTER,
                glyph,
                egui::FontId::proportional(piece.size),
                color.gamma_multiply(piece.alpha),
            );
        }
    }
}

/// The critters, each on the top edge of its card or in the air between
/// two. One under the pointer looks at it; a click on one pets it and does
/// not reach the card.
fn critters(
    ui: &mut Ui,
    troupe: &Troupe,
    clock: f64,
    asleep: bool,
    places: &[(PathBuf, egui::Rect)],
    actions: &mut Vec<BoardAction>,
) {
    let pointer = ui.input(|i| i.pointer.hover_pos());
    for (index, one) in troupe.critters.iter().enumerate() {
        // Where it stands on a card: along the top edge, each critter at a
        // place of its own so two on one card do not hide each other.
        let stand = |card: &Path| {
            let rect = places.iter().find(|(path, _)| path == card)?.1;
            let along = 26.0 + (index % 4) as f32 * 34.0;
            Some(egui::pos2(rect.left() + along, rect.top() + 2.0))
        };
        let Some(home) = stand(&one.home) else {
            continue;
        };
        let pose = critters::pose(one, clock, asleep);
        let from = stand(&one.from).unwrap_or(home);
        let feet = from.lerp(home, pose.along) - egui::vec2(0.0, pose.lift * critter::HOP_HEIGHT);
        let body = egui::Rect::from_min_max(
            feet - egui::vec2(critter::HALF_WIDTH, critter::HEIGHT),
            feet + egui::vec2(critter::HALF_WIDTH, 0.0),
        );
        let touch = ui.interact(body, ui.id().with(("critter", index)), egui::Sense::click());
        let name = one.species.label();
        touch.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        if touch.clicked() {
            actions.push(BoardAction::Pet(index));
        }
        let face = body.center();
        let look = pointer
            .map(|at| at - face)
            .filter(|to| to.length() < 120.0 && to.length() > 1.0);
        critter::paint(ui.painter(), feet, one, &pose, look);
    }
}

/// Draw `content` moved by `offset` (right, down) inside room that stays the
/// same size, so the cards around it keep their place.
fn moved(ui: &mut Ui, offset: (f32, f32), content: impl FnOnce(&mut Ui)) {
    let (dx, dy) = offset;
    ui.vertical(|ui| {
        ui.add_space(kawaii::BOB + dy);
        ui.horizontal(|ui| {
            ui.add_space(kawaii::WOBBLE + dx);
            content(ui);
            ui.add_space(kawaii::WOBBLE - dx);
        });
        ui.add_space(kawaii::BOB - dy);
    });
}

/// One card and what drawing it needs from the board.
struct CardView<'a> {
    card: &'a Card,
    marked: bool,
    busy: bool,
    buttons: &'a [crate::domain::config::ButtonDef],
    favorites: &'a [Favorite],
    /// The marked cards the board shows: what a right click on a marked
    /// card acts on.
    marked_shown: &'a [PathBuf],
    /// The Kawaii theme: a solid card over the rainbow, a ring under the
    /// pointer.
    kawaii: bool,
    active_favorite: Option<usize>,
}

impl CardView<'_> {
    fn show(&self, ui: &mut Ui, actions: &mut Vec<BoardAction>) {
        let Self {
            card,
            marked,
            busy,
            buttons,
            ..
        } = *self;
        let chips: Vec<Chip> = favorites_of(self.favorites, &card.path)
            .into_iter()
            .filter_map(|index| {
                Some(Chip {
                    index,
                    favorite: self.favorites.get(index)?,
                    active: self.active_favorite == Some(index),
                })
            })
            .collect();
        let content = CardContent {
            card,
            busy,
            buttons,
            chips: &chips,
        };
        let mut anchors = card_ui(ui, &content, marked, self.kawaii, actions);
        if let Some(whole) = anchors.first().filter(|_| self.kawaii) {
            // The card grows a little under the pointer.
            let grown = ui
                .ctx()
                .animate_bool(whole.id.with("grow"), whole.contains_pointer());
            if grown > 0.0 {
                let radius = ui.visuals().widgets.noninteractive.corner_radius;
                ui.painter().rect_stroke(
                    whole.rect.expand(2.0 * grown),
                    radius,
                    egui::Stroke::new(2.5 * grown, KAWAII_PINK),
                    egui::StrokeKind::Outside,
                );
            }
        }
        for anchor in anchors.drain(..) {
            anchor.context_menu(|ui| self.menu(ui, actions));
        }
    }

    /// The right-click menu of the card. It acts on this card, or on all
    /// marked cards when this one is marked.
    fn menu(&self, ui: &mut Ui, actions: &mut Vec<BoardAction>) {
        let path = &self.card.path;
        let own = [path.clone()];
        let targets: &[PathBuf] = if self.marked { self.marked_shown } else { &own };
        ui.strong(if targets.len() == 1 {
            title(path)
        } else {
            repositories(targets.len())
        });
        for (i, favorite) in self.favorites.iter().enumerate() {
            if ui.button(format!("Add to {}", favorite.label())).clicked() {
                actions.push(BoardAction::AddToFavorite(i, path.clone()));
            }
        }
        if ui.button("New favorite\u{2026}").clicked() {
            actions.push(BoardAction::NewFavorite(path.clone()));
        }
        let holds_a_target = |f: &Favorite| targets.iter().any(|t| f.contains(t));
        let mut separated = false;
        for (i, favorite) in self.favorites.iter().enumerate() {
            if !holds_a_target(favorite) {
                continue;
            }
            if !std::mem::replace(&mut separated, true) {
                ui.separator();
            }
            if ui
                .button(format!("Remove from {}", favorite.label()))
                .clicked()
            {
                actions.push(BoardAction::RemoveFromFavorite(i, path.clone()));
            }
        }
        ui.separator();
        if ui
            .button("Open")
            .on_hover_text("Look into this repository: changed files, commits, branches")
            .clicked()
        {
            actions.push(BoardAction::Open(path.clone()));
        }
        if ui
            .button("Hide")
            .on_hover_text("Off the board, but remembered: adding its folder again leaves it out")
            .clicked()
        {
            actions.push(BoardAction::Hide(path.clone()));
        }
        if ui
            .button("Remove from board")
            .on_hover_text("Off the board; the files stay")
            .clicked()
        {
            actions.push(BoardAction::Remove(path.clone()));
        }
    }
}

/// What a card's frame holds.
struct CardContent<'a> {
    card: &'a Card,
    busy: bool,
    buttons: &'a [crate::domain::config::ButtonDef],
    /// The card's favorites.
    chips: &'a [Chip<'a>],
}

/// A favorite as a card shows it: a small button in the favorite's color
/// that does what the favorite's toolbar button does.
struct Chip<'a> {
    /// Position of the favorite in the config.
    index: usize,
    favorite: &'a Favorite,
    /// It is the favorite whose cards alone are shown.
    active: bool,
}

/// Draws the card; the responses that carry the card's right-click menu, the
/// whole card first.
fn card_ui(
    ui: &mut Ui,
    content: &CardContent,
    marked: bool,
    solid: bool,
    actions: &mut Vec<BoardAction>,
) -> Vec<egui::Response> {
    let card = content.card;
    let mut frame = egui::Frame::group(ui.style()).inner_margin(CARD_MARGIN);
    if solid {
        // Over a background that is no plain color the text needs a card
        // that is one.
        frame = frame.fill(ui.visuals().window_fill);
    }
    if marked {
        // Marked cards get a thick selection-colored border and a tinted background.
        let selection = ui.visuals().selection.bg_fill;
        let tint = if solid {
            ui.visuals().window_fill.lerp_to_gamma(selection, 0.3)
        } else {
            selection.gamma_multiply(0.18)
        };
        frame = frame
            .stroke(egui::Stroke::new(MARKED_STROKE, selection))
            .fill(tint);
    }
    // A click on the card toggles its mark; its buttons and the texts that
    // copy themselves sit above this sense and keep their clicks. Texts are
    // not selectable: a drag to select would fight those clicks.
    let card_sense = egui::UiBuilder::new().sense(egui::Sense::click());
    let mut anchors = Vec::new();
    let response = ui.scope_builder(card_sense, |ui| {
        ui.style_mut().interaction.selectable_labels = false;
        card_frame(ui, frame, content, actions, &mut anchors);
    });
    if response.response.clicked() {
        actions.push(BoardAction::Toggle(card.path.clone()));
    }
    // The second click of a double click has flipped the mark back.
    if response.response.double_clicked() {
        actions.push(BoardAction::Open(card.path.clone()));
    }
    anchors.insert(0, response.response);
    anchors
}

fn card_frame(
    ui: &mut Ui,
    frame: egui::Frame,
    content: &CardContent,
    actions: &mut Vec<BoardAction>,
    anchors: &mut Vec<egui::Response>,
) {
    let CardContent {
        card,
        busy,
        buttons,
        chips,
    } = *content;
    let path_text = card.path.display().to_string();
    frame.show(ui, |ui| {
        ui.set_width(CARD_WIDTH);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let name = title(&card.path);
                let text = RichText::new(&name).strong().size(15.0);
                anchors.push(copyable(ui, text, name, actions).on_hover_text(&path_text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // No terminal in a folder that is gone.
                    let terminal = egui::Button::new(TERMINAL_LABEL).small();
                    if ui
                        .add_enabled(card.snapshot().is_some(), terminal)
                        .on_hover_text("Open a terminal in this repository")
                        .clicked()
                    {
                        actions.push(BoardAction::Terminal(card.path.clone()));
                    }
                });
            });
            if !chips.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                    for chip in chips {
                        let rgb = color_rgb(&chip.favorite.color);
                        let button = swatch(ui, &chip.favorite.label(), rgb, chip.active, true)
                            .on_hover_text(if chip.active {
                                "Show all repositories again"
                            } else {
                                "Show only this favorite's repositories"
                            });
                        if button.clicked() {
                            actions.push(BoardAction::ToggleFavorite(chip.index));
                        }
                        // A right click on a chip is a right click on the card.
                        anchors.push(button);
                    }
                });
            }
            let parents = parent_line(&card.path, |text| fits(ui, text, CARD_WIDTH));
            let line = copyable(ui, weak(parents), path_text.clone(), actions);
            anchors.push(line.on_hover_text(&path_text));
            let greyed = card.snapshot().is_none();
            if greyed {
                ui.multiply_opacity(0.55);
            }
            status_lines(ui, card, actions);
            if let Some(error) = &card.fetch_error {
                // One line however long git's message is; a click copies
                // all of the output and takes the error off the card.
                let red = RichText::new(error.card_text()).color(ui.visuals().error_fg_color);
                let line = egui::Label::new(red).truncate().sense(egui::Sense::click());
                let response = ui
                    .add(line)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Click to copy the whole error and remove it");
                if response.clicked() {
                    let at = response
                        .interact_pointer_pos()
                        .unwrap_or_else(|| response.rect.center());
                    actions.push(BoardAction::CopyFetchError(card.path.clone(), at));
                }
            }
            if busy {
                ui.label(RichText::new("running").color(ui.visuals().warn_fg_color));
            } else {
                match card.last_command {
                    Some(true) => {
                        ui.label(RichText::new("last command: ok").color(ok_color(ui)));
                    }
                    Some(false) => {
                        let red = ui.visuals().error_fg_color;
                        ui.label(RichText::new("last command: failed").color(red));
                    }
                    None => {}
                }
            }
            ui.horizontal_wrapped(|ui| {
                for (i, b) in buttons.iter().enumerate() {
                    if ui
                        .add_enabled(!greyed, egui::Button::new(&b.label))
                        .on_hover_text(display(&b.args))
                        .clicked()
                    {
                        actions.push(BoardAction::Button(i, card.path.clone()));
                    }
                }
            });
        });
    });
}

/// A card text that copies `copy` to the clipboard on a click (and does not
/// toggle the card); the pointer turns into a hand over it.
fn copyable(
    ui: &mut Ui,
    text: RichText,
    copy: String,
    actions: &mut Vec<BoardAction>,
) -> egui::Response {
    let response = ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if response.clicked() {
        let at = response
            .interact_pointer_pos()
            .unwrap_or_else(|| response.rect.center());
        actions.push(BoardAction::Copy(copy, at));
    }
    response
}

fn weak(text: impl Into<String>) -> RichText {
    RichText::new(text).weak()
}

/// The right-click menu of a card's branch or tag: a "Switch to" list with a
/// filter and, for branches, a field that makes a new one.
struct SwitchMenu<'a> {
    id: egui::Id,
    /// Makes the list; only called while the menu is open, not for every
    /// card in every frame.
    choices: &'a dyn Fn() -> Vec<Choice>,
    /// Shown instead of the list when there is nothing to switch to.
    none: &'a str,
    new_branch: bool,
}

impl SwitchMenu<'_> {
    /// Attach the menu to `anchor`; the git arguments of what was picked.
    fn show(&self, anchor: &egui::Response) -> Option<String> {
        if anchor.secondary_clicked() {
            // The menu opens: start with empty fields.
            anchor.ctx.data_mut(|d| {
                d.remove::<String>(self.id.with("filter"));
                d.remove::<String>(self.id.with("new"));
            });
        }
        let mut command = None;
        // Typing into the fields must not close the menu.
        egui::Popup::context_menu(anchor)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                command = self.contents(ui);
                if command.is_some() {
                    ui.close();
                }
            });
        command
    }

    fn contents(&self, ui: &mut Ui) -> Option<String> {
        let mut command = None;
        ui.set_min_width(240.0);
        ui.strong("Switch to");
        let choices = (self.choices)();
        if choices.is_empty() {
            ui.add_enabled(false, egui::Button::new(self.none));
        } else {
            let filter = text_field(ui, self.id.with("filter"), "Filter");
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for choice in matching(&choices, &filter.text) {
                        let entry = egui::Button::new(RichText::new(&choice.label).monospace());
                        if ui.add(entry.frame(false)).clicked() {
                            command = Some(choice.command.clone());
                        }
                    }
                });
        }
        if self.new_branch {
            ui.separator();
            ui.strong("Create new branch");
            ui.horizontal(|ui| {
                let create = ui.button("Create");
                let name = text_field(ui, self.id.with("new"), "name");
                if create.clicked() || name.entered {
                    command = new_branch_command(&name.text).or(command.take());
                }
            });
        }
        command
    }
}

/// A text field of a menu, kept in egui's memory under `id`.
struct MenuText {
    text: String,
    /// Enter was pressed in the field.
    entered: bool,
}

fn text_field(ui: &mut Ui, id: egui::Id, hint: &str) -> MenuText {
    let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    let entered = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    ui.data_mut(|d| d.insert_temp(id, text.clone()));
    MenuText { text, entered }
}

/// Width of `text` in the font of `style`.
fn text_width(ui: &Ui, text: &str, style: egui::TextStyle) -> f32 {
    let font = style.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, Color32::PLACEHOLDER);
    galley.size().x
}

/// Whether `text` in the body font is at most `width` wide.
fn fits(ui: &Ui, text: &str, width: f32) -> bool {
    text_width(ui, text, egui::TextStyle::Body) <= width
}

/// How many of the items with these widths fit into `room` on one line,
/// `separator` wide gaps between them and an `ellipsis` after them when some
/// are left out. The first always shows.
fn fitting(widths: &[f32], separator: f32, ellipsis: f32, room: f32) -> usize {
    let mut used = 0.0;
    let mut count = 0;
    for (i, width) in widths.iter().enumerate() {
        used += width + if i > 0 { separator } else { 0.0 };
        let rest = if i + 1 < widths.len() { ellipsis } else { 0.0 };
        if i > 0 && used + rest > room {
            break;
        }
        count = i + 1;
    }
    count
}

/// A card text that is no value but carries a right-click menu: a click on
/// it still marks the card.
fn menu_anchor(
    ui: &mut Ui,
    text: RichText,
    path: &Path,
    actions: &mut Vec<BoardAction>,
) -> egui::Response {
    let response = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
    if response.clicked() {
        actions.push(BoardAction::Toggle(path.to_path_buf()));
    }
    response
}

/// The card's tag line: every tag of the tagged commit, each copying itself
/// on a click, on one line; the tooltip has them all. A right click anywhere
/// on the line switches this one repository to a tag.
fn tag_line(ui: &mut Ui, path: &Path, snap: &Snapshot, actions: &mut Vec<BoardAction>) {
    let menu = SwitchMenu {
        id: egui::Id::new(("tag-menu", path)),
        choices: &|| tag_choices(&snap.dated),
        none: "no tags",
        new_branch: false,
    };
    let mut anchors = Vec::new();
    tag_texts(ui, path, &snap.tags, actions, &mut anchors);
    for anchor in &anchors {
        if let Some(command) = menu.show(anchor) {
            actions.push(BoardAction::Run(path.to_path_buf(), command));
        }
    }
}

/// The texts of the tag line; every one of them goes to `anchors`.
fn tag_texts(
    ui: &mut Ui,
    path: &Path,
    tags: &[String],
    actions: &mut Vec<BoardAction>,
    anchors: &mut Vec<egui::Response>,
) {
    if tags.is_empty() {
        anchors.push(menu_anchor(ui, weak("no tag"), path, actions));
        return;
    }
    anchors.push(menu_anchor(ui, weak("tag"), path, actions));
    let mono = egui::TextStyle::Monospace;
    let widths: Vec<f32> = tags
        .iter()
        .map(|t| text_width(ui, t, mono.clone()))
        .collect();
    let separator = text_width(ui, ", ", mono.clone());
    let ellipsis = text_width(ui, ", \u{2026}", mono);
    let shown = fitting(&widths, separator, ellipsis, ui.available_width());
    let all = tags.join(", ");
    // The commas sit right behind their tag.
    ui.spacing_mut().item_spacing.x = 0.0;
    for (i, tag) in tags.iter().take(shown).enumerate() {
        if i > 0 {
            ui.label(RichText::new(", ").monospace());
        }
        let text = RichText::new(tag).monospace();
        anchors.push(copyable(ui, text, tag.clone(), actions).on_hover_text(&all));
    }
    if shown < tags.len() {
        let more = RichText::new(", \u{2026}").monospace();
        anchors.push(menu_anchor(ui, more, path, actions).on_hover_text(&all));
    }
}

/// The right-click menu of a card's remote line: the remotes of the
/// repository. Picking one makes the branch track its namesake there; a
/// remote without that branch cannot be picked.
fn remote_menu(
    anchor: &egui::Response,
    path: &Path,
    snap: &Snapshot,
    actions: &mut Vec<BoardAction>,
) {
    egui::Popup::context_menu(anchor).show(|ui| {
        ui.set_min_width(240.0);
        let branch = match &snap.status.head {
            Head::Branch(branch) => Some(branch.as_str()),
            Head::Unborn(_) | Head::Detached(_) => None,
        };
        ui.strong(match branch {
            Some(branch) => format!("Remote of {branch}"),
            None => "Remotes".to_owned(),
        });
        let tracking = snap.status.tracking.as_deref();
        let tracked = tracking
            .and_then(|upstream| remote_of(&snap.remotes, upstream))
            .map(|remote| remote.name.as_str());
        for choice in remote_choices(branch, tracked, &snap.remotes, &snap.dated) {
            let text = RichText::new(&choice.label).monospace();
            match choice.command {
                Some(command) => {
                    if ui.add(egui::Button::new(text).frame(false)).clicked() {
                        actions.push(BoardAction::Run(path.to_path_buf(), command));
                        ui.close();
                    }
                }
                None => {
                    let why = match (choice.current, branch) {
                        (true, _) => "The branch tracks this remote".to_owned(),
                        (false, Some(branch)) => format!("This remote has no branch {branch}"),
                        (false, None) => "No branch is checked out".to_owned(),
                    };
                    let entry = egui::Button::selectable(choice.current, text);
                    ui.add_enabled(false, entry).on_disabled_hover_text(why);
                }
            }
        }
    });
}

/// The card's remote line: the remote the branch tracks, its URL written
/// short with the protocol first; a click copies the URL as git has it, a
/// right click lists the remotes to switch to. A second line only when the
/// branch pushes to another remote.
fn remote_lines(ui: &mut Ui, path: &Path, snap: &Snapshot, actions: &mut Vec<BoardAction>) {
    ui.horizontal(|ui| {
        let Some(remote) = snap.remote() else {
            if snap.remotes.is_empty() {
                menu_anchor(ui, weak("no remote"), path, actions);
            } else {
                let none = menu_anchor(ui, weak("no remote for this branch"), path, actions);
                remote_menu(&none, path, snap, actions);
            }
            return;
        };
        let word = menu_anchor(ui, weak("remote"), path, actions);
        remote_menu(&word, path, snap, actions);
        let text = RichText::new(short_url(&remote.url)).monospace();
        let line = egui::Label::new(text)
            .truncate()
            .show_tooltip_when_elided(false)
            .sense(egui::Sense::click());
        let hover = format!("{}: {}", remote.name, safe_url(&remote.url));
        let url = ui
            .add(line)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(hover);
        if url.clicked() {
            let at = url
                .interact_pointer_pos()
                .unwrap_or_else(|| url.rect.center());
            actions.push(BoardAction::Copy(remote.url.clone(), at));
        }
        remote_menu(&url, path, snap, actions);
    });
    if let Some(push) = snap.push_remote() {
        ui.horizontal(|ui| {
            let hover = format!(
                "This branch pushes to {}: {}",
                push.name,
                safe_url(&push.url)
            );
            let warn = ui.visuals().warn_fg_color;
            menu_anchor(ui, weak("push:"), path, actions).on_hover_text(&hover);
            let name = RichText::new(&push.name).monospace().color(warn);
            menu_anchor(ui, name, path, actions).on_hover_text(&hover);
        });
    }
}

/// The status of a card. Only names a user pastes somewhere copy themselves
/// (branch, sha, tag); a click on anything else reaches the card.
fn status_lines(ui: &mut Ui, card: &Card, actions: &mut Vec<BoardAction>) {
    let snap = match &card.probe {
        None => {
            ui.label(weak("loading"));
            return;
        }
        Some(Err(e)) => {
            ui.label(RichText::new(e.to_string()).color(ui.visuals().error_fg_color));
            return;
        }
        Some(Ok(snap)) => snap,
    };
    let warn = ui.visuals().warn_fg_color;
    ui.horizontal(|ui| {
        ui.label(weak("branch"));
        let head = match &snap.status.head {
            Head::Branch(b) => copyable(ui, RichText::new(b).monospace(), b.clone(), actions),
            Head::Unborn(b) => {
                let name = copyable(ui, RichText::new(b).monospace(), b.clone(), actions);
                ui.label(RichText::new("(no commits)").monospace());
                name
            }
            Head::Detached(sha) => {
                ui.label(RichText::new("detached @").monospace());
                copyable(ui, RichText::new(sha).monospace(), sha.clone(), actions)
            }
        };
        // A right click on the branch switches this one repository.
        let menu = SwitchMenu {
            id: egui::Id::new(("branch-menu", &card.path)),
            choices: &|| branch_choices(&snap.dated, &snap.refs.remotes),
            none: "no other branch",
            new_branch: true,
        };
        if let Some(command) = menu.show(&head) {
            actions.push(BoardAction::Run(card.path.clone(), command));
        }
        match snap.state {
            RepoState::Rebasing => {
                ui.label(RichText::new("rebasing").color(warn));
            }
            RepoState::Merging => {
                ui.label(RichText::new("merging").color(warn));
            }
            RepoState::Normal => {}
        }
    });
    ui.horizontal(|ui| tag_line(ui, &card.path, snap, actions));
    if let Some(commit) = &snap.commit {
        ui.horizontal(|ui| {
            ui.label(weak("commit"));
            let text = format!("{} {}", commit.short, commit.subject);
            // A click copies the whole hash, not the short one shown. The
            // tooltip is the subject; the one a cut-off label brings along
            // would say it a second time.
            let line = egui::Label::new(text)
                .truncate()
                .show_tooltip_when_elided(false)
                .sense(egui::Sense::click());
            let response = ui
                .add(line)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(&commit.subject);
            if response.clicked() {
                let at = response
                    .interact_pointer_pos()
                    .unwrap_or_else(|| response.rect.center());
                actions.push(BoardAction::Copy(commit.hash.clone(), at));
            }
        });
    }
    let c = snap.status.counts;
    ui.horizontal(|ui| {
        ui.label(weak("status"));
        if c.is_clean() {
            ui.label(RichText::new("clean").color(ok_color(ui)));
        } else {
            let counts = format!(
                "+{} ~{} ?{} !{}",
                c.staged, c.modified, c.untracked, c.conflicts
            );
            ui.label(RichText::new(counts).monospace())
                .on_hover_text("+staged ~modified ?untracked !conflicts");
        }
    });
    remote_lines(ui, &card.path, snap, actions);
    ui.horizontal(|ui| match snap.status.upstream {
        Upstream::None => {
            ui.label(weak("no upstream"));
        }
        Upstream::Gone => {
            ui.label(RichText::new("upstream gone").color(warn));
        }
        Upstream::Tracking { ahead, behind } => {
            ui.label(weak("unpushed"));
            ui.label(RichText::new(ahead.to_string()).strong());
            ui.label(weak("unpulled"));
            ui.label(RichText::new(behind.to_string()).strong());
        }
    });
}

/// A run or one of its sections the user wants in an editor.
type OpenOutput = (u64, Option<PathBuf>);

/// The button that opens output in the system's editor; disabled until the
/// output is complete.
fn open_in_editor(ui: &mut Ui, done: bool, what: &str) -> bool {
    ui.add_enabled(done, egui::Button::new("Open in editor").small())
        .on_hover_text(format!("Open {what} as a text file"))
        .on_disabled_hover_text("Not done yet")
        .clicked()
}

fn run_ui(ui: &mut Ui, run: &Run, opened: &mut Vec<OpenOutput>) {
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(RichText::new(display(&run.command)).monospace().strong());
        if open_in_editor(ui, run.is_done(None), "the output of all repositories") {
            opened.push((run.id, None));
        }
    });
    for section in &run.sections {
        let color = match &section.state {
            SectionState::Queued => ui.visuals().weak_text_color(),
            SectionState::Running => ui.visuals().warn_fg_color,
            SectionState::Done(out) if out.success() => ok_color(ui),
            SectionState::Done(_) | SectionState::SpawnFailed(_) => ui.visuals().error_fg_color,
        };
        let status = section.status();
        let header = RichText::new(format!("{}  {status}", title(&section.repo))).color(color);
        egui::CollapsingHeader::new(header)
            .id_salt((run.id, &section.repo))
            .default_open(true)
            .show(ui, |ui| {
                let done = section.state.outcome().is_some();
                if open_in_editor(ui, done, "this repository's output") {
                    opened.push((run.id, Some(section.repo.clone())));
                }
                let shown = ui.scope(|ui| section_output(ui, &section.state));
                let text = TextOf::Output(run.id, section.repo.clone());
                text_menu::area(ui, shown.response.rect, text);
            });
    }
}

fn section_output(ui: &mut Ui, state: &SectionState) {
    match state {
        SectionState::Done(out) => {
            if !out.stdout.is_empty() {
                ui.label(RichText::new(out.stdout.trim_end()).monospace());
            }
            if !out.stderr.is_empty() {
                ui.label(RichText::new(out.stderr.trim_end()).monospace().weak());
            }
        }
        SectionState::SpawnFailed(e) => {
            ui.label(RichText::new(e).monospace());
        }
        SectionState::Queued | SectionState::Running => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_many_items_as_fit_show_and_the_first_always() {
        let widths = [30.0, 30.0, 30.0];
        assert_eq!(fitting(&widths, 5.0, 10.0, 100.0), 3);
        // Two and the ellipsis need 75; three need 100.
        assert_eq!(fitting(&widths, 5.0, 10.0, 99.0), 2);
        assert_eq!(fitting(&widths, 5.0, 10.0, 74.0), 1);
        assert_eq!(fitting(&widths, 5.0, 10.0, 0.0), 1);
        assert_eq!(fitting(&[], 5.0, 10.0, 100.0), 0);
    }

    #[test]
    fn copy_flash_runs_for_its_duration_then_ends() {
        assert_eq!(flash_progress(10.0, 10.0), Some(0.0));
        let half = flash_progress(10.0, 10.3).unwrap();
        assert!((half - 0.5).abs() < 1e-3, "{half}");
        assert_eq!(flash_progress(10.0, 10.7), None);
        assert_eq!(flash_progress(10.0, 9.0), None);
    }
}
