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
fn clicking_a_value_copies_it_and_keeps_the_mark() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    // The path line of `/repo` shows its parent and copies the whole path.
    for (label, copy) in [("repo", "repo"), ("main", "main"), ("/", "/repo")] {
        h.get_by_label(label).click();
        // The click lands on the last of the queued events: read that pass.
        h.step();
        assert_eq!(copied(&h), vec![copy.to_owned()], "{label}");
        assert!(h.state().copy_flash.is_some(), "{label}");
        h.run();
    }
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));
}

#[test]
fn clicking_a_field_name_or_state_marks_the_card_and_copies_nothing() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    let mut marked = true;
    for label in ["branch", "clean", "no upstream", "no tag", "status"] {
        h.get_by_label(label).click();
        h.step();
        assert!(copied(&h).is_empty(), "{label}");
        h.run();
        marked = !marked;
        assert_eq!(
            h.state().marked.contains(&PathBuf::from("/repo")),
            marked,
            "{label}"
        );
    }
    assert!(h.state().copy_flash.is_none());
}

#[test]
fn detached_head_copies_the_sha_only() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = marked_app(backend.clone(), &[]);
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.status.head = crate::domain::status::Head::Detached("1a2b3c4".into());
    // The first refresh must bring the same snapshot.
    *backend.snapshot.lock().unwrap() = Some(snap.clone());
    app.cards[0].probe = Some(Ok(snap));
    let mut h = harness(app);
    h.run();
    h.get_by_label("detached @");
    h.get_by_label("1a2b3c4").click();
    h.step();
    assert_eq!(copied(&h), vec!["1a2b3c4".to_owned()]);
}

#[test]
fn fetch_error_is_one_line_and_a_click_copies_all_of_it_and_removes_it() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    // Set after the start, whose first (successful) fetch would clear it.
    settle(&mut h, |a| a.cards[0].snapshot().is_some());
    let long =
        "cannot lock ref 'refs/remotes/origin/some/very/long/branch/name/that/goes/on': exists";
    let output = format!("From x\nerror: {long}\n ! [new branch] a");
    h.state_mut().cards[0].fetch_error = Some(crate::domain::repo::FetchError {
        summary: format!("error: {long}"),
        output: output.clone(),
    });
    h.run();
    let line = h.get_by_label_contains("Error: cannot lock ref");
    let card_right = h.get_by_label("status").rect().left() + 290.0;
    assert!(line.rect().right() <= card_right + 1.0, "{:?}", line.rect());
    assert!(line.rect().height() < 30.0, "{:?}", line.rect());
    line.click();
    h.step();
    assert_eq!(copied(&h), vec![output]);
    h.run();
    assert!(h.state().cards[0].fetch_error.is_none());
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));
}

#[test]
fn filter_field_hides_cards_and_counts_the_shown_ones() {
    let mut app = app_with(Arc::default(), None);
    app.cards = vec![card("/r/etl-load"), card("/r/web"), card("/r/Etl-store")];
    app.mark_all();
    let mut h = harness(app);
    h.run();
    assert!(
        h.query_by_label("3 of 3").is_none(),
        "no count without text"
    );
    // The field keeps the right end of the toolbar, in the buttons' row.
    // Two text fields exist: the filter on top, the command line at the bottom.
    let field = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .min_by(|a, b| a.top().total_cmp(&b.top()))
        .unwrap();
    let first = h.get_by_label(" + ").rect();
    assert!(field.right() > 1150.0, "{field:?}");
    assert!(
        (field.top() - first.top()).abs() < 3.0,
        "{field:?} {first:?}"
    );
    h.ctx
        .memory_mut(|m| m.request_focus(crate::ui::view::filter_id()));
    h.run();
    type_text(&mut h, "etl");
    assert_eq!(h.state().filter, "etl");
    h.get_by_label("etl-load");
    h.get_by_label("Etl-store");
    assert!(h.query_by_label("web").is_none());
    h.get_by_label("2 of 3");
    h.get_by_label("2 marked:");

    type_text(&mut h, "x");
    h.get_by_label("No repository matches the filter.");
    h.get_by_label("0 of 3");
    // The command line has nothing to run in.
    assert!(h.state().marked_repos().is_empty());
}

#[test]
fn sort_button_cycles_the_key_and_reorders_the_cards() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_with(backend, None);
    app.cards = vec![card("/b/alpha"), card("/a/zeta")];
    let mut h = harness(app);
    h.step();
    let left_of = |h: &Harness<'static, App>, a: &str, b: &str| {
        h.get_by_label(a).rect().left() < h.get_by_label(b).rect().left()
    };
    assert!(left_of(&h, "alpha", "zeta"));
    h.get_by_label("Sort: name").click();
    h.step();
    h.step();
    h.get_by_label("Sort: path");
    assert!(left_of(&h, "zeta", "alpha"));
    for label in ["path", "branch", "tag", "dirty first", "behind first"] {
        h.get_by_label(&format!("Sort: {label}")).click();
        h.step();
        h.step();
    }
    h.get_by_label("Sort: name");
    assert!(left_of(&h, "alpha", "zeta"));
}

/// A harness with one card whose checked-out commit has this subject.
fn commit_card(subject: &str) -> Harness<'static, App> {
    let backend = Arc::new(FakeBackend::default());
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.commit = Some(crate::domain::repo::Commit {
        hash: "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d".into(),
        short: "1a2b3c4".into(),
        subject: subject.into(),
    });
    *backend.snapshot.lock().unwrap() = Some(snap.clone());
    let mut app = app_with(backend, None);
    app.cards = vec![card("/r/repo")];
    app.cards[0].probe = Some(Ok(snap));
    let mut h = harness(app);
    h.run();
    h
}

#[test]
fn card_shows_the_commit_on_one_line() {
    let h = commit_card("Fix race in SSE handler");
    h.get_by_label("commit");
    h.get_by_label("1a2b3c4 Fix race in SSE handler");

    let long = "Rewrite the whole thing from scratch, twice, with a very long subject line";
    let h = commit_card(long);
    let line = h.get_by_label_contains("1a2b3c4 Rewrite").rect();
    let card_right = h.get_by_label("status").rect().left() + 290.0;
    assert!(line.right() <= card_right + 1.0, "{line:?}");
    assert!(line.height() < 30.0, "{line:?}");
}

#[test]
fn card_shows_the_tag() {
    let backend = Arc::new(FakeBackend::default());
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.tags = vec!["v1.2.0".into(), "latest".into()];
    *backend.snapshot.lock().unwrap() = Some(snap);
    let mut app = app_with(backend, None);
    app.cards = vec![card("/r/tagged")];
    let mut h = harness(app);
    h.run();
    settle(&mut h, |a| {
        a.cards[0].snapshot().is_some_and(|s| !s.tags.is_empty())
    });
    h.run();
    h.get_by_label("tag");
    // Every tag is its own text and copies itself.
    h.get_by_label("v1.2.0");
    h.get_by_label("latest").click();
    h.step();
    assert_eq!(copied(&h), vec!["latest".to_owned()]);
}

#[test]
fn tags_that_do_not_fit_the_card_are_cut_with_an_ellipsis() {
    let backend = Arc::new(FakeBackend::default());
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.tags = (0..12).map(|i| format!("release-candidate-{i}")).collect();
    *backend.snapshot.lock().unwrap() = Some(snap.clone());
    let mut app = app_with(backend, None);
    app.cards = vec![card("/r/tagged")];
    app.cards[0].probe = Some(Ok(snap));
    let mut h = harness(app);
    h.run();
    let first = h.get_by_label("release-candidate-0").rect();
    let ellipsis = h.get_by_label(", \u{2026}").rect();
    assert!(
        (first.center().y - ellipsis.center().y).abs() < 2.0,
        "one line"
    );
    assert!(h.query_by_label("release-candidate-11").is_none());
    let card_right = h.get_by_label("status").rect().left() + 290.0;
    assert!(ellipsis.right() <= card_right + 1.0, "{ellipsis:?}");
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
fn theme_setting_switches_between_light_and_dark() {
    let mut app = app_with(Arc::default(), None);
    app.settings_open = true;
    let mut h = harness(app);
    h.run();
    h.get_by_label("Dark").click();
    h.run();
    assert_eq!(h.state().config.theme, crate::domain::config::Theme::Dark);
    assert!(h.ctx.global_style().visuals.dark_mode);
    h.get_by_label("Light").click();
    h.run();
    assert!(!h.ctx.global_style().visuals.dark_mode);
    // The roomier buttons hold in every theme.
    assert_eq!(
        h.ctx.global_style().spacing.button_padding,
        crate::ui::style::BUTTON_PADDING
    );
    h.get_by_label("System").click();
    h.run();
    assert_eq!(h.state().config.theme, crate::domain::config::Theme::System);
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
