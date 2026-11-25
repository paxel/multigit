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
    assert!(shown(&lines, " keys  j/k scroll "));
    press(&mut app, KeyCode::Char('G'));
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, "quit"), "scrolled to the end");
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

#[test]
fn space_and_clicks_mark_rows_and_the_mark_keys_act_like_the_buttons() {
    let mut app = board_app();
    app.cards[1].probe = Some(Ok(snapshot(
        Counts {
            untracked: 1,
            ..Counts::default()
        },
        Upstream::None,
        &["main"],
    )));
    press(&mut app, KeyCode::Char(' '));
    assert!(app.marked.contains(&PathBuf::from("/repos/alpha")));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "1 marked"));
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.marked_repos(), vec![PathBuf::from("/repos/beta")]);
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(
        app.marked_repos().len(),
        2,
        "a second press marks the others"
    );
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(app.marked_repos().len(), 3);
    press(&mut app, KeyCode::Char('a'));
    assert!(app.marked_repos().is_empty(), "a second all marks nothing");
}

#[test]
fn the_status_filter_shows_only_the_fitting_rows() {
    let mut app = board_app();
    app.cards[2].probe = Some(Ok(snapshot(
        Counts {
            staged: 1,
            ..Counts::default()
        },
        Upstream::None,
        &["main"],
    )));
    press(&mut app, KeyCode::Char('D'));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "showing dirty (1)"));
    assert!(!shown(&lines, "alpha"));
    press(&mut app, KeyCode::Char('A'));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "alpha"));
}

#[test]
fn slash_types_into_the_filter_field() {
    let mut app = board_app();
    press(&mut app, KeyCode::Char('/'));
    for c in "et".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "/et"));
    assert!(shown(&lines, "beta"));
    assert!(!shown(&lines, "alpha"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.gui.mode, super::state::Mode::Board);
    press(&mut app, KeyCode::Char('q'));
    assert!(app.gui.quit, "keys act on the board again");
}

#[test]
fn s_picks_a_sort_key_from_a_list() {
    let mut app = board_app();
    press(&mut app, KeyCode::Char('s'));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, " sort by "));
    assert!(shown(&lines, "dirty first"));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.sort, crate::domain::board::SortKey::Path);
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "sort: path"));
}

fn type_line(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn ctrl(app: &mut App, c: char) {
    on_event(
        app,
        Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)),
    );
}

#[test]
fn colon_runs_git_in_the_marked_rows() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_on_front::<Tui>(backend.clone(), Arc::new(FakeDesktop::default()), None);
    app.cards
        .push(card("alpha", Counts::default(), Upstream::None));
    app.cards
        .push(card("beta", Counts::default(), Upstream::None));
    app.marked.insert(PathBuf::from("/repos/beta"));
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "git : to type git arguments"));
    press(&mut app, KeyCode::Char(':'));
    type_line(&mut app, "git status -s");
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "git git status -s"));
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |_| !backend.runs.lock().unwrap().is_empty());
    let runs = backend.runs.lock().unwrap().clone();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].0, PathBuf::from("/repos/beta"));
    assert_eq!(runs[0].1, vec!["status".to_owned(), "-s".to_owned()]);
    assert_eq!(
        app.history.entries().last().map(String::as_str),
        Some("status -s")
    );
    assert!(app.gui.cli.input.text.is_empty());
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.gui.mode, super::state::Mode::Board);
}

#[test]
fn a_shell_line_asks_first_and_esc_cancels() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_on_front::<Tui>(backend.clone(), Arc::new(FakeDesktop::default()), None);
    app.cards
        .push(card("alpha", Counts::default(), Upstream::None));
    app.marked.insert(PathBuf::from("/repos/alpha"));
    press(&mut app, KeyCode::Char(':'));
    type_line(&mut app, "!ls");
    press(&mut app, KeyCode::Enter);
    assert!(app.confirm.is_some());
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "Shell command"));
    assert!(shown(&lines, "Enter runs, Esc cancels"));
    press(&mut app, KeyCode::Esc);
    assert!(app.confirm.is_none());
    assert!(backend.shell_runs.lock().unwrap().is_empty());
    type_line(&mut app, "!ls");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |_| !backend.shell_runs.lock().unwrap().is_empty());
}

#[test]
fn history_up_suggestion_and_search() {
    let mut app = board_app();
    app.history = crate::domain::history::History::from_lines(
        ["fetch --all", "status", "log --oneline"].map(str::to_owned),
    );
    press(&mut app, KeyCode::Char(':'));
    press(&mut app, KeyCode::Up);
    assert_eq!(app.gui.cli.input.text, "log --oneline");
    ctrl(&mut app, 'u');
    type_line(&mut app, "fe");
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "git fetch --all"), "the grey suggestion");
    press(&mut app, KeyCode::Right);
    assert_eq!(app.gui.cli.input.text, "fetch --all");
    ctrl(&mut app, 'u');
    type_line(&mut app, "stt");
    ctrl(&mut app, 'r');
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, " history "));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.gui.cli.input.text, "status");
    assert!(app.gui.cli.popup.is_none());
}

#[test]
fn tab_completes_a_branch_of_the_marked_rows() {
    let mut app = board_app();
    app.cards[0].probe = Some(Ok(snapshot(
        Counts::default(),
        Upstream::None,
        &["main", "feature-a", "feature-b"],
    )));
    app.marked.insert(PathBuf::from("/repos/alpha"));
    app.vocab.subcommands = vec!["switch".to_owned()];
    press(&mut app, KeyCode::Char(':'));
    type_line(&mut app, "switch fea");
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.gui.cli.input.text, "switch feature-",
        "the common prefix"
    );
    assert!(app.gui.cli.popup.is_some());
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.gui.cli.input.text, "switch feature-b ");
}

/// An app whose one marked row ran `status`, settled.
fn ran_status() -> (App, Arc<FakeDesktop>) {
    let backend = Arc::new(FakeBackend::default());
    let desktop = Arc::new(FakeDesktop::default());
    let mut app = app_on_front::<Tui>(backend, desktop.clone(), None);
    app.cards
        .push(card("alpha", Counts::default(), Upstream::None));
    app.marked.insert(PathBuf::from("/repos/alpha"));
    app.run_cli_line("status");
    crate::app::tests::settle(&mut app, |app| {
        app.log.last().is_some_and(|run| run.outcome().is_some())
    });
    (app, desktop)
}

#[test]
fn the_output_panel_shows_each_run_and_section() {
    let (mut app, _) = ran_status();
    let lines = screen(&mut app, 100, 40);
    assert!(shown(&lines, " output "));
    assert!(shown(&lines, "$ git status"));
    assert!(shown(&lines, "\u{25be} alpha  exit 0"));
    press(&mut app, KeyCode::Char('o'));
    assert!(app.config.output_hidden);
    let lines = screen(&mut app, 100, 40);
    assert!(!shown(&lines, "$ git status"));
    press(&mut app, KeyCode::Char('o'));
    assert!(!app.config.output_hidden);
}

#[test]
fn tab_focuses_the_output_where_enter_folds_and_e_opens_the_pager() {
    let (mut app, desktop) = ran_status();
    screen(&mut app, 100, 40);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.gui.mode, super::state::Mode::Output);
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    let lines = screen(&mut app, 100, 40);
    assert!(shown(&lines, "\u{25b8} alpha  exit 0"), "folded");
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(
        desktop.files.lock().unwrap().len(),
        1,
        "the run went to the pager"
    );
    press(&mut app, KeyCode::Char('C'));
    assert!(app.log.is_empty());
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.gui.mode, super::state::Mode::Board);
}

#[test]
fn the_hidden_panel_marks_news_on_the_command_line() {
    let (mut app, _) = ran_status();
    app.config.output_hidden = true;
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "\u{2022} git"));
    app.config.output_hidden = false;
    screen(&mut app, 100, 24);
    app.config.output_hidden = true;
    let lines = screen(&mut app, 100, 24);
    assert!(!shown(&lines, "\u{2022} git"), "the panel showed it");
}

#[test]
fn digits_run_card_buttons_and_m_runs_one_on_the_marked_rows() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_on_front::<Tui>(backend.clone(), Arc::new(FakeDesktop::default()), None);
    for name in ["alpha", "beta", "gamma"] {
        app.cards
            .push(card(name, Counts::default(), Upstream::None));
    }
    let first = app.config.buttons[0].clone();
    let lines = screen(&mut app, 120, 24);
    assert!(shown(&lines, &format!("1 {}", first.label)));
    press(&mut app, KeyCode::Char('1'));
    crate::app::tests::settle(&mut app, |_| backend.runs.lock().unwrap().len() == 1);
    assert_eq!(
        backend.runs.lock().unwrap()[0].0,
        PathBuf::from("/repos/alpha")
    );
    app.marked.insert(PathBuf::from("/repos/beta"));
    app.marked.insert(PathBuf::from("/repos/gamma"));
    press(&mut app, KeyCode::Char('m'));
    let lines = screen(&mut app, 120, 30);
    assert!(shown(&lines, " run in 2 repositories "));
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |_| backend.runs.lock().unwrap().len() == 3);
}

#[test]
fn t_opens_a_shell_in_the_row_and_ignores_the_terminal_setting() {
    let desktop = Arc::new(FakeDesktop::default());
    let mut app = app_on_front::<Tui>(Arc::default(), desktop.clone(), None);
    app.cards
        .push(card("alpha", Counts::default(), Upstream::None));
    press(&mut app, KeyCode::Char('t'));
    let asked = desktop.terminals.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].0, PathBuf::from("/repos/alpha"));
}

fn pick(app: &mut App, label: &str) {
    let super::state::Mode::Menu(menu) = &mut app.gui.mode else {
        panic!("no menu is open");
    };
    let at = menu
        .shown()
        .iter()
        .position(|item| item.label == label)
        .unwrap_or_else(|| panic!("no {label}"));
    menu.selected = at;
    press(app, KeyCode::Enter);
}

#[test]
fn x_opens_the_card_menu_whose_copy_entries_go_to_the_clipboard() {
    let mut app = board_app();
    press(&mut app, KeyCode::Char('x'));
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " alpha "));
    assert!(shown(&lines, "Copy branch"));
    pick(&mut app, "Copy branch");
    assert_eq!(app.gui.copies, vec!["main".to_owned()]);
    assert!(app.toasts.iter().any(|t| t.text == "Copied main"));
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Copy path");
    assert_eq!(app.gui.copies[1], "/repos/alpha");
}

#[test]
fn copying_the_fetch_error_takes_it_off_the_row() {
    let mut app = board_app();
    app.cards[0].fetch_error = Some(crate::domain::repo::FetchError {
        summary: "auth failed".to_owned(),
        output: "fatal: could not read Username".to_owned(),
    });
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "!alpha"));
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Copy the fetch error and take it off");
    assert_eq!(
        app.gui.copies,
        vec!["fatal: could not read Username".to_owned()]
    );
    assert!(app.cards[0].fetch_error.is_none());
}

#[test]
fn a_right_click_opens_the_menu_of_that_row() {
    let mut app = board_app();
    screen(&mut app, 100, 24);
    let (y, _) = app.gui.rows_at[1].clone();
    keys::on_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: 5,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
    );
    let super::state::Mode::Menu(menu) = &app.gui.mode else {
        panic!("no menu");
    };
    assert_eq!(menu.title, "beta");
    assert!(app.marked.is_empty(), "a right click does not mark");
}

/// An app with one row `alpha` on `main` that has two more branches.
fn branchy() -> (App, Arc<FakeBackend>) {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_on_front::<Tui>(backend.clone(), Arc::new(FakeDesktop::default()), None);
    let mut alpha = card("alpha", Counts::default(), Upstream::None);
    alpha.probe = Some(Ok(snapshot(
        Counts::default(),
        Upstream::None,
        &["main", "feature-a", "feature-b"],
    )));
    app.cards.push(alpha);
    (app, backend)
}

#[test]
fn the_card_menu_switches_to_a_branch_from_a_filtered_list() {
    let (mut app, backend) = branchy();
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Switch to\u{2026}");
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " switch to "));
    assert!(shown(&lines, "feature-b"));
    type_line(&mut app, "-a");
    let lines = screen(&mut app, 100, 30);
    assert!(!shown(&lines, "feature-b"), "the filter");
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |_| !backend.runs.lock().unwrap().is_empty());
    assert_eq!(
        backend.runs.lock().unwrap()[0].1,
        vec!["switch".to_owned(), "feature-a".to_owned()]
    );
}

#[test]
fn create_new_branch_asks_for_its_name() {
    let (mut app, backend) = branchy();
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Create new branch\u{2026}");
    type_line(&mut app, "topic");
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " new branch from the checked-out commit "));
    assert!(shown(&lines, "topic"));
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |_| !backend.runs.lock().unwrap().is_empty());
    assert_eq!(
        backend.runs.lock().unwrap()[0].1,
        vec!["switch".to_owned(), "-c".to_owned(), "topic".to_owned()]
    );
}

#[test]
fn the_tag_and_remote_lists_say_when_there_is_nothing() {
    let (mut app, _) = branchy();
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Switch to tag\u{2026}");
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, "no tags"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Track remote\u{2026}");
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, "no remote"));
}

#[test]
fn a_new_favorite_from_the_card_menu_with_name_color_and_emoji() {
    let mut app = board_app();
    app.marked.insert(PathBuf::from("/repos/alpha"));
    app.marked.insert(PathBuf::from("/repos/beta"));
    press(&mut app, KeyCode::Char('x'));
    let lines = screen(&mut app, 100, 30);
    assert!(
        shown(&lines, " 2 repositories "),
        "a marked row's menu is for all marked"
    );
    pick(&mut app, "New favorite\u{2026}");
    type_line(&mut app, "etl");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Right);
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " new favorite "));
    assert!(shown(&lines, "orange"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.favorites.len(), 1);
    let favorite = &app.config.favorites[0];
    assert_eq!(favorite.name, "etl");
    assert_eq!(favorite.color, "orange");
    assert_eq!(favorite.emoji.as_deref(), Some("\u{2b50}"));
    assert_eq!(favorite.repos.len(), 2);
    let lines = screen(&mut app, 100, 30);
    // A wide emoji leaves a filler cell behind it in the buffer.
    assert!(
        lines
            .iter()
            .any(|l| l.contains('\u{2b50}') && l.contains(" etl ")),
        "the favorites line"
    );
}

#[test]
fn a_favorite_without_a_name_stays_open_and_esc_cancels() {
    let mut app = board_app();
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "New favorite\u{2026}");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(app.gui.mode, super::state::Mode::Favorite(_)));
    press(&mut app, KeyCode::Esc);
    assert!(app.favorite_dialog.is_none());
    assert!(app.config.favorites.is_empty());
}

#[test]
fn f_lists_the_favorites_to_show_edit_and_delete() {
    let mut app = board_app();
    app.config
        .favorites
        .push(crate::domain::favorites::Favorite {
            name: "web".to_owned(),
            color: "blue".to_owned(),
            emoji: None,
            repos: vec![PathBuf::from("/repos/gamma")],
        });
    press(&mut app, KeyCode::Char('f'));
    pick(&mut app, "web");
    pick(&mut app, "Show only these");
    assert_eq!(app.active_favorite, Some(0));
    let lines = screen(&mut app, 100, 24);
    assert!(!shown(&lines, "alpha"));
    assert!(shown(&lines, "gamma"));
    press(&mut app, KeyCode::Char('f'));
    pick(&mut app, "web (shown)");
    pick(&mut app, "Edit\u{2026}");
    ctrl(&mut app, 'u');
    type_line(&mut app, "front");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.favorites[0].name, "front");
    press(&mut app, KeyCode::Char('f'));
    pick(&mut app, "front (shown)");
    pick(&mut app, "Delete");
    assert!(app.config.favorites.is_empty());
    assert_eq!(app.active_favorite, None);
}

#[test]
fn plus_adds_the_repositories_below_a_typed_folder() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["one", "two"] {
        let repo = dir.path().join("work").join(name);
        std::fs::create_dir_all(repo.join(".git/objects")).unwrap();
        std::fs::create_dir_all(repo.join(".git/refs")).unwrap();
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    }
    let mut app = app();
    let lines = screen(&mut app, 100, 24);
    assert!(shown(&lines, "+ adds one"));
    press(&mut app, KeyCode::Char('+'));
    type_line(&mut app, &format!("{}/wo", dir.path().display()));
    press(&mut app, KeyCode::Tab);
    assert!(
        matches!(&app.gui.mode, super::state::Mode::Prompt(p) if p.input.text.ends_with("/work/"))
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.cards.len(), 2, "{:?}", app.toasts);
}

#[test]
fn a_folder_that_is_not_there_says_so() {
    let mut app = app();
    press(&mut app, KeyCode::Char('+'));
    type_line(&mut app, "/no/such/folder/here");
    press(&mut app, KeyCode::Enter);
    assert!(app.toasts.iter().any(|t| t.text.starts_with("No folder")));
}

#[test]
fn the_card_menu_hides_or_removes_the_marked_rows() {
    let mut app = board_app();
    app.marked.insert(PathBuf::from("/repos/alpha"));
    app.marked.insert(PathBuf::from("/repos/beta"));
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Hide");
    assert_eq!(app.cards.len(), 1);
    assert_eq!(app.config.hidden.len(), 2);
    press(&mut app, KeyCode::Char('x'));
    pick(&mut app, "Remove from board");
    assert!(app.cards.is_empty());
}

/// An app with one row `alpha` whose repository view has changed files and
/// one commit.
fn repo_app() -> (App, Arc<FakeBackend>) {
    use crate::domain::details::{ChangeKind, Details, FileChange, parse_log};
    let backend = Arc::new(FakeBackend::default());
    *backend.details.lock().unwrap() = Some(Details {
        files: vec![
            FileChange {
                path: "src/main.rs".into(),
                kind: ChangeKind::Modified,
            },
            FileChange {
                path: "src/ui/deep/new.rs".into(),
                kind: ChangeKind::Added,
            },
            FileChange {
                path: "notes.txt".into(),
                kind: ChangeKind::Added,
            },
        ],
        commits: parse_log(
            "aaaabbbb\taaaabbb\tAda Lovelace\t1700000000\t2023-11-14 22:13\t\tFix the thing\n",
        ),
        branches: Vec::new(),
    });
    let mut app = app_on_front::<Tui>(backend.clone(), Arc::new(FakeDesktop::default()), None);
    app.cards
        .push(card("alpha", Counts::default(), Upstream::None));
    (app, backend)
}

fn open_view(app: &mut App) {
    press(app, KeyCode::Enter);
    crate::app::tests::settle(app, |app| {
        app.repo_view.as_ref().is_some_and(|v| v.details.is_some())
    });
}

fn settle_diff(app: &mut App) {
    crate::app::tests::settle(app, |app| {
        app.repo_view.as_ref().is_some_and(|v| v.diff.is_some())
    });
}

#[test]
fn enter_opens_the_repository_view_on_its_files_as_a_tree() {
    let (mut app, _) = repo_app();
    app.cards[0].probe = Some(Ok(snapshot(
        Counts {
            modified: 1,
            ..Counts::default()
        },
        Upstream::None,
        &["main"],
    )));
    open_view(&mut app);
    let lines = screen(&mut app, 120, 30);
    assert!(shown(&lines, " 1 Files "));
    assert!(shown(&lines, " Changed files (3) "));
    assert!(shown(&lines, "\u{25be} src/"));
    assert!(shown(&lines, "M main.rs"));
    assert!(shown(&lines, "Pick a file to see its changes."));
    press(&mut app, KeyCode::Enter);
    let lines = screen(&mut app, 120, 30);
    assert!(shown(&lines, "\u{25b8} src/"), "Enter folds the folder");
    assert!(!shown(&lines, "M main.rs"));
    press(&mut app, KeyCode::Esc);
    assert!(app.repo_view.is_none());
}

#[test]
fn a_picked_file_shows_its_diff_and_y_copies_it() {
    let (mut app, _) = repo_app();
    app.cards[0].probe = Some(Ok(snapshot(
        Counts {
            modified: 1,
            ..Counts::default()
        },
        Upstream::None,
        &["main"],
    )));
    open_view(&mut app);
    let main = super::repo::rows(&app)
        .iter()
        .position(|r| r.line.to_string().contains("M main.rs"))
        .unwrap();
    app.gui
        .view_cursor
        .insert(crate::domain::details::RepoTab::Files, main);
    press(&mut app, KeyCode::Enter);
    settle_diff(&mut app);
    let lines = screen(&mut app, 120, 30);
    assert!(shown(&lines, "modified src/main.rs"));
    assert!(shown(&lines, "+new"));
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(app.gui.copies.len(), 1);
    assert!(app.gui.copies[0].contains("+new"));
}

#[test]
fn number_keys_switch_tabs_and_brackets_go_back_and_forward() {
    let (mut app, _) = repo_app();
    open_view(&mut app);
    let tab = |app: &App| app.repo_view.as_ref().unwrap().tab;
    assert_eq!(
        tab(&app),
        crate::domain::details::RepoTab::Commits,
        "a clean repository"
    );
    press(&mut app, KeyCode::Char('1'));
    assert_eq!(tab(&app), crate::domain::details::RepoTab::Files);
    press(&mut app, KeyCode::Char('['));
    assert_eq!(tab(&app), crate::domain::details::RepoTab::Commits);
    press(&mut app, KeyCode::Char(']'));
    assert_eq!(tab(&app), crate::domain::details::RepoTab::Files);
}

#[test]
fn a_double_click_on_a_row_opens_it_and_keeps_its_mark() {
    let (mut app, _) = repo_app();
    screen(&mut app, 100, 24);
    let (y, _) = app.gui.rows_at[0].clone();
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    keys::on_mouse(&mut app, click);
    keys::on_mouse(&mut app, click);
    assert!(app.repo_view.is_some());
    assert!(app.marked.is_empty());
}

#[test]
fn the_commits_tab_lists_the_history_and_shows_a_picked_commit() {
    let (mut app, _) = repo_app();
    open_view(&mut app);
    let lines = screen(&mut app, 120, 34);
    assert!(shown(&lines, "AL aaaabbb Fix the thing"));
    press(&mut app, KeyCode::Enter);
    settle_diff(&mut app);
    let lines = screen(&mut app, 120, 34);
    assert!(shown(&lines, "aaaabbbb [main] [v9]"), "{lines:#?}");
    assert!(shown(&lines, "Ada Lovelace <ada@example.org>"));
    assert!(shown(&lines, "Because it was broken."));
    assert!(shown(&lines, "2 files changed +15 -1"));
    assert!(shown(&lines, "\u{25b8} M src/main.rs +3 -1"));
}

#[test]
fn tab_goes_to_the_commit_pane_where_enter_unfolds_a_file() {
    let (mut app, _) = repo_app();
    open_view(&mut app);
    press(&mut app, KeyCode::Enter);
    settle_diff(&mut app);
    screen(&mut app, 120, 34);
    press(&mut app, KeyCode::Tab);
    assert!(app.gui.pane_focus);
    press(&mut app, KeyCode::Char('G'));
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Enter);
    crate::app::tests::settle(&mut app, |app| {
        app.repo_view
            .as_ref()
            .is_some_and(|v| v.unfolded.values().any(Option::is_some))
    });
    let lines = screen(&mut app, 120, 34);
    assert!(shown(&lines, "+after in src/main.rs"), "{lines:#?}");
    press(&mut app, KeyCode::Esc);
    assert!(!app.gui.pane_focus, "Esc gives the keys back to the list");
    assert!(app.repo_view.is_some());
}

#[test]
fn c_copies_a_commit_value_and_p_goes_to_a_parent() {
    let (mut app, _) = repo_app();
    open_view(&mut app);
    press(&mut app, KeyCode::Enter);
    settle_diff(&mut app);
    press(&mut app, KeyCode::Char('c'));
    pick(&mut app, "Copy mail ada@example.org");
    assert_eq!(app.gui.copies, vec!["ada@example.org".to_owned()]);
    press(&mut app, KeyCode::Char('p'));
    pick(&mut app, "bbbbcccc");
    assert_eq!(
        app.repo_view.as_ref().unwrap().selected,
        Some(crate::domain::details::DiffTarget::Commit(
            "bbbbcccc".to_owned()
        ))
    );
}

/// `repo_app` with a graph: main with two plain commits over the base, and
/// side-branch with one commit on the base.
fn graph_app() -> (App, Arc<FakeBackend>) {
    use crate::domain::graph::{GraphCommit, GraphData, GraphRef, RefKind};
    let (app, backend) = repo_app();
    let commit = |hash: &str, parent: &str, subject: &str| GraphCommit {
        hash: hash.into(),
        parents: vec![parent.into()],
        author: "Ada Lovelace".into(),
        subject: subject.into(),
    };
    let name = |name: &str, kind, hash: &str| GraphRef {
        name: name.into(),
        kind,
        hash: hash.into(),
        time: 0,
    };
    *backend.graph.lock().unwrap() = Some(GraphData {
        commits: vec![
            commit("aa01", "aa02", "Tip of main"),
            commit("aa02", "aa03", "Plain one"),
            commit("aa03", "ba5e", "Plain two"),
            commit("51de", "ba5e", "Work on the side"),
            commit("ba5e", "0000", "Where they part"),
        ],
        refs: vec![
            name("main", RefKind::Local, "aa01"),
            name("origin/main", RefKind::Remote, "aa01"),
            name("side-branch", RefKind::Local, "51de"),
            name("v1", RefKind::Tag, "ba5e"),
        ],
        hidden: 2,
        cut: false,
    });
    (app, backend)
}

#[test]
fn the_graph_tab_draws_the_branches_and_unfolds_a_run() {
    let (mut app, _) = graph_app();
    open_view(&mut app);
    press(&mut app, KeyCode::Char('3'));
    crate::app::tests::settle(&mut app, |app| {
        app.repo_view.as_ref().is_some_and(|v| v.graph.is_some())
    });
    let lines = screen(&mut app, 140, 34);
    assert!(shown(&lines, "2 old remote branches hidden"), "{lines:#?}");
    assert!(shown(&lines, "[main = origin]"));
    assert!(shown(&lines, "Tip of main"));
    assert!(shown(&lines, "2 commits"));
    assert!(shown(&lines, "[v1]"));
    assert!(lines.iter().any(|l| l.contains('\u{25cf}')), "commit dots");
    let rows = super::repo::rows(&app);
    let fold = rows
        .iter()
        .position(|r| matches!(r.pick, super::repo::Pick::Unfold(_)))
        .unwrap();
    app.gui
        .view_cursor
        .insert(crate::domain::details::RepoTab::Graph, fold);
    press(&mut app, KeyCode::Enter);
    let lines = screen(&mut app, 140, 34);
    assert!(shown(&lines, "Plain one"), "the run unfolded");
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Enter);
    assert!(
        app.repo_view.as_ref().unwrap().with_old,
        "the old branches line"
    );
}

/// Put the Settings cursor on the first row that is `row`.
fn settings_to(app: &mut App, row: super::settings::Row) {
    let rows = super::settings::rows(app);
    let at = rows.iter().position(|r| *r == row).unwrap();
    app.gui.settings_cursor = at;
}

#[test]
fn settings_change_values_with_the_arrows_and_leave_out_sounds_and_the_terminal() {
    use super::settings::Row;
    let mut app = board_app();
    press(&mut app, KeyCode::Char('S'));
    let lines = screen(&mut app, 100, 40);
    assert!(shown(&lines, " Settings "));
    assert!(shown(&lines, "Parallel git processes"));
    assert!(!shown(&lines, "Sounds"));
    assert!(!shown(&lines, "music"));
    assert!(!shown(&lines, "Terminal command"));
    let before = app.config.concurrency;
    settings_to(&mut app, Row::Concurrency);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.config.concurrency, before + 1);
    settings_to(&mut app, Row::Theme);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.config.theme, crate::domain::config::Theme::Light);
    settings_to(&mut app, Row::SingleMatch);
    let single = app.config.auto_insert_single_match;
    press(&mut app, KeyCode::Enter);
    assert_ne!(app.config.auto_insert_single_match, single);
    press(&mut app, KeyCode::Esc);
    assert!(!app.settings_open);
}

#[test]
fn settings_edit_add_move_and_remove_card_buttons() {
    use super::settings::Row;
    let mut app = board_app();
    let count = app.config.buttons.len();
    press(&mut app, KeyCode::Char('S'));
    settings_to(&mut app, Row::AddButton);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.buttons.len(), count + 1);
    settings_to(&mut app, Row::Button(count));
    press(&mut app, KeyCode::Enter);
    ctrl(&mut app, 'u');
    type_line(&mut app, "log");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('a'));
    ctrl(&mut app, 'u');
    type_line(&mut app, "log --oneline -5");
    press(&mut app, KeyCode::Enter);
    let last = app.config.buttons.last().unwrap();
    assert_eq!(
        (last.label.as_str(), last.args.as_str()),
        ("log", "log --oneline -5")
    );
    press(&mut app, KeyCode::Left);
    assert_eq!(app.config.buttons[count - 1].label, "log", "moved up");
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.config.buttons.len(), count);
    assert!(app.config.buttons.iter().all(|b| b.label != "log"));
    settings_to(&mut app, Row::AddPreset);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.buttons.len(), count + 1, "a preset");
}

#[test]
fn hidden_repositories_are_unhidden_in_settings() {
    use super::settings::Row;
    let mut app = board_app();
    app.hide_cards(&[PathBuf::from("/repos/alpha"), PathBuf::from("/repos/beta")]);
    press(&mut app, KeyCode::Char('S'));
    let lines = screen(&mut app, 100, 40);
    assert!(shown(&lines, "Hidden repositories (2)"));
    settings_to(&mut app, Row::Hidden(PathBuf::from("/repos/alpha")));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.hidden, vec![PathBuf::from("/repos/beta")]);
    settings_to(&mut app, Row::UnhideAll);
    press(&mut app, KeyCode::Enter);
    assert!(app.config.hidden.is_empty());
}

#[test]
fn about_copies_its_links_and_lists_the_licenses() {
    use super::settings::Row;
    let mut app = board_app();
    press(&mut app, KeyCode::Char('S'));
    settings_to(&mut app, Row::About);
    press(&mut app, KeyCode::Enter);
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, " About multigit "));
    assert!(shown(&lines, crate::domain::about::VERSION));
    assert!(shown(&lines, "https://github.com/paxel/multigit"));
    settings_to(
        &mut app,
        Row::Link(
            "Write the developer",
            crate::domain::about::DEVELOPER_MAIL.to_owned(),
        ),
    );
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.gui.copies,
        vec![crate::domain::about::DEVELOPER_MAIL.to_owned()]
    );
    settings_to(&mut app, Row::Licenses);
    press(&mut app, KeyCode::Enter);
    let lines = screen(&mut app, 100, 30);
    assert!(shown(&lines, "ratatui"));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.settings_page, crate::app::SettingsPage::Settings);
}

#[test]
fn kawaii_puts_critters_and_confetti_on_the_rows_in_pastels() {
    let mut app = board_app();
    app.config.theme = crate::domain::config::Theme::Kawaii;
    app.advance_kawaii(0.1, true);
    app.tend_critters();
    assert!(!app.critters.critters.is_empty());
    app.bursts
        .push((PathBuf::from("/repos/beta"), app.kawaii_clock));
    let mut terminal = Terminal::new(TestBackend::new(110, 24)).unwrap();
    terminal.draw(|frame| board::draw(frame, &mut app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = buffer.content.iter().map(|c| c.symbol()).collect();
    assert!(
        text.contains("'.'") || text.contains("-.-"),
        "a critter's face"
    );
    assert!(
        text.contains('\u{2665}') || text.contains('\u{2726}'),
        "confetti"
    );
    assert!(
        buffer.content.iter().all(|c| !matches!(
            c.fg,
            ratatui::style::Color::Green | ratatui::style::Color::Cyan
        )),
        "no plain terminal colors left"
    );
}

#[test]
fn a_click_on_a_critter_pets_it_and_marks_nothing() {
    let mut app = board_app();
    app.config.theme = crate::domain::config::Theme::Kawaii;
    app.advance_kawaii(0.1, true);
    app.tend_critters();
    screen(&mut app, 110, 24);
    let home = app.critters.critters[0].home.clone();
    let (y, _) = app
        .gui
        .rows_at
        .iter()
        .find(|(_, path)| *path == home)
        .cloned()
        .unwrap();
    keys::on_mouse(
        &mut app,
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: y,
            modifiers: KeyModifiers::NONE,
        },
    );
    assert!(app.marked.is_empty());
    assert_eq!(
        app.critters.critters[0].mood,
        crate::domain::critters::Mood::Pet
    );
}
