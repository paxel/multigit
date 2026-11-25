//! Drawing the board: a table with one row per repository, the full card of
//! the selected row under it, and a status line.

use super::App;
use super::state::selected_index;
use crate::app::Card;
use crate::domain::naming::title;
use crate::domain::remote::short_url;
use crate::domain::repo::Snapshot;
use crate::domain::status::{Head, RepoState, Upstream};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState};

/// Lines of the detail strip, its frame included.
const DETAIL_HEIGHT: u16 = 9;

/// The keys, as the help shows them.
pub const HELP: &[(&str, &str)] = &[
    ("j / k, arrows", "select a row (the wheel too)"),
    ("g / G, Home / End", "first / last row"),
    ("?", "show or hide this help"),
    ("q", "quit"),
];

/// Draw everything.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let [top, table, detail, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(DETAIL_HEIGHT),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(Paragraph::new(toolbar_line(app)), top);
    draw_table(frame, app, table);
    draw_detail(frame, app, detail);
    frame.render_widget(Paragraph::new(status_line(app)), status);
    if app.gui.help {
        draw_help(frame);
    }
}

fn toolbar_line(app: &App) -> Line<'static> {
    let shown = app.shown_cards().len();
    let all = app.cards.len();
    let count = if shown == all {
        repositories(all)
    } else {
        format!("{shown} of {}", repositories(all))
    };
    Line::from(vec![
        Span::styled("multigit", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {count}")),
        Span::styled("   ? keys  q quit", dim()),
    ])
}

/// "1 repository", "3 repositories".
pub fn repositories(n: usize) -> String {
    if n == 1 {
        "1 repository".to_owned()
    } else {
        format!("{n} repositories")
    }
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn ok() -> Style {
    Style::new().fg(Color::Green)
}

fn warn() -> Style {
    Style::new().fg(Color::Yellow)
}

fn bad() -> Style {
    Style::new().fg(Color::Red)
}

/// The branch as a card shows it.
pub fn branch_text(snap: &Snapshot) -> String {
    match &snap.status.head {
        Head::Branch(b) => b.clone(),
        Head::Unborn(b) => format!("{b} (no commits)"),
        Head::Detached(sha) => format!("detached @ {sha}"),
    }
}

/// The nearest tag, and how many more are on its commit.
pub fn tag_text(snap: &Snapshot) -> String {
    match snap.tags.split_first() {
        None => "no tag".to_owned(),
        Some((first, [])) => first.clone(),
        Some((first, rest)) => format!("{first} +{}", rest.len()),
    }
}

/// The working tree: `clean` or the counts, and an operation in progress.
pub fn tree_text(snap: &Snapshot) -> String {
    let c = snap.status.counts;
    let mut text = if c.is_clean() {
        "clean".to_owned()
    } else {
        format!(
            "+{} ~{} ?{} !{}",
            c.staged, c.modified, c.untracked, c.conflicts
        )
    };
    match snap.state {
        RepoState::Rebasing => text.push_str(" rebasing"),
        RepoState::Merging => text.push_str(" merging"),
        RepoState::Normal => {}
    }
    text
}

/// Unpushed and unpulled commits, or why there are none to count.
pub fn sync_text(snap: &Snapshot) -> String {
    match snap.status.upstream {
        Upstream::None => "no upstream".to_owned(),
        Upstream::Gone => "upstream gone".to_owned(),
        Upstream::Tracking { ahead, behind } => format!("\u{2191}{ahead} \u{2193}{behind}"),
    }
}

/// The last command's outcome, or that one is running.
pub fn last_text(card: &Card, busy: bool) -> (&'static str, Style) {
    if busy {
        return ("running", warn());
    }
    match card.last_command {
        Some(true) => ("ok", ok()),
        Some(false) => ("failed", bad()),
        None => ("", Style::new()),
    }
}

fn table_row(app: &App, card: &Card) -> Row<'static> {
    let busy = app.is_busy(&card.path);
    let (last, last_style) = last_text(card, busy);
    let mark = if app.marked.contains(&card.path) {
        "\u{25cf}"
    } else {
        " "
    };
    let fetch = if card.fetch_error.is_some() { "!" } else { " " };
    let name = Cell::from(Line::from(vec![
        Span::styled(mark, Style::new().fg(Color::Cyan)),
        Span::styled(fetch, bad()),
        Span::styled(title(&card.path), Style::new().add_modifier(Modifier::BOLD)),
    ]));
    let cells = match &card.probe {
        None => vec![name, Cell::from(Span::styled("loading", dim()))],
        Some(Err(e)) => vec![name, Cell::from(Span::styled(e.to_string(), bad()))],
        Some(Ok(snap)) => {
            let tree_style = if snap.status.counts.is_clean() && snap.state == RepoState::Normal {
                ok()
            } else {
                warn()
            };
            vec![
                name,
                Cell::from(branch_text(snap)),
                Cell::from(Span::styled(tag_text(snap), dim())),
                Cell::from(Span::styled(tree_text(snap), tree_style)),
                Cell::from(sync_text(snap)),
                Cell::from(Span::styled(last, last_style)),
            ]
        }
    };
    let row = Row::new(cells);
    if card.snapshot().is_none() {
        row.style(dim())
    } else {
        row
    }
}

fn draw_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let cards = app.shown_cards();
    let rows: Vec<Row> = cards.iter().map(|c| table_row(app, c)).collect();
    let paths: Vec<_> = cards.iter().map(|c| c.path.clone()).collect();
    let header = Row::new(["  repository", "branch", "tag", "tree", "sync", "last"]).style(dim());
    let table = Table::new(
        rows,
        [
            Constraint::Fill(3),
            Constraint::Fill(3),
            Constraint::Fill(2),
            Constraint::Length(18),
            Constraint::Length(13),
            Constraint::Length(7),
        ],
    )
    .header(header)
    .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    let mut state = TableState::default()
        .with_offset(app.gui.offset)
        .with_selected(selected_index(app));
    if paths.is_empty() {
        let empty = Paragraph::new("No repositories yet.").style(dim());
        frame.render_widget(empty, area);
    } else {
        frame.render_stateful_widget(table, area, &mut state);
    }
    app.gui.offset = state.offset();
    app.gui.table = area;
    let first_row = area.y.saturating_add(1);
    let visible = usize::from(area.height.saturating_sub(1));
    app.gui.rows_at = paths
        .into_iter()
        .skip(state.offset())
        .take(visible)
        .zip(first_row..)
        .map(|(path, y)| (y, path))
        .collect();
}

/// The full card of a repository, as lines.
pub fn detail_lines(app: &App, card: &Card) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(title(&card.path), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}", card.path.display()), dim()),
    ])];
    let snap = match &card.probe {
        None => {
            lines.push(Line::styled("loading", dim()));
            return lines;
        }
        Some(Err(e)) => {
            lines.push(Line::styled(e.to_string(), bad()));
            return lines;
        }
        Some(Ok(snap)) => snap,
    };
    let label = |text: &str| Span::styled(format!("{text:<8}"), dim());
    lines.push(Line::from(vec![
        label("branch"),
        Span::raw(branch_text(snap)),
    ]));
    let tags = if snap.tags.is_empty() {
        "no tag".to_owned()
    } else {
        snap.tags.join(" ")
    };
    lines.push(Line::from(vec![label("tag"), Span::raw(tags)]));
    if let Some(commit) = &snap.commit {
        lines.push(Line::from(vec![
            label("commit"),
            Span::raw(format!("{} {}", commit.short, commit.subject)),
        ]));
    }
    lines.push(Line::from(vec![
        label("status"),
        Span::raw(tree_text(snap)),
    ]));
    let mut remote = vec![label("remote")];
    match snap.remote() {
        Some(r) => remote.push(Span::raw(short_url(&r.url))),
        None if snap.remotes.is_empty() => remote.push(Span::styled("no remote", dim())),
        None => remote.push(Span::styled("no remote for this branch", dim())),
    }
    if let Some(push) = snap.push_remote() {
        remote.push(Span::styled(format!("  push: {}", push.name), warn()));
    }
    lines.push(Line::from(remote));
    let mut sync = vec![label("sync"), Span::raw(sync_text(snap))];
    let (last, style) = last_text(card, app.is_busy(&card.path));
    if !last.is_empty() {
        sync.push(Span::styled(format!("  last command: {last}"), style));
    }
    lines.push(Line::from(sync));
    if let Some(error) = &card.fetch_error {
        lines.push(Line::styled(error.card_text(), bad()));
    }
    lines
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::new().borders(Borders::TOP).border_style(dim());
    let selected = selected_index(app).and_then(|i| app.shown_cards().into_iter().nth(i));
    let lines = match selected {
        Some(card) => detail_lines(app, card),
        None => Vec::new(),
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn status_line(app: &App) -> Line<'static> {
    match app.toasts.last() {
        Some(toast) => Line::styled(toast.text.clone(), warn()),
        None => Line::default(),
    }
}

fn draw_help(frame: &mut Frame) {
    let width = 64.min(frame.area().width);
    let height = u16::try_from(HELP.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2)
        .min(frame.area().height);
    let area = Rect {
        x: frame.area().width.saturating_sub(width) / 2,
        y: frame.area().height.saturating_sub(height) / 2,
        width,
        height,
    };
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(key, what)| {
            Line::from(vec![
                Span::styled(format!("{key:<20}"), Style::new().fg(Color::Cyan)),
                Span::raw(*what),
            ])
        })
        .collect();
    let block = Block::bordered().title(" keys ");
    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
