//! What the terminal's menus can ask for, and doing it.

use super::App;
use crate::domain::board::SortKey;
use std::path::PathBuf;

/// An entry's deed.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A heading or a greyed entry.
    Nothing,
    Sort(SortKey),
    /// Card button `n` on every marked row.
    ButtonOnMarked(usize),
    /// Put this text on the clipboard.
    Copy(String),
    /// Copy the whole fetch error of this row and take it off the row.
    CopyFetchError(PathBuf),
}

/// Do what a picked entry asks for.
pub fn perform(app: &mut App, action: Action) {
    match action {
        Action::Nothing => {}
        Action::Sort(key) => app.sort = key,
        Action::ButtonOnMarked(index) => app.run_button_on_marked(index),
        Action::Copy(text) => copy(app, text),
        Action::CopyFetchError(path) => {
            if let Some(text) = app.take_fetch_error(&path) {
                copy(app, text);
            }
        }
    }
}

/// Send `text` to the terminal's clipboard with the next frame.
pub fn copy(app: &mut App, text: String) {
    let shown: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(40)
        .collect();
    let more = if shown.len() < text.len() {
        "\u{2026}"
    } else {
        ""
    };
    app.toast(format!("Copied {shown}{more}"));
    app.gui.copies.push(text);
}
