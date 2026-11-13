//! The read-only view of one repository, over the board: three tabs
//! (Files, Commits, Graph), a list on the left and what is picked in it on
//! the right, and Back and Forward through what was looked at.

use crate::domain::details::{
    ChangeKind, Details, DiffTarget, LineKind, RepoTab, file_lines, line_kind, tree_rows,
};
use crate::domain::naming::title;
use crate::ui::app::{App, RepoView};
use crate::ui::view::ok_color;
use egui::{Color32, RichText, Ui};

/// What was clicked in the repository view.
enum Act {
    Close,
    Refresh,
    Back,
    Forward,
    Tab(RepoTab),
    /// Fold or unfold the folder with this path.
    Fold(String),
    Select(DiffTarget),
}

impl App {
    pub(super) fn repo_view_dialog(&mut self, ui: &Ui) {
        let Some(view) = &self.repo_view else {
            return;
        };
        let mut act = None;
        let screen = ui.ctx().content_rect();
        egui::Modal::new(egui::Id::new("repo-view")).show(ui.ctx(), |ui| {
            ui.set_width((screen.width() - 80.0).max(400.0));
            ui.set_height((screen.height() - 80.0).max(300.0));
            ui.horizontal(|ui| {
                ui.heading(title(&view.repo));
                ui.weak(view.repo.display().to_string());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        act = Some(Act::Close);
                    }
                    if ui
                        .button("Refresh")
                        .on_hover_text("Read the repository again now")
                        .clicked()
                    {
                        act = Some(Act::Refresh);
                    }
                });
            });
            ui.horizontal(|ui| navigation(ui, view, &mut act));
            ui.separator();
            match &view.details {
                None => {
                    ui.weak("loading");
                }
                Some(Err(error)) => {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                Some(Ok(details)) => {
                    egui::Panel::left("repo-view-lists")
                        .resizable(true)
                        .default_size(380.0)
                        .show(ui, |ui| match view.tab {
                            RepoTab::Files => file_list(ui, details, view, &mut act),
                            RepoTab::Commits => commit_list(ui, details, view, &mut act),
                            RepoTab::Graph => branch_list(ui, details),
                        });
                    egui::CentralPanel::default().show(ui, |ui| diff_pane(ui, view));
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                act = Some(Act::Close);
            }
        });
        match act {
            Some(Act::Close) => self.close_repo_view(),
            Some(Act::Refresh) => self.refresh_repo_view(),
            Some(Act::Back) => self.repo_view_back(),
            Some(Act::Forward) => self.repo_view_forward(),
            Some(Act::Tab(tab)) => self.switch_repo_tab(tab),
            Some(Act::Fold(path)) => self.toggle_repo_folder(&path),
            Some(Act::Select(target)) => self.select_in_repo_view(target),
            None => {}
        }
    }
}

/// Back and Forward, then the three tabs.
fn navigation(ui: &mut Ui, view: &RepoView, act: &mut Option<Act>) {
    let back = ui
        .add_enabled(view.trail.can_back(), egui::Button::new("Back"))
        .on_hover_text("What was looked at before");
    if back.clicked() {
        *act = Some(Act::Back);
    }
    let forward = ui
        .add_enabled(view.trail.can_forward(), egui::Button::new("Forward"))
        .on_hover_text("Where Back was pressed");
    if forward.clicked() {
        *act = Some(Act::Forward);
    }
    ui.separator();
    for tab in RepoTab::ALL {
        if ui
            .add(egui::Button::selectable(view.tab == tab, tab.label()))
            .clicked()
        {
            *act = Some(Act::Tab(tab));
        }
    }
}

/// A list of the view under its heading, scrolling on its own. Each tab's
/// list keeps its own scroll position.
fn list(ui: &mut Ui, heading: &str, tab: RepoTab, rows: impl FnOnce(&mut Ui)) {
    ui.strong(heading);
    egui::ScrollArea::vertical()
        .id_salt(tab)
        .auto_shrink([false, false])
        .show(ui, rows);
}

/// A row of a list; a click picks its target for the right pane.
fn row(
    ui: &mut Ui,
    view: &RepoView,
    target: DiffTarget,
    text: RichText,
    hover: &str,
    act: &mut Option<Act>,
) {
    let picked = view.selected.as_ref() == Some(&target);
    let entry = egui::Button::selectable(picked, text).truncate();
    if ui.add(entry).on_hover_text(hover).clicked() {
        *act = Some(Act::Select(target));
    }
}

/// The color that says how a file changed.
pub(super) fn change_color(ui: &Ui, kind: ChangeKind) -> Color32 {
    match kind {
        ChangeKind::Added => ok_color(ui),
        ChangeKind::Modified => ui.visuals().warn_fg_color,
        ChangeKind::Deleted | ChangeKind::Conflict => ui.visuals().error_fg_color,
    }
}

/// How far a row of the tree is moved in for each level.
const INDENT: f32 = 14.0;

/// The Files tab: the changed files as a tree. A folder folds on a click;
/// a file is picked for the right pane.
fn file_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    let heading = format!("Changed files ({})", details.files.len());
    list(ui, &heading, RepoTab::Files, |ui| {
        if details.files.is_empty() {
            ui.weak("none: the working tree is clean");
        }
        for entry in tree_rows(&details.files, &view.closed) {
            ui.horizontal(|ui| {
                ui.add_space(entry.depth as f32 * INDENT);
                let Some(file) = entry.file else {
                    let size = egui::vec2(INDENT, ui.spacing().interact_size.y);
                    let (_, icon) = ui.allocate_exact_size(size, egui::Sense::click());
                    let openness = if entry.open { 1.0 } else { 0.0 };
                    egui::collapsing_header::paint_default_icon(ui, openness, &icon);
                    let name = egui::Button::new(format!("{}/", entry.name)).frame(false);
                    let hover = if entry.open { "Fold" } else { "Unfold" };
                    if ui.add(name).on_hover_text(hover).clicked() || icon.clicked() {
                        *act = Some(Act::Fold(entry.path));
                    }
                    return;
                };
                ui.add_space(INDENT);
                let color = change_color(ui, file.kind);
                let text = format!("{} {}", file.kind.letter(), entry.name);
                let hover = format!("{}: {}", file.kind.label(), file.path);
                let target = DiffTarget::File(file);
                row(
                    ui,
                    view,
                    target,
                    RichText::new(text).color(color),
                    &hover,
                    act,
                );
            });
        }
    });
}

/// The Commits tab: the history of the checked-out branch.
fn commit_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    list(ui, "Commits", RepoTab::Commits, |ui| {
        if details.commits.is_empty() {
            ui.weak("no commits yet");
        }
        for commit in &details.commits {
            let text = format!("{}  {}", commit.short, commit.subject);
            let hover = format!("{}, {}\n{}", commit.author, commit.age, commit.subject);
            let target = DiffTarget::Commit(commit.hash.clone());
            row(
                ui,
                view,
                target,
                RichText::new(text).monospace(),
                &hover,
                act,
            );
        }
    });
}

/// The Graph tab: the branches.
fn branch_list(ui: &mut Ui, details: &Details) {
    list(ui, "Branches", RepoTab::Graph, |ui| {
        for branch in &details.branches {
            let name = RichText::new(&branch.name).monospace();
            if branch.current {
                ui.label(name.strong()).on_hover_text("checked out");
            } else {
                ui.label(name);
            }
        }
    });
}

/// What is picked, on the right: the content of an added file, what is
/// gone of a deleted one, and the diff of any other file or of a commit.
fn diff_pane(ui: &mut Ui, view: &RepoView) {
    let (target, diff) = match (&view.selected, &view.diff) {
        (None, _) => {
            ui.weak(match view.tab {
                RepoTab::Files => "Pick a file to see its changes.",
                RepoTab::Commits | RepoTab::Graph => "Pick a commit to see it.",
            });
            return;
        }
        (Some(_), None) => {
            ui.weak("loading");
            return;
        }
        (Some(_), Some(Err(error))) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return;
        }
        (Some(target), Some(Ok(diff))) => (target, diff),
    };
    let lines = match target {
        DiffTarget::File(file) => {
            ui.horizontal(|ui| {
                let color = change_color(ui, file.kind);
                ui.label(RichText::new(file.kind.label()).color(color));
                ui.label(RichText::new(&file.path).monospace().strong());
            });
            ui.separator();
            file_lines(file.kind, diff)
        }
        DiffTarget::Commit(_) => diff.lines().map(|l| (line_kind(l), l)).collect(),
    };
    if lines.is_empty() {
        ui.weak(match target {
            DiffTarget::File(file) if file.kind == ChangeKind::Added => "empty file",
            _ => "no difference",
        });
        return;
    }
    diff_lines(ui, "repo-view-diff", &lines);
}

/// Lines of a diff or of a file, colored by kind, scrolling both ways.
fn diff_lines(ui: &mut Ui, id: &str, lines: &[(LineKind, &str)]) {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let (added, removed) = (ok_color(ui), ui.visuals().error_fg_color);
    egui::ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show_rows(ui, row_height, lines.len(), |ui, rows| {
            // Rows keep their width: a long line must not wrap.
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for (kind, line) in lines.get(rows).unwrap_or_default() {
                let text = RichText::new(*line).monospace();
                ui.label(match kind {
                    LineKind::Added => text.color(added),
                    LineKind::Removed => text.color(removed),
                    LineKind::Header => text.strong(),
                    LineKind::Plain => text,
                });
            }
        });
}
