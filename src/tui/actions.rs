//! What the terminal's menus can ask for, and doing it.

use super::App;
use crate::domain::board::SortKey;

/// An entry's deed.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A heading or a greyed entry.
    Nothing,
    Sort(SortKey),
}

/// Do what a picked entry asks for.
pub fn perform(app: &mut App, action: Action) {
    match action {
        Action::Nothing => {}
        Action::Sort(key) => app.sort = key,
    }
}
