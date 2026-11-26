//! The Kawaii theme in the terminal: pastel colors, bunnies and cats drawn
//! in text on the rows, and a burst of hearts and stars from a row whose
//! command ended well. No sounds and no music.

use super::App;
use crate::domain::critters::{Critter, Eyes, Mood, PASTELS, Pose, Species, pose};
use crate::domain::kawaii;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::text::Span;
use std::path::Path;

/// The pastel a terminal color becomes.
pub fn pastel(color: Color) -> Color {
    match color {
        Color::Cyan | Color::LightCyan => Color::Rgb(255, 160, 200),
        Color::Green | Color::LightGreen => Color::Rgb(150, 220, 170),
        Color::Yellow | Color::LightYellow => Color::Rgb(255, 222, 140),
        Color::Red | Color::LightRed => Color::Rgb(255, 140, 160),
        Color::Magenta | Color::LightMagenta => Color::Rgb(205, 170, 240),
        Color::Blue | Color::LightBlue => Color::Rgb(150, 200, 250),
        Color::DarkGray | Color::Gray => Color::Rgb(185, 160, 190),
        other => other,
    }
}

/// Paint everything drawn in pastels.
pub fn tint(buffer: &mut Buffer) {
    for cell in &mut buffer.content {
        cell.fg = pastel(cell.fg);
        cell.bg = pastel(cell.bg);
    }
}

/// A critter in text, as it holds itself.
pub fn critter_text(critter: &Critter, pose: &Pose) -> String {
    let (left, right) = match critter.species {
        Species::Bunny => ("(=", "=)"),
        Species::Cat => ("=", "="),
    };
    let eyes = match pose.eyes {
        Eyes::Open if pose.ears < -0.3 => ";.;",
        Eyes::Open => "'.'",
        Eyes::Closed => "-.-",
        Eyes::Up => "\u{b0}.\u{b0}",
    };
    let mut text = format!("{left}{eyes}{right}");
    if pose.sway < -0.2 {
        text = format!("~{text}");
    } else if pose.sway > 0.2 {
        text.push('~');
    }
    if pose.lift > 0.3 {
        text.insert(0, '^');
    }
    if pose.curled {
        text.push('z');
    }
    if pose.heart.is_some() || critter.mood == Mood::Pet {
        text.push('\u{2665}');
    }
    text
}

/// The critters on the row of `path`, each with its index in the troupe,
/// as colored text. A hopping critter is on the card it left for the first
/// half of its hop.
pub fn critters_on(app: &App, path: &Path) -> Vec<(usize, Span<'static>)> {
    let clock = app.kawaii_clock;
    let asleep = app.critters.asleep(clock);
    app.critters
        .critters
        .iter()
        .enumerate()
        .filter_map(|(index, critter)| {
            let pose = pose(critter, clock, asleep);
            let at: &Path = if pose.along < 0.5 {
                &critter.from
            } else {
                &critter.home
            };
            (at == path).then(|| {
                let (r, g, b) = PASTELS
                    .get(critter.color % PASTELS.len())
                    .copied()
                    .unwrap_or((255, 209, 220));
                let span = Span::styled(
                    critter_text(critter, &pose),
                    ratatui::style::Style::new().fg(Color::Rgb(r, g, b)),
                );
                (index, span)
            })
        })
        .collect()
}

/// The hearts and stars of a burst from the row of `path`, while it flies.
pub fn confetti_on(app: &App, path: &Path) -> Option<Span<'static>> {
    let t = app
        .bursts
        .iter()
        .filter(|(card, _)| card == path)
        .find_map(|(_, started)| kawaii::burst(app.kawaii_clock, *started))?;
    const PIECES: [char; 4] = ['\u{2665}', '\u{2726}', '\u{2661}', '\u{2727}'];
    // The burst grows, then thins out as it falls.
    let count = if t < 0.5 {
        2 + (t * 8.0) as usize
    } else {
        2 + ((1.0 - t) * 8.0) as usize
    };
    let shift = (t * 8.0) as usize;
    let text: String = (0..count)
        .map(|i| {
            PIECES
                .get((i + shift) % PIECES.len())
                .copied()
                .unwrap_or('*')
        })
        .collect();
    Some(Span::styled(
        text,
        ratatui::style::Style::new().fg(Color::Rgb(255, 222, 140)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn critter(species: Species) -> Critter {
        Critter {
            species,
            color: 0,
            home: PathBuf::from("/a"),
            from: PathBuf::from("/a"),
            hop_at: -100.0,
            mood: Mood::Plain,
            mood_at: -100.0,
        }
    }

    fn sitting() -> Pose {
        Pose {
            along: 1.0,
            lift: 0.0,
            squash: 1.0,
            ears: 0.0,
            eyes: Eyes::Open,
            curled: false,
            sway: 0.0,
            heart: None,
        }
    }

    #[test]
    fn bunnies_and_cats_in_text() {
        assert_eq!(
            critter_text(&critter(Species::Bunny), &sitting()),
            "(='.'=)"
        );
        assert_eq!(critter_text(&critter(Species::Cat), &sitting()), "='.'=");
        let asleep = Pose {
            eyes: Eyes::Closed,
            curled: true,
            ..sitting()
        };
        assert_eq!(critter_text(&critter(Species::Bunny), &asleep), "(=-.-=)z");
        let up = Pose {
            eyes: Eyes::Up,
            ..sitting()
        };
        assert_eq!(critter_text(&critter(Species::Cat), &up), "=\u{b0}.\u{b0}=");
        let droop = Pose {
            ears: -1.0,
            ..sitting()
        };
        assert_eq!(critter_text(&critter(Species::Bunny), &droop), "(=;.;=)");
        let petted = Pose {
            heart: Some(0.5),
            lift: 0.8,
            ..sitting()
        };
        assert_eq!(
            critter_text(&critter(Species::Cat), &petted),
            "^='.'=\u{2665}"
        );
    }

    #[test]
    fn terminal_colors_become_pastels() {
        assert_eq!(pastel(Color::Green), Color::Rgb(150, 220, 170));
        assert_eq!(pastel(Color::Reset), Color::Reset);
    }
}
