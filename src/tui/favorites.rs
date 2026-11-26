//! Favorites in the terminal: the line of favorites over the table, their
//! list, and the form for a favorite's name, color and emoji.

use super::App;
use super::actions::Action;
use super::widgets::{Input, Item, Menu};
use crate::domain::favorites::{EMOJIS, Favorite, PALETTE, color_rgb, dark_text_on};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// The form's fields, top to bottom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Field {
    #[default]
    Name,
    Color,
    Emoji,
}

/// The open form; what it edits is `app.favorite_dialog`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Form {
    pub field: Field,
    pub name: Input,
}

/// A favorite's color as the terminal draws it.
pub fn color(favorite_color: &str) -> Color {
    let [r, g, b] = color_rgb(favorite_color);
    Color::Rgb(r, g, b)
}

/// A favorite as a chip: its label on its color; reversed while it is the
/// one shown.
pub fn chip(favorite: &Favorite, active: bool) -> Span<'static> {
    let rgb = color_rgb(&favorite.color);
    let text = if dark_text_on(rgb) {
        Color::Black
    } else {
        Color::White
    };
    let mut style = Style::new().bg(color(&favorite.color)).fg(text);
    if active {
        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    Span::styled(format!(" {} ", favorite.label()), style)
}

/// The line of favorites over the table, with the screen columns each one
/// takes; empty without favorites.
pub fn line(app: &App) -> (Line<'static>, Vec<(u16, u16, usize)>) {
    let mut spans = vec![Span::styled("f ", Style::new().fg(Color::DarkGray))];
    let mut x: u16 = 2;
    let mut at = Vec::new();
    for (i, favorite) in app.config.favorites.iter().enumerate() {
        let chip = chip(favorite, app.active_favorite == Some(i));
        let width = u16::try_from(chip.width()).unwrap_or(u16::MAX);
        at.push((x, x.saturating_add(width), i));
        x = x.saturating_add(width).saturating_add(1);
        spans.push(chip);
        spans.push(Span::raw(" "));
    }
    (Line::from(spans), at)
}

/// The favorites, each opening what can be done with it.
pub fn list(app: &App) -> Menu {
    let mut items: Vec<Item> = app
        .config
        .favorites
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let shown = if app.active_favorite == Some(i) {
                " (shown)"
            } else {
                ""
            };
            Item::new(format!("{}{shown}", f.label()), Action::Favorite(i))
        })
        .collect();
    if items.is_empty() {
        items.push(Item::disabled("no favorites: x on a row makes one"));
    }
    Menu::new("favorites", items)
}

/// What can be done with favorite `index`, as a right click on its button
/// offers in the window.
pub fn of(app: &App, index: usize) -> Menu {
    let Some(favorite) = app.config.favorites.get(index) else {
        return Menu::new("favorite", vec![Item::disabled("gone")]);
    };
    let show = if app.active_favorite == Some(index) {
        "Show all again"
    } else {
        "Show only these"
    };
    let marked = app.marked_repos();
    let mut items = vec![
        Item::new(show, Action::ToggleFavorite(index)),
        Item::new("Edit\u{2026}", Action::EditFavorite(index)),
    ];
    if marked.is_empty() {
        items.push(Item::disabled("Add the marked rows (none marked)"));
    } else {
        items.push(Item::new(
            "Add the marked rows",
            Action::AddToFavorite(index, marked.clone()),
        ));
        if marked.iter().any(|r| favorite.contains(r)) {
            items.push(Item::new(
                "Remove the marked rows",
                Action::RemoveFromFavorite(index, marked),
            ));
        }
    }
    items.push(Item::new("Delete", Action::DeleteFavorite(index)));
    Menu::new(favorite.label(), items)
}

/// Step `current` by one in `choices`, wrapping.
fn cycle<'a>(choices: &[&'a str], current: &str, forward: bool) -> &'a str {
    let at = choices.iter().position(|c| *c == current).unwrap_or(0);
    let n = choices.len().max(1);
    let next = if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    };
    choices.get(next).copied().unwrap_or_default()
}

/// A key on the open form: `false` when it closed.
pub fn key(app: &mut App, form: &mut Form, key: KeyEvent) -> bool {
    let Some(dialog) = &mut app.favorite_dialog else {
        return false;
    };
    match key.code {
        KeyCode::Esc => {
            app.favorite_dialog = None;
            return false;
        }
        KeyCode::Enter => {
            dialog.draft.name = form.name.text.clone();
            app.save_favorite();
            // A draft without a name stays open.
            return app.favorite_dialog.is_some();
        }
        KeyCode::Tab | KeyCode::Down => {
            form.field = match form.field {
                Field::Name => Field::Color,
                Field::Color => Field::Emoji,
                Field::Emoji => Field::Name,
            };
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.field = match form.field {
                Field::Name => Field::Emoji,
                Field::Color => Field::Name,
                Field::Emoji => Field::Color,
            };
        }
        KeyCode::Left | KeyCode::Right if form.field != Field::Name => {
            let forward = key.code == KeyCode::Right;
            if form.field == Field::Color {
                let names: Vec<&str> = PALETTE.iter().map(|(name, _)| *name).collect();
                dialog.draft.color = cycle(&names, &dialog.draft.color, forward).to_owned();
            } else {
                let mut emojis = vec![""];
                emojis.extend(EMOJIS);
                let current = dialog.draft.emoji.clone().unwrap_or_default();
                let next = cycle(&emojis, &current, forward);
                dialog.draft.emoji = (!next.is_empty()).then(|| next.to_owned());
            }
        }
        _ if form.field == Field::Name => {
            form.name.edit(key);
        }
        _ => {}
    }
    true
}

/// The form's lines: the three fields, the one with the keys marked.
pub fn form_lines(app: &App, form: &Form) -> Vec<Line<'static>> {
    let Some(dialog) = &app.favorite_dialog else {
        return Vec::new();
    };
    let label = |field: Field, text: &str| {
        let style = if form.field == field {
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::DarkGray)
        };
        Span::styled(format!("{text:<7}"), style)
    };
    let preview = Favorite {
        name: if form.name.text.trim().is_empty() {
            "name".to_owned()
        } else {
            form.name.text.clone()
        },
        color: dialog.draft.color.clone(),
        emoji: dialog.draft.emoji.clone(),
        repos: Vec::new(),
    };
    vec![
        Line::from(vec![
            label(Field::Name, "name"),
            Span::raw(form.name.text.clone()),
        ]),
        Line::from(vec![
            label(Field::Color, "color"),
            Span::styled(
                format!("\u{25c0} {} \u{25b6}", dialog.draft.color),
                Style::new().fg(color(&dialog.draft.color)),
            ),
        ]),
        Line::from(vec![
            label(Field::Emoji, "emoji"),
            Span::raw(format!(
                "\u{25c0} {} \u{25b6}",
                dialog.draft.emoji.as_deref().unwrap_or("none")
            )),
        ]),
        Line::from(vec![Span::raw("       "), chip(&preview, false)]),
        Line::styled(
            "Tab: next field  \u{2190}/\u{2192}: change  Enter: save  Esc: cancel",
            Style::new().fg(Color::DarkGray),
        ),
    ]
}
