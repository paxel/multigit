//! What keys and the mouse do in the terminal.

use super::App;
use super::actions::{Action, perform};
use super::cli;
use super::state::{Mode, move_selection, select_end, selected};
use super::widgets::{Item, Menu, MenuKey};
use crate::app::MarkBy;
use crate::domain::board::SortKey;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// A key press.
pub fn on_key(app: &mut App, key: KeyEvent) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.gui.quit = true;
        return;
    }
    if app.gui.help {
        // Any key closes the help; `q` still quits.
        app.gui.help = false;
        if key.code != KeyCode::Char('q') {
            return;
        }
    }
    if app.confirm.is_some() {
        match key.code {
            KeyCode::Enter => app.confirm_run(),
            KeyCode::Esc => app.cancel_run(),
            _ => {}
        }
        return;
    }
    match std::mem::take(&mut app.gui.mode) {
        Mode::Board => board_key(app, key),
        Mode::Command => {
            if cli::key(app, key) == cli::Outcome::Stay {
                app.gui.mode = Mode::Command;
            }
        }
        Mode::Filter => filter_key(app, key),
        Mode::Menu(mut menu) => match menu.key(key) {
            MenuKey::Stay => app.gui.mode = Mode::Menu(menu),
            MenuKey::Close => {}
            MenuKey::Pick(action) => perform(app, action),
        },
    }
}

fn board_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('q') => app.gui.quit = true,
        KeyCode::Char('?') => app.gui.help = true,
        KeyCode::Char('j') | KeyCode::Down => move_selection(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_selection(app, -1),
        KeyCode::PageDown => move_selection(app, page(app)),
        KeyCode::PageUp => move_selection(app, -page(app)),
        KeyCode::Char('g') | KeyCode::Home => select_end(app, false),
        KeyCode::Char('G') | KeyCode::End => select_end(app, true),
        KeyCode::Char(' ') => {
            if let Some(path) = selected(app) {
                app.toggle_mark(path);
            }
        }
        KeyCode::Char('a') => app.press_mark(MarkBy::All),
        KeyCode::Char('d') => app.press_mark(MarkBy::Dirty),
        KeyCode::Char('b') => app.press_mark(MarkBy::Behind),
        KeyCode::Char('p') => app.press_mark(MarkBy::Ahead),
        KeyCode::Char('A') => app.status_filter = None,
        KeyCode::Char('D') => app.filter_by(MarkBy::Dirty),
        KeyCode::Char('B') => app.filter_by(MarkBy::Behind),
        KeyCode::Char('P') => app.filter_by(MarkBy::Ahead),
        KeyCode::Char('/') => app.gui.mode = Mode::Filter,
        KeyCode::Char(':') => app.gui.mode = Mode::Command,
        KeyCode::Char('s') => app.gui.mode = Mode::Menu(sort_menu(app)),
        _ => {}
    }
}

fn filter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter | KeyCode::Esc => return,
        KeyCode::Backspace => {
            app.filter.pop();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => app.filter.clear(),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => app.filter.push(c),
        _ => {}
    }
    app.gui.mode = Mode::Filter;
}

/// The sort keys, the one in use selected.
pub fn sort_menu(app: &App) -> Menu {
    let items = SortKey::ALL
        .iter()
        .map(|&key| Item::new(key.label(), Action::Sort(key)))
        .collect();
    let mut menu = Menu::new("sort by", items);
    menu.selected = SortKey::ALL
        .iter()
        .position(|&key| key == app.sort)
        .unwrap_or(0);
    menu
}

/// Rows of one page of the table.
fn page(app: &App) -> isize {
    isize::try_from(app.gui.rows_at.len().max(1)).unwrap_or(1)
}

/// A mouse event.
pub fn on_mouse(app: &mut App, mouse: MouseEvent) {
    if let Mode::Menu(menu) = &mut app.gui.mode {
        if let MouseEventKind::Down(_) = mouse.kind {
            let hit = app.gui.menu_rows.iter().find(|(y, _)| *y == mouse.row);
            match hit {
                Some(&(_, index)) => {
                    menu.selected = index;
                    if let Some(action) = menu.picked() {
                        app.gui.mode = Mode::Board;
                        perform(app, action);
                    }
                }
                None => app.gui.mode = Mode::Board,
            }
        }
        return;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => move_selection(app, 1),
        MouseEventKind::ScrollUp => move_selection(app, -1),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(path) = row_at(app, mouse.column, mouse.row) {
                app.gui.selected = Some(path.clone());
                app.toggle_mark(path);
            }
        }
        _ => {}
    }
}

/// The repository of the board row at this screen cell.
pub fn row_at(app: &App, column: u16, row: u16) -> Option<std::path::PathBuf> {
    let table = app.gui.table;
    if column < table.x || column >= table.x.saturating_add(table.width) {
        return None;
    }
    app.gui
        .rows_at
        .iter()
        .find(|(y, _)| *y == row)
        .map(|(_, path)| path.clone())
}
