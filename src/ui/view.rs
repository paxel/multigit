//! Drawing of the board: toolbar, cards, output panel, CLI, toasts, dialogs.

use crate::domain::command::{Job, display, shell_name, shell_program};
use crate::domain::naming::{subtitles, title};
use crate::domain::output::{Run, SectionState};
use crate::domain::status::{Head, RepoState, Upstream};
use crate::ui::app::{App, Card, CopyFlash};
use crate::ui::cli::{CliAction, CliContext};
use crate::ui::settings;
use egui::{Color32, RichText, Ui};
use std::path::PathBuf;

/// Content width of one card.
const CARD_WIDTH: f32 = 290.0;
/// Inner margin of a card frame.
const CARD_MARGIN: f32 = 8.0;
/// Border width of a marked card.
const MARKED_STROKE: f32 = 3.0;
/// Width a card takes on the board: content, margins and the widest frame stroke.
const CARD_OUTER_WIDTH: f32 = CARD_WIDTH + 2.0 * CARD_MARGIN + 2.0 * MARKED_STROKE;

/// Something the user clicked that changes the board; applied after drawing.
enum BoardAction {
    Remove(PathBuf),
    Button(usize, PathBuf),
    Toggle(PathBuf),
    /// A card text was clicked at this pointer position.
    Copy(String, egui::Pos2),
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

fn ok_color(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x6c, 0xc6, 0x6c)
    } else {
        Color32::from_rgb(0x1a, 0x7f, 0x37)
    }
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
        // Taken before the command line sees the key, which keeps focus under the dialog.
        let enter_confirms = self.confirm.is_some() && take_fresh_enter(ui);
        if let Some(folder) = self.take_picked_folder() {
            self.add_folder(&folder);
        }
        let mut next = self.tick(std::time::Instant::now());
        if !self.toasts.is_empty() {
            // Repaint soon enough to take expired toasts down.
            next = next.min(std::time::Duration::from_millis(500));
        }
        ui.ctx()
            .request_repaint_after(next.max(std::time::Duration::from_millis(200)));

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("cli").show(ui, |ui| {
            ui.add_space(4.0);
            self.cli_line(ui);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("output")
            .resizable(true)
            .default_size(220.0)
            .min_size(60.0)
            .show(ui, |ui| self.output_panel(ui));
        egui::CentralPanel::default().show(ui, |ui| self.board(ui));

        self.toasts_overlay(ui);
        self.copy_flash_overlay(ui);
        self.confirm_dialog(ui, enter_confirms);
        settings::show(self, ui.ctx());
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
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
            if ui.button("behind").clicked() {
                self.mark_behind();
            }
            ui.separator();
            let any_marked = !self.marked.is_empty();
            ui.label(format!("{} marked:", self.marked.len()));
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
            if ui
                .add_enabled(any_marked, egui::Button::new("Remove marked"))
                .on_hover_text("Take the marked repositories off the board (files stay)")
                .clicked()
            {
                self.remove_marked();
            }
            ui.separator();
            if ui.button("Settings").clicked() {
                self.settings_open = true;
            }
        });
        if let Some(error) = &self.config_error {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Config not loaded, changes are not saved: {error}"),
            );
        }
    }

    fn board(&mut self, ui: &mut Ui) {
        if self.cards.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.weak("No repositories yet. Click + to add a repository or a folder.");
            });
            return;
        }
        let paths: Vec<PathBuf> = self.cards.iter().map(|c| c.path.clone()).collect();
        let subs = subtitles(&paths);
        let mut actions = Vec::new();
        let gap = 10.0;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Containers do not wrap inside `horizontal_wrapped`, so lay out
                // fixed rows: as many columns as fit the width.
                let columns = (((ui.available_width() + gap) / (CARD_OUTER_WIDTH + gap)).floor()
                    as usize)
                    .max(1);
                let rows = self.cards.iter().zip(&subs).collect::<Vec<_>>();
                for row in rows.chunks(columns) {
                    ui.horizontal_top(|ui| {
                        for (i, (card, sub)) in row.iter().enumerate() {
                            if i > 0 {
                                ui.add_space(gap);
                            }
                            let marked = self.marked.contains(&card.path);
                            let busy = self.is_busy(&card.path);
                            card_ui(
                                ui,
                                card,
                                sub.as_deref(),
                                marked,
                                busy,
                                &self.config.buttons,
                                &mut actions,
                            );
                        }
                    });
                    ui.add_space(gap);
                }
            });
        for action in actions {
            match action {
                BoardAction::Remove(p) => self.remove(&p),
                BoardAction::Button(i, p) => self.run_button(i, &p),
                BoardAction::Toggle(p) => {
                    if !self.marked.remove(&p) {
                        self.marked.insert(p);
                    }
                }
                BoardAction::Copy(text, at) => {
                    ui.ctx().copy_text(text);
                    self.copy_flash = Some(CopyFlash {
                        at,
                        started: ui.input(|i| i.time),
                    });
                }
            }
        }
    }

    fn cli_line(&mut self, ui: &mut Ui) {
        let values = self.repo_values();
        let cx = CliContext {
            enabled: !self.marked.is_empty(),
            history: &self.history,
            vocab: &self.vocab,
            values: &values,
            auto_insert_single: self.config.auto_insert_single_match,
            shell: login_shell_name(),
        };
        let mut cli = std::mem::take(&mut self.cli);
        let actions = cli.show(ui, &cx);
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for run in self.log.runs() {
                    run_ui(ui, run);
                }
            });
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

fn card_ui(
    ui: &mut Ui,
    card: &Card,
    subtitle: Option<&str>,
    marked: bool,
    busy: bool,
    buttons: &[crate::domain::config::ButtonDef],
    actions: &mut Vec<BoardAction>,
) {
    let mut frame = egui::Frame::group(ui.style()).inner_margin(CARD_MARGIN);
    if marked {
        // Marked cards get a thick selection-colored border and a tinted background.
        let selection = ui.visuals().selection.bg_fill;
        frame = frame
            .stroke(egui::Stroke::new(MARKED_STROKE, selection))
            .fill(selection.gamma_multiply(0.18));
    }
    // The card's empty space toggles its mark; its buttons and texts (which
    // copy themselves) sit above this sense and keep their clicks. Texts are
    // not selectable: a drag to select would fight the click that copies.
    let card_sense = egui::UiBuilder::new().sense(egui::Sense::click());
    let response = ui.scope_builder(card_sense, |ui| {
        ui.style_mut().interaction.selectable_labels = false;
        card_frame(ui, frame, card, subtitle, busy, buttons, actions);
    });
    if response.response.clicked() {
        actions.push(BoardAction::Toggle(card.path.clone()));
    }
}

fn card_frame(
    ui: &mut Ui,
    frame: egui::Frame,
    card: &Card,
    subtitle: Option<&str>,
    busy: bool,
    buttons: &[crate::domain::config::ButtonDef],
    actions: &mut Vec<BoardAction>,
) {
    let path_text = card.path.display().to_string();
    frame.show(ui, |ui| {
        ui.set_width(CARD_WIDTH);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let name = RichText::new(title(&card.path)).strong().size(15.0);
                copyable(ui, name, actions).on_hover_text(&path_text);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove from board (files stay)")
                        .clicked()
                    {
                        actions.push(BoardAction::Remove(card.path.clone()));
                    }
                });
            });
            if let Some(sub) = subtitle {
                copyable(ui, RichText::new(sub).weak(), actions).on_hover_text(&path_text);
            }
            let greyed = card.snapshot().is_none();
            if greyed {
                ui.multiply_opacity(0.55);
            }
            status_lines(ui, card, actions);
            if let Some(error) = &card.fetch_error {
                let error =
                    RichText::new(format!("fetch: {error}")).color(ui.visuals().error_fg_color);
                copyable(ui, error, actions);
            }
            if busy {
                let running = RichText::new("running").color(ui.visuals().warn_fg_color);
                copyable(ui, running, actions);
            } else {
                match card.last_command {
                    Some(true) => {
                        let ok = RichText::new("last command: ok").color(ok_color(ui));
                        copyable(ui, ok, actions);
                    }
                    Some(false) => {
                        let failed = RichText::new("last command: failed")
                            .color(ui.visuals().error_fg_color);
                        copyable(ui, failed, actions);
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

/// A card text: a click copies it to the clipboard (and does not toggle the
/// card); the pointer turns into a hand over it.
fn copyable(ui: &mut Ui, text: RichText, actions: &mut Vec<BoardAction>) -> egui::Response {
    let copy = text.text().to_owned();
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

fn status_lines(ui: &mut Ui, card: &Card, actions: &mut Vec<BoardAction>) {
    let snap = match &card.probe {
        None => {
            copyable(ui, weak("loading"), actions);
            return;
        }
        Some(Err(e)) => {
            let error = RichText::new(e.to_string()).color(ui.visuals().error_fg_color);
            copyable(ui, error, actions);
            return;
        }
        Some(Ok(snap)) => snap,
    };
    let branch = match &snap.status.head {
        Head::Branch(b) => b.clone(),
        Head::Unborn(b) => format!("{b} (no commits)"),
        Head::Detached(sha) => format!("detached @ {sha}"),
    };
    let warn = ui.visuals().warn_fg_color;
    ui.horizontal(|ui| {
        copyable(ui, weak("branch"), actions);
        copyable(ui, RichText::new(branch).monospace(), actions);
        match snap.state {
            RepoState::Rebasing => {
                copyable(ui, RichText::new("rebasing").color(warn), actions);
            }
            RepoState::Merging => {
                copyable(ui, RichText::new("merging").color(warn), actions);
            }
            RepoState::Normal => {}
        }
    });
    ui.horizontal(|ui| match &snap.tag {
        Some(tag) => {
            copyable(ui, weak("tag"), actions);
            copyable(ui, RichText::new(tag).monospace(), actions);
        }
        None => {
            copyable(ui, weak("no tag"), actions);
        }
    });
    let c = snap.status.counts;
    ui.horizontal(|ui| {
        copyable(ui, weak("status"), actions);
        if c.is_clean() {
            copyable(ui, RichText::new("clean").color(ok_color(ui)), actions);
        } else {
            let counts = format!(
                "+{} ~{} ?{} !{}",
                c.staged, c.modified, c.untracked, c.conflicts
            );
            copyable(ui, RichText::new(counts).monospace(), actions)
                .on_hover_text("+staged ~modified ?untracked !conflicts");
        }
    });
    ui.horizontal(|ui| match snap.status.upstream {
        Upstream::None => {
            copyable(ui, weak("no upstream"), actions);
        }
        Upstream::Gone => {
            copyable(ui, RichText::new("upstream gone").color(warn), actions);
        }
        Upstream::Tracking { ahead, behind } => {
            copyable(ui, weak("unpushed"), actions);
            copyable(ui, RichText::new(ahead.to_string()).strong(), actions);
            copyable(ui, weak("unpulled"), actions);
            copyable(ui, RichText::new(behind.to_string()).strong(), actions);
        }
    });
}

fn run_ui(ui: &mut Ui, run: &Run) {
    ui.separator();
    ui.label(RichText::new(display(&run.command)).monospace().strong());
    for section in &run.sections {
        let (status, color) = match &section.state {
            SectionState::Queued => ("queued".to_owned(), ui.visuals().weak_text_color()),
            SectionState::Running => ("running".to_owned(), ui.visuals().warn_fg_color),
            SectionState::Done(out) if out.success() => ("exit 0".to_owned(), ok_color(ui)),
            SectionState::Done(out) => (
                out.exit_code
                    .map_or("killed".to_owned(), |c| format!("exit {c}")),
                ui.visuals().error_fg_color,
            ),
            SectionState::SpawnFailed(_) => {
                ("failed to start".to_owned(), ui.visuals().error_fg_color)
            }
        };
        let header = RichText::new(format!("{}  {status}", title(&section.repo))).color(color);
        egui::CollapsingHeader::new(header)
            .id_salt((run.id, &section.repo))
            .default_open(true)
            .show(ui, |ui| match &section.state {
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
                _ => {}
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_flash_runs_for_its_duration_then_ends() {
        assert_eq!(flash_progress(10.0, 10.0), Some(0.0));
        let half = flash_progress(10.0, 10.3).unwrap();
        assert!((half - 0.5).abs() < 1e-3, "{half}");
        assert_eq!(flash_progress(10.0, 10.7), None);
        assert_eq!(flash_progress(10.0, 9.0), None);
    }
}
