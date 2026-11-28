//! The commit dialog in the terminal: one box whose first line is the
//! subject.

use super::App;
use super::widgets::Input;
use crate::domain::commit_message::{sends, subject_parts};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

/// What the hint in the box says. Terminals rarely tell Ctrl+Enter from
/// Enter, so Ctrl+S is the key that always works.
pub const SEND_HINT: &str = "Ctrl+S commits";

/// The text before the cursor (a byte position) as line and column.
fn line_and_column(text: &str, cursor: usize) -> (usize, usize) {
    let before = text.get(..cursor).unwrap_or(text);
    let line = before.matches('\n').count();
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count());
    (line, column)
}

/// The byte position of `column` in line `line`, or the end of the line.
fn position(text: &str, line: usize, column: usize) -> Option<usize> {
    let mut start = 0;
    for (i, part) in text.split('\n').enumerate() {
        if i == line {
            let within = part
                .char_indices()
                .nth(column)
                .map_or(part.len(), |(byte, _)| byte);
            return Some(start + within);
        }
        start += part.len() + 1;
    }
    None
}

/// A key while the commit dialog is open.
pub fn key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let send = matches!(
        (key.code, ctrl),
        (KeyCode::Char('s' | 'j'), true) | (KeyCode::Enter, true)
    );
    if send {
        app.send_commit();
        return;
    }
    if key.code == KeyCode::Esc {
        app.cancel_commit();
        return;
    }
    let Some(dialog) = &mut app.commit_dialog else {
        return;
    };
    if dialog.running.is_some() {
        return;
    }
    let mut input = Input {
        cursor: app.gui.commit_cursor.min(dialog.text.len()),
        text: std::mem::take(&mut dialog.text),
    };
    if !input.text.is_char_boundary(input.cursor) {
        input.cursor = input.text.len();
    }
    let mut sent = false;
    match key.code {
        KeyCode::Enter => {
            input.text.insert(input.cursor, '\n');
            input.cursor += 1;
            sent = sends(&input.text, input.cursor);
        }
        KeyCode::Up | KeyCode::Down => {
            let (line, column) = line_and_column(&input.text, input.cursor);
            let target = if key.code == KeyCode::Up {
                line.checked_sub(1)
            } else {
                Some(line + 1)
            };
            if let Some(at) = target.and_then(|l| position(&input.text, l, column)) {
                input.cursor = at;
            }
        }
        _ => {
            input.edit(key);
        }
    }
    dialog.text = input.text;
    app.gui.commit_cursor = input.cursor;
    if sent {
        app.send_commit();
    }
}

/// Draw the commit dialog over the screen.
pub fn draw(frame: &mut Frame, app: &App) {
    let Some(dialog) = &app.commit_dialog else {
        return;
    };
    let screen = frame.area();
    let width = 80.min(screen.width);
    let height = 16.min(screen.height);
    let area = Rect {
        x: screen.width.saturating_sub(width) / 2,
        y: screen.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Commit ")
        .title_bottom(Line::styled(format!(" {SEND_HINT}  Esc cancels "), dim()).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    for (i, text) in dialog.text.split('\n').enumerate() {
        if i == 0 {
            let (shown, past) = subject_parts(text);
            lines.push(Line::from(vec![
                Span::styled(shown.to_owned(), Style::new().add_modifier(Modifier::BOLD)),
                Span::styled(past.to_owned(), Style::new().fg(Color::Yellow)),
            ]));
        } else {
            lines.push(Line::raw(text.to_owned()));
        }
    }
    let room = usize::from(inner.height.saturating_sub(1));
    let cursor = app.gui.commit_cursor.min(dialog.text.len());
    let (line, column) = line_and_column(&dialog.text, cursor);
    let top = line.saturating_sub(room.saturating_sub(1));
    let mut shown: Vec<Line> = lines.into_iter().skip(top).take(room).collect();
    while shown.len() < room {
        shown.push(Line::default());
    }
    let status = match (&dialog.error, dialog.running) {
        (_, Some(_)) => Line::styled("committing\u{2026}", dim()),
        (Some(error), None) => Line::styled(
            error.lines().next().unwrap_or_default().to_owned(),
            Style::new().fg(Color::Red),
        ),
        (None, None) => Line::default(),
    };
    shown.push(status);
    frame.render_widget(Paragraph::new(shown), inner);
    let x = u16::try_from(column).unwrap_or(u16::MAX);
    let y = u16::try_from(line - top).unwrap_or(0);
    frame.set_cursor_position((inner.x.saturating_add(x), inner.y.saturating_add(y)));
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_moves_between_lines_by_column() {
        let text = "subject\n\nbody text";
        assert_eq!(line_and_column(text, 3), (0, 3));
        assert_eq!(line_and_column(text, text.len()), (2, 9));
        assert_eq!(position(text, 1, 5), Some(8), "an empty line ends early");
        assert_eq!(position(text, 2, 4), Some(13));
        assert_eq!(position(text, 3, 0), None);
    }
}
