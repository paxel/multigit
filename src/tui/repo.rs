//! The read-only view of one repository in the terminal: its tabs, a list
//! on the left, what is picked in it on the right, Back and Forward.

use super::App;
use super::actions::Action;
use super::actions::copy;
use super::commits::PaneRow;
use super::state::Mode;
use super::widgets::{Item, Menu};
use crate::app::{RepoView, TextOf};
use crate::domain::details::{ChangeKind, DiffTarget, LineKind, RepoTab, file_lines, file_rows};
use crate::domain::naming::title;
use crate::domain::staging::{FileAct, FileState, StashAct};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

/// What picking a row of a list does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    /// A heading or a note.
    Nothing,
    /// Fold or unfold the folder with this path.
    Fold(String),
    /// Show this in the right pane.
    Select(DiffTarget),
    /// Unfold the folded run of the graph whose newest commit this is.
    Unfold(String),
    /// Draw the old remote branches too.
    Old,
}

/// One row of a list.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewRow {
    pub line: Line<'static>,
    pub pick: Pick,
}

impl ViewRow {
    fn note(text: impl Into<String>) -> Self {
        Self {
            line: Line::styled(text.into(), dim()),
            pick: Pick::Nothing,
        }
    }
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The color of how a file changed.
pub fn change_color(kind: ChangeKind) -> Color {
    match kind {
        ChangeKind::Added => Color::Green,
        ChangeKind::Modified => Color::Yellow,
        ChangeKind::Deleted | ChangeKind::Conflict => Color::Red,
    }
}

/// The color of a diff line.
pub fn line_style(kind: LineKind) -> Style {
    match kind {
        LineKind::Added => Style::new().fg(Color::Green),
        LineKind::Removed => Style::new().fg(Color::Red),
        LineKind::Header => Style::new().fg(Color::Cyan),
        LineKind::Plain => Style::new(),
    }
}

/// A changed file's row: its two status letters and its name, colored as
/// lazygit colors them: the staged letter green, the unstaged one red, and
/// the name green when all of it is staged, red when anything is not.
fn state_line(indent: &str, state: FileState, name: &str) -> Line<'static> {
    let letters: Vec<char> = state.letters().chars().collect();
    let index = letters.first().copied().unwrap_or(' ');
    let worktree = letters.get(1).copied().unwrap_or(' ');
    let red = Style::new().fg(Color::Red);
    let green = Style::new().fg(Color::Green);
    Line::from(vec![
        Span::raw(format!("{indent}  ")),
        Span::styled(
            index.to_string(),
            if state.untracked() { red } else { green },
        ),
        Span::styled(worktree.to_string(), red),
        Span::styled(
            format!(" {name}"),
            if state.unstaged() { red } else { green },
        ),
    ])
}

/// The path of a Files tab row: a file's, or a folder's with its `/`.
fn row_path(row: &ViewRow) -> Option<String> {
    match &row.pick {
        Pick::Fold(path) => Some(path.clone()),
        Pick::Select(DiffTarget::File(file)) => Some(file.path.clone()),
        _ => None,
    }
}

/// The menu of a Files tab row: stage, unstage and the two discards, or
/// the discards alone.
fn file_menu(path: &str, discard_only: bool, editable: bool) -> Menu {
    let acts: &[FileAct] = if discard_only {
        &[FileAct::DiscardAll, FileAct::DiscardUnstaged]
    } else {
        &[
            FileAct::Stage,
            FileAct::Unstage,
            FileAct::DiscardAll,
            FileAct::DiscardUnstaged,
        ]
    };
    let mut items: Vec<Item> = acts
        .iter()
        .map(|act| Item::new(act.label(), Action::File(*act, path.to_owned())))
        .collect();
    if !discard_only {
        items.push(Item::new(
            "Stash\u{2026}",
            Action::StartStash(path.to_owned()),
        ));
        if !path.ends_with('/') {
            let edit = if editable {
                Item::new("Edit", Action::Edit(path.to_owned()))
            } else {
                Item::disabled("Edit (not in the working tree)")
            };
            items.insert(0, edit);
        }
    }
    Menu::new(path.to_owned(), items)
}

/// The name git gives the stash of a Stash tab row (`stash@{0}`).
fn stash_name(app: &App, row: &ViewRow) -> Option<String> {
    let Pick::Select(DiffTarget::Commit(hash)) = &row.pick else {
        return None;
    };
    let view = app.repo_view.as_ref().filter(|v| v.tab == RepoTab::Stash)?;
    let Some(Ok(details)) = &view.details else {
        return None;
    };
    let stash = details.stashes.iter().find(|s| &s.hash == hash)?;
    Some(stash.name.clone())
}

/// The commit of a Graph tab row.
fn graph_commit(app: &App, row: &ViewRow) -> Option<String> {
    let Pick::Select(DiffTarget::Commit(hash)) = &row.pick else {
        return None;
    };
    app.repo_view
        .as_ref()
        .filter(|v| v.tab == RepoTab::Graph)
        .map(|_| hash.clone())
}

/// The menu of a graph row: its branches and the commit to check out.
fn checkout_menu(app: &App, hash: &str) -> Menu {
    let items = app
        .checkouts_of(hash)
        .into_iter()
        .map(|way| Item::new(way.label.clone(), Action::Checkout(way)))
        .collect();
    Menu::new("check out", items)
}

/// `o` on a graph row: its one branch straight away; with several or
/// none the menu.
fn check_out_row(app: &mut App, hash: &str) {
    let ways = app.checkouts_of(hash);
    let mut branches = ways.iter().filter(|w| w.branch);
    match (branches.next(), branches.next()) {
        (Some(only), None) => {
            let only = only.clone();
            app.check_out(only);
        }
        _ => app.gui.mode = Mode::Menu(checkout_menu(app, hash)),
    }
}

/// The menu of a stash: apply, pop, drop.
fn stash_menu(name: &str) -> Menu {
    let items = [StashAct::Apply, StashAct::Pop, StashAct::Drop]
        .into_iter()
        .map(|act| Item::new(act.label(), Action::Stash(act, name.to_owned())))
        .collect();
    Menu::new(name.to_owned(), items)
}

/// The rows of the shown tab's list.
pub fn rows(app: &App) -> Vec<ViewRow> {
    let Some(view) = &app.repo_view else {
        return Vec::new();
    };
    let details = match &view.details {
        None => return vec![ViewRow::note("loading")],
        Some(Err(error)) => {
            return vec![ViewRow {
                line: Line::styled(error.clone(), Style::new().fg(Color::Red)),
                pick: Pick::Nothing,
            }];
        }
        Some(Ok(details)) => details,
    };
    match view.tab {
        RepoTab::Files => {
            if details.files.is_empty() {
                return vec![ViewRow::note("none: the working tree is clean")];
            }
            file_rows(&details.files, &view.closed, view.tree)
                .into_iter()
                .map(|entry| {
                    let indent = "  ".repeat(entry.depth);
                    match entry.file {
                        None => {
                            let arrow = if entry.open { "\u{25be}" } else { "\u{25b8}" };
                            ViewRow {
                                line: Line::raw(format!("{indent}{arrow} {}/", entry.name)),
                                pick: Pick::Fold(entry.path),
                            }
                        }
                        Some(file) => ViewRow {
                            line: state_line(
                                &indent,
                                details
                                    .states
                                    .get(&file.path)
                                    .copied()
                                    .unwrap_or(FileState::guessed(file.kind)),
                                &entry.name,
                            ),
                            pick: Pick::Select(DiffTarget::File(file)),
                        },
                    }
                })
                .collect()
        }
        RepoTab::Commits => super::commits::rows(details, view),
        RepoTab::Graph => super::graph::rows(details, view),
        RepoTab::Reflog => {
            if details.reflog.is_empty() {
                return vec![ViewRow::note("no steps yet")];
            }
            details
                .reflog
                .iter()
                .map(|step| ViewRow {
                    line: Line::from(vec![
                        Span::styled(format!("{} ", step.short), dim()),
                        Span::styled(format!("{} ", step.action), Style::new().fg(Color::Yellow)),
                        Span::raw(step.message.clone()),
                    ]),
                    pick: Pick::Select(DiffTarget::Commit(step.hash.clone())),
                })
                .collect()
        }
        RepoTab::Stash => {
            if details.stashes.is_empty() {
                return vec![ViewRow::note("no stashes")];
            }
            details
                .stashes
                .iter()
                .map(|stash| ViewRow {
                    line: Line::from(vec![
                        Span::styled(format!("{} ", stash.name), dim()),
                        Span::raw(stash.subject.clone()),
                    ]),
                    pick: Pick::Select(DiffTarget::Commit(stash.hash.clone())),
                })
                .collect()
        }
    }
}

/// The cursor's row in the shown tab: the picked one when there is one and
/// the cursor was not moved, else where it was left.
pub fn cursor(app: &App, count: usize) -> usize {
    let tab = app.repo_view.as_ref().map_or(RepoTab::Files, |v| v.tab);
    app.gui
        .view_cursor
        .get(&tab)
        .copied()
        .unwrap_or(0)
        .min(count.saturating_sub(1))
}

fn set_cursor(app: &mut App, at: usize) {
    if let Some(view) = &app.repo_view {
        app.gui.view_cursor.insert(view.tab, at);
    }
}

/// At the end of the Commits tab the next page of the history is read.
fn read_on(app: &mut App, last: usize) {
    let at_end = cursor(app, last.saturating_add(1)) == last;
    let more = app
        .repo_view
        .as_ref()
        .is_some_and(|v| v.tab == RepoTab::Commits && v.more_commits && !v.loading_commits);
    if at_end && more {
        app.load_more_commits();
    }
}

/// Pick the row under the cursor.
pub fn pick(app: &mut App, row: &ViewRow) {
    match &row.pick {
        Pick::Nothing => {}
        Pick::Fold(path) => app.toggle_repo_folder(path),
        Pick::Unfold(first) => app.unfold_graph_run(first),
        Pick::Old => app.show_old_branches(),
        Pick::Select(target) => {
            app.select_in_repo_view(target.clone());
            app.gui.pane_scroll = 0;
        }
    }
}

/// The right pane's rows.
pub fn pane_rows(view: &RepoView) -> Vec<PaneRow> {
    pane_lines(view)
}

fn pane_lines(view: &RepoView) -> Vec<PaneRow> {
    let text = |line: Line<'static>| vec![PaneRow::text(line)];
    let (target, diff) = match (&view.selected, &view.diff) {
        (None, _) => {
            let hint = match view.tab {
                RepoTab::Files => "Pick a file to see its changes.",
                RepoTab::Commits | RepoTab::Graph => "Pick a commit to see it.",
                RepoTab::Stash => "Pick a stash to see it.",
                RepoTab::Reflog => "Pick a step to see its commit.",
            };
            return text(Line::styled(hint, dim()));
        }
        (Some(_), None) => return text(Line::styled("loading", dim())),
        (Some(_), Some(Err(error))) => {
            return text(Line::styled(error.clone(), Style::new().fg(Color::Red)));
        }
        (Some(target), Some(Ok(diff))) => (target, diff),
    };
    let file = match target {
        DiffTarget::File(file) => file,
        DiffTarget::Commit(_) => return super::commits::pane(view, diff),
        // Shown inside its commit, never picked.
        DiffTarget::CommitFile { .. } => return Vec::new(),
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", file.kind.label()),
            Style::new().fg(change_color(file.kind)),
        ),
        Span::styled(file.path.clone(), Style::new().add_modifier(Modifier::BOLD)),
    ])];
    let body = file_lines(file.kind, diff);
    if body.is_empty() {
        let empty = if file.kind == ChangeKind::Added {
            "empty file"
        } else {
            "no difference"
        };
        lines.push(Line::styled(empty, dim()));
    }
    lines.extend(
        body.into_iter()
            .map(|(kind, text)| Line::styled(text.to_owned(), line_style(kind))),
    );
    lines.into_iter().map(PaneRow::text).collect()
}

/// A key while the right pane has the keys: move, and fold a commit's
/// file.
fn pane_key(app: &mut App, key: KeyEvent) {
    let Some(view) = app.repo_view.clone() else {
        return;
    };
    let rows = pane_rows(&view);
    let last = rows.len().saturating_sub(1);
    let at = app.gui.pane_cursor.min(last);
    match key.code {
        KeyCode::Tab | KeyCode::Esc => app.gui.pane_focus = false,
        KeyCode::Char('e') => {
            if let Some(path) = rows.get(at).and_then(|row| row.file.clone()) {
                app.edit_file(&path);
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.gui.pane_cursor = at.saturating_add(1).min(last),
        KeyCode::Char('k') | KeyCode::Up => app.gui.pane_cursor = at.saturating_sub(1),
        KeyCode::Char('g') | KeyCode::Home => app.gui.pane_cursor = 0,
        KeyCode::Char('G') | KeyCode::End => app.gui.pane_cursor = last,
        KeyCode::Enter | KeyCode::Char(' ') => match rows.get(at) {
            Some(PaneRow {
                folder: Some(path), ..
            }) => app.toggle_commit_folder(&path.clone()),
            Some(PaneRow {
                file: Some(path), ..
            }) => app.toggle_commit_file(&path.clone()),
            _ => {}
        },
        _ => list_key(app, key),
    }
}

/// A key while the view is open.
pub fn key(app: &mut App, key: KeyEvent) {
    if app.gui.pane_focus {
        pane_key(app, key);
    } else {
        list_key(app, key);
    }
}

/// The Files tab is shown.
fn files_tab(app: &App) -> bool {
    app.repo_view
        .as_ref()
        .is_some_and(|v| v.tab == RepoTab::Files)
}

fn list_key(app: &mut App, key: KeyEvent) {
    let all = rows(app);
    let at = cursor(app, all.len());
    let last = all.len().saturating_sub(1);
    let page = usize::from(app.gui.list_area.height.max(1));
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_repo_view();
            app.gui.view_cursor.clear();
            app.gui.pane_focus = false;
        }
        KeyCode::Char('1') => app.switch_repo_tab(RepoTab::Files),
        KeyCode::Char('2') => app.switch_repo_tab(RepoTab::Commits),
        KeyCode::Char('3') => app.switch_repo_tab(RepoTab::Graph),
        KeyCode::Char('4') => app.switch_repo_tab(RepoTab::Stash),
        KeyCode::Char('5') => app.switch_repo_tab(RepoTab::Reflog),
        KeyCode::Char('[') | KeyCode::Backspace => app.repo_view_back(),
        KeyCode::Char(']') => app.repo_view_forward(),
        KeyCode::Char('r') => app.refresh_repo_view(),
        KeyCode::Char('`') => app.toggle_file_tree(),
        KeyCode::Char('?') => app.gui.help = true,
        KeyCode::Tab => {
            app.gui.pane_focus = true;
            app.gui.pane_cursor = 0;
        }
        KeyCode::Char('c') if files_tab(app) => {
            app.gui.commit_cursor = usize::MAX;
            app.start_commit();
        }
        KeyCode::Char('c') => {
            if let Some(commit) = super::commits::picked(app) {
                app.gui.mode = Mode::Menu(super::commits::copy_menu(&commit));
            }
        }
        KeyCode::Char('a')
            if app
                .repo_view
                .as_ref()
                .is_some_and(|v| v.tab != RepoTab::Stash) =>
        {
            if let Some(commit) = super::commits::picked(app) {
                app.start_apply(&commit.hash);
                app.gui.view_cursor.clear();
                app.gui.pane_focus = false;
            }
        }
        KeyCode::Char('p') => {
            if let Some(commit) = super::commits::picked(app) {
                app.gui.mode = Mode::Menu(super::commits::parents_menu(&commit));
            }
        }
        KeyCode::Char('j') | KeyCode::Down => {
            set_cursor(app, at.saturating_add(1).min(last));
            read_on(app, last);
        }
        KeyCode::Char('k') | KeyCode::Up => set_cursor(app, at.saturating_sub(1)),
        KeyCode::Char('g') | KeyCode::Home => set_cursor(app, 0),
        KeyCode::Char('G') | KeyCode::End => {
            set_cursor(app, last);
            read_on(app, last);
        }
        KeyCode::Char(' ' | 'P' | 'd')
            if all.get(at).and_then(|r| stash_name(app, r)).is_some() =>
        {
            if let Some(name) = all.get(at).and_then(|r| stash_name(app, r)) {
                let act = match key.code {
                    KeyCode::Char(' ') => StashAct::Apply,
                    KeyCode::Char('P') => StashAct::Pop,
                    _ => StashAct::Drop,
                };
                app.stash_act(act, &name);
            }
        }
        KeyCode::Char(' ') if files_tab(app) => {
            if let Some(path) = all.get(at).and_then(row_path) {
                app.toggle_staged(&path);
            }
        }
        KeyCode::Char('o') if all.get(at).and_then(|r| graph_commit(app, r)).is_some() => {
            if let Some(hash) = all.get(at).and_then(|r| graph_commit(app, r)) {
                check_out_row(app, &hash);
            }
        }
        KeyCode::Char('e') if files_tab(app) => {
            if let Some(path) = all.get(at).and_then(row_path).filter(|p| !p.ends_with('/')) {
                app.edit_file(&path);
            }
        }
        KeyCode::Char('d') if files_tab(app) => {
            if let Some(path) = all.get(at).and_then(row_path) {
                app.gui.mode = Mode::Menu(file_menu(&path, true, false));
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(row) = all.get(at) {
                pick(app, row);
            }
        }
        KeyCode::Char('J') => app.gui.pane_scroll = app.gui.pane_scroll.saturating_add(1),
        KeyCode::Char('K') => app.gui.pane_scroll = app.gui.pane_scroll.saturating_sub(1),
        KeyCode::PageDown => {
            app.gui.pane_scroll = app.gui.pane_scroll.saturating_add(page);
        }
        KeyCode::PageUp => app.gui.pane_scroll = app.gui.pane_scroll.saturating_sub(page),
        KeyCode::Char('y') => {
            if let Some(text) = app.text_of(&TextOf::Pane) {
                copy(app, text);
            }
        }
        _ => {}
    }
}

/// Draw the view over the whole screen.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let Some(view) = app.repo_view.clone() else {
        return;
    };
    let [head, tabs, body, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(title(&view.repo), Style::new().add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", view.repo.display()), dim()),
            Span::styled(
                app.view_detached()
                    .map(|d| format!("  {d}"))
                    .unwrap_or_default(),
                Style::new().fg(Color::Yellow),
            ),
            Span::styled("   Tab pane  r refresh  y copy all  Esc close", dim()),
        ])),
        head,
    );
    let mut tab_spans = Vec::new();
    let nav = |enabled: bool, text: &str| {
        Span::styled(text.to_owned(), if enabled { Style::new() } else { dim() })
    };
    tab_spans.push(nav(view.trail.can_back(), "[ back"));
    tab_spans.push(Span::raw("  "));
    tab_spans.push(nav(view.trail.can_forward(), "] forward"));
    tab_spans.push(Span::raw("   "));
    for (key, tab) in (1..).zip(RepoTab::ALL) {
        let style = if tab == view.tab {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        tab_spans.push(Span::styled(format!(" {key} {} ", tab.label()), style));
        tab_spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tabs);
    // The graph needs room for lanes, names and titles.
    let share = if view.tab == RepoTab::Graph { 60 } else { 40 };
    let [list_area, pane_area] = Layout::horizontal([
        Constraint::Percentage(share),
        Constraint::Percentage(100 - share),
    ])
    .areas(body);
    let all = rows(app);
    let at = cursor(app, all.len());
    let height = usize::from(list_area.height.saturating_sub(1));
    let top = at.saturating_sub(height.saturating_sub(1));
    let picked = |row: &ViewRow| match (&row.pick, &view.selected) {
        (Pick::Select(target), Some(selected)) => target == selected,
        _ => false,
    };
    let lines: Vec<Line> = all
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(i, row)| {
            let mut line = row.line.clone();
            if picked(row) {
                line = line.style(Style::new().add_modifier(Modifier::BOLD));
            }
            if i == at {
                line = line.patch_style(Style::new().add_modifier(Modifier::REVERSED));
            }
            line
        })
        .collect();
    let heading = match (view.tab, &view.details) {
        (RepoTab::Files, Some(Ok(details))) => format!(" Changed files ({}) ", details.files.len()),
        (tab, _) => format!(" {} ", tab.label()),
    };
    let block = Block::new()
        .borders(Borders::TOP | Borders::RIGHT)
        .border_style(dim())
        .title(heading);
    let inner = block.inner(list_area);
    frame.render_widget(Paragraph::new(lines).block(block), list_area);
    app.gui.list_area = inner;
    app.gui.list_top = top;
    let pane = pane_rows(&view);
    let room = usize::from(pane_area.height.saturating_sub(1));
    let focused = app.gui.pane_focus;
    let cursor_at = app.gui.pane_cursor.min(pane.len().saturating_sub(1));
    // With the keys the pane follows its cursor.
    if focused {
        if cursor_at < app.gui.pane_scroll {
            app.gui.pane_scroll = cursor_at;
        } else if cursor_at >= app.gui.pane_scroll + room {
            app.gui.pane_scroll = cursor_at + 1 - room.max(1);
        }
    }
    let scroll = app.gui.pane_scroll.min(pane.len().saturating_sub(room));
    app.gui.pane_scroll = scroll;
    let shown: Vec<Line> = pane
        .into_iter()
        .enumerate()
        .skip(scroll)
        .take(room)
        .map(|(i, row)| {
            if focused && i == cursor_at {
                row.line
                    .patch_style(Style::new().add_modifier(Modifier::REVERSED))
            } else {
                row.line
            }
        })
        .collect();
    let border = if focused {
        Style::new().fg(Color::Cyan)
    } else {
        dim()
    };
    let block = Block::new().borders(Borders::TOP).border_style(border);
    frame.render_widget(Paragraph::new(shown).block(block), pane_area);
    let toast = app
        .toasts
        .last()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(Line::styled(toast, Style::new().fg(Color::Yellow))),
        status,
    );
}

/// A click in the list picks its row; the wheel scrolls the list, or the
/// pane under the pointer.
pub fn mouse(app: &mut App, column: u16, row: u16, kind: MouseAt) {
    let area = app.gui.list_area;
    let in_list = column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height);
    let all = rows(app);
    let at = cursor(app, all.len());
    let last = all.len().saturating_sub(1);
    match (kind, in_list) {
        (MouseAt::Click, true) => {
            let index = app.gui.list_top + usize::from(row.saturating_sub(area.y));
            if let Some(entry) = all.get(index) {
                set_cursor(app, index);
                pick(app, entry);
            }
        }
        (MouseAt::RightClick, true) => {
            let index = app.gui.list_top + usize::from(row.saturating_sub(area.y));
            if let Some(name) = all.get(index).and_then(|r| stash_name(app, r)) {
                set_cursor(app, index);
                app.gui.mode = Mode::Menu(stash_menu(&name));
                return;
            }
            if let Some(hash) = all.get(index).and_then(|r| graph_commit(app, r)) {
                set_cursor(app, index);
                app.gui.mode = Mode::Menu(checkout_menu(app, &hash));
                return;
            }
            if let Some(path) = all.get(index).filter(|_| files_tab(app)).and_then(row_path) {
                set_cursor(app, index);
                let editable = app.can_edit(&path);
                app.gui.mode = Mode::Menu(file_menu(&path, false, editable));
            }
        }
        (MouseAt::RightClick, false) => {}
        (MouseAt::Down, true) => set_cursor(app, at.saturating_add(3).min(last)),
        (MouseAt::Up, true) => set_cursor(app, at.saturating_sub(3)),
        (MouseAt::Down, false) => app.gui.pane_scroll = app.gui.pane_scroll.saturating_add(3),
        (MouseAt::Up, false) => app.gui.pane_scroll = app.gui.pane_scroll.saturating_sub(3),
        (MouseAt::Click, false) => {}
    }
}

/// What the mouse did in the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAt {
    Click,
    RightClick,
    Down,
    Up,
}
