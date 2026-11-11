//! Headless interaction tests of the full app view (no GPU rendering).

use crate::domain::history::History;
use crate::domain::status::{Counts, Upstream};
use crate::ui::app::tests::{FakeBackend, app_with, snapshot};
use crate::ui::app::{App, Card};
use crate::ui::cli::{PopupKind, input_id};
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn harness(app: App) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .build_ui_state(|ui, app: &mut App| app.show(ui), app);
    crate::ui::style::apply(&h.ctx);
    h
}

/// App with one marked card whose snapshot has the given branches.
fn marked_app(backend: Arc<FakeBackend>, history: &[&str]) -> App {
    *backend.snapshot.lock().unwrap() = Some(snapshot(
        Counts::default(),
        Upstream::None,
        &["main", "feature-a", "feature-b"],
    ));
    let mut app = app_with(backend, None);
    let mut card = Card {
        path: PathBuf::from("/repo"),
        probe: None,
        fetch_error: None,
        last_command: None,
    };
    card.probe = Some(Ok(snapshot(
        Counts::default(),
        Upstream::None,
        &["main", "feature-a", "feature-b"],
    )));
    app.cards.push(card);
    app.marked.insert(PathBuf::from("/repo"));
    app.history = History::from_lines(history.iter().map(|s| s.to_string()));
    app
}

/// Step until worker events (subcommand list etc.) have arrived.
fn settle(h: &mut Harness<'static, App>, done: impl Fn(&App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(h.state()) {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
        h.step();
    }
}

fn focus_cli(h: &mut Harness<'static, App>) {
    h.ctx.memory_mut(|m| m.request_focus(input_id()));
    h.run();
}

fn type_text(h: &mut Harness<'static, App>, text: &str) {
    h.event(egui::Event::Text(text.to_owned()));
    h.run();
}

fn key(h: &mut Harness<'static, App>, key: Key) {
    h.key_press(key);
    h.run();
}

fn ready(backend: Arc<FakeBackend>, history: &[&str]) -> Harness<'static, App> {
    let mut h = harness(marked_app(backend, history));
    h.run();
    settle(&mut h, |a| !a.vocab.subcommands.is_empty());
    focus_cli(&mut h);
    h
}

#[test]
fn tab_single_match_inserts_inline() {
    let mut h = ready(Arc::default(), &[]);
    type_text(&mut h, "reba");
    key(&mut h, Key::Tab);
    assert_eq!(h.state().cli.text, "rebase ");
    assert!(h.state().cli.popup.is_none());
}

#[test]
fn tab_many_opens_popup_and_enter_accepts() {
    let mut h = ready(Arc::default(), &[]);
    type_text(&mut h, "switch fea");
    key(&mut h, Key::Tab);
    assert_eq!(h.state().cli.text, "switch feature-");
    assert_eq!(
        h.state().cli.popup.as_ref().map(|p| p.kind),
        Some(PopupKind::Completion)
    );
    key(&mut h, Key::ArrowDown);
    key(&mut h, Key::Enter);
    assert_eq!(h.state().cli.text, "switch feature-b ");
    assert!(h.state().cli.popup.is_none());
    // Focus stays in the command line.
    assert!(h.ctx.memory(|m| m.has_focus(input_id())));
}

#[test]
fn popup_row_click_inserts() {
    let mut h = ready(Arc::default(), &[]);
    type_text(&mut h, "switch fea");
    key(&mut h, Key::Tab);
    h.get_by_label("feature-a").click();
    h.run();
    assert_eq!(h.state().cli.text, "switch feature-a ");
    assert!(h.state().cli.popup.is_none());
}

#[test]
fn auto_insert_off_opens_popup_for_single_match() {
    let mut app = marked_app(Arc::default(), &[]);
    app.config.auto_insert_single_match = false;
    let mut h = harness(app);
    h.run();
    settle(&mut h, |a| !a.vocab.subcommands.is_empty());
    focus_cli(&mut h);
    type_text(&mut h, "reba");
    key(&mut h, Key::Tab);
    assert_eq!(h.state().cli.text, "reba");
    assert!(h.state().cli.popup.is_some());
    key(&mut h, Key::Escape);
    assert!(h.state().cli.popup.is_none());
    assert!(h.ctx.memory(|m| m.has_focus(input_id())));
}

#[test]
fn history_suggestion_and_up_down() {
    let mut h = ready(Arc::default(), &["pull --rebase", "status"]);
    type_text(&mut h, "pu");
    key(&mut h, Key::ArrowRight);
    assert_eq!(h.state().cli.text, "pull --rebase");

    let mut h = ready(Arc::default(), &["pull --rebase", "status", "push"]);
    type_text(&mut h, "pu");
    key(&mut h, Key::ArrowUp);
    assert_eq!(h.state().cli.text, "push");
    key(&mut h, Key::ArrowUp);
    assert_eq!(h.state().cli.text, "pull --rebase");
    key(&mut h, Key::ArrowDown);
    key(&mut h, Key::ArrowDown);
    assert_eq!(h.state().cli.text, "pu");
    assert!(h.ctx.memory(|m| m.has_focus(input_id())));
}

#[test]
fn ctrl_r_fuzzy_history() {
    let mut h = ready(
        Arc::default(),
        &["log --oneline", "status", "pull --rebase"],
    );
    h.key_press_modifiers(Modifiers::CTRL, Key::R);
    h.run();
    assert_eq!(
        h.state().cli.popup.as_ref().map(|p| p.kind),
        Some(PopupKind::History)
    );
    type_text(&mut h, "lgo");
    key(&mut h, Key::Enter);
    assert_eq!(h.state().cli.text, "log --oneline");
}

#[test]
fn enter_runs_in_marked_repos() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = ready(backend.clone(), &[]);
    type_text(&mut h, "status -sb");
    key(&mut h, Key::Enter);
    assert_eq!(h.state().cli.text, "");
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    let runs = backend.runs.lock().unwrap().clone();
    assert!(runs.contains(&(
        PathBuf::from("/repo"),
        vec!["status".to_owned(), "-sb".to_owned()]
    )));
    assert_eq!(h.state().history.entries(), &["status -sb"]);
    assert!(h.ctx.memory(|m| m.has_focus(input_id())));
}

#[test]
fn bulk_destructive_shows_confirm_dialog() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = marked_app(backend.clone(), &[]);
    app.cards.push(Card {
        path: PathBuf::from("/other"),
        probe: None,
        fetch_error: None,
        last_command: None,
    });
    app.marked.insert(PathBuf::from("/other"));
    let mut h = harness(app);
    h.run();
    h.state_mut()
        .start_run("reset --hard", vec!["/repo".into(), "/other".into()]);
    h.run();
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().confirm.is_none());
    assert!(h.state().log.is_empty());
}

#[test]
fn card_shows_status_and_remove_button_works() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    h.get_by_label("repo");
    h.get_by_label("clean");
    h.get_by_label("no upstream");
    h.get_by_label("no tag");
    h.get_by_label("×").click();
    h.run();
    assert!(h.state().cards.is_empty());
    assert!(h.state().marked.is_empty());
}

/// A primary click at `pos` (for places without a widget of their own).
fn click_at(h: &mut Harness<'static, App>, pos: egui::Pos2) {
    h.event(egui::Event::PointerMoved(pos));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run();
}

/// Empty card space right of the title, left of the × button.
fn card_space(h: &Harness<'static, App>) -> egui::Pos2 {
    let title = h.get_by_label("repo").rect();
    title.right_center() + egui::vec2(60.0, 0.0)
}

#[test]
fn clicking_card_space_toggles_its_mark_but_its_buttons_do_not() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = harness(marked_app(backend.clone(), &[]));
    h.run();
    let space = card_space(&h);
    click_at(&mut h, space);
    assert!(h.state().marked.is_empty());
    click_at(&mut h, space);
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));

    // A card button runs its command and leaves the mark alone.
    h.get_all_by_label("push").last().unwrap().click();
    h.run();
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));
    assert!(
        backend
            .runs
            .lock()
            .unwrap()
            .contains(&(PathBuf::from("/repo"), vec!["push".to_owned()]))
    );
}

fn copied(h: &Harness<'static, App>) -> Vec<String> {
    h.output()
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(text) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn clicking_a_card_text_copies_it_and_keeps_the_mark() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    for text in ["repo", "branch", "main", "clean", "no upstream", "no tag"] {
        h.get_by_label(text).click();
        // The click lands on the last of the queued events: read that pass.
        h.step();
        assert_eq!(copied(&h), vec![text.to_owned()], "{text}");
        assert!(h.state().copy_flash.is_some(), "{text}");
        h.run();
    }
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));
}

#[test]
fn card_shows_the_tag() {
    let backend = Arc::new(FakeBackend::default());
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.tag = Some("v1.2.0".into());
    *backend.snapshot.lock().unwrap() = Some(snap);
    let mut app = app_with(backend, None);
    app.cards = vec![card("/r/tagged")];
    let mut h = harness(app);
    h.run();
    settle(&mut h, |a| {
        a.cards[0].snapshot().is_some_and(|s| s.tag.is_some())
    });
    h.run();
    h.get_by_label("tag");
    h.get_by_label("v1.2.0");
}

#[test]
fn shell_line_asks_with_the_shell_name_and_cancel_runs_nothing() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = ready(backend.clone(), &[]);
    type_text(&mut h, "!ls");
    let shell = crate::ui::view::login_shell_name();
    h.get_by_label(&shell);
    key(&mut h, Key::Enter);
    h.get_by_label("Shell command");
    h.get_by_label(&format!("Run `ls` with {shell} in 1 repository?"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.state().confirm.is_none());
    assert!(backend.shell_runs.lock().unwrap().is_empty());
    assert_eq!(h.state().history.entries(), &["!ls"]);
}

fn enter_event(pressed: bool) -> egui::Event {
    egui::Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn enter_confirms_a_shell_line_but_a_key_repeat_does_not() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = ready(backend.clone(), &[]);
    type_text(&mut h, "!ls");
    // Enter runs the line and stays held: egui marks the next presses as repeats.
    h.event(enter_event(true));
    h.run();
    assert!(h.state().confirm.is_some());
    h.event(enter_event(true));
    h.run();
    assert!(h.state().confirm.is_some(), "a held Enter must not confirm");
    h.event(enter_event(false));
    h.run();
    assert!(h.state().confirm.is_some());
    key(&mut h, Key::Enter);
    assert!(h.state().confirm.is_none());
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    assert_eq!(
        backend.shell_runs.lock().unwrap().clone(),
        vec![(PathBuf::from("/repo"), "ls".to_owned())]
    );
}

fn card(path: &str) -> Card {
    Card {
        path: PathBuf::from(path),
        probe: Some(Ok(snapshot(Counts::default(), Upstream::None, &["main"]))),
        fetch_error: None,
        last_command: None,
    }
}

#[test]
fn many_cards_wrap_into_rows_within_the_window() {
    let mut app = app_with(Arc::default(), None);
    app.cards = (0..12).map(|i| card(&format!("/r/repo{i:02}"))).collect();
    let mut h = harness(app);
    h.run();
    let mut tops = Vec::new();
    for i in 0..12 {
        let rect = h.get_by_label(&format!("repo{i:02}")).rect();
        assert!(
            rect.right() <= 1200.0,
            "repo{i:02} at {rect:?} is off screen"
        );
        tops.push(rect.top().round() as i64);
    }
    tops.sort_unstable();
    tops.dedup();
    assert!(tops.len() > 1, "all cards in one row");
}

#[test]
fn settings_button_editor() {
    let mut app = app_with(Arc::default(), None);
    app.settings_open = true;
    let mut h = harness(app);
    h.run();
    let args = |h: &Harness<'static, App>| {
        h.state()
            .config
            .buttons
            .iter()
            .map(|b| b.args.clone())
            .collect::<Vec<_>>()
    };
    // Remove the first default button ("pull --ff-only").
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run();
    assert_eq!(
        args(&h),
        vec!["pull --rebase", "push", "fetch --all --prune"]
    );
    // Move "push" up (second row's Up button; the first row's is disabled).
    h.get_all_by_label("Up").nth(1).unwrap().click();
    h.run();
    assert_eq!(
        args(&h),
        vec!["push", "pull --rebase", "fetch --all --prune"]
    );
    h.get_by_label("Add button").click();
    h.run();
    assert_eq!(args(&h).len(), 4);
    h.get_by_value("Add preset").click();
    h.run();
    h.get_by_label("stash pop  (git stash pop)")
        .click_accesskit();
    h.run();
    assert_eq!(args(&h).last().map(String::as_str), Some("stash pop"));
}

#[test]
fn glyphs_used_in_labels_exist_in_default_fonts() {
    let mut h = harness(app_with(Arc::default(), None));
    h.run();
    let font = egui::FontId::proportional(14.0);
    for c in ['×', '+', '~', '?', '!', '@'] {
        assert!(
            h.ctx.fonts_mut(|f| f.has_glyph(&font, c)),
            "missing glyph {c}"
        );
    }
}

#[test]
fn buttons_have_room_around_their_text() {
    let mut h = harness(app_with(Arc::default(), None));
    h.run();
    let rect = h.get_by_label("Settings").rect();
    let text_height = h.ctx.global_style().text_styles[&egui::TextStyle::Button].size;
    assert!(
        rect.height() >= text_height + 2.0 * crate::ui::style::BUTTON_PADDING.y,
        "button {rect:?} too tight"
    );
    assert!(rect.height() >= 28.0);
}
