//! What only the terminal front end keeps beside the board.

use super::App;
use ratatui::layout::Rect;
use std::path::PathBuf;

/// The terminal's own state: selection, what has the keys, what was drawn
/// where (for the mouse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tui {
    /// The user asked to quit.
    pub quit: bool,
    /// The terminal has focus; it says so only when it reports focus.
    pub focused: bool,
    /// The selected row, by repository; `None` picks the first shown.
    pub selected: Option<PathBuf>,
    /// The key help is shown over the board.
    pub help: bool,
    /// Where the board's rows were drawn last: their screen line and
    /// repository, for clicks.
    pub rows_at: Vec<(u16, PathBuf)>,
    /// The table's first shown row, so the selection stays in view.
    pub offset: usize,
    /// The table's area as last drawn.
    pub table: Rect,
}

impl Default for Tui {
    fn default() -> Self {
        Self {
            quit: false,
            focused: true,
            selected: None,
            help: false,
            rows_at: Vec::new(),
            offset: 0,
            table: Rect::default(),
        }
    }
}

/// The repositories of the shown rows, in board order.
pub fn shown(app: &App) -> Vec<PathBuf> {
    app.shown_cards()
        .into_iter()
        .map(|c| c.path.clone())
        .collect()
}

/// Position of the selected row among the shown ones: the remembered
/// repository while it is shown, the first row otherwise. `None` on an
/// empty board.
pub fn selected_index(app: &App) -> Option<usize> {
    let shown = shown(app);
    if shown.is_empty() {
        return None;
    }
    let at = app
        .gui
        .selected
        .as_ref()
        .and_then(|sel| shown.iter().position(|p| p == sel));
    Some(at.unwrap_or(0))
}

/// The selected repository, if any row is shown.
pub fn selected(app: &App) -> Option<PathBuf> {
    let index = selected_index(app)?;
    shown(app).into_iter().nth(index)
}

/// Move the selection by `delta` rows, stopping at the ends.
pub fn move_selection(app: &mut App, delta: isize) {
    let shown = shown(app);
    let Some(index) = selected_index(app) else {
        return;
    };
    let last = shown.len().saturating_sub(1);
    let next = index.saturating_add_signed(delta).min(last);
    app.gui.selected = shown.into_iter().nth(next);
}

/// Select the first (`false`) or the last (`true`) row.
pub fn select_end(app: &mut App, last: bool) {
    let shown = shown(app);
    app.gui.selected = if last {
        shown.last().cloned()
    } else {
        shown.first().cloned()
    };
}
