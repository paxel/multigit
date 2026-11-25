//! The terminal front end, drawn into a test buffer.

use super::{App, Tui, board, keys, on_event};
use crate::app::Card;
use crate::app::tests::{FakeBackend, FakeDesktop, app_on_front, snapshot};
use crate::domain::status::{Counts, Upstream};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use std::path::PathBuf;
use std::sync::Arc;

pub fn app() -> App {
    app_on_front::<Tui>(
        Arc::new(FakeBackend::default()),
        Arc::new(FakeDesktop::default()),
        None,
    )
}

pub fn card(name: &str, counts: Counts, upstream: Upstream) -> Card {
    Card {
        path: PathBuf::from(format!("/repos/{name}")),
        probe: Some(Ok(snapshot(counts, upstream, &["main"]))),
        fetch_error: None,
        last_command: None,
    }
}

/// An app with three clean cards: alpha, beta, gamma.
pub fn board_app() -> App {
    let mut app = app();
    for name in ["alpha", "beta", "gamma"] {
        app.cards
            .push(card(name, Counts::default(), Upstream::None));
    }
    app
}

/// What the screen shows, line by line.
pub fn screen(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| board::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect()
}

pub fn press(app: &mut App, code: KeyCode) {
    on_event(app, Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

pub fn shown(lines: &[String], text: &str) -> bool {
    lines.iter().any(|line| line.contains(text))
}

#[test]
fn the_board_is_a_table_with_one_row_per_repository() {
    let mut app = board_app();
    app.cards[1].probe = Some(Ok(snapshot(
        Counts {
            modified: 2,
            ..Counts::default()
        },
        Upstream::Tracking {
            ahead: 1,
            behind: 3,
        },
        &["main"],
    )));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "3 repositories"));
    for name in ["alpha", "beta", "gamma"] {
        assert!(shown(&lines, name), "{name}");
    }
    assert!(shown(&lines, "+0 ~2 ?0 !0"));
    assert!(shown(&lines, "\u{2191}1 \u{2193}3"));
    assert!(shown(&lines, "no upstream"));
}

#[test]
fn the_detail_strip_shows_the_selected_card() {
    let mut app = board_app();
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "/repos/alpha"), "the first row is selected");
    press(&mut app, KeyCode::Char('j'));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "/repos/beta"));
    assert!(!shown(&lines, "/repos/alpha"));
    press(&mut app, KeyCode::Char('G'));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "/repos/gamma"));
    press(&mut app, KeyCode::Down);
    assert_eq!(
        super::state::selected(&app),
        Some(PathBuf::from("/repos/gamma")),
        "the selection stops at the end"
    );
    press(&mut app, KeyCode::Home);
    assert_eq!(
        super::state::selected(&app),
        Some(PathBuf::from("/repos/alpha"))
    );
}

#[test]
fn a_card_still_loading_or_broken_says_so() {
    let mut app = app();
    let mut loading = card("loading-one", Counts::default(), Upstream::None);
    loading.probe = None;
    app.cards.push(loading);
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "loading"));
}

#[test]
fn a_click_selects_a_row_and_the_wheel_moves_the_selection() {
    let mut app = board_app();
    screen(&mut app, 100, 24);
    let (y, path) = app.gui.rows_at[2].clone();
    assert_eq!(path, PathBuf::from("/repos/gamma"));
    let click = |kind, row| MouseEvent {
        kind,
        column: 5,
        row,
        modifiers: KeyModifiers::NONE,
    };
    keys::on_mouse(&mut app, click(MouseEventKind::Down(MouseButton::Left), y));
    assert_eq!(super::state::selected(&app), Some(path));
    keys::on_mouse(&mut app, click(MouseEventKind::ScrollUp, 0));
    assert_eq!(
        super::state::selected(&app),
        Some(PathBuf::from("/repos/beta"))
    );
}

#[test]
fn help_shows_the_keys_and_q_quits() {
    let mut app = board_app();
    press(&mut app, KeyCode::Char('?'));
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " keys "));
    assert!(shown(&lines, "quit"));
    press(&mut app, KeyCode::Esc);
    assert!(!app.gui.help, "any key closes the help");
    assert!(!app.gui.quit);
    press(&mut app, KeyCode::Char('q'));
    assert!(app.gui.quit);
}

#[test]
fn the_terminal_never_plays_sounds_or_music() {
    let desktop = Arc::new(FakeDesktop::default());
    let mut app = app_on_front::<Tui>(Arc::default(), desktop.clone(), None);
    app.silent = true;
    app.config.sounds = true;
    app.sound_news(
        &[crate::domain::audio::Sound::Beep],
        std::time::Instant::now(),
    );
    app.config.theme = crate::domain::config::Theme::Kawaii;
    app.config.music = true;
    app.sync_music();
    assert!(!app.music_plays());
    assert!(desktop.played.lock().unwrap().is_empty());
}
