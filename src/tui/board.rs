//! Drawing the board: a table with one row per repository, the full card of
//! the selected row under it, and a status line.

use super::App;
use super::cli;
use super::output;
use super::state::{Mode, selected_index};
use super::widgets::Menu;
use crate::app::{Card, MarkBy};
use crate::domain::command::{Job, display};
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
const DETAIL_HEIGHT: u16 = 8;

/// The keys, as the help shows them.
pub const HELP: &[(&str, &str)] = &[
    ("j / k, arrows", "select a row (the wheel too)"),
    ("g / G, Home / End", "first / last row"),
    ("space, click", "mark or unmark the row"),
    ("a d b p", "mark all / dirty / behind / ahead"),
    ("D B P", "show only dirty / behind / ahead"),
    ("A", "show all again"),
    ("/", "filter by name (Enter, Esc: done)"),
    ("s", "sort"),
    ("x, right click", "the row's menu: copy, and more"),
    ("1 .. 9", "run that card button on the row"),
    ("m", "run a card button on all marked rows"),
    ("t", "a shell in the row's folder (exit: back)"),
    ("o", "hide or show the output panel"),
    ("Tab", "to the output panel and back"),
    ("  j / k, Enter", "move / fold a section (a click too)"),
    ("  e / E", "open the command / the section in $PAGER"),
    ("  C", "clear the output"),
    (":", "command line: git arguments, ! for a shell line"),
    ("  Tab", "complete; in the list: next"),
    ("  Up / Down", "history entries with the typed text"),
    ("  Ctrl+R", "search the history"),
    ("  Right", "take the grey suggestion"),
    (
        "  Enter / Esc",
        "run in the marked rows / back to the board",
    ),
    ("?", "show or hide this help"),
    ("q", "quit"),
];

/// Draw everything.
pub fn draw(frame: &mut Frame, app: &mut App) {
    // Nothing to show yet: the panel takes no room from the table.
    let panel = if app.config.output_hidden || app.log.is_empty() {
        0
    } else {
        (frame.area().height / 4).max(5)
    };
    let [top, table, detail, out, command, status] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(DETAIL_HEIGHT),
        Constraint::Length(panel),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(Paragraph::new(toolbar_lines(app)), top);
    if app.gui.mode == Mode::Filter {
        // The filter text ends the first line; the cursor stands after it.
        let before = toolbar_lines(app).first().map_or(0, Line::width);
        let x = u16::try_from(before).unwrap_or(u16::MAX);
        frame.set_cursor_position((top.x.saturating_add(x), top.y));
    }
    draw_table(frame, app, table);
    draw_detail(frame, app, detail);
    frame.render_widget(Paragraph::new(status_line(app)), status);
    if panel > 0 {
        draw_output(frame, app, out);
    } else {
        app.gui.output_area = Rect::default();
    }
    draw_cli(frame, app, command);
    if let Mode::Menu(menu) = &app.gui.mode {
        let menu = menu.clone();
        app.gui.menu_rows = draw_menu(frame, &menu);
    }
    if app.confirm.is_some() {
        draw_confirm(frame, app);
    }
    if app.gui.help {
        draw_help(frame);
    }
}

fn toolbar_lines(app: &App) -> Vec<Line<'static>> {
    let shown = app.shown_cards().len();
    let all = app.cards.len();
    let count = if shown == all {
        repositories(all)
    } else {
        format!("{shown} of {}", repositories(all))
    };
    let mut first = vec![
        Span::styled("multigit", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {count}")),
    ];
    let marked = app.marked_repos().len();
    if marked > 0 {
        first.push(Span::styled(
            format!(", {marked} marked"),
            Style::new().fg(Color::Cyan),
        ));
    }
    first.push(Span::styled(format!("  sort: {}", app.sort.label()), dim()));
    let filtering = app.gui.mode == Mode::Filter;
    if filtering || !app.filter.is_empty() {
        let style = if filtering {
            Style::new().add_modifier(Modifier::UNDERLINED)
        } else {
            Style::new()
        };
        first.push(Span::raw("  /"));
        first.push(Span::styled(app.filter.clone(), style));
    } else {
        first.push(Span::styled("   ? keys  q quit", dim()));
    }
    let mut second = vec![Span::styled("mark ", dim())];
    for (key, by) in [
        ('a', MarkBy::All),
        ('d', MarkBy::Dirty),
        ('b', MarkBy::Behind),
        ('p', MarkBy::Ahead),
    ] {
        second.push(mark_button(app, key, by));
        second.push(Span::raw(" "));
    }
    if let Some(by) = app.status_filter {
        second.push(Span::styled(
            format!(" showing {} ({shown})", by.label()),
            Style::new().fg(Color::Magenta),
        ));
        second.push(Span::styled("  A: all", dim()));
    }
    vec![Line::from(first), Line::from(second)]
}

/// A mark button: its key and name, reversed while pressed and struck
/// through when it marks the other cards.
fn mark_button(app: &App, key: char, by: MarkBy) -> Span<'static> {
    let mut style = Style::new();
    match app.pressed_mark {
        Some((pressed, inverted)) if pressed == by => {
            style = style.add_modifier(Modifier::REVERSED);
            if inverted {
                style = style.add_modifier(Modifier::CROSSED_OUT);
            }
        }
        _ => {}
    }
    if app.status_filter == Some(by) {
        style = style.fg(Color::Magenta);
    }
    Span::styled(format!("[{key}] {}", by.label()), style)
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
    let mut first = vec![
        Span::styled(title(&card.path), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {}", card.path.display()), dim()),
    ];
    // The card buttons with their keys.
    for (key, button) in (1..=9).zip(&app.config.buttons) {
        first.push(Span::styled(
            format!("  {key}"),
            Style::new().fg(Color::Cyan),
        ));
        first.push(Span::raw(format!(" {}", button.label)));
    }
    let mut lines = vec![Line::from(first)];
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
    let mut status = vec![
        label("status"),
        Span::raw(tree_text(snap)),
        Span::raw(format!("   {}", sync_text(snap))),
    ];
    let (last, style) = last_text(card, app.is_busy(&card.path));
    if !last.is_empty() {
        status.push(Span::styled(format!("   last command: {last}"), style));
    }
    lines.push(Line::from(status));
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

/// Draw an open menu in the middle of the screen; returns the screen line
/// of each shown item.
pub fn draw_menu(frame: &mut Frame, menu: &Menu) -> Vec<(u16, usize)> {
    let shown = menu.shown();
    let widest = shown
        .iter()
        .map(|item| item.label.chars().count())
        .chain([menu.title.chars().count() + 2, 24])
        .max()
        .unwrap_or(24);
    let screen = frame.area();
    let width = u16::try_from(widest + 4)
        .unwrap_or(u16::MAX)
        .min(screen.width);
    let filter_lines: u16 = if menu.filter.is_some() { 1 } else { 0 };
    let room = screen.height.saturating_sub(2 + filter_lines).max(1);
    let listed = u16::try_from(shown.len().max(1))
        .unwrap_or(u16::MAX)
        .min(room);
    let height = (listed + 2 + filter_lines).min(screen.height);
    let area = Rect {
        x: screen.width.saturating_sub(width) / 2,
        y: screen.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, area);
    let block = Block::bordered().title(format!(" {} ", menu.title));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    if let Some(filter) = &menu.filter {
        lines.push(Line::from(vec![
            Span::styled("filter: ", dim()),
            Span::raw(filter.text.clone()),
        ]));
        let x = inner
            .x
            .saturating_add(8)
            .saturating_add(filter.cursor_column());
        frame.set_cursor_position((x, inner.y));
    }
    // Keep the selected item in view.
    let skip = menu
        .selected
        .saturating_sub(usize::from(listed).saturating_sub(1));
    let mut rows = Vec::new();
    if shown.is_empty() {
        lines.push(Line::styled(menu.none.clone(), dim()));
    }
    for (index, item) in shown
        .iter()
        .enumerate()
        .skip(skip)
        .take(usize::from(listed))
    {
        let mut style = if item.enabled { Style::new() } else { dim() };
        if index == menu.selected && item.enabled {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let y = inner
            .y
            .saturating_add(u16::try_from(lines.len()).unwrap_or(u16::MAX));
        rows.push((y, index));
        lines.push(Line::styled(item.label.clone(), style));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    rows
}

fn draw_cli(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.gui.mode == Mode::Command;
    let text = app.gui.cli.input.text.clone();
    let mut label = cli::label(&text);
    let mut spans = Vec::new();
    if app.config.output_hidden
        && let Some(outcome) = app.unseen_outcome()
    {
        // The hidden panel has news: running, then green or red.
        let color = match outcome {
            None => Color::Yellow,
            Some(true) => Color::Green,
            Some(false) => Color::Red,
        };
        spans.push(Span::styled("\u{2022} ", Style::new().fg(color)));
        label.insert_str(0, "  ");
    }
    spans.push(Span::styled(
        cli::label(&text),
        Style::new().add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    if text.is_empty() && !focused {
        let hint = if app.marked_repos().is_empty() {
            "mark repositories to run commands"
        } else {
            ": to type git arguments, ! for a shell line"
        };
        spans.push(Span::styled(hint, dim()));
    } else {
        spans.push(Span::raw(text));
        if focused && let Some(rest) = cli::suggestion(app) {
            spans.push(Span::styled(rest, dim()));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
    if !focused {
        return;
    }
    let start = u16::try_from(label.chars().count() + 1).unwrap_or(u16::MAX);
    let x = area
        .x
        .saturating_add(start)
        .saturating_add(app.gui.cli.input.cursor_column());
    frame.set_cursor_position((x, area.y));
    let Some(popup) = app.gui.cli.popup else {
        return;
    };
    let rows = cli::rows(app);
    let height = u16::try_from(rows.len().clamp(1, 10))
        .unwrap_or(10)
        .saturating_add(2)
        .min(area.y);
    let popup_area = Rect {
        x: area.x,
        y: area.y.saturating_sub(height),
        width: area.width.min(80),
        height,
    };
    let listed = usize::from(height.saturating_sub(2));
    let selected = popup.selected % rows.len().max(1);
    let skip = selected.saturating_sub(listed.saturating_sub(1));
    let mut lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(skip)
        .take(listed)
        .map(|(i, row)| {
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            let mut spans = vec![Span::styled(row.text.clone(), style)];
            if let Some(tag) = &row.tag {
                spans.push(Span::styled(format!("  {tag}"), warn()));
            }
            if let Some(description) = &row.description {
                spans.push(Span::styled(format!("  {description}"), dim()));
            }
            Line::from(spans)
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::styled("no matches", dim()));
    }
    let title = match popup.kind {
        cli::PopupKind::Completion => " complete ",
        cli::PopupKind::History => " history ",
    };
    frame.render_widget(Clear, popup_area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title)),
        popup_area,
    );
}

/// The question before a shell line, or a destructive command in more than
/// one repository, runs.
fn draw_confirm(frame: &mut Frame, app: &App) {
    let Some(pending) = &app.confirm else {
        return;
    };
    let names: Vec<String> = pending.repos.iter().map(|p| title(p)).collect();
    let (heading, question) = match &pending.job {
        Job::Shell(line) => (
            " Shell command ",
            format!(
                "Run `{line}` with {} in {}?",
                cli::login_shell_name(),
                repositories(names.len())
            ),
        ),
        Job::Git(_) => (
            " Destructive command ",
            format!(
                "Run `{}` in {}? This can discard work.",
                display(&pending.command),
                repositories(names.len())
            ),
        ),
    };
    let screen = frame.area();
    let width = 70.min(screen.width);
    let area = Rect {
        x: screen.width.saturating_sub(width) / 2,
        y: screen.height.saturating_sub(8) / 2,
        width,
        height: 8.min(screen.height),
    };
    let lines = vec![
        Line::raw(question),
        Line::styled(names.join(", "), dim()),
        Line::default(),
        Line::from(vec![
            Span::styled("Enter", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" runs, "),
            Span::styled("Esc", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" cancels"),
        ]),
    ];
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(ratatui::widgets::Wrap { trim: true })
            .block(Block::bordered().title(heading)),
        area,
    );
}

fn draw_output(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.gui.mode == Mode::Output;
    let hint = if focused {
        " output  Tab: board  e/E: pager  C: clear "
    } else {
        " output  Tab: focus  o: hide "
    };
    let border = if focused {
        Style::new().fg(Color::Cyan)
    } else {
        dim()
    };
    let block = Block::new()
        .borders(Borders::TOP)
        .border_style(border)
        .title(hint);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let all = output::lines(app);
    let height = usize::from(inner.height);
    let at = output::cursor(app, all.len());
    // The cursor line stays in view; without a cursor the newest lines show.
    let mut top = app.gui.output_top.min(all.len().saturating_sub(height));
    if app.gui.output_cursor.is_none() {
        top = all.len().saturating_sub(height);
    } else if at < top {
        top = at;
    } else if at >= top + height {
        top = at + 1 - height;
    }
    let lines: Vec<Line> = all
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(i, line)| line.to_line(focused && i == at))
        .collect();
    let empty = lines.is_empty();
    frame.render_widget(Paragraph::new(lines), inner);
    if empty {
        frame.render_widget(Paragraph::new("No commands yet.").style(dim()), inner);
    }
    app.gui.output_top = top;
    app.gui.output_area = inner;
    app.output_shown();
}
