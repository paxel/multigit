//! The read-only view of one repository, over the board: three tabs
//! (Files, Commits, Graph), a list on the left and what is picked in it on
//! the right, and Back and Forward through what was looked at.

use crate::app::{CommitRow, commit_rows};
use crate::domain::checkout::{Checkout, checkouts};
use crate::domain::details::{
    ChangeKind, Details, DiffTarget, LineKind, LogEntry, RepoTab, author_hue, file_lines,
    file_rows, initials, parse_commit, short_age,
};
use crate::domain::graph::{GraphData, GraphRow, LabelKind, RowKind, graph_rows, labels};
use crate::domain::naming::title;
use crate::domain::staging::{FileAct, FileState, Staged, StashAct, files_of, staged};
use crate::ui::app::{App, RepoView};
use crate::ui::text_menu::{self, TextOf};
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
    /// Unfold or fold the diff of this file of the picked commit.
    Unfold(String),
    /// Fold or unfold this folder of the picked commit's files.
    FoldCommit(String),
    /// Flip the file lists between a tree and a flat list.
    Tree,
    /// Commit what is staged.
    Commit,
    /// Ask for the message of a stash of the file or folder at this path.
    StartStash(String),
    /// Edit the file at this path in the working tree.
    Edit(String),
    /// Check out a branch or a commit of the graph.
    Checkout(Checkout),
    /// Take this commit to the board, to apply it to other repositories.
    Apply(String),
    /// Apply, pop or drop the stash git names so.
    Stash(StashAct, String),
    /// Stage the file or folder at this path, or unstage it when it is
    /// staged whole.
    Stage(String),
    /// Stage, unstage or discard the file or folder at this path.
    File(FileAct, String),
    /// The end of the commit list came into view.
    More,
    /// Unfold the run of the graph that starts with this commit.
    Run(String),
    /// Draw the old remote branches too.
    Old,
    Select(DiffTarget),
    /// Put this text on the clipboard; the click was there.
    Copy(String, egui::Pos2),
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
            let detached = self.view_detached();
            ui.horizontal(|ui| {
                ui.heading(title(&view.repo));
                ui.weak(view.repo.display().to_string());
                if let Some(detached) = &detached {
                    ui.colored_label(ui.visuals().warn_fg_color, detached);
                }
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
                    // Each tab has a width of its own, kept while the app
                    // runs; the graph needs room for lanes, names and titles.
                    let width = match view.tab {
                        RepoTab::Files | RepoTab::Commits | RepoTab::Stash | RepoTab::Reflog => {
                            LIST_WIDTH
                        }
                        RepoTab::Graph => (ui.available_width() * GRAPH_SHARE).max(LIST_WIDTH),
                    };
                    egui::Panel::left(egui::Id::new(("repo-view-lists", view.tab)))
                        .resizable(true)
                        .default_size(width)
                        .show(ui, |ui| {
                            // A panel is as wide as what it shows. While a
                            // list only says "loading" that is one word,
                            // and the panel would keep that width.
                            ui.take_available_width();
                            match view.tab {
                                RepoTab::Files => file_list(ui, details, view, &mut act),
                                RepoTab::Commits => commit_list(ui, details, view, &mut act),
                                RepoTab::Graph => graph_list(ui, details, view, &mut act),
                                RepoTab::Stash => stash_list(ui, details, view, &mut act),
                                RepoTab::Reflog => reflog_list(ui, details, view, &mut act),
                            }
                        });
                    egui::CentralPanel::default().show(ui, |ui| diff_pane(ui, view, &mut act));
                }
            }
            // Esc in a question or dialog over the view is theirs.
            let over = self.confirm.is_some()
                || self.commit_dialog.is_some()
                || self.stash_prompt.is_some();
            if !over && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
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
            Some(Act::Unfold(path)) => self.toggle_commit_file(&path),
            Some(Act::FoldCommit(path)) => self.toggle_commit_folder(&path),
            Some(Act::Tree) => self.toggle_file_tree(),
            Some(Act::Commit) => self.start_commit(),
            Some(Act::StartStash(path)) => self.start_stash(&path),
            Some(Act::Edit(path)) => self.edit_file(&path),
            Some(Act::Checkout(way)) => self.check_out(way),
            Some(Act::Apply(hash)) => self.start_apply(&hash),
            Some(Act::Stash(stash_act, name)) => self.stash_act(stash_act, &name),
            Some(Act::Stage(path)) => self.toggle_staged(&path),
            Some(Act::File(file_act, path)) => self.file_act(file_act, &path),
            Some(Act::More) => self.load_more_commits(),
            Some(Act::Run(first)) => self.unfold_graph_run(&first),
            Some(Act::Old) => self.show_old_branches(),
            Some(Act::Select(target)) => self.select_in_repo_view(target),
            Some(Act::Copy(text, at)) => self.copy(ui, text, at),
            None => {}
        }
        self.commit_dialog_window(ui);
        self.stash_prompt_window(ui);
    }
}

/// How wide the left side of the Files and Commits tabs is when it opens,
/// and which share of the dialog the left side of the Graph tab takes.
const LIST_WIDTH: f32 = 380.0;
const GRAPH_SHARE: f32 = 0.6;

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
    if !heading.is_empty() {
        ui.strong(heading);
    }
    egui::ScrollArea::vertical()
        .id_salt(tab)
        .auto_shrink([false, false])
        .show(ui, rows);
}

/// The switch at the right end of a file list's heading: the files as a
/// tree of folders, or flat. Both lists share it.
fn tree_switch(ui: &mut Ui, view: &RepoView, act: &mut Option<Act>) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        tree_toggle(ui, view, act);
    });
}

/// The Tree button itself.
fn tree_toggle(ui: &mut Ui, view: &RepoView, act: &mut Option<Act>) {
    let hover = if view.tree {
        "Folders: click for a flat list of the files"
    } else {
        "Flat list: click to show the folders"
    };
    let switch = egui::Button::selectable(view.tree, "Tree");
    if ui.add(switch).on_hover_text(hover).clicked() {
        *act = Some(Act::Tree);
    }
}

/// A row of a list; a click picks its target for the right pane.
fn row(
    ui: &mut Ui,
    view: &RepoView,
    target: DiffTarget,
    text: impl Into<egui::WidgetText>,
    hover: &str,
    act: &mut Option<Act>,
) -> egui::Response {
    let picked = view.selected.as_ref() == Some(&target);
    let entry = egui::Button::selectable(picked, text.into()).truncate();
    let response = ui.add(entry).on_hover_text(hover);
    if response.clicked() {
        *act = Some(Act::Select(target));
    }
    response
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

/// How long a side of the fold triangle is.
const FOLD_SIDE: f32 = 8.0;

/// The three corners of the fold triangle in a square box around `center`:
/// all sides of the same length, pointing right when folded (`▸`) and down
/// when open (`▾`).
pub(super) fn fold_triangle(center: egui::Pos2, open: bool) -> [egui::Pos2; 3] {
    // Height of a triangle whose sides are all FOLD_SIDE long.
    let height = FOLD_SIDE * 3f32.sqrt() / 2.0;
    let (half, third) = (FOLD_SIDE / 2.0, height / 3.0);
    if open {
        [
            center + egui::vec2(-half, -third),
            center + egui::vec2(half, -third),
            center + egui::vec2(0.0, 2.0 * third),
        ]
    } else {
        [
            center + egui::vec2(-third, -half),
            center + egui::vec2(2.0 * third, 0.0),
            center + egui::vec2(-third, half),
        ]
    }
}

/// The fold triangle of a folder or a file row, in the color its widget
/// state gives it.
fn fold_icon(ui: &Ui, open: bool, response: &egui::Response) {
    let color = ui.style().interact(response).fg_stroke.color;
    let points = fold_triangle(response.rect.center(), open).to_vec();
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
}

/// The Files tab: the changed files as a tree. A folder folds on a click;
/// a file is picked for the right pane.
fn file_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    ui.horizontal(|ui| {
        ui.strong(format!("Changed files ({})", details.files.len()));
        // Both at the right end: anything after the right-to-left part
        // would make the list wider than its panel.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let commit = ui
                .button("Commit")
                .on_hover_text("Commit what is staged (lazygit: c)");
            if commit.clicked() {
                *act = Some(Act::Commit);
            }
            tree_toggle(ui, view, act);
        });
    });
    list(ui, "", RepoTab::Files, |ui| {
        if details.files.is_empty() {
            ui.weak("none: the working tree is clean");
        }
        for entry in file_rows(&details.files, &view.closed, view.tree) {
            ui.horizontal(|ui| {
                ui.add_space(entry.depth as f32 * INDENT);
                let paths: Vec<&str> = files_of(&details.files, &entry.path)
                    .iter()
                    .map(|f| f.path.as_str())
                    .collect();
                stage_box(ui, staged(&details.states, &paths), &entry.path, act);
                let Some(file) = entry.file else {
                    let size = egui::vec2(INDENT, ui.spacing().interact_size.y);
                    let (_, icon) = ui.allocate_exact_size(size, egui::Sense::click());
                    fold_icon(ui, entry.open, &icon);
                    let name = egui::Button::new(format!("{}/", entry.name)).frame(false);
                    let hover = if entry.open { "Fold" } else { "Unfold" };
                    let name = ui.add(name).on_hover_text(hover);
                    if name.clicked() || icon.clicked() {
                        *act = Some(Act::Fold(entry.path.clone()));
                    }
                    file_menu(&name, &view.repo, &entry.path, act);
                    return;
                };
                let state = details.states.get(&file.path).copied();
                let state = state.unwrap_or(FileState::guessed(file.kind));
                let letters = state_text(ui, state, &entry.name);
                let hover = format!("{}: {}", file.kind.label(), file.path);
                let path = file.path.clone();
                let target = DiffTarget::File(file);
                let shown = row(ui, view, target, letters, &hover, act);
                file_menu(&shown, &view.repo, &path, act);
            });
        }
    });
}

/// The checkbox in front of a file or folder: checked when all of it is
/// staged, half filled when a part is. A click stages it, or unstages it
/// when it is staged whole.
fn stage_box(ui: &mut Ui, staged: Staged, path: &str, act: &mut Option<Act>) {
    let mut checked = staged == Staged::All;
    let hover = match staged {
        Staged::All => "Staged: click to unstage",
        Staged::Partly => "Partly staged: click to stage all of it",
        Staged::Nothing => "Not staged: click to stage",
    };
    let check = egui::Checkbox::without_text(&mut checked).indeterminate(staged == Staged::Partly);
    if ui.add(check).on_hover_text(hover).clicked() {
        *act = Some(Act::Stage(path.to_owned()));
    }
}

/// A file's two status letters and its name, colored as lazygit colors
/// them: the staged letter green, the unstaged one red, and the name green
/// when all of it is staged, red when anything is not.
fn state_text(ui: &Ui, state: FileState, name: &str) -> egui::text::LayoutJob {
    let (green, red) = (ok_color(ui), ui.visuals().error_fg_color);
    let mono = egui::TextStyle::Monospace.resolve(ui.style());
    let body = egui::TextStyle::Body.resolve(ui.style());
    let part = |font: &egui::FontId, color| egui::TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let mut text = egui::text::LayoutJob::default();
    let letters: Vec<char> = state.letters().chars().collect();
    let index = letters.first().copied().unwrap_or(' ');
    let worktree = letters.get(1).copied().unwrap_or(' ');
    let index_color = if state.untracked() { red } else { green };
    text.append(&index.to_string(), 0.0, part(&mono, index_color));
    text.append(&worktree.to_string(), 0.0, part(&mono, red));
    let name_color = if state.unstaged() { red } else { green };
    text.append(&format!(" {name}"), 0.0, part(&body, name_color));
    text
}

/// The right-click menu of a file or folder in the Files tab.
fn file_menu(response: &egui::Response, repo: &std::path::Path, path: &str, act: &mut Option<Act>) {
    response.context_menu(|ui| {
        if !path.ends_with('/') {
            edit_entry(ui, repo, path, act);
        }
        for file_act in [
            FileAct::Stage,
            FileAct::Unstage,
            FileAct::DiscardAll,
            FileAct::DiscardUnstaged,
        ] {
            if ui.button(file_act.label()).clicked() {
                *act = Some(Act::File(file_act, path.to_owned()));
            }
        }
        if ui.button("Stash\u{2026}").clicked() {
            *act = Some(Act::StartStash(path.to_owned()));
        }
    });
}

/// "Edit": today's copy of the file in the working tree; greyed out when
/// it is not there.
fn edit_entry(ui: &mut Ui, repo: &std::path::Path, path: &str, act: &mut Option<Act>) {
    let there = repo.join(path).is_file();
    let edit = ui
        .add_enabled(there, egui::Button::new("Edit"))
        .on_disabled_hover_text("Not in the working tree");
    if edit.clicked() {
        *act = Some(Act::Edit(path.to_owned()));
    }
}

/// The Reflog tab: where HEAD has been, newest first. A click picks the
/// commit of a step for the right pane.
fn reflog_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    list(ui, "Where HEAD has been", RepoTab::Reflog, |ui| {
        if details.reflog.is_empty() {
            ui.weak("no steps yet");
        }
        let mono = egui::TextStyle::Monospace.resolve(ui.style());
        let body = egui::TextStyle::Body.resolve(ui.style());
        for step in &details.reflog {
            let mut text = egui::text::LayoutJob::default();
            let weak = ui.visuals().weak_text_color();
            text.append(
                &format!("{} ", step.short),
                0.0,
                egui::TextFormat::simple(mono.clone(), weak),
            );
            text.append(
                &format!("{} ", step.action),
                0.0,
                egui::TextFormat::simple(body.clone(), ui.visuals().warn_fg_color),
            );
            text.append(
                &step.message,
                0.0,
                egui::TextFormat::simple(body.clone(), ui.visuals().text_color()),
            );
            let hover = format!("{}: {}: {}", step.name, step.action, step.message);
            let target = DiffTarget::Commit(step.hash.clone());
            let shown = row(ui, view, target, text, &hover, act);
            shown.context_menu(|ui| apply_entry(ui, &step.hash, act));
        }
    });
}

/// The Stash tab: the stashes, newest first. A click picks one for the
/// right pane; its right-click menu applies, pops or drops it.
fn stash_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    list(ui, "Stashes", RepoTab::Stash, |ui| {
        if details.stashes.is_empty() {
            ui.weak("no stashes");
        }
        for stash in &details.stashes {
            let mut text = egui::text::LayoutJob::default();
            let mono = egui::TextStyle::Monospace.resolve(ui.style());
            let body = egui::TextStyle::Body.resolve(ui.style());
            text.append(
                &format!("{} ", stash.name),
                0.0,
                egui::TextFormat::simple(mono, ui.visuals().weak_text_color()),
            );
            text.append(
                &stash.subject,
                0.0,
                egui::TextFormat::simple(body, ui.visuals().text_color()),
            );
            let target = DiffTarget::Commit(stash.hash.clone());
            let shown = row(ui, view, target, text, &stash.subject, act);
            shown.context_menu(|ui| {
                for stash_act in [StashAct::Apply, StashAct::Pop, StashAct::Drop] {
                    if ui.button(stash_act.label()).clicked() {
                        *act = Some(Act::Stash(stash_act, stash.name.clone()));
                    }
                }
            });
        }
    });
}

/// The color of an author: the same for the same name everywhere.
fn author_color(ui: &Ui, author: &str) -> Color32 {
    let value = if ui.visuals().dark_mode { 0.95 } else { 0.6 };
    egui::ecolor::Hsva::new(author_hue(author), 0.6, value, 1.0).into()
}

/// Seconds since 1970, for the ages in the commit list.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A row of the commit list: the author's initials in the author's color,
/// the short hash and the title, and the age at the right edge.
fn commit_row(ui: &mut Ui, view: &RepoView, commit: &LogEntry, now: i64, act: &mut Option<Act>) {
    // A row of its own height: a centered layout left to itself takes all
    // the height there is.
    let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    ui.allocate_ui_with_layout(size, layout, |ui| {
        ui.weak(short_age(now, commit.time))
            .on_hover_text(&commit.date);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            let mono = egui::TextStyle::Monospace.resolve(ui.style());
            let body = egui::TextStyle::Body.resolve(ui.style());
            let part = |font: &egui::FontId, color| egui::TextFormat {
                font_id: font.clone(),
                color,
                ..Default::default()
            };
            let mut text = egui::text::LayoutJob::default();
            let author = part(&mono, author_color(ui, &commit.author));
            text.append(&format!("{:<2} ", initials(&commit.author)), 0.0, author);
            let hash = part(&mono, ui.visuals().weak_text_color());
            text.append(&format!("{} ", commit.short), 0.0, hash);
            for tag in &commit.tags {
                let mut name = part(&mono, ui.visuals().warn_fg_color);
                name.background = ui.visuals().faint_bg_color;
                text.append(&format!(" {tag} "), 0.0, name);
                text.append(" ", 0.0, part(&mono, Color32::TRANSPARENT));
            }
            let title = part(&body, ui.visuals().text_color());
            text.append(&commit.subject, 0.0, title);
            let hover = format!("{}, {}\n{}", commit.author, commit.date, commit.subject);
            let target = DiffTarget::Commit(commit.hash.clone());
            let shown = row(ui, view, target, text, &hover, act);
            shown.context_menu(|ui| apply_entry(ui, &commit.hash, act));
        });
    });
}

/// "Apply to repositories…": the commit goes to the board, where the
/// repositories it goes to are marked.
fn apply_entry(ui: &mut Ui, hash: &str, act: &mut Option<Act>) {
    let entry = ui
        .button("Apply to repositories\u{2026}")
        .on_hover_text("Mark them on the board, then Apply");
    if entry.clicked() {
        *act = Some(Act::Apply(hash.to_owned()));
    }
}

/// The Commits tab: the history of the checked-out branch. The next page
/// is read when the end of the list comes into view.
fn commit_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    list(ui, "Commits", RepoTab::Commits, |ui| {
        if details.commits.is_empty() {
            ui.weak("no commits yet");
            return;
        }
        let now = now();
        for commit in &details.commits {
            commit_row(ui, view, commit, now, act);
        }
        if view.more_commits {
            let end = ui.weak("more commits are read\u{2026}");
            if act.is_none() && ui.is_rect_visible(end.rect) {
                *act = Some(Act::More);
            }
        }
    });
}

/// How wide a lane of the graph is, and how high a row.
const LANE: f32 = 14.0;
const GRAPH_ROW: f32 = 22.0;

/// The color of a lane: neighbours far apart on the color wheel.
fn lane_color(ui: &Ui, lane: usize) -> Color32 {
    let value = if ui.visuals().dark_mode { 0.95 } else { 0.65 };
    let hue = (lane as f32 * 0.381_966).fract();
    egui::ecolor::Hsva::new(hue, 0.65, value, 1.0).into()
}

/// The lines and the dot of one row of the graph, in the `gutter`.
fn graph_lines(ui: &Ui, gutter: egui::Rect, entry: &GraphRow) {
    let painter = ui.painter_at(gutter);
    let x = |lane: usize| gutter.left() + LANE * (lane as f32 + 0.5);
    let (top, middle, bottom) = (gutter.top(), gutter.center().y, gutter.bottom());
    let line = |lane: usize| egui::Stroke::new(1.5, lane_color(ui, lane));
    for lane in &entry.through {
        let (from, to) = (egui::pos2(x(*lane), top), egui::pos2(x(*lane), bottom));
        painter.line_segment([from, to], line(*lane));
    }
    let dot = egui::pos2(x(entry.lane), middle);
    if entry.up {
        painter.line_segment([egui::pos2(dot.x, top), dot], line(entry.lane));
    }
    if entry.down {
        painter.line_segment([dot, egui::pos2(dot.x, bottom)], line(entry.lane));
    }
    for lane in &entry.into {
        painter.line_segment([egui::pos2(x(*lane), top), dot], line(*lane));
    }
    for lane in &entry.out {
        painter.line_segment([dot, egui::pos2(x(*lane), bottom)], line(*lane));
    }
    let color = lane_color(ui, entry.lane);
    match &entry.kind {
        // A merge is a ring, any other commit a dot.
        RowKind::Commit { merge: true, .. } => {
            painter.circle_filled(dot, 4.5, ui.visuals().panel_fill);
            painter.circle_stroke(dot, 4.5, egui::Stroke::new(1.5, color));
        }
        RowKind::Commit { .. } => {
            painter.circle_filled(dot, 4.0, color);
        }
        // A folded run: three small dots on its line.
        RowKind::Fold { .. } => {
            for offset in [-5.0, 0.0, 5.0] {
                painter.circle_filled(dot + egui::vec2(0.0, offset), 1.5, color);
            }
        }
    }
}

/// The Graph tab: all branches as a condensed graph. A click on a commit
/// picks it for the right pane; a click on a folded run unfolds it.
fn graph_list(ui: &mut Ui, details: &Details, view: &RepoView, act: &mut Option<Act>) {
    let graph = match &view.graph {
        None => {
            ui.weak("loading");
            return;
        }
        Some(Err(error)) => {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return;
        }
        Some(Ok(graph)) => graph,
    };
    let head = details.branches.iter().find(|b| b.current);
    let names = labels(&graph.refs, head.map(|b| b.name.as_str()));
    let rows = graph_rows(&graph.commits, &names, &view.graph_open);
    let ways = |hash: &str| checkouts(&graph.refs, hash, head.map(|b| b.name.as_str()));
    let lanes = rows.iter().map(GraphRow::width).max().unwrap_or(1);
    ui.strong("All branches");
    graph_notes(ui, graph, view, act);
    if rows.is_empty() {
        ui.weak("no commits yet");
        return;
    }
    egui::ScrollArea::both()
        .id_salt(RepoTab::Graph)
        .auto_shrink([false, false])
        .show_rows(ui, GRAPH_ROW, rows.len(), |ui, shown| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for entry in rows.get(shown).unwrap_or_default() {
                ui.horizontal(|ui| {
                    ui.set_height(GRAPH_ROW);
                    let size = egui::vec2(lanes as f32 * LANE, GRAPH_ROW);
                    let (gutter, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                    graph_lines(ui, gutter, entry);
                    graph_row_text(ui, view, entry, &ways, act);
                });
            }
        });
}

/// The lines above the graph: old remote branches left out, and a history
/// too long to be read whole.
fn graph_notes(ui: &mut Ui, graph: &GraphData, view: &RepoView, act: &mut Option<Act>) {
    if graph.hidden > 0 && !view.with_old {
        let text = if graph.hidden == 1 {
            "1 old remote branch hidden".to_owned()
        } else {
            format!("{} old remote branches hidden", graph.hidden)
        };
        let hover =
            "Remote branches nobody has locally, without a commit for 90 days. Click to show them.";
        if ui.link(text).on_hover_text(hover).clicked() {
            *act = Some(Act::Old);
        }
    }
    if graph.cut {
        ui.weak("older history is not shown");
    }
}

/// What stands beside the lines of a row: the names on the commit and its
/// title, or how many commits a folded run holds.
fn graph_row_text(
    ui: &mut Ui,
    view: &RepoView,
    entry: &GraphRow,
    ways: &dyn Fn(&str) -> Vec<Checkout>,
    act: &mut Option<Act>,
) {
    match &entry.kind {
        RowKind::Fold { count, first } => {
            let text = RichText::new(format!("{count} commits")).weak();
            let run = egui::Button::new(text).frame(false);
            if ui.add(run).on_hover_text("Show these commits").clicked() {
                *act = Some(Act::Run(first.clone()));
            }
        }
        RowKind::Commit {
            hash,
            author,
            subject,
            labels,
            ..
        } => {
            for label in labels {
                let fill = match label.kind {
                    LabelKind::Local | LabelKind::Both => lane_color(ui, entry.lane),
                    LabelKind::Remote => ui.visuals().weak_text_color(),
                    LabelKind::Tag => ui.visuals().warn_fg_color,
                };
                let mut text = RichText::new(format!(" {} ", label.text))
                    .monospace()
                    .background_color(fill.gamma_multiply(0.3))
                    .color(ui.visuals().strong_text_color());
                if label.head {
                    text = text.strong().underline();
                }
                let hover = match (label.kind, label.head) {
                    (_, true) => "The checked-out branch",
                    (LabelKind::Local, _) => "A local branch",
                    (LabelKind::Remote, _) => "A remote branch",
                    (LabelKind::Both, _) => {
                        "A local branch and its remote branch, on the same commit"
                    }
                    (LabelKind::Tag, _) => "A tag",
                };
                ui.label(text).on_hover_text(hover);
            }
            let target = DiffTarget::Commit(hash.clone());
            let hover = format!("{author}\n{subject}");
            let shown = row(ui, view, target, RichText::new(subject), &hover, act);
            shown.context_menu(|ui| {
                for way in ways(hash) {
                    if ui.button(&way.label).clicked() {
                        *act = Some(Act::Checkout(way));
                    }
                }
                ui.separator();
                apply_entry(ui, hash, act);
            });
        }
    }
}

/// What is picked, on the right: a commit, or a file with the content of
/// an added one, what is gone of a deleted one, and the diff of any other.
fn diff_pane(ui: &mut Ui, view: &RepoView, act: &mut Option<Act>) {
    let (target, diff) = match (&view.selected, &view.diff) {
        (None, _) => {
            ui.weak(match view.tab {
                RepoTab::Files => "Pick a file to see its changes.",
                RepoTab::Commits | RepoTab::Graph => "Pick a commit to see it.",
                RepoTab::Stash => "Pick a stash to see it.",
                RepoTab::Reflog => "Pick a step to see its commit.",
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
    let file = match target {
        DiffTarget::File(file) => file,
        DiffTarget::Commit(_) => return commit_pane(ui, view, diff, act),
        // Shown inside its commit, never picked.
        DiffTarget::CommitFile { .. } => return,
    };
    ui.horizontal(|ui| {
        let color = change_color(ui, file.kind);
        ui.label(RichText::new(file.kind.label()).color(color));
        ui.label(RichText::new(&file.path).monospace().strong());
    });
    ui.separator();
    let lines = file_lines(file.kind, diff);
    if lines.is_empty() {
        ui.weak(if file.kind == ChangeKind::Added {
            "empty file"
        } else {
            "no difference"
        });
        return;
    }
    diff_lines(ui, "repo-view-diff", &lines);
}

/// A value of the commit pane that goes to the clipboard on a click, like
/// the values of a card; `hover` says what is copied.
fn copy_value(ui: &mut Ui, text: RichText, copy: &str, hover: &str, act: &mut Option<Act>) {
    let value = ui
        .add(egui::Label::new(text).sense(egui::Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(hover);
    if value.clicked() {
        let at = value
            .interact_pointer_pos()
            .unwrap_or_else(|| value.rect.center());
        *act = Some(Act::Copy(copy.to_owned(), at));
    }
}

/// The lines added and taken away, each in its color.
fn line_counts(ui: &mut Ui, (added, removed): (u32, u32)) {
    ui.label(RichText::new(format!("+{added}")).color(ok_color(ui)));
    ui.label(RichText::new(format!("{MINUS}{removed}")).color(ui.visuals().error_fg_color));
}

/// The sign before the lines a commit took away.
pub(super) const MINUS: char = '\u{2212}';

/// The picked commit: its header, which stays in place, and under it the
/// files it changed. A click on a file unfolds its diff in place.
fn commit_pane(ui: &mut Ui, view: &RepoView, raw: &str, act: &mut Option<Act>) {
    let Some(commit) = parse_commit(raw) else {
        ui.weak("this commit cannot be read");
        return;
    };
    ui.horizontal_wrapped(|ui| {
        let short: String = commit.hash.chars().take(10).collect();
        let hover = format!("{}\nA click copies the hash", commit.hash);
        let short = RichText::new(short).monospace().strong();
        copy_value(ui, short, &commit.hash, &hover, act);
        for name in &commit.refs {
            let label = RichText::new(format!(" {name} "))
                .monospace()
                .background_color(ui.visuals().faint_bg_color)
                .color(ui.visuals().strong_text_color());
            copy_value(ui, label, name, "A click copies the name", act);
        }
    });
    ui.horizontal_wrapped(|ui| {
        let color = author_color(ui, &commit.author);
        ui.label(
            RichText::new(initials(&commit.author))
                .monospace()
                .color(color),
        );
        let author = RichText::new(&commit.author);
        copy_value(ui, author, &commit.author, "A click copies the name", act);
        let mail = RichText::new(format!("<{}>", commit.email));
        copy_value(ui, mail, &commit.email, "A click copies the address", act);
        let date = RichText::new(&commit.date).weak();
        copy_value(ui, date, &commit.date, "A click copies the date", act);
    });
    if let Some((name, mail)) = &commit.committer {
        ui.weak(format!("committed by {name} <{mail}>"));
    }
    if !commit.parents.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.weak(if commit.parents.len() == 1 {
                "parent"
            } else {
                "parents"
            });
            for parent in &commit.parents {
                let short: String = parent.chars().take(10).collect();
                let go = ui
                    .link(RichText::new(short).monospace())
                    .on_hover_text("Go to this commit");
                if go.clicked() {
                    *act = Some(Act::Select(DiffTarget::Commit(parent.clone())));
                }
            }
        });
    }
    let message = egui::ScrollArea::vertical()
        .id_salt("repo-view-message")
        .max_height(140.0)
        .show(ui, |ui| {
            ui.label(&commit.message);
        });
    text_menu::area(ui, message.inner_rect, TextOf::Message);
    ui.separator();
    let count = commit.files.len();
    ui.horizontal(|ui| {
        ui.strong(if count == 1 {
            "1 file changed".to_owned()
        } else {
            format!("{count} files changed")
        });
        if count > 0 {
            line_counts(ui, commit.total_lines());
        }
        tree_switch(ui, view, act);
    });
    let rows = commit_rows(&commit, view);
    let row_height = ui.spacing().interact_size.y;
    let (added, removed) = (ok_color(ui), ui.visuals().error_fg_color);
    let files = egui::ScrollArea::both()
        .id_salt("repo-view-commit")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, rows.len(), |ui, shown| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for entry in rows.get(shown).unwrap_or_default() {
                ui.horizontal(|ui| {
                    ui.set_height(row_height);
                    match entry {
                        CommitRow::Folder(folder) => {
                            ui.add_space(folder.depth as f32 * INDENT);
                            let size = egui::vec2(INDENT, row_height);
                            let (_, icon) = ui.allocate_exact_size(size, egui::Sense::click());
                            fold_icon(ui, folder.open, &icon);
                            let name = egui::Button::new(format!("{}/", folder.name)).frame(false);
                            let hover = if folder.open { "Fold" } else { "Unfold" };
                            if ui.add(name).on_hover_text(hover).clicked() || icon.clicked() {
                                *act = Some(Act::FoldCommit(folder.path.clone()));
                            }
                        }
                        CommitRow::File(row, open, lines) => {
                            let file = row.file;
                            ui.add_space(row.depth as f32 * INDENT);
                            let size = egui::vec2(INDENT, row_height);
                            let (_, icon) = ui.allocate_exact_size(size, egui::Sense::click());
                            fold_icon(ui, *open, &icon);
                            let text = format!("{} {}", file.kind.letter(), row.name);
                            let text = RichText::new(text).color(change_color(ui, file.kind));
                            let name = egui::Button::new(text).frame(false);
                            let hover = if *open { "Fold" } else { "Show what changed" };
                            let name = ui.add(name).on_hover_text(hover);
                            if name.clicked() || icon.clicked() {
                                *act = Some(Act::Unfold(file.path.clone()));
                            }
                            name.context_menu(|ui| edit_entry(ui, &view.repo, &file.path, act));
                            match lines {
                                Some(lines) => line_counts(ui, *lines),
                                None => {
                                    ui.weak("binary");
                                }
                            }
                        }
                        CommitRow::Line(kind, line) => {
                            ui.add_space(INDENT);
                            let text = RichText::new(*line).monospace();
                            ui.label(match kind {
                                LineKind::Added => text.color(added),
                                LineKind::Removed => text.color(removed),
                                LineKind::Header => text.strong(),
                                LineKind::Plain => text,
                            });
                        }
                        CommitRow::Note(note) => {
                            ui.add_space(INDENT);
                            ui.weak(*note);
                        }
                    }
                });
            }
        });
    text_menu::area(ui, files.inner_rect, TextOf::Pane);
}

/// Lines of a diff or of a file, colored by kind, scrolling both ways.
fn diff_lines(ui: &mut Ui, id: &str, lines: &[(LineKind, &str)]) {
    let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
    let (added, removed) = (ok_color(ui), ui.visuals().error_fg_color);
    let shown = egui::ScrollArea::both()
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
    text_menu::area(ui, shown.inner_rect, TextOf::Pane);
}
