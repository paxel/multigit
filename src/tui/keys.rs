//! What keys and the mouse do in the terminal.

use super::App;
use super::actions::{Action, Prompt, PromptFor, answer, perform};
use super::card_menu::card_menu;
use super::cli;
use super::output;
use super::state::{Mode, move_selection, select_end, selected};
use super::widgets::{Item, Menu, MenuKey};
use crate::app::MarkBy;
use crate::domain::board::SortKey;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

/// Two clicks on the same row within this time are a double click.
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

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
        // The scroll keys scroll it, any other closes it; `q` still quits.
        let scroll = &mut app.gui.help_scroll;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => *scroll = scroll.saturating_add(1),
            KeyCode::Char('k') | KeyCode::Up => *scroll = scroll.saturating_sub(1),
            KeyCode::PageDown | KeyCode::Char('G') | KeyCode::End => {
                *scroll = scroll.saturating_add(usize::from(u16::MAX));
            }
            KeyCode::PageUp | KeyCode::Char('g') | KeyCode::Home => *scroll = 0,
            _ => {
                app.gui.help = false;
                app.gui.help_scroll = 0;
                if key.code != KeyCode::Char('q') {
                    return;
                }
            }
        }
        if app.gui.help {
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
        Mode::Board if app.settings_open => super::settings::key(app, key),
        Mode::Board if app.repo_view.is_some() => super::repo::key(app, key),
        Mode::Board => board_key(app, key),
        Mode::Prompt(mut prompt) => match key.code {
            KeyCode::Esc => {}
            KeyCode::Enter => answer(app, prompt.purpose, &prompt.input.text),
            KeyCode::Tab if prompt.purpose == PromptFor::AddFolder => {
                let (home, cwd) = super::actions::places();
                if let Some(text) =
                    super::folders::complete(&prompt.input.text, home.as_deref(), &cwd)
                {
                    prompt.input.set(text, None);
                }
                app.gui.mode = Mode::Prompt(prompt);
            }
            _ => {
                prompt.input.edit(key);
                app.gui.mode = Mode::Prompt(prompt);
            }
        },
        Mode::Favorite(mut form) => {
            if super::favorites::key(app, &mut form, key) {
                app.gui.mode = Mode::Favorite(form);
            }
        }
        Mode::Output => {
            if output::key(app, key) {
                app.gui.mode = Mode::Output;
            }
        }
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
        KeyCode::Enter => {
            if let Some(path) = selected(app) {
                app.open_repo_view(&path);
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
        KeyCode::Char('o') => app.config.output_hidden = !app.config.output_hidden,
        KeyCode::Char('t') => {
            if let Some(path) = selected(app) {
                app.open_terminal(&path);
            }
        }
        KeyCode::Char(c @ '1'..='9') => run_button(app, c),
        KeyCode::Char('m') => app.gui.mode = Mode::Menu(marked_buttons_menu(app)),
        KeyCode::Char('S') => {
            app.settings_open = true;
            app.settings_page = crate::app::SettingsPage::Settings;
        }
        KeyCode::Char('+') => {
            app.gui.mode = Mode::Prompt(Prompt {
                title: "add a repository, or a folder to scan (Tab completes)".to_owned(),
                input: super::widgets::Input::default(),
                purpose: PromptFor::AddFolder,
            });
        }
        KeyCode::Char('f') => app.gui.mode = Mode::Menu(super::favorites::list(app)),
        KeyCode::Char('x') => {
            if let Some(path) = selected(app) {
                app.gui.mode = Mode::Menu(card_menu(app, &path));
            }
        }
        KeyCode::Tab if !app.config.output_hidden && !app.log.is_empty() => {
            app.gui.mode = Mode::Output;
        }
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

/// A digit runs that card button on the selected row, unless the row has
/// no status yet (the window greys its buttons then).
fn run_button(app: &mut App, digit: char) {
    let Some(index) = digit.to_digit(10).and_then(|d| usize::try_from(d).ok()) else {
        return;
    };
    let Some(path) = selected(app) else {
        return;
    };
    let ready = app
        .cards
        .iter()
        .any(|card| card.path == path && card.snapshot().is_some());
    if ready {
        app.run_button(index.saturating_sub(1), &path);
    }
}

/// The card buttons, for all marked rows.
pub fn marked_buttons_menu(app: &App) -> Menu {
    let marked = app.marked_repos().len();
    let items = if marked == 0 {
        vec![Item::disabled("mark repositories first")]
    } else {
        app.config
            .buttons
            .iter()
            .enumerate()
            .map(|(i, b)| Item::new(b.label.clone(), Action::ButtonOnMarked(i)))
            .collect()
    };
    Menu::new(
        format!("run in {}", super::board::repositories(marked)),
        items,
    )
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
    if app.settings_open {
        return;
    }
    if app.repo_view.is_some() {
        let kind = match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => super::repo::MouseAt::Click,
            MouseEventKind::ScrollDown => super::repo::MouseAt::Down,
            MouseEventKind::ScrollUp => super::repo::MouseAt::Up,
            _ => return,
        };
        super::repo::mouse(app, mouse.column, mouse.row, kind);
        return;
    }
    if !app.config.output_hidden && contains(app.gui.output_area, mouse.column, mouse.row) {
        output_mouse(app, mouse);
        return;
    }
    if let MouseEventKind::Down(button) = mouse.kind
        && mouse.row == app.gui.favorites_at.0
        && let Some(&(_, _, index)) = app
            .gui
            .favorites_at
            .1
            .iter()
            .find(|(from, to, _)| (*from..*to).contains(&mouse.column))
    {
        // A click shows only its rows; a right click opens what can be
        // done with it, as on its button in the window.
        if button == MouseButton::Right {
            app.gui.mode = Mode::Menu(super::favorites::of(app, index));
        } else {
            app.toggle_favorite(index);
        }
        return;
    }
    match mouse.kind {
        MouseEventKind::ScrollDown => move_selection(app, 1),
        MouseEventKind::ScrollUp => move_selection(app, -1),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(path) = row_at(app, mouse.column, mouse.row) {
                let on_critters = app.is_kawaii()
                    && mouse.column < app.gui.table.x.saturating_add(super::board::KAWAII_WIDTH);
                let petted = super::kawaii::critters_on(app, &path)
                    .first()
                    .map(|(i, _)| *i);
                if on_critters && let Some(index) = petted {
                    // A click on a critter: it jumps, with a heart.
                    app.pet_critter(index);
                    return;
                }
                app.gui.selected = Some(path.clone());
                app.toggle_mark(path.clone());
                // The second click of a double click marks back, as in the
                // window, and opens the repository.
                let now = std::time::Instant::now();
                let double = app.gui.last_click.as_ref().is_some_and(|(last, at)| {
                    *last == path && now.duration_since(*at) < DOUBLE_CLICK
                });
                if double {
                    app.gui.last_click = None;
                    app.open_repo_view(&path);
                } else {
                    app.gui.last_click = Some((path, now));
                }
            }
        }
        MouseEventKind::Down(MouseButton::Right) => {
            if let Some(path) = row_at(app, mouse.column, mouse.row) {
                app.gui.selected = Some(path.clone());
                app.gui.mode = Mode::Menu(card_menu(app, &path));
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

fn contains(area: ratatui::layout::Rect, column: u16, row: u16) -> bool {
    column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

/// The wheel scrolls the output panel; a click on a section's header folds
/// it.
fn output_mouse(app: &mut App, mouse: MouseEvent) {
    let all = output::lines(app);
    let at = output::cursor(app, all.len());
    let last = all.len().saturating_sub(1);
    match mouse.kind {
        MouseEventKind::ScrollDown => app.gui.output_cursor = Some(at.saturating_add(3).min(last)),
        MouseEventKind::ScrollUp => app.gui.output_cursor = Some(at.saturating_sub(3)),
        MouseEventKind::Down(MouseButton::Left) => {
            let index =
                app.gui.output_top + usize::from(mouse.row.saturating_sub(app.gui.output_area.y));
            if let Some(line) = all.get(index) {
                app.gui.output_cursor = Some(index);
                if matches!(line, output::OutLine::Section { .. }) {
                    output::toggle_fold(app, line);
                }
            }
        }
        _ => {}
    }
}
