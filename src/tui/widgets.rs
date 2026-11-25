//! The terminal's small building blocks: a line of text being edited and a
//! list to pick from.

use super::actions::Action;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A line of text being edited, with a cursor (a byte position on a char
/// boundary).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    pub text: String,
    pub cursor: usize,
}

impl Input {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            cursor: text.len(),
            text,
        }
    }

    /// Replace the text; the cursor goes to `cursor`, or the end.
    pub fn set(&mut self, text: impl Into<String>, cursor: Option<usize>) {
        self.text = text.into();
        let at = cursor.unwrap_or(self.text.len()).min(self.text.len());
        self.cursor = if self.text.is_char_boundary(at) {
            at
        } else {
            self.text.len()
        };
    }

    fn prev_boundary(&self) -> usize {
        self.text
            .get(..self.cursor)
            .and_then(|s| s.char_indices().next_back())
            .map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self) -> usize {
        self.text
            .get(self.cursor..)
            .and_then(|s| s.chars().next())
            .map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }

    /// Apply an editing key; `false` when the key is no editing key.
    pub fn edit(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('u') if ctrl => {
                self.text.replace_range(..self.cursor, "");
                self.cursor = 0;
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.text.len(),
            KeyCode::Char(c) if !ctrl => {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                let start = self.prev_boundary();
                self.text.replace_range(start..self.cursor, "");
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = self.next_boundary();
                self.text.replace_range(self.cursor..end, "");
            }
            KeyCode::Left => self.cursor = self.prev_boundary(),
            KeyCode::Right => self.cursor = self.next_boundary(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            _ => return false,
        }
        true
    }

    /// Columns from the start of the text to the cursor.
    pub fn cursor_column(&self) -> u16 {
        let chars = self
            .text
            .get(..self.cursor)
            .map_or(0, |s| s.chars().count());
        u16::try_from(chars).unwrap_or(u16::MAX)
    }
}

/// One entry of a [`Menu`].
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub action: Action,
    /// Greyed out and not picked.
    pub enabled: bool,
}

impl Item {
    pub fn new(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            action,
            enabled: true,
        }
    }

    pub fn disabled(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: Action::Nothing,
            enabled: false,
        }
    }
}

/// A list to pick one entry from, over the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub title: String,
    pub items: Vec<Item>,
    /// Position among the shown items.
    pub selected: usize,
    /// The filter field, for long lists; `None` without one.
    pub filter: Option<Input>,
    /// Shown when no item fits the filter.
    pub none: String,
}

impl Menu {
    pub fn new(title: impl Into<String>, items: Vec<Item>) -> Self {
        let mut menu = Self {
            title: title.into(),
            items,
            selected: 0,
            filter: None,
            none: "nothing".to_owned(),
        };
        menu.selected = menu.first_enabled();
        menu
    }

    /// The same with a filter field.
    pub fn filtered(mut self, none: impl Into<String>) -> Self {
        self.filter = Some(Input::default());
        self.none = none.into();
        self
    }

    fn first_enabled(&self) -> usize {
        self.shown()
            .iter()
            .position(|item| item.enabled)
            .unwrap_or(0)
    }

    /// The items that fit the filter (case ignored).
    pub fn shown(&self) -> Vec<&Item> {
        let needle = self
            .filter
            .as_ref()
            .map(|f| f.text.to_lowercase())
            .unwrap_or_default();
        self.items
            .iter()
            .filter(|item| needle.is_empty() || item.label.to_lowercase().contains(&needle))
            .collect()
    }

    /// Move the selection to the next enabled item in direction `down`.
    pub fn step(&mut self, down: bool) {
        let enabled: Vec<bool> = self.shown().iter().map(|item| item.enabled).collect();
        let mut at = self.selected;
        for _ in 0..enabled.len() {
            at = if down {
                (at + 1) % enabled.len()
            } else {
                at.checked_sub(1).unwrap_or(enabled.len() - 1)
            };
            if enabled.get(at).copied().unwrap_or(false) {
                self.selected = at;
                return;
            }
        }
    }

    /// The selected item's action, if it can be picked.
    pub fn picked(&self) -> Option<Action> {
        self.shown()
            .get(self.selected)
            .filter(|item| item.enabled)
            .map(|item| item.action.clone())
    }

    /// A key on the open menu.
    pub fn key(&mut self, key: KeyEvent) -> MenuKey {
        match key.code {
            KeyCode::Esc => return MenuKey::Close,
            KeyCode::Enter => return self.picked().map_or(MenuKey::Stay, MenuKey::Pick),
            KeyCode::Down | KeyCode::Tab => self.step(true),
            KeyCode::Up | KeyCode::BackTab => self.step(false),
            KeyCode::Char('j') if self.filter.is_none() => self.step(true),
            KeyCode::Char('k') if self.filter.is_none() => self.step(false),
            _ => {
                if let Some(filter) = &mut self.filter
                    && filter.edit(key)
                {
                    self.selected = self.first_enabled();
                }
            }
        }
        MenuKey::Stay
    }
}

/// What a key did to a [`Menu`].
#[derive(Debug, Clone, PartialEq)]
pub enum MenuKey {
    /// It stays open.
    Stay,
    /// It closes without a pick.
    Close,
    /// This was picked; it closes.
    Pick(Action),
}
