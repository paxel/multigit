//! The output panel: the last commands, one section per repository with
//! its exit code, folded or not, and opening it in the pager.

use super::App;
use crate::domain::command::display;
use crate::domain::naming::title;
use crate::domain::output::SectionState;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use std::path::PathBuf;

/// One line of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutLine {
    /// A run's command line.
    Run(u64, String),
    /// A repository's section header: run, folder, its status, whether it
    /// is folded and its outcome so far.
    Section {
        run: u64,
        repo: PathBuf,
        status: String,
        folded: bool,
        outcome: Option<bool>,
        running: bool,
    },
    /// A line a command printed; `err` for its error output.
    Text {
        run: u64,
        repo: PathBuf,
        text: String,
        err: bool,
    },
}

impl OutLine {
    /// The run and, but for a run line, the repository this line is part
    /// of.
    pub fn place(&self) -> (u64, Option<PathBuf>) {
        match self {
            Self::Run(run, _) => (*run, None),
            Self::Section { run, repo, .. } | Self::Text { run, repo, .. } => {
                (*run, Some(repo.clone()))
            }
        }
    }

    pub fn to_line(&self, cursor: bool) -> Line<'static> {
        let mark = |style: Style| {
            if cursor {
                style.add_modifier(Modifier::REVERSED)
            } else {
                style
            }
        };
        match self {
            Self::Run(_, command) => Line::styled(
                format!("$ {}", display(command)),
                mark(Style::new().add_modifier(Modifier::BOLD)),
            ),
            Self::Section {
                repo,
                status,
                folded,
                outcome,
                running,
                ..
            } => {
                let color = match (outcome, running) {
                    (Some(true), _) => Color::Green,
                    (Some(false), _) => Color::Red,
                    (None, true) => Color::Yellow,
                    (None, false) => Color::DarkGray,
                };
                let arrow = if *folded { "\u{25b8}" } else { "\u{25be}" };
                Line::from(vec![Span::styled(
                    format!("{arrow} {}  {status}", title(repo)),
                    mark(Style::new().fg(color)),
                )])
            }
            Self::Text { text, err, .. } => {
                let style = if *err {
                    Style::new().fg(Color::DarkGray)
                } else {
                    Style::new()
                };
                Line::styled(format!("  {text}"), mark(style))
            }
        }
    }
}

/// Every line of the panel, oldest run first.
pub fn lines(app: &App) -> Vec<OutLine> {
    let mut lines = Vec::new();
    for run in app.log.runs() {
        lines.push(OutLine::Run(run.id, run.command.clone()));
        for section in &run.sections {
            let folded = app.gui.folded.contains(&(run.id, section.repo.clone()));
            lines.push(OutLine::Section {
                run: run.id,
                repo: section.repo.clone(),
                status: section.status(),
                folded,
                outcome: section.state.outcome(),
                running: section.state == SectionState::Running,
            });
            if folded {
                continue;
            }
            let text = |text: &str, err: bool| {
                text.trim_end()
                    .lines()
                    .map(|line| OutLine::Text {
                        run: run.id,
                        repo: section.repo.clone(),
                        text: line.to_owned(),
                        err,
                    })
                    .collect::<Vec<_>>()
            };
            match &section.state {
                SectionState::Done(out) => {
                    lines.extend(text(&out.stdout, false));
                    lines.extend(text(&out.stderr, true));
                }
                SectionState::SpawnFailed(error) => lines.extend(text(error, true)),
                SectionState::Queued | SectionState::Running => {}
            }
        }
    }
    lines
}

/// The line the cursor is on: the remembered one, the last otherwise.
pub fn cursor(app: &App, count: usize) -> usize {
    app.gui
        .output_cursor
        .unwrap_or(usize::MAX)
        .min(count.saturating_sub(1))
}

/// Fold or unfold the section of `line`.
pub fn toggle_fold(app: &mut App, line: &OutLine) {
    if let (run, Some(repo)) = line.place()
        && !app.gui.folded.remove(&(run, repo.clone()))
    {
        app.gui.folded.insert((run, repo));
    }
}

/// What a key in the focused panel does: `false` when the focus goes back
/// to the board.
pub fn key(app: &mut App, key: KeyEvent) -> bool {
    let all = lines(app);
    let at = cursor(app, all.len());
    let last = all.len().saturating_sub(1);
    let page = usize::from(app.gui.output_area.height.saturating_sub(1)).max(1);
    let mut go = |to: usize| app.gui.output_cursor = Some(to.min(last));
    match key.code {
        KeyCode::Esc | KeyCode::Tab => return false,
        KeyCode::Char('o') => {
            app.config.output_hidden = true;
            return false;
        }
        KeyCode::Char('j') | KeyCode::Down => go(at.saturating_add(1)),
        KeyCode::Char('k') | KeyCode::Up => go(at.saturating_sub(1)),
        KeyCode::PageDown => go(at.saturating_add(page)),
        KeyCode::PageUp => go(at.saturating_sub(page)),
        KeyCode::Char('g') | KeyCode::Home => go(0),
        KeyCode::Char('G') | KeyCode::End => go(last),
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(line) = all.get(at) {
                toggle_fold(app, line);
                // The cursor stays on the section's header.
                let header = lines(app).iter().position(|l| {
                    matches!(l, OutLine::Section { .. }) && l.place() == line.place()
                });
                app.gui.output_cursor = header.or(Some(at));
            }
        }
        KeyCode::Char('e') => {
            if let Some(line) = all.get(at) {
                app.open_output(line.place().0, None);
            }
        }
        KeyCode::Char('E') => {
            if let Some((run, repo)) = all.get(at).map(OutLine::place) {
                app.open_output(run, repo.as_deref());
            }
        }
        KeyCode::Char('C') => {
            app.log.clear();
            app.gui.output_cursor = None;
        }
        _ => {}
    }
    true
}
