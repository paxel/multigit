//! The command line at the bottom of the terminal: the same completion,
//! history, suggestion and search as the window's, on the same domain
//! logic.

use super::App;
use super::widgets::Input;
use crate::domain::command::{shell_line, shell_name, shell_program};
use crate::domain::completion::{TabAction, complete, replace_word, tab_action};
use crate::domain::history::HistoryNav;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Rows a popup lists at most.
const MAX_POPUP_ROWS: usize = 200;

/// What the open popup lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupKind {
    Completion,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Popup {
    pub kind: PopupKind,
    /// Wraps around the rows.
    pub selected: usize,
}

/// The command line's state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cli {
    pub input: Input,
    pub popup: Option<Popup>,
    nav: HistoryNav,
}

/// One popup row: what is shown and the line it makes when accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub description: Option<String>,
    pub tag: Option<String>,
    pub line: String,
    pub cursor: usize,
}

/// What a key on the command line asks of the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It stays focused.
    Stay,
    /// The focus goes back to the board.
    Leave,
}

/// The name in front of the line: `git`, or the shell for a `!` line.
pub fn label(text: &str) -> String {
    if shell_line(text).is_some() {
        login_shell_name()
    } else {
        "git".to_owned()
    }
}

pub fn login_shell_name() -> String {
    shell_name(&shell_program(std::env::var_os("SHELL")))
}

/// The rows of the open popup.
pub fn rows(app: &mut App) -> Vec<Row> {
    let Some(popup) = app.gui.cli.popup else {
        return Vec::new();
    };
    let text = app.gui.cli.input.text.clone();
    let cursor = app.gui.cli.input.cursor;
    match popup.kind {
        PopupKind::Completion => {
            let values = app.repo_values();
            let c = complete(&text, cursor, &app.vocab, &values);
            if let Some(cmd) = &c.missing_options_for {
                app.request_options(cmd);
            }
            c.candidates
                .iter()
                .take(MAX_POPUP_ROWS)
                .map(|cand| {
                    let (line, cursor) =
                        replace_word(&text, c.word_start, cursor, &cand.insertion());
                    Row {
                        text: cand.text.clone(),
                        description: cand.description.clone(),
                        tag: cand.tag.clone(),
                        line,
                        cursor,
                    }
                })
                .collect()
        }
        PopupKind::History => app
            .history
            .fuzzy(&text)
            .into_iter()
            .take(MAX_POPUP_ROWS)
            .map(|e| Row {
                text: e.to_owned(),
                description: None,
                tag: None,
                line: e.to_owned(),
                cursor: e.len(),
            })
            .collect(),
    }
}

/// The grey rest of the newest history entry that starts with the line,
/// while the cursor is at its end and no popup is open.
pub fn suggestion(app: &App) -> Option<String> {
    let cli = &app.gui.cli;
    if cli.popup.is_some() || cli.input.cursor != cli.input.text.len() {
        return None;
    }
    let full = app.history.suggestion(&cli.input.text)?;
    full.get(cli.input.text.len()..).map(str::to_owned)
}

fn on_tab(app: &mut App) {
    if let Some(popup) = &mut app.gui.cli.popup {
        popup.selected = popup.selected.wrapping_add(1);
        return;
    }
    let values = app.repo_values();
    let text = app.gui.cli.input.text.clone();
    let cursor = app.gui.cli.input.cursor;
    let c = complete(&text, cursor, &app.vocab, &values);
    if let Some(cmd) = &c.missing_options_for {
        app.request_options(cmd);
    }
    match tab_action(&c, app.config.auto_insert_single_match) {
        TabAction::Nothing => {}
        TabAction::Insert(insert) => {
            let (line, cursor) = replace_word(&text, c.word_start, cursor, &insert);
            app.gui.cli.input.set(line, Some(cursor));
        }
        TabAction::Popup { extend_to } => {
            if let Some(prefix) = extend_to {
                let (line, cursor) = replace_word(&text, c.word_start, cursor, &prefix);
                app.gui.cli.input.set(line, Some(cursor));
            }
            app.gui.cli.popup = Some(Popup {
                kind: PopupKind::Completion,
                selected: 0,
            });
        }
    }
}

/// A key while the command line has the focus.
pub fn key(app: &mut App, key: KeyEvent) -> Outcome {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('r') {
        app.gui.cli.popup = Some(Popup {
            kind: PopupKind::History,
            selected: 0,
        });
        return Outcome::Stay;
    }
    if key.code == KeyCode::Tab {
        on_tab(app);
        return Outcome::Stay;
    }
    if let Some(popup) = app.gui.cli.popup {
        match key.code {
            KeyCode::Esc => app.gui.cli.popup = None,
            KeyCode::Up => {
                let rows = rows(app).len().max(1);
                let at = popup.selected % rows;
                let selected = at.checked_sub(1).unwrap_or(rows - 1);
                app.gui.cli.popup = Some(Popup { selected, ..popup });
            }
            KeyCode::Down => {
                app.gui.cli.popup = Some(Popup {
                    selected: popup.selected.wrapping_add(1),
                    ..popup
                });
            }
            KeyCode::Enter => {
                let rows = rows(app);
                let at = popup.selected % rows.len().max(1);
                if let Some(row) = rows.get(at) {
                    app.gui.cli.input.set(row.line.clone(), Some(row.cursor));
                }
                app.gui.cli.popup = None;
            }
            _ => edit(app, key),
        }
        return Outcome::Stay;
    }
    match key.code {
        KeyCode::Esc => return Outcome::Leave,
        KeyCode::Up => {
            let text = app.gui.cli.input.text.clone();
            if let Some(line) = app.gui.cli.nav.up(&app.history, &text) {
                app.gui.cli.input.set(line, None);
            }
        }
        KeyCode::Down => {
            if let Some(line) = app.gui.cli.nav.down(&app.history) {
                app.gui.cli.input.set(line, None);
            }
        }
        KeyCode::Enter => {
            let line = std::mem::take(&mut app.gui.cli.input).text;
            app.gui.cli.nav.reset();
            if !line.trim().is_empty() {
                app.run_cli_line(&line);
            }
        }
        KeyCode::Right if suggestion(app).is_some() => {
            if let Some(rest) = suggestion(app) {
                let mut text = app.gui.cli.input.text.clone();
                text.push_str(&rest);
                app.gui.cli.input.set(text, None);
            }
        }
        _ => edit(app, key),
    }
    Outcome::Stay
}

/// An editing key: the line changes, history navigation starts over and
/// the popup's selection goes back to its first row.
fn edit(app: &mut App, key: KeyEvent) {
    let before = app.gui.cli.input.text.clone();
    app.gui.cli.input.edit(key);
    if app.gui.cli.input.text != before {
        app.gui.cli.nav.reset();
        if let Some(popup) = &mut app.gui.cli.popup {
            popup.selected = 0;
        }
    }
}
