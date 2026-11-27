//! The Commits tab in the terminal: the rows of the history, and the pane
//! of a picked commit with its files.

use super::App;
use super::actions::Action;
use super::repo::{Pick, ViewRow, change_color, line_style};
use super::widgets::{Item, Menu};
use crate::app::{CommitRow, RepoView, commit_rows};
use crate::domain::details::{
    CommitDetail, Details, DiffTarget, LogEntry, initials, parse_commit, short_age,
};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// Seconds since 1970, now.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The color of an author: the same for the same name everywhere.
pub fn author_color(author: &str) -> Color {
    const COLORS: [Color; 6] = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
    ];
    let hue = crate::domain::details::author_hue(author);
    // The hue is in 0..1; six terminal colors stand for the circle.
    let at = (hue.clamp(0.0, 0.999) * 6.0) as usize;
    COLORS.get(at).copied().unwrap_or(Color::White)
}

/// A commit as a row: author's initials, short hash, its tags, title, age.
pub fn commit_line(commit: &LogEntry, now: i64) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!("{:<2} ", initials(&commit.author)),
            Style::new().fg(author_color(&commit.author)),
        ),
        Span::styled(format!("{} ", commit.short), dim()),
    ];
    for tag in &commit.tags {
        spans.push(Span::styled(
            format!("[{tag}] "),
            Style::new().fg(Color::Yellow),
        ));
    }
    spans.push(Span::raw(commit.subject.clone()));
    spans.push(Span::styled(
        format!("  {}", short_age(now, commit.time)),
        dim(),
    ));
    Line::from(spans)
}

/// The rows of the Commits tab.
pub fn rows(details: &Details, view: &RepoView) -> Vec<ViewRow> {
    if details.commits.is_empty() {
        return vec![ViewRow {
            line: Line::styled("no commits yet", dim()),
            pick: Pick::Nothing,
        }];
    }
    let now = now();
    let mut rows: Vec<ViewRow> = details
        .commits
        .iter()
        .map(|commit| ViewRow {
            line: commit_line(commit, now),
            pick: Pick::Select(DiffTarget::Commit(commit.hash.clone())),
        })
        .collect();
    if view.more_commits {
        rows.push(ViewRow {
            line: Line::styled("more commits are read\u{2026}", dim()),
            pick: Pick::Nothing,
        });
    }
    rows
}

/// One line of the commit pane; `file` is the path a pick folds or unfolds.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneRow {
    pub line: Line<'static>,
    /// The commit's file on this row, whose diff Enter unfolds.
    pub file: Option<String>,
    /// The folder of the commit's file tree on this row, which Enter folds.
    pub folder: Option<String>,
}

impl PaneRow {
    pub fn text(line: Line<'static>) -> Self {
        Self {
            line,
            file: None,
            folder: None,
        }
    }
}

fn counts((added, removed): (u32, u32)) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!(" +{added}"), Style::new().fg(Color::Green)),
        Span::styled(format!(" -{removed}"), Style::new().fg(Color::Red)),
    ]
}

/// The pane of a picked commit: hash and names, author, mail and date, the
/// committer, the parents, the whole message, then its files, each with
/// its diff when unfolded.
pub fn pane(view: &RepoView, raw: &str) -> Vec<PaneRow> {
    let Some(commit) = parse_commit(raw) else {
        return vec![PaneRow::text(Line::styled(
            "this commit cannot be read",
            dim(),
        ))];
    };
    let mut rows = Vec::new();
    let short: String = commit.hash.chars().take(10).collect();
    let mut head = vec![Span::styled(
        short,
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for name in &commit.refs {
        head.push(Span::styled(
            format!(" [{name}]"),
            Style::new().fg(Color::Yellow),
        ));
    }
    rows.push(PaneRow::text(Line::from(head)));
    rows.push(PaneRow::text(Line::from(vec![
        Span::styled(
            format!("{} ", initials(&commit.author)),
            Style::new().fg(author_color(&commit.author)),
        ),
        Span::raw(format!("{} <{}>  ", commit.author, commit.email)),
        Span::styled(commit.date.clone(), dim()),
    ])));
    if let Some((name, mail)) = &commit.committer {
        rows.push(PaneRow::text(Line::styled(
            format!("committed by {name} <{mail}>"),
            dim(),
        )));
    }
    if !commit.parents.is_empty() {
        let word = if commit.parents.len() == 1 {
            "parent"
        } else {
            "parents"
        };
        let parents: Vec<String> = commit
            .parents
            .iter()
            .map(|p| p.chars().take(10).collect())
            .collect();
        rows.push(PaneRow::text(Line::from(vec![
            Span::styled(format!("{word} "), dim()),
            Span::raw(parents.join(" ")),
            Span::styled("  (p goes there)", dim()),
        ])));
    }
    rows.push(PaneRow::text(Line::default()));
    for line in commit.message.lines() {
        rows.push(PaneRow::text(Line::raw(line.to_owned())));
    }
    rows.push(PaneRow::text(Line::default()));
    let count = commit.files.len();
    let mut files = vec![Span::styled(
        if count == 1 {
            "1 file changed".to_owned()
        } else {
            format!("{count} files changed")
        },
        Style::new().add_modifier(Modifier::BOLD),
    )];
    if count > 0 {
        files.extend(counts(commit.total_lines()));
    }
    rows.push(PaneRow::text(Line::from(files)));
    for row in commit_rows(&commit, view) {
        rows.push(match row {
            CommitRow::Folder(folder) => {
                let arrow = if folder.open { "\u{25be}" } else { "\u{25b8}" };
                let indent = "  ".repeat(folder.depth);
                PaneRow {
                    line: Line::raw(format!("{indent}{arrow} {}/", folder.name)),
                    file: None,
                    folder: Some(folder.path),
                }
            }
            CommitRow::File(row, open, lines) => {
                let file = row.file;
                let arrow = if open { "\u{25be}" } else { "\u{25b8}" };
                let indent = "  ".repeat(row.depth);
                let mut spans = vec![Span::styled(
                    format!("{indent}{arrow} {} {}", file.kind.letter(), row.name),
                    Style::new().fg(change_color(file.kind)),
                )];
                match lines {
                    Some(lines) => spans.extend(counts(lines)),
                    None => spans.push(Span::styled(" binary", dim())),
                }
                PaneRow {
                    line: Line::from(spans),
                    file: Some(file.path.clone()),
                    folder: None,
                }
            }
            CommitRow::Line(kind, line) => {
                PaneRow::text(Line::styled(format!("  {line}"), line_style(kind)))
            }
            CommitRow::Note(note) => PaneRow::text(Line::styled(format!("  {note}"), dim())),
        });
    }
    rows
}

/// The picked commit, read.
pub fn picked(app: &App) -> Option<CommitDetail> {
    let view = app.repo_view.as_ref()?;
    let Some(DiffTarget::Commit(_)) = &view.selected else {
        return None;
    };
    let Some(Ok(raw)) = &view.diff else {
        return None;
    };
    parse_commit(raw)
}

/// The picked commit's values to copy, as a click on them copies them in
/// the window.
pub fn copy_menu(commit: &CommitDetail) -> Menu {
    let mut items = vec![
        Item::new(
            format!("Copy hash {}", commit.hash),
            Action::Copy(commit.hash.clone()),
        ),
        Item::new(
            format!("Copy author {}", commit.author),
            Action::Copy(commit.author.clone()),
        ),
        Item::new(
            format!("Copy mail {}", commit.email),
            Action::Copy(commit.email.clone()),
        ),
        Item::new(
            format!("Copy date {}", commit.date),
            Action::Copy(commit.date.clone()),
        ),
        Item::new("Copy the message", Action::Copy(commit.message.clone())),
    ];
    for name in &commit.refs {
        items.push(Item::new(
            format!("Copy {name}"),
            Action::Copy(name.clone()),
        ));
    }
    Menu::new("copy", items)
}

/// The picked commit's parents to go to.
pub fn parents_menu(commit: &CommitDetail) -> Menu {
    let items: Vec<Item> = commit
        .parents
        .iter()
        .map(|p| Item::new(p.clone(), Action::GoToCommit(p.clone())))
        .collect();
    if items.is_empty() {
        return Menu::new("parents", vec![Item::disabled("the first commit has none")]);
    }
    Menu::new("go to parent", items)
}
