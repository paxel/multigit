//! Settings and About in the terminal: a page of rows, one per setting,
//! changed with the arrow keys or Enter. Sounds, the Kawaii music and the
//! terminal command are left out: they have nothing to do in a terminal.

use super::App;
use super::actions::{Action, Prompt, PromptFor, copy};
use super::state::Mode;
use super::widgets::{Input, Item, Menu};
use crate::app::SettingsPage;
use crate::domain::about;
use crate::domain::config::{ButtonDef, Theme, button_presets};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use std::path::PathBuf;

/// What a row of a page is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Heading(String),
    Note(String),
    FetchInterval,
    Concurrency,
    ScanDepth,
    Theme,
    SingleMatch,
    Editor,
    ChipsInterval,
    ChipsGitHub,
    ChipsGitLab,
    Button(usize),
    AddButton,
    AddPreset,
    UnhideAll,
    Hidden(PathBuf),
    About,
    /// An About link: its URL, or the mail address of a `mailto:`.
    Link(&'static str, String),
    Licenses,
    /// A crate of the binary and its licence.
    License(String),
    Back,
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The rows of the shown page.
pub fn rows(app: &App) -> Vec<Row> {
    match app.settings_page {
        SettingsPage::Settings => {
            let mut rows = vec![
                Row::FetchInterval,
                Row::Concurrency,
                Row::ScanDepth,
                Row::Theme,
                Row::SingleMatch,
                Row::Editor,
                Row::ChipsInterval,
                Row::ChipsGitHub,
                Row::ChipsGitLab,
                Row::Heading("Card buttons".to_owned()),
            ];
            rows.extend((0..app.config.buttons.len()).map(Row::Button));
            rows.push(Row::AddButton);
            rows.push(Row::AddPreset);
            if !app.config.hidden.is_empty() {
                rows.push(Row::Heading(format!(
                    "Hidden repositories ({})",
                    app.config.hidden.len()
                )));
                rows.push(Row::UnhideAll);
                rows.extend(app.config.hidden.iter().cloned().map(Row::Hidden));
            }
            rows.push(Row::Heading(String::new()));
            rows.push(Row::About);
            rows
        }
        SettingsPage::About => {
            let mut rows = vec![
                Row::Heading(format!("multigit {}", about::VERSION)),
                Row::Note(about::TAGLINE.to_owned()),
                Row::Heading(String::new()),
            ];
            for link in about::LINKS {
                let target = link
                    .url
                    .strip_prefix("mailto:")
                    .map(|rest| rest.split('?').next().unwrap_or(rest).to_owned())
                    .unwrap_or_else(|| link.url.to_owned());
                rows.push(Row::Link(link.title, target));
            }
            rows.push(Row::Licenses);
            rows.push(Row::Back);
            rows
        }
        SettingsPage::Licenses => {
            let mut rows = vec![Row::Back];
            rows.extend(
                about::crate_licenses()
                    .into_iter()
                    .map(|l| Row::License(format!("{}  {}", l.krate, l.license))),
            );
            rows
        }
    }
}

/// Whether the cursor stops on the row.
fn active(row: &Row) -> bool {
    !matches!(row, Row::Heading(_) | Row::Note(_))
}

/// The row as text.
pub fn line(app: &App, row: &Row) -> Line<'static> {
    let config = &app.config;
    let value = |name: &str, value: String| {
        Line::from(vec![
            Span::raw(format!("{name:<30}")),
            Span::styled(
                format!("\u{25c0} {value} \u{25b6}"),
                Style::new().fg(Color::Cyan),
            ),
        ])
    };
    match row {
        Row::Heading(text) => Line::styled(text.clone(), Style::new().add_modifier(Modifier::BOLD)),
        Row::Note(text) => Line::styled(text.clone(), dim()),
        Row::FetchInterval => value(
            "Background fetch every",
            format!("{} s", config.fetch_interval_secs),
        ),
        Row::Concurrency => value("Parallel git processes", config.concurrency.to_string()),
        Row::ChipsInterval => value(
            "PR and CI chips every",
            format!("{} s", config.chips_interval_secs),
        ),
        Row::ChipsGitHub => {
            let mark = if config.chips_github { "[x]" } else { "[ ]" };
            Line::raw(format!("{mark} Chips for GitHub (gh)"))
        }
        Row::ChipsGitLab => {
            let mark = if config.chips_gitlab { "[x]" } else { "[ ]" };
            Line::raw(format!("{mark} Chips for GitLab (glab)"))
        }
        Row::ScanDepth => value("Folder scan depth", config.scan_depth.to_string()),
        Row::Theme => value("Theme", config.theme.label().to_owned()),
        Row::SingleMatch => {
            let mark = if config.auto_insert_single_match {
                "[x]"
            } else {
                "[ ]"
            };
            Line::raw(format!("{mark} Tab inserts a single completion directly"))
        }
        Row::Editor => Line::from(vec![
            Span::raw(format!("{:<30}", "Editor command")),
            Span::styled(
                if config.editor.is_empty() {
                    "$VISUAL, $EDITOR or vi".to_owned()
                } else {
                    config.editor.clone()
                },
                Style::new().fg(Color::Cyan),
            ),
        ]),
        Row::Button(i) => match config.buttons.get(*i) {
            Some(b) => Line::from(vec![
                Span::raw(format!("  {:<16}", b.label)),
                Span::styled(format!("git {}", b.args), dim()),
            ]),
            None => Line::default(),
        },
        Row::AddButton => Line::raw("  + Add button"),
        Row::AddPreset => Line::raw("  + Add a preset\u{2026}"),
        Row::UnhideAll => Line::raw("  Unhide all"),
        Row::Hidden(path) => Line::raw(format!("  Unhide {}", path.display())),
        Row::About => Line::raw("About multigit"),
        Row::Link(title, target) => Line::from(vec![
            Span::raw(format!("{title:<28}")),
            Span::styled(target.clone(), dim()),
        ]),
        Row::Licenses => Line::raw("Open-source licenses"),
        Row::License(text) => Line::raw(text.clone()),
        Row::Back => Line::raw("Back"),
    }
}

/// Step a value by one in direction `up`.
fn step(app: &mut App, row: &Row, up: bool) {
    let config = &mut app.config;
    let by = |value: usize, min: usize, max: usize| {
        if up {
            value.saturating_add(1).min(max)
        } else {
            value.saturating_sub(1).max(min)
        }
    };
    match row {
        Row::FetchInterval => {
            let value = config.fetch_interval_secs;
            config.fetch_interval_secs = if up {
                value.saturating_add(10).min(86_400)
            } else {
                value.saturating_sub(10).max(10)
            };
        }
        Row::Concurrency => config.concurrency = by(config.concurrency, 1, 32),
        Row::ChipsInterval => {
            let value = config.chips_interval_secs;
            config.chips_interval_secs = if up {
                value.saturating_add(60).min(86_400)
            } else {
                value.saturating_sub(60).max(60)
            };
        }
        Row::ChipsGitHub => config.chips_github = !config.chips_github,
        Row::ChipsGitLab => config.chips_gitlab = !config.chips_gitlab,
        Row::ScanDepth => config.scan_depth = by(config.scan_depth, 1, 10),
        Row::Theme => {
            let at = Theme::ALL
                .iter()
                .position(|t| *t == config.theme)
                .unwrap_or(0);
            let n = Theme::ALL.len();
            let next = if up { (at + 1) % n } else { (at + n - 1) % n };
            config.theme = Theme::ALL.get(next).copied().unwrap_or_default();
        }
        Row::SingleMatch => {
            config.auto_insert_single_match = !config.auto_insert_single_match;
        }
        Row::Button(i) => {
            // \u{2190} moves the button up the list, \u{2192} down.
            let earlier = !up;
            let to = if earlier {
                i.checked_sub(1)
            } else {
                Some(i + 1).filter(|to| *to < config.buttons.len())
            };
            if let Some(to) = to {
                config.buttons.swap(*i, to);
                let at = app.gui.settings_cursor;
                app.gui.settings_cursor = if earlier {
                    at.saturating_sub(1)
                } else {
                    at + 1
                };
            }
        }
        _ => {}
    }
}

/// Enter on a row.
fn enter(app: &mut App, row: &Row) {
    match row {
        Row::SingleMatch | Row::Theme | Row::ChipsGitHub | Row::ChipsGitLab => step(app, row, true),
        Row::Editor => {
            app.gui.mode = Mode::Prompt(Prompt {
                title: "editor command ({file} stands for the file; empty: $EDITOR)".to_owned(),
                input: Input::new(app.config.editor.clone()),
                purpose: PromptFor::Editor,
            });
        }
        Row::Button(i) => {
            let label = app.config.buttons.get(*i).map(|b| b.label.clone());
            if let Some(label) = label {
                app.gui.mode = Mode::Prompt(Prompt {
                    title: "the button's label".to_owned(),
                    input: Input::new(label),
                    purpose: PromptFor::ButtonLabel(*i),
                });
            }
        }
        Row::AddButton => app.config.buttons.push(ButtonDef::new("new", "status")),
        Row::AddPreset => {
            let items = button_presets()
                .into_iter()
                .map(|p| {
                    let text = format!("{}  (git {})", p.label, p.args);
                    Item::new(text, Action::AddButton(p.label, p.args))
                })
                .collect();
            app.gui.mode = Mode::Menu(Menu::new("add a preset", items));
        }
        Row::UnhideAll => app.unhide_all(),
        Row::Hidden(path) => app.unhide(path),
        Row::About => go(app, SettingsPage::About),
        Row::Licenses => go(app, SettingsPage::Licenses),
        Row::Link(_, target) => copy(app, target.clone()),
        Row::Back => back(app),
        _ => {}
    }
}

fn go(app: &mut App, page: SettingsPage) {
    app.settings_page = page;
    app.gui.settings_cursor = 0;
}

/// Esc, or Back: Licenses to About, About to Settings, Settings closes.
fn back(app: &mut App) {
    match app.settings_page {
        SettingsPage::Licenses => go(app, SettingsPage::About),
        SettingsPage::About => go(app, SettingsPage::Settings),
        SettingsPage::Settings => {
            app.settings_open = false;
            app.gui.settings_cursor = 0;
        }
    }
}

/// The cursor's row: the remembered one, moved to the next row it can
/// stop on.
pub fn cursor(app: &App, rows: &[Row]) -> usize {
    let at = app.gui.settings_cursor.min(rows.len().saturating_sub(1));
    (at..rows.len())
        .chain((0..at).rev())
        .find(|&i| rows.get(i).is_some_and(active))
        .unwrap_or(0)
}

fn move_cursor(app: &mut App, rows: &[Row], down: bool) {
    let at = cursor(app, rows);
    let next = if down {
        (at + 1..rows.len()).find(|&i| rows.get(i).is_some_and(active))
    } else {
        (0..at).rev().find(|&i| rows.get(i).is_some_and(active))
    };
    app.gui.settings_cursor = next.unwrap_or(at);
}

/// A key while Settings is open.
pub fn key(app: &mut App, key: KeyEvent) {
    let rows = rows(app);
    let at = cursor(app, &rows);
    let row = rows.get(at).cloned();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => back(app),
        KeyCode::Char('j') | KeyCode::Down => move_cursor(app, &rows, true),
        KeyCode::Char('k') | KeyCode::Up => move_cursor(app, &rows, false),
        KeyCode::Char('?') => app.gui.help = true,
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(row) = row {
                step(app, &row, false);
            }
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(row) = row {
                step(app, &row, true);
            }
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            if let Some(row) = row {
                enter(app, &row);
            }
        }
        KeyCode::Char('a') => {
            if let Some(Row::Button(i)) = row {
                let args = app.config.buttons.get(i).map(|b| b.args.clone());
                if let Some(args) = args {
                    app.gui.mode = Mode::Prompt(Prompt {
                        title: "the button's git arguments, or ! and a shell line".to_owned(),
                        input: Input::new(args),
                        purpose: PromptFor::ButtonArgs(i),
                    });
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(Row::Button(i)) = row
                && i < app.config.buttons.len()
            {
                app.config.buttons.remove(i);
            }
        }
        _ => {}
    }
}

/// Draw the open page over the whole screen.
pub fn draw(frame: &mut Frame, app: &App) {
    let title = match app.settings_page {
        SettingsPage::Settings => " Settings ",
        SettingsPage::About => " About multigit ",
        SettingsPage::Licenses => " Open-source licenses ",
    };
    let [body, hint] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(frame.area());
    let block = Block::bordered().title(title);
    let inner = block.inner(body);
    let rows = rows(app);
    let at = cursor(app, &rows);
    let height = usize::from(inner.height);
    let top = at.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(i, row)| {
            let line = line(app, row);
            if i == at {
                line.patch_style(Style::new().add_modifier(Modifier::REVERSED))
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), body);
    let text = match (app.settings_page, rows.get(at)) {
        (SettingsPage::Settings, Some(Row::Button(_))) => {
            "Enter: label  a: git arguments  \u{2190}/\u{2192}: move  d: remove  Esc: close"
        }
        (SettingsPage::Settings, _) => "j/k: row  \u{2190}/\u{2192}: change  Enter: do  Esc: close",
        (SettingsPage::About, _) => {
            "Enter on a link copies it: there is no browser here  Esc: back"
        }
        (SettingsPage::Licenses, _) => "j/k: scroll  Esc: back",
    };
    let toast = app.toasts.last().map(|t| t.text.clone());
    let line = Line::styled(toast.unwrap_or_else(|| text.to_owned()), dim());
    frame.render_widget(Paragraph::new(line), hint);
}
