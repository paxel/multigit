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
    harness_stepping(app, 0.25)
}

/// A harness whose frames are `step` seconds apart.
fn harness_stepping(app: App, step: f32) -> Harness<'static, App> {
    let h = Harness::builder()
        .with_size(egui::vec2(1200.0, 800.0))
        .with_step_dt(step)
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
fn card_shows_its_status() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    h.get_by_label("repo");
    h.get_by_label("clean");
    h.get_by_label("no upstream");
    h.get_by_label("no tag");
    assert!(
        h.query_by_label("\u{d7}").is_none(),
        "no remove button on the card"
    );
    assert!(h.query_by_label("Remove marked").is_none());
}

#[test]
fn right_click_removes_or_hides_the_marked_cards_and_settings_unhides() {
    let mut h = favorites_app();
    // An unmarked card: it alone is removed.
    h.get_by_label("web").click_secondary();
    h.run();
    h.get_by_label("Remove from board").click();
    h.run();
    assert_eq!(h.state().cards.len(), 2);
    assert!(h.state().config.hidden.is_empty());

    // A marked card: all marked cards are hidden.
    h.get_by_label("etl-load").click_secondary();
    h.run();
    h.get_by_label("2 repositories");
    h.get_by_label("Hide").click();
    h.run();
    assert!(h.state().cards.is_empty());
    assert!(h.state().marked.is_empty());
    let hidden = vec![PathBuf::from("/r/etl-load"), PathBuf::from("/r/etl-store")];
    assert_eq!(h.state().config.hidden, hidden);

    h.state_mut().settings_open = true;
    h.run();
    h.get_by_label("Hidden repositories");
    h.get_by_label("/r/etl-store");
    h.get_all_by_label("Unhide").next().unwrap().click();
    h.run();
    assert_eq!(h.state().config.hidden, hidden[1..].to_vec());
    assert_eq!(h.state().cards.len(), 1);
    h.get_by_label("Unhide all").click();
    h.run();
    assert!(h.state().config.hidden.is_empty());
    assert_eq!(h.state().cards.len(), 2);
    assert!(h.query_by_label("Hidden repositories").is_none());
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
fn sort_button_lists_the_keys_and_a_pick_reorders_the_cards() {
    let backend = Arc::new(FakeBackend::default());
    let mut app = app_with(backend, None);
    app.cards = vec![card("/b/alpha"), card("/a/zeta")];
    let mut h = harness(app);
    h.run();
    let left_of = |h: &Harness<'static, App>, a: &str, b: &str| {
        h.get_by_label(a).rect().left() < h.get_by_label(b).rect().left()
    };
    assert!(left_of(&h, "alpha", "zeta"));
    h.get_by_label("Sort: name").click();
    h.run();
    // Every key is in the list; one click picks any of them.
    for key in crate::domain::board::SortKey::ALL {
        h.get_by_label(key.label());
    }
    h.get_by_label("path").click();
    h.run();
    h.get_by_label("Sort: path");
    assert!(
        h.query_by_label("behind first").is_none(),
        "the list closed"
    );
    assert!(left_of(&h, "zeta", "alpha"));
    h.get_by_label("Sort: path").click();
    h.run();
    h.get_by_label("name").click();
    h.run();
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
fn commit_line_has_one_tooltip_with_the_subject() {
    let long = "Rewrite the whole thing from scratch, twice, with a very long subject line";
    let mut h = commit_card(long);
    h.get_by_label_contains("1a2b3c4 Rewrite").hover();
    // Tooltips come up after the pointer has rested.
    for _ in 0..8 {
        h.step();
    }
    // The line itself and one tooltip; a cut-off label would bring a second.
    assert_eq!(
        h.get_all_by_label_contains("with a very long subject line")
            .count(),
        2
    );
    h.get_by_label(long);
}

#[test]
fn clicking_the_commit_line_copies_the_full_hash() {
    let mut h = commit_card("Fix race in SSE handler");
    h.get_by_label("1a2b3c4 Fix race in SSE handler").click();
    h.step();
    assert_eq!(
        copied(&h),
        vec!["1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d".to_owned()]
    );
    h.run();
    assert!(
        h.state().marked.is_empty(),
        "the click does not mark the card"
    );
}

#[test]
fn output_button_folds_the_panel_away_and_marks_unseen_results() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = harness(marked_app(backend, &[]));
    h.run();
    h.get_by_label("Clear");
    // The panel's heading reads "Output" too; the button is the lower one.
    let toggle = |h: &Harness<'static, App>| {
        h.get_all_by_label("Output")
            .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
            .unwrap()
            .click();
    };
    toggle(&h);
    h.run();
    assert!(h.state().config.output_hidden);
    assert!(h.query_by_label("Clear").is_none(), "the panel is gone");

    // A command leaves the panel hidden; the button tells its result.
    h.state_mut().start_run("status", vec!["/repo".into()]);
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    h.run();
    assert!(h.state().config.output_hidden);
    assert_eq!(h.state().unseen_outcome(), Some(Some(true)));
    let mark = format!("Output {}", crate::ui::view::OUTPUT_MARK);
    h.get_by_label(&mark).click();
    h.run();
    h.get_by_label("Clear");
    // Shown once, the result is no news when the panel is hidden again.
    toggle(&h);
    h.run();
    assert_eq!(h.state().unseen_outcome(), None);
    h.get_by_label("Output");
}

#[test]
fn open_in_editor_buttons_hand_the_output_to_the_desktop() {
    let desktop = Arc::new(crate::ui::app::tests::FakeDesktop::default());
    let backend = Arc::new(FakeBackend::default());
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    let mut app = crate::ui::app::tests::app_on(backend, desktop.clone(), None);
    app.cards = vec![card("/r/one"), card("/r/two")];
    let mut h = harness(app);
    h.run();
    h.state_mut()
        .start_run("status", vec!["/r/one".into(), "/r/two".into()]);
    settle(&mut h, |a| a.log.last().unwrap().outcome().is_some());
    h.run();
    // One button for the command, one per repository.
    let buttons: Vec<_> = h.get_all_by_label("Open in editor").collect();
    assert_eq!(buttons.len(), 3);
    for button in buttons {
        button.click();
    }
    h.run();
    let scratch = h.state().scratch.clone();
    assert_eq!(
        desktop.files.lock().unwrap().clone(),
        ["output-0.txt", "output-0-one.txt", "output-0-two.txt"].map(|f| scratch.join(f))
    );
}

#[test]
fn terminal_button_opens_a_terminal_in_the_repository() {
    let desktop = Arc::new(crate::ui::app::tests::FakeDesktop::default());
    let backend = Arc::new(FakeBackend::default());
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    let mut app = crate::ui::app::tests::app_on(backend.clone(), desktop.clone(), None);
    app.cards = vec![card("/r/one")];
    app.config.terminal = "foot".into();
    let mut h = harness(app);
    h.run();
    h.get_by_label(crate::ui::view::TERMINAL_LABEL).click();
    h.run();
    assert_eq!(
        desktop.terminals.lock().unwrap().clone(),
        vec![(PathBuf::from("/r/one"), "foot".to_owned())]
    );
    assert!(h.state().marked.is_empty(), "the button does not mark");

    // A folder that is gone gets no terminal.
    h.state_mut().cards[0].probe = Some(Err(crate::domain::repo::ProbeError::PathMissing));
    h.run();
    h.get_by_label(crate::ui::view::TERMINAL_LABEL).click();
    h.run();
    assert_eq!(desktop.terminals.lock().unwrap().len(), 1);
}

#[test]
fn right_click_on_the_branch_switches_this_repository() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = harness(marked_app(backend.clone(), &[]));
    h.run();
    h.get_by_label("main").click_secondary();
    h.run();
    h.get_by_label("Switch to");
    h.get_by_label("Create new branch");
    // Newest first: the snapshot lists main, feature-a, feature-b oldest first.
    let top = |h: &Harness<'static, App>, label: &str| h.get_by_label(label).rect().top();
    assert!(top(&h, "feature-b") < top(&h, "feature-a"));

    // A click into a field of the menu leaves it open.
    let mut fields: Vec<_> = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .collect();
    fields.sort_by(|a, b| a.top().total_cmp(&b.top()));
    assert_eq!(
        fields.len(),
        4,
        "board filter, menu filter, new branch, command line"
    );
    click_at(&mut h, fields[1].center());
    type_text(&mut h, "-a");
    h.get_by_label("Switch to");
    assert!(h.query_by_label("feature-b").is_none(), "filtered out");
    h.get_by_label("feature-a").click();
    h.run();
    assert!(h.query_by_label("Switch to").is_none(), "the menu closed");
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    let switched = (
        PathBuf::from("/repo"),
        vec!["switch".to_owned(), "feature-a".to_owned()],
    );
    assert_eq!(backend.runs.lock().unwrap().last(), Some(&switched));
    assert!(h.state().marked.contains(&PathBuf::from("/repo")));
}

#[test]
fn branch_menu_creates_a_new_branch() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = harness(marked_app(backend.clone(), &[]));
    h.run();
    h.get_by_label("main").click_secondary();
    h.run();
    let mut fields: Vec<_> = h
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .map(|n| n.rect())
        .collect();
    fields.sort_by(|a, b| a.top().total_cmp(&b.top()));
    click_at(&mut h, fields[2].center());
    type_text(&mut h, "fix/it");
    // The filter of the list is not what names the new branch.
    h.get_by_label("feature-a");
    h.get_by_label("Create").click();
    h.run();
    assert!(h.query_by_label("Switch to").is_none(), "the menu closed");
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    let created = (
        PathBuf::from("/repo"),
        vec!["switch".to_owned(), "-c".to_owned(), "fix/it".to_owned()],
    );
    assert_eq!(backend.runs.lock().unwrap().last(), Some(&created));

    // Opened again, the fields are empty.
    h.get_by_label("main").click_secondary();
    h.run();
    h.get_by_label("feature-b");
    h.get_by_label("Create").click();
    h.run();
    assert_eq!(
        backend.runs.lock().unwrap().last(),
        Some(&created),
        "a blank name creates nothing"
    );
}

/// Tags `old`, `v2`, `v1` made in that order on a card that shows `shown`.
fn tagged_card(backend: &Arc<FakeBackend>, shown: &[&str]) -> Harness<'static, App> {
    let mut snap = snapshot(Counts::default(), Upstream::None, &["main"]);
    snap.tags = shown.iter().map(|t| t.to_string()).collect();
    snap.dated.tags = ["old", "v2", "v1"]
        .iter()
        .zip(0..)
        .map(|(name, date)| crate::domain::repo::DatedRef {
            name: name.to_string(),
            commit: String::new(),
            date,
        })
        .collect();
    *backend.snapshot.lock().unwrap() = Some(snap.clone());
    let mut app = app_with(backend.clone(), None);
    app.cards = vec![card("/r/repo")];
    app.cards[0].probe = Some(Ok(snap));
    let mut h = harness(app);
    h.run();
    h
}

#[test]
fn right_click_on_the_tag_line_switches_to_a_tag() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = tagged_card(&backend, &["v2"]);
    h.get_by_label("v2").click_secondary();
    h.run();
    h.get_by_label("Switch to");
    assert!(h.query_by_label("Create new branch").is_none());
    // The tag made last is on top; the card's own v2 is the upper of the two.
    let top = |h: &Harness<'static, App>, label: &str| {
        h.get_all_by_label(label)
            .map(|n| n.rect().top())
            .fold(f32::MIN, f32::max)
    };
    assert!(top(&h, "v1") < top(&h, "v2"));
    assert!(top(&h, "v2") < top(&h, "old"));
    h.get_by_label("old").click();
    h.run();
    settle(&mut h, |a| a.cards[0].last_command == Some(true));
    let args = ["switch", "--detach", "old"].map(str::to_owned).to_vec();
    assert_eq!(
        backend.runs.lock().unwrap().last(),
        Some(&(PathBuf::from("/r/repo"), args))
    );
}

#[test]
fn no_tag_has_the_menu_too_and_a_click_on_it_still_marks_the_card() {
    let backend = Arc::new(FakeBackend::default());
    let mut h = tagged_card(&backend, &[]);
    h.get_by_label("no tag").click();
    h.run();
    assert!(h.state().marked.contains(&PathBuf::from("/r/repo")));
    h.get_by_label("no tag").click_secondary();
    h.run();
    h.get_by_label("Switch to");
    h.get_by_label("v1");

    // A repository without any tag has nothing to pick.
    let backend = Arc::new(FakeBackend::default());
    let mut h = tagged_card(&backend, &[]);
    h.state_mut().cards[0].probe = Some(Ok(snapshot(Counts::default(), Upstream::None, &["main"])));
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    h.run();
    h.get_by_label("no tag").click_secondary();
    h.run();
    h.get_by_label("no tags");
    assert!(
        backend
            .runs
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.1[0] != "switch")
    );
}

/// Whether one of egui's bundled fonts draws `c`. `Fonts::has_glyph` cannot
/// tell: it calls every glyph of the font that holds the replacement glyph
/// missing, and that font holds most emojis.
fn bundled_fonts_draw(c: char) -> bool {
    use epaint_default_fonts::{EMOJI_ICON, HACK_REGULAR, NOTO_EMOJI_REGULAR, UBUNTU_LIGHT};
    [UBUNTU_LIGHT, NOTO_EMOJI_REGULAR, EMOJI_ICON, HACK_REGULAR]
        .into_iter()
        .any(|font| {
            ttf_parser::Face::parse(font, 0)
                .unwrap()
                .glyph_index(c)
                .is_some()
        })
}

#[test]
fn every_favorite_emoji_has_a_glyph() {
    let missing: Vec<&str> = crate::domain::favorites::EMOJIS
        .into_iter()
        .filter(|e| !e.chars().all(bundled_fonts_draw))
        .collect();
    assert!(missing.is_empty(), "no glyph for {missing:?}");
    assert!(!bundled_fonts_draw('\u{10ffff}'));
}

/// The toolbar button of the favorite with this label: the topmost node
/// that carries it (the cards' chips carry it too).
fn favorite_button<'a>(h: &'a Harness<'static, App>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_all_by_label(label)
        .min_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
}

/// A chip of that favorite on a card: the lowest node with the label.
fn favorite_chip<'a>(h: &'a Harness<'static, App>, label: &'a str) -> egui_kittest::Node<'a> {
    h.get_all_by_label(label)
        .max_by(|a, b| a.rect().top().total_cmp(&b.rect().top()))
        .unwrap()
}

/// Three cards, the two `etl` ones marked.
fn favorites_app() -> Harness<'static, App> {
    let backend = Arc::new(FakeBackend::default());
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    let mut app = app_with(backend, None);
    app.cards = vec![card("/r/etl-load"), card("/r/etl-store"), card("/r/web")];
    app.marked.insert("/r/etl-load".into());
    app.marked.insert("/r/etl-store".into());
    let mut h = harness(app);
    h.run();
    h
}

#[test]
fn favorite_is_made_from_the_marked_cards_and_its_button_filters_the_board() {
    let mut h = favorites_app();
    // A right click on a marked card is about all marked cards.
    h.get_by_label("etl-load").click_secondary();
    h.run();
    h.get_by_label("2 repositories");
    h.get_by_label("New favorite\u{2026}").click();
    h.run();
    h.get_by_label("New favorite");
    h.get_by_label("with 2 repositories");
    h.ctx
        .memory_mut(|m| m.request_focus(crate::ui::view::favorite_name_id()));
    h.run();
    type_text(&mut h, "ETL");
    h.get_by_label("teal").click();
    h.run();
    let rocket = crate::domain::favorites::EMOJIS[3];
    h.get_by_label(rocket).click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert!(h.state().favorite_dialog.is_none());
    let favorite = h.state().config.favorites[0].clone();
    assert_eq!(favorite.name, "ETL");
    assert_eq!(favorite.color, "teal");
    assert_eq!(favorite.emoji.as_deref(), Some(rocket));
    assert_eq!(
        favorite.repos,
        vec![PathBuf::from("/r/etl-load"), PathBuf::from("/r/etl-store")]
    );
    // The favorite has its toolbar button, and both cards carry its chip;
    // the emoji stands nowhere on its own.
    let button = format!("{rocket} ETL");
    assert_eq!(h.get_all_by_label(&button).count(), 3);
    assert!(h.query_by_label(rocket).is_none());
    let chip_top = favorite_chip(&h, &button).rect().top();
    let name_bottom = h.get_by_label("etl-store").rect().bottom();
    assert!(chip_top >= name_bottom - 1.0, "the chip is under the name");
    favorite_button(&h, &button).click();
    h.run();
    assert_eq!(h.state().active_favorite, Some(0));
    assert!(h.query_by_label("web").is_none());
    h.get_by_label("etl-load");
    // A chip does what the toolbar button does.
    favorite_chip(&h, &button).click();
    h.run();
    assert_eq!(h.state().active_favorite, None);
    h.get_by_label("web");
    assert!(
        h.state().marked.contains(&PathBuf::from("/r/etl-store")),
        "no mark flipped"
    );

    // A right click on a chip is a right click on its card.
    favorite_chip(&h, &button).click_secondary();
    h.run();
    h.get_by_label("2 repositories");
    h.get_by_label(&format!("Remove from {button}"));
}

#[test]
fn right_click_menus_change_and_delete_a_favorite() {
    let mut h = favorites_app();
    h.state_mut().new_favorite(vec![]);
    h.state_mut().favorite_dialog.as_mut().unwrap().draft.name = "Front".into();
    h.state_mut().save_favorite();
    h.run();

    // An unmarked card: the menu is about that card alone.
    h.get_by_label("web").click_secondary();
    h.run();
    assert!(h.query_by_label("Remove from Front").is_none());
    h.get_by_label("Add to Front").click();
    h.run();
    assert_eq!(
        h.state().config.favorites[0].repos,
        vec![PathBuf::from("/r/web")]
    );
    h.get_by_label("web").click_secondary();
    h.run();
    h.get_by_label("Remove from Front").click();
    h.run();
    assert!(h.state().config.favorites[0].repos.is_empty());

    // The favorite's own menu takes the marked cards and edits it.
    favorite_button(&h, "Front").click_secondary();
    h.run();
    h.get_by_label("Add marked (2)").click();
    h.run();
    assert_eq!(h.state().config.favorites[0].repos.len(), 2);
    favorite_button(&h, "Front").click_secondary();
    h.run();
    h.get_by_label("Remove marked (2)").click();
    h.run();
    assert!(h.state().config.favorites[0].repos.is_empty());
    favorite_button(&h, "Front").click_secondary();
    h.run();
    h.get_by_label("Edit\u{2026}").click();
    h.run();
    h.get_by_label("Edit favorite");
    h.get_by_label("pink").click();
    h.run();
    h.get_by_label("Save").click();
    h.run();
    assert_eq!(h.state().config.favorites[0].color, "pink");

    favorite_button(&h, "Front").click_secondary();
    h.run();
    h.get_by_label("Delete").click();
    h.run();
    assert!(h.state().config.favorites.is_empty());
    assert!(
        h.query_by_label("Front").is_none(),
        "the favorites line is gone"
    );
    assert_eq!(h.state().cards.len(), 3, "the repositories stay");
}

#[test]
fn right_click_on_the_branch_opens_the_branch_menu_not_the_cards() {
    let mut h = favorites_app();
    h.get_all_by_label("main").next().unwrap().click_secondary();
    h.run();
    h.get_by_label("Switch to");
    assert!(h.query_by_label("New favorite\u{2026}").is_none());
}

/// One marked card whose repository view has two files, a commit and two
/// branches.
fn repo_view_app() -> Harness<'static, App> {
    use crate::domain::details::{BranchEntry, ChangeKind, Details, FileChange, parse_log};
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
                path: "src/old.rs".into(),
                kind: ChangeKind::Deleted,
            },
            FileChange {
                path: "notes.txt".into(),
                kind: ChangeKind::Added,
            },
        ],
        commits: parse_log(
            "aaaabbbb\taaaabbb\tAda Lovelace\t1700000000\t2023-11-14 22:13\t\tFix the thing\n",
        ),
        branches: vec![
            BranchEntry {
                name: "main".into(),
                current: true,
            },
            BranchEntry {
                name: "side-branch".into(),
                current: false,
            },
        ],
    });
    // main: tip, two plain commits, base; side-branch: one commit on base.
    use crate::domain::graph::{GraphCommit, GraphData, GraphRef, RefKind};
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
    // Frames close enough together for two clicks to be a double click.
    let mut h = harness_stepping(marked_app(backend, &[]), 0.05);
    h.run();
    h
}

#[test]
fn double_click_opens_the_repository_view_and_leaves_the_mark() {
    let mut h = repo_view_app();
    let space = card_space(&h);
    h.event(egui::Event::PointerMoved(space));
    // Two clicks, one frame apart: within the time of a double click.
    for click in 0..2 {
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos: space,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
        }
        h.step();
        let marked = h.state().marked.contains(&PathBuf::from("/repo"));
        assert_eq!(marked, click == 1, "each click flips the mark");
    }
    h.run();
    assert_eq!(
        h.state().repo_view.as_ref().unwrap().repo,
        PathBuf::from("/repo")
    );
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().details.is_some());
    h.run();
    // A clean repository opens on its history.
    use crate::domain::details::RepoTab;
    let tab = |h: &Harness<'static, App>| h.state().repo_view.as_ref().unwrap().tab;
    assert_eq!(tab(&h), RepoTab::Commits);
    h.get_by_label("Pick a commit to see it.");
    assert!(h.query_by_label("Changed files (2)").is_none());
    // A row: initials, short hash, title; the age stands at the right.
    h.get_by_label("AL aaaabbb Fix the thing").click();
    h.run();
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().diff.is_some());
    h.run();
    // The header of the commit: hash, names on it, author, date, message.
    h.get_by_label("aaaabbbb");
    h.get_by_label(" main ");
    h.get_by_label(" v9 ");
    h.get_by_label("Ada Lovelace <ada@example.org>");
    h.get_by_label("2026-11-12 20:03");
    h.get_by_label("Fix the thing\n\nBecause it was broken.");
    // Its files, folded; a click unfolds the diff of one in place.
    h.get_by_label("2 files changed");
    h.get_by_label("A docs/new.md");
    assert!(h.query_by_label("+after in src/main.rs").is_none());
    h.get_by_label("M src/main.rs").click();
    h.run();
    settle(&mut h, |a| {
        let view = a.repo_view.as_ref().unwrap();
        matches!(view.unfolded.get("src/main.rs"), Some(Some(_)))
    });
    h.run();
    h.get_by_label("+after in src/main.rs");
    h.get_by_label("-before");
    h.get_by_label("A docs/new.md");
    h.get_by_label("Ada Lovelace <ada@example.org>");
    h.get_by_label("M src/main.rs").click();
    h.run();
    assert!(h.query_by_label("+after in src/main.rs").is_none());

    // The Files tab has the changed files; a file shows its diff.
    h.get_by_label("Files").click();
    h.run();
    assert_eq!(tab(&h), RepoTab::Files);
    h.get_by_label("Changed files (4)");
    h.get_by_label("Pick a file to see its changes.");
    assert!(h.query_by_label("AL aaaabbb Fix the thing").is_none());
    // A tree: folders first, a chain of single folders as one row, every
    // file with the letter of its change.
    h.get_by_label("src/");
    h.get_by_label("ui/deep/");
    h.get_by_label("A new.rs");
    h.get_by_label("D old.rs");
    h.get_by_label("A notes.txt");
    // A folder folds away what is in it, and unfolds again.
    h.get_by_label("ui/deep/").click();
    h.run();
    assert!(h.query_by_label("A new.rs").is_none());
    h.get_by_label("M main.rs");
    h.get_by_label("src/").click();
    h.run();
    assert!(h.query_by_label("M main.rs").is_none());
    assert!(h.query_by_label("ui/deep/").is_none());
    h.get_by_label("A notes.txt");
    h.get_by_label("src/").click();
    h.run();
    h.get_by_label("ui/deep/").click();
    h.run();
    let pick = |h: &mut Harness<'static, App>, label: &str| {
        h.get_by_label(label).click();
        h.run();
        settle(h, |a| a.repo_view.as_ref().unwrap().diff.is_some());
        h.run();
    };
    // An added file shows its content, a deleted one what is gone, a
    // modified one its diff.
    pick(&mut h, "A new.rs");
    h.get_by_label("added");
    h.get_by_label("src/ui/deep/new.rs");
    h.get_by_label("fresh line");
    assert!(h.query_by_label("+fresh line").is_none());
    pick(&mut h, "D old.rs");
    h.get_by_label("deleted");
    h.get_by_label("gone line");
    pick(&mut h, "M main.rs");
    h.get_by_label("modified");
    h.get_by_label("+++ b/src/main.rs");
    h.get_by_label("+new");

    // The Graph tab has the branches.
    h.get_by_label("Graph").click();
    h.run();
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().graph.is_some());
    h.run();
    h.get_by_label(" side-branch ");

    // Back goes to the file, on through the files picked before it to the
    // Files tab without a pick, then to the commit; Forward returns.
    h.get_by_label("Back").click();
    h.run();
    assert_eq!(tab(&h), RepoTab::Files);
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().diff.is_some());
    h.run();
    h.get_by_label("+new");
    for _ in 0..3 {
        h.get_by_label("Back").click();
        h.run();
    }
    h.get_by_label("Pick a file to see its changes.");
    h.get_by_label("Back").click();
    h.run();
    assert_eq!(tab(&h), RepoTab::Commits);
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().diff.is_some());
    h.run();
    h.get_by_label("2 files changed");
    h.get_by_label("Forward").click();
    h.run();
    assert_eq!(tab(&h), RepoTab::Files);

    // The board is out of reach while the view is open; Esc returns to it.
    key(&mut h, Key::Escape);
    assert!(h.state().repo_view.is_none());
    assert!(h.query_by_label("Commits").is_none());
}

#[test]
fn open_in_the_card_menu_and_the_buttons_of_the_view() {
    let mut h = repo_view_app();
    h.get_by_label("repo").click_secondary();
    h.run();
    h.get_by_label("Open").click();
    h.run();
    settle(&mut h, |a| {
        a.repo_view.as_ref().is_some_and(|v| v.details.is_some())
    });
    h.run();
    h.get_by_label("Files").click();
    h.run();
    h.get_by_label("A notes.txt");
    h.get_by_label("Refresh").click();
    h.run();
    assert!(h.state().repo_view.is_some());
    h.get_by_label("Close").click();
    h.run();
    assert!(h.state().repo_view.is_none());
}

#[test]
fn the_graph_tab_draws_named_commits_folds_plain_runs_and_picks_a_commit() {
    let mut h = repo_view_app();
    h.state_mut().open_repo_view(std::path::Path::new("/repo"));
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().details.is_some());
    h.run();
    h.get_by_label("Graph").click();
    h.run();
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().graph.is_some());
    h.run();
    h.get_by_label("All branches");
    // The graph has most of the width: the right pane starts past the middle.
    let pane = h.get_by_label("Pick a commit to see it.").rect();
    assert!(pane.left() > 650.0 && pane.left() < 800.0, "{pane:?}");
    // A local branch and its remote branch on one commit share a label.
    h.get_by_label(" main = origin ");
    assert!(h.query_by_label(" origin/main ").is_none());
    h.get_by_label(" side-branch ");
    h.get_by_label(" v1 ");
    h.get_by_label("Tip of main");
    h.get_by_label("Work on the side");
    h.get_by_label("Where they part");
    // The two plain commits between the tip and the fork are one row.
    assert!(h.query_by_label("Plain one").is_none());
    h.get_by_label("2 commits").click();
    h.run();
    h.get_by_label("Plain one");
    h.get_by_label("Plain two");
    assert!(h.query_by_label("2 commits").is_none());

    // A commit of the graph shows on the right like one of the list.
    h.get_by_label("Work on the side").click();
    h.run();
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().diff.is_some());
    h.run();
    h.get_by_label("2 files changed");
    h.get_by_label("Ada Lovelace <ada@example.org>");

    // Old remote branches are left out behind a line that shows them.
    assert!(h.query_by_label(" origin/ancient ").is_none());
    h.get_by_label("2 old remote branches hidden").click();
    h.run();
    settle(&mut h, |a| {
        let view = a.repo_view.as_ref().unwrap();
        matches!(&view.graph, Some(Ok(graph)) if graph.refs.len() == 5)
    });
    h.run();
    h.get_by_label(" origin/ancient ");
    assert!(h.query_by_label("2 old remote branches hidden").is_none());
    // The pick is still there, and Back leaves the graph.
    h.get_by_label("2 files changed");
    h.get_by_label("Back").click();
    h.run();
    h.get_by_label("Back").click();
    h.run();
    assert!(h.query_by_label("All branches").is_none());
}

#[test]
fn the_commit_list_reads_its_next_page_when_its_end_comes_into_view() {
    use crate::domain::details::Details;
    let backend = Arc::new(FakeBackend::default());
    *backend.details.lock().unwrap() = Some(Details::default());
    // A history of 130 commits: the first page holds 100.
    *backend.log_length.lock().unwrap() = 130;
    let mut h = harness(marked_app(backend, &[]));
    h.run();
    let commits = |h: &Harness<'static, App>| {
        let view = h.state().repo_view.as_ref().unwrap();
        view.details
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .commits
            .len()
    };
    h.state_mut().open_repo_view(std::path::Path::new("/repo"));
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().details.is_some());
    h.run();
    assert_eq!(commits(&h), 100);
    assert!(h.state().repo_view.as_ref().unwrap().more_commits);
    // The list is the narrow side.
    let pane = h.get_by_label("Pick a commit to see it.").rect();
    assert!(pane.left() < 500.0, "{pane:?}");
    // A tagged commit carries the name of its tag before its title.
    h.get_by_label("AL 0000007  v7  Commit number 7");
    // Every row is one compact line, the first one too.
    let first = h.get_by_label("AL 0000000 Commit number 0").rect();
    let second = h.get_by_label("AL 0000001 Commit number 1").rect();
    assert!(first.height() < 30.0, "{first:?}");
    let step = second.top() - first.top();
    assert!(step > 0.0 && step < 30.0, "{first:?} {second:?}");
    // The end of the list is far below: nothing more is read.
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(commits(&h), 100);
    // Scrolled to its end, the list reads the rest, once.
    let list = h.get_by_label("AL 0000000 Commit number 0").rect().center();
    h.event(egui::Event::PointerMoved(list));
    for _ in 0..40 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, -40.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        });
        h.step();
    }
    // Off the list again: a pointer resting on a row shows its tooltip.
    h.event(egui::Event::PointerGone);
    settle(&mut h, |a| {
        let view = a.repo_view.as_ref().unwrap();
        view.details
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .commits
            .len()
            == 130
    });
    h.run();
    assert!(!h.state().repo_view.as_ref().unwrap().more_commits);
    assert!(h.query_by_label_contains("more commits are read").is_none());
    // A reload keeps the list as long as it was.
    h.state_mut().refresh_repo_view();
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(5));
        h.step();
    }
    assert_eq!(commits(&h), 130);
}

#[test]
fn repository_view_shows_why_a_repository_cannot_be_read() {
    let mut h = harness(marked_app(Arc::default(), &[]));
    h.run();
    h.state_mut().open_repo_view(std::path::Path::new("/repo"));
    settle(&mut h, |a| a.repo_view.as_ref().unwrap().details.is_some());
    h.run();
    h.get_by_label("not a git repository");
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
fn about_page_opens_from_settings_and_its_rows_open_links() {
    use crate::domain::about;
    let desktop = Arc::new(crate::ui::app::tests::FakeDesktop::default());
    let mut app = crate::ui::app::tests::app_on(Arc::default(), desktop.clone(), None);
    app.settings_open = true;
    let mut h = harness(app);
    h.run();
    h.get_by_label("About multigit").click();
    h.run();
    h.get_by_label("multigit");
    h.get_by_label(&format!("Version {}", about::VERSION));
    h.get_by_label(about::TAGLINE);
    assert!(h.state().icon.is_some(), "the icon was loaded");
    for link in about::LINKS {
        h.get_by_label(link.subtitle);
        h.get_by_label(link.title).click();
        h.run();
    }
    assert_eq!(
        desktop.urls.lock().unwrap().clone(),
        about::LINKS.map(|l| l.url.to_owned())
    );

    h.get_by_label("Open-source licenses").click();
    h.run();
    h.get_by_label_contains("ab_glyph ");
    h.get_by_label("Back").click();
    h.run();
    h.get_by_label(about::TAGLINE);
    h.get_by_label("Back").click();
    h.run();
    h.get_by_label("Card buttons");

    // Closed on the About page, the window opens with the settings again.
    h.get_by_label("About multigit").click();
    h.run();
    h.state_mut().settings_open = false;
    h.run();
    h.state_mut().settings_open = true;
    h.run();
    h.get_by_label("Card buttons");
}

fn repaint_delay(h: &Harness<'static, App>) -> Duration {
    h.output().viewport_output[&egui::ViewportId::ROOT].repaint_delay
}

#[test]
fn kawaii_theme_animates_with_focus_and_stands_still_without() {
    let backend = Arc::new(FakeBackend::default());
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    let mut app = app_with(backend, None);
    app.cards = (0..6).map(|i| card(&format!("/r/repo{i}"))).collect();
    app.settings_open = true;
    let mut h = harness(app);
    // Until the first probes are in and the window has faded in.
    h.run();
    settle(&mut h, |a| {
        a.cards.iter().all(|c| c.snapshot().is_some()) && !a.vocab.subcommands.is_empty()
    });
    h.run();
    h.step();
    // Plain themes ask for a frame only for their timers.
    assert!(repaint_delay(&h) >= Duration::from_millis(200));
    assert_eq!(h.state().kawaii_clock, 0.0);

    h.get_by_label("Kawaii").click();
    // The click, the frame that sets style and fonts, the frames that use them.
    for _ in 0..4 {
        h.step();
    }
    assert!(h.state().is_kawaii());
    let style = h.ctx.global_style();
    assert!(!style.visuals.dark_mode);
    assert!(
        style.visuals.widgets.inactive.corner_radius.nw >= 12,
        "round"
    );
    assert!(
        style.visuals.widgets.hovered.expansion > 0.0,
        "grows on hover"
    );
    assert!(
        h.ctx
            .fonts(|f| f.definitions().font_data.contains_key("Fredoka"))
    );
    h.state_mut().settings_open = false;
    h.step();

    // With focus: about 30 frames a second, the clock runs, cards move.
    let top = |h: &Harness<'static, App>| h.get_by_label("repo1").rect().top();
    let (clock, y) = (h.state().kawaii_clock, top(&h));
    h.step();
    h.step();
    assert!(repaint_delay(&h) <= Duration::from_millis(34));
    assert!(h.state().kawaii_clock > clock);
    assert!((top(&h) - y).abs() > 0.01, "the card bobbed");
    // The click target stays under a card that moves.
    // Empty space right of the name; `click_at` would wait for a still picture.
    let space = h.get_by_label("repo3").rect().right_center() + egui::vec2(40.0, 0.0);
    h.event(egui::Event::PointerMoved(space));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: space,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.step();
    h.step();
    assert!(h.state().marked.contains(&PathBuf::from("/r/repo3")));

    // Without focus: no frame asked for, a frozen clock, a frozen picture.
    h.input_mut().focused = false;
    // Let what the click started (the swing's last frames) run out.
    h.run();
    let (clock, y) = (h.state().kawaii_clock, top(&h));
    for _ in 0..4 {
        h.step();
        assert!(repaint_delay(&h) >= Duration::from_millis(200));
    }
    assert_eq!(h.state().kawaii_clock, clock);
    assert_eq!(top(&h), y);

    // Back in a plain theme the look and the fonts are egui's again.
    h.input_mut().focused = true;
    h.state_mut().config.theme = crate::domain::config::Theme::Light;
    h.run();
    h.step();
    assert!(repaint_delay(&h) >= Duration::from_millis(200));
    assert!(
        !h.ctx
            .fonts(|f| f.definitions().font_data.contains_key("Fredoka"))
    );
    assert_eq!(
        h.ctx.global_style().visuals.widgets,
        egui::Visuals::light().widgets
    );
    assert_eq!(
        h.ctx.global_style().spacing.button_padding,
        crate::ui::style::BUTTON_PADDING
    );
}

#[test]
fn bunnies_and_cats_sit_on_the_cards_and_a_click_pets_one_without_marking() {
    use crate::domain::critters::{Mood, Species};
    let backend = Arc::new(FakeBackend::default());
    *backend.snapshot.lock().unwrap() =
        Some(snapshot(Counts::default(), Upstream::None, &["main"]));
    let mut app = app_with(backend, None);
    app.cards = (0..6).map(|i| card(&format!("/r/repo{i}"))).collect();
    let mut h = harness(app);
    h.run();
    settle(&mut h, |a| a.cards.iter().all(|c| c.snapshot().is_some()));
    h.run();
    // No critters in a plain theme.
    assert!(h.query_by_label("bunny").is_none());
    assert!(h.state().critters.critters.is_empty());

    h.state_mut().config.theme = crate::domain::config::Theme::Kawaii;
    for _ in 0..4 {
        h.step();
    }
    // Six cards: the fewest, a bunny and a cat, each on a card's top edge.
    let species: Vec<_> = h
        .state()
        .critters
        .critters
        .iter()
        .map(|c| c.species)
        .collect();
    assert_eq!(species, [Species::Bunny, Species::Cat]);
    let bunny = h.get_by_label("bunny").rect();
    let home = h.state().critters.critters[0].home.clone();
    let name = home.file_name().unwrap().to_string_lossy().into_owned();
    let card_name = h.get_by_label(name.as_str()).rect();
    assert!(
        bunny.bottom() < card_name.top(),
        "above what the card shows"
    );
    let gap = card_name.top() - bunny.bottom();
    assert!(gap < 60.0, "and on that card: {gap} above its name");
    let aside = bunny.center().x - card_name.left();
    assert!((0.0..300.0).contains(&aside), "{aside}");
    h.get_by_label("cat");

    // A click on the bunny pets it; the card under it keeps its mark off.
    let at = bunny.center();
    h.event(egui::Event::PointerMoved(at));
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.step();
    h.step();
    assert_eq!(h.state().critters.critters[0].mood, Mood::Pet);
    assert!(h.state().marked.is_empty(), "{:?}", h.state().marked);
    // Confetti out of a card flies for a good second and is gone.
    let clock = h.state().kawaii_clock;
    h.state_mut().bursts.push((home.clone(), clock));
    h.step();
    assert_eq!(h.state().bursts.len(), 1);
    for _ in 0..10 {
        h.step();
    }
    assert!(h.state().bursts.is_empty());
    // Without focus and with nothing happening the picture stands still,
    // critters and all.
    h.input_mut().focused = false;
    for _ in 0..200 {
        h.step();
    }
    assert!(repaint_delay(&h) >= Duration::from_millis(200));

    // Back in a plain theme they are gone.
    h.state_mut().config.theme = crate::domain::config::Theme::Light;
    h.input_mut().focused = true;
    h.run();
    assert!(h.query_by_label("bunny").is_none());
    assert!(h.state().critters.critters.is_empty());
}

#[test]
fn kawaii_glyphs_and_font_are_there() {
    for glyph in [crate::ui::view::HEART, crate::ui::view::STAR] {
        assert!(glyph.chars().all(bundled_fonts_draw), "{glyph}");
    }
    let font = include_bytes!("../../assets/fonts/Fredoka.ttf");
    let face = ttf_parser::Face::parse(font, 0).unwrap();
    assert!(face.is_variable());
    for c in ('a'..='z').chain('A'..='Z').chain('0'..='9') {
        assert!(face.glyph_index(c).is_some(), "{c}");
    }
    let license = include_str!("../../assets/fonts/Fredoka-OFL.txt");
    assert!(license.contains("SIL Open Font License"));
}

#[test]
fn kawaii_music_setting_starts_and_stops_the_music() {
    let desktop = Arc::new(crate::ui::app::tests::FakeDesktop::default());
    let mut app = crate::ui::app::tests::app_on(Arc::default(), desktop.clone(), None);
    app.settings_open = true;
    app.config.theme = crate::domain::config::Theme::Kawaii;
    let mut h = harness(app);
    h.step();
    h.step();
    assert!(!h.state().music_plays(), "off until switched on");
    h.get_by_label("Kawaii music").click();
    h.step();
    h.step();
    assert!(h.state().config.music && h.state().music_plays());
    h.get_by_label("Kawaii music").click();
    h.step();
    h.step();
    assert!(!h.state().music_plays());
}

#[test]
fn sound_settings_switch_play_and_reset() {
    let desktop = Arc::new(crate::ui::app::tests::FakeDesktop::default());
    let mut app = crate::ui::app::tests::app_on(Arc::default(), desktop.clone(), None);
    app.settings_open = true;
    app.config.boop_file = Some("/sounds/My Boop.ogg".into());
    let mut h = harness(app);
    h.run();
    assert!(!h.state().config.sounds);
    h.get_by_label("Sounds").click();
    h.run();
    assert!(h.state().config.sounds);
    h.get_by_label("built-in tone");
    h.get_by_label("My Boop.ogg");

    // Play works whether sounds are on or off: it is for trying them out.
    let plays: Vec<_> = h.get_all_by_label("Play").collect();
    assert_eq!(plays.len(), 2);
    plays[0].click();
    h.run();
    let played = desktop.played.lock().unwrap().len();
    assert_eq!(played, 1);

    // "Default" takes the user's file away; the beep has none to take.
    let defaults: Vec<_> = h.get_all_by_label("Default").collect();
    defaults[1].click();
    h.run();
    assert_eq!(h.state().config.boop_file, None);
    assert_eq!(h.get_all_by_label("built-in tone").count(), 2);
    assert_eq!(h.get_all_by_label("Choose file\u{2026}").count(), 2);
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
    for c in ['+', '~', '?', '!', '@', '…', crate::ui::view::OUTPUT_MARK] {
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
