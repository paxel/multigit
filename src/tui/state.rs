//! What only the terminal front end keeps beside the board.

use super::App;
use super::cli::Cli;
use super::widgets::Menu;
use ratatui::layout::Rect;
use std::collections::HashSet;
use std::path::PathBuf;

/// The terminal's own state: selection, what has the keys, what was drawn
/// where (for the mouse).
#[derive(Debug, Clone, PartialEq)]
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
    /// What has the keys.
    pub mode: Mode,
    /// Where the open menu's items were drawn: screen line and position
    /// among the shown items.
    pub menu_rows: Vec<(u16, usize)>,
    /// The command line.
    pub cli: Cli,
    /// The folded sections of the output panel: run and repository.
    pub folded: HashSet<(u64, PathBuf)>,
    /// The output panel's cursor line; `None` follows the newest line.
    pub output_cursor: Option<usize>,
    /// The output panel's lines as last drawn, its frame left out.
    pub output_area: Rect,
    /// The first output line drawn.
    pub output_top: usize,
    /// Texts for the clipboard, sent to the terminal with the next frame.
    pub copies: Vec<String>,
}

/// What has the keys.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Mode {
    /// The board: plain keys are shortcuts.
    #[default]
    Board,
    /// The filter field, which `app.filter` holds.
    Filter,
    /// A list to pick from.
    Menu(Menu),
    /// The command line.
    Command,
    /// The output panel.
    Output,
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
            mode: Mode::Board,
            menu_rows: Vec::new(),
            cli: Cli::default(),
            folded: HashSet::new(),
            output_cursor: None,
            output_area: Rect::default(),
            output_top: 0,
            copies: Vec::new(),
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
