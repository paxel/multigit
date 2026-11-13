//! The read-only view of one repository, over the board: three tabs
//! (Files, Commits, Graph), a list on the left and what is picked in it on
//! the right, and Back and Forward through what was looked at.

use crate::domain::details::{
    ChangeKind, Details, DiffTarget, FileChange, LineKind, LogEntry, RepoTab, author_hue,
    file_lines, initials, parse_commit, short_age, tree_rows,
};
use crate::domain::graph::{GraphData, GraphRow, LabelKind, RowKind, graph_rows, labels};
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
    /// Unfold or fold the diff of this file of the picked commit.
    Unfold(String),
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
                    // Each tab has a width of its own, kept while the app
                    // runs; the graph needs room for lanes, names and titles.
                    let width = match view.tab {
                        RepoTab::Files | RepoTab::Commits => LIST_WIDTH,
                        RepoTab::Graph => (ui.available_width() * GRAPH_SHARE).max(LIST_WIDTH),
                    };
                    egui::Panel::left(egui::Id::new(("repo-view-lists", view.tab)))
                        .resizable(true)
                        .default_size(width)
                        .show(ui, |ui| match view.tab {
                            RepoTab::Files => file_list(ui, details, view, &mut act),
                            RepoTab::Commits => commit_list(ui, details, view, &mut act),
                            RepoTab::Graph => graph_list(ui, details, view, &mut act),
                        });
                    egui::CentralPanel::default().show(ui, |ui| diff_pane(ui, view, &mut act));
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
            Some(Act::Unfold(path)) => self.toggle_commit_file(&path),
            Some(Act::More) => self.load_more_commits(),
            Some(Act::Run(first)) => self.unfold_graph_run(&first),
            Some(Act::Old) => self.show_old_branches(),
            Some(Act::Select(target)) => self.select_in_repo_view(target),
            Some(Act::Copy(text, at)) => self.copy(ui, text, at),
            None => {}
        }
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
    text: impl Into<egui::WidgetText>,
    hover: &str,
    act: &mut Option<Act>,
) {
    let picked = view.selected.as_ref() == Some(&target);
    let entry = egui::Button::selectable(picked, text.into()).truncate();
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
            row(ui, view, target, text, &hover, act);
        });
    });
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
                    graph_row_text(ui, view, entry, act);
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
fn graph_row_text(ui: &mut Ui, view: &RepoView, entry: &GraphRow, act: &mut Option<Act>) {
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
            row(ui, view, target, RichText::new(subject), &hover, act);
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

/// A row under the header of a commit.
enum CommitRow<'a> {
    File(&'a FileChange, bool),
    Line(LineKind, &'a str),
    Note(&'a str),
}

/// The picked commit: its header, which stays in place, and under it the
/// files it changed. A click on a file unfolds its diff in place.
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
    egui::ScrollArea::vertical()
        .id_salt("repo-view-message")
        .max_height(140.0)
        .show(ui, |ui| {
            ui.label(&commit.message);
        });
    ui.separator();
    let count = commit.files.len();
    ui.strong(if count == 1 {
        "1 file changed".to_owned()
    } else {
        format!("{count} files changed")
    });
    let mut rows = Vec::new();
    for file in &commit.files {
        let diff = view.unfolded.get(&file.path);
        rows.push(CommitRow::File(file, diff.is_some()));
        match diff {
            None => {}
            Some(None) => rows.push(CommitRow::Note("loading")),
            Some(Some(Err(error))) => rows.push(CommitRow::Note(error)),
            Some(Some(Ok(diff))) => {
                let lines = file_lines(file.kind, diff);
                if lines.is_empty() {
                    rows.push(CommitRow::Note("nothing to show"));
                }
                rows.extend(
                    lines
                        .into_iter()
                        .map(|(kind, line)| CommitRow::Line(kind, line)),
                );
            }
        }
    }
    let row_height = ui.spacing().interact_size.y;
    let (added, removed) = (ok_color(ui), ui.visuals().error_fg_color);
    egui::ScrollArea::both()
        .id_salt("repo-view-commit")
        .auto_shrink([false, false])
        .show_rows(ui, row_height, rows.len(), |ui, shown| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
            for entry in rows.get(shown).unwrap_or_default() {
                ui.horizontal(|ui| {
                    ui.set_height(row_height);
                    match entry {
                        CommitRow::File(file, open) => {
                            let size = egui::vec2(INDENT, row_height);
                            let (_, icon) = ui.allocate_exact_size(size, egui::Sense::click());
                            let openness = if *open { 1.0 } else { 0.0 };
                            egui::collapsing_header::paint_default_icon(ui, openness, &icon);
                            let text = format!("{} {}", file.kind.letter(), file.path);
                            let text = RichText::new(text).color(change_color(ui, file.kind));
                            let name = egui::Button::new(text).frame(false);
                            let hover = if *open { "Fold" } else { "Show what changed" };
                            if ui.add(name).on_hover_text(hover).clicked() || icon.clicked() {
                                *act = Some(Act::Unfold(file.path.clone()));
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
