//! What keys and the mouse do in the terminal.

use super::App;
use super::state::{move_selection, select_end};
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
    board_key(app, key);
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
        _ => {}
    }
}

/// Rows of one page of the table.
fn page(app: &App) -> isize {
    isize::try_from(app.gui.rows_at.len().max(1)).unwrap_or(1)
}

/// A mouse event.
pub fn on_mouse(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollDown => move_selection(app, 1),
        MouseEventKind::ScrollUp => move_selection(app, -1),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(path) = row_at(app, mouse.column, mouse.row) {
                app.gui.selected = Some(path);
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
