//! Board state and the actions the UI triggers on it (no drawing here).

use crate::application::engine::{Backend, Engine, Event, Notify};
use crate::application::schedule::{Due, Schedule};
use crate::domain::command::{Job, shell_line};
use crate::domain::completion::{RepoValues, Vocabulary, strip_git_prefix};
use crate::domain::config::Config;
use crate::domain::destructive::is_destructive;
use crate::domain::history::History;
use crate::domain::naming::board_order;
use crate::domain::output::OutputLog;
use crate::domain::repo::{FetchError, ProbeError, Snapshot};
use crate::domain::status::{RepoState, Upstream};
use crate::infrastructure::scan::scan;
use crate::infrastructure::store::{self, Paths};
use crate::ui::cli::CliState;
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// How long a toast stays on screen.
const TOAST_DURATION: Duration = Duration::from_secs(4);

/// One repository on the board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub path: PathBuf,
    /// `None` until the first probe returns.
    pub probe: Option<Result<Snapshot, ProbeError>>,
    /// The last failed background fetch; replaced by every fetch result and
    /// taken off by a click on it.
    pub fetch_error: Option<FetchError>,
    /// Outcome of the last command run on this card.
    pub last_command: Option<bool>,
}

impl Card {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            probe: None,
            fetch_error: None,
            last_command: None,
        }
    }

    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.probe.as_ref().and_then(|p| p.as_ref().ok())
    }

    /// Has changes or an operation in progress.
    pub fn is_dirty(&self) -> bool {
        self.snapshot()
            .is_some_and(|s| !s.status.counts.is_clean() || s.state != RepoState::Normal)
    }

    /// Has unpulled commits.
    pub fn is_behind(&self) -> bool {
        self.snapshot().is_some_and(
            |s| matches!(s.status.upstream, Upstream::Tracking { behind, .. } if behind > 0),
        )
    }
}

/// A command waiting for confirmation: a destructive git command on several
/// repositories, or any shell line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRun {
    /// As typed (without a leading `git`); a shell line keeps its `!`.
    pub command: String,
    pub job: Job,
    pub repos: Vec<PathBuf>,
}

/// The "Copied" animation after a click on a card text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CopyFlash {
    /// Pointer position of the click.
    pub at: egui::Pos2,
    /// `egui` input time when the copy happened, in seconds.
    pub started: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub text: String,
    pub until: Instant,
}

/// The whole application state.
pub struct App {
    pub config: Config,
    saved_config: Config,
    /// Set when the config file could not be read; saving is then disabled so a
    /// hand-edited file is never overwritten.
    pub config_error: Option<String>,
    paths: Option<Paths>,
    pub cards: Vec<Card>,
    pub marked: BTreeSet<PathBuf>,
    engine: Engine,
    events: Receiver<Event>,
    schedule: Schedule,
    applied_concurrency: usize,
    pub log: OutputLog,
    pub history: History,
    pub cli: CliState,
    pub vocab: Vocabulary,
    builtins: HashSet<String>,
    options_requested: HashSet<String>,
    commands_requested: bool,
    pub toasts: Vec<Toast>,
    pub copy_flash: Option<CopyFlash>,
    pub confirm: Option<PendingRun>,
    pub settings_open: bool,
    notify: Notify,
    /// Folder dialog running on its own thread; yields the picked folder.
    picker: Option<Receiver<Option<PathBuf>>>,
}

impl App {
    /// `config` is the result of loading the config file.
    pub fn new(
        config: Result<Config, String>,
        paths: Option<Paths>,
        history: History,
        backend: Arc<dyn Backend>,
        notify: Notify,
    ) -> Self {
        let (config, config_error) = match config {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e)),
        };
        let (engine, events) = Engine::new(backend, config.effective_concurrency(), notify.clone());
        let mut cards: Vec<Card> = config.repos.iter().cloned().map(Card::new).collect();
        cards.sort_by(|a, b| board_order(&a.path, &b.path));
        Self {
            applied_concurrency: config.effective_concurrency(),
            saved_config: config.clone(),
            config,
            config_error,
            paths,
            cards,
            marked: BTreeSet::new(),
            engine,
            events,
            schedule: Schedule::default(),
            log: OutputLog::default(),
            history,
            cli: CliState::default(),
            vocab: Vocabulary::default(),
            builtins: HashSet::new(),
            options_requested: HashSet::new(),
            commands_requested: false,
            toasts: Vec::new(),
            copy_flash: None,
            confirm: None,
            settings_open: false,
            notify,
            picker: None,
        }
    }

    /// Run a (blocking) folder dialog on its own thread so the board keeps
    /// refreshing. Ignored while a dialog is already open.
    pub fn open_folder_picker<F>(&mut self, pick: F)
    where
        F: FnOnce() -> Option<PathBuf> + Send + 'static,
    {
        if self.picker.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let notify = self.notify.clone();
        std::thread::spawn(move || {
            let _ = tx.send(pick());
            notify();
        });
        self.picker = Some(rx);
    }

    /// The folder picked in the dialog, once it closed with a choice.
    pub fn take_picked_folder(&mut self) -> Option<PathBuf> {
        let result = self.picker.as_ref()?.try_recv();
        match result {
            Ok(folder) => {
                self.picker = None;
                folder
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.picker = None;
                None
            }
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            until: Instant::now() + TOAST_DURATION,
        });
    }

    fn sync_repos_to_config(&mut self) {
        self.config.repos = self.cards.iter().map(|c| c.path.clone()).collect();
    }

    /// (+): add a repository, or every repository found below a folder.
    pub fn add_folder(&mut self, folder: &Path) {
        let found = scan(folder, self.config.scan_depth);
        if found.is_empty() {
            self.toast(format!("No git repository found in {}", folder.display()));
            return;
        }
        let mut duplicates = 0;
        for path in found {
            if self.cards.iter().any(|c| c.path == path) {
                duplicates += 1;
                continue;
            }
            self.engine.poll(&path);
            self.cards.push(Card::new(path));
        }
        self.cards.sort_by(|a, b| board_order(&a.path, &b.path));
        self.sync_repos_to_config();
        if duplicates > 0 {
            self.toast(format!("{duplicates} already on board"));
        }
    }

    /// ×: take a card off the board (nothing on disk changes).
    pub fn remove(&mut self, path: &Path) {
        self.cards.retain(|c| c.path != path);
        self.marked.remove(path);
        self.sync_repos_to_config();
    }

    pub fn remove_marked(&mut self) {
        let marked = std::mem::take(&mut self.marked);
        self.cards.retain(|c| !marked.contains(&c.path));
        self.sync_repos_to_config();
    }

    /// A click on a card's fetch error: its whole output for the clipboard;
    /// the error leaves the card until a fetch fails again.
    pub fn take_fetch_error(&mut self, path: &Path) -> Option<String> {
        let error = self.card_mut(path)?.fetch_error.take()?;
        Some(error.output)
    }

    fn mark_where(&mut self, keep: impl Fn(&Card) -> bool) {
        self.marked = self
            .cards
            .iter()
            .filter(|c| keep(c))
            .map(|c| c.path.clone())
            .collect();
    }

    pub fn mark_all(&mut self) {
        self.mark_where(|_| true);
    }
    pub fn mark_none(&mut self) {
        self.marked.clear();
    }
    pub fn mark_dirty(&mut self) {
        self.mark_where(Card::is_dirty);
    }
    pub fn mark_behind(&mut self) {
        self.mark_where(Card::is_behind);
    }

    /// Marked repositories in board order.
    pub fn marked_repos(&self) -> Vec<PathBuf> {
        self.cards
            .iter()
            .filter(|c| self.marked.contains(&c.path))
            .map(|c| c.path.clone())
            .collect()
    }

    pub fn is_busy(&self, repo: &Path) -> bool {
        self.engine.is_busy(repo)
    }

    /// Run `git <command>` in `repos`, or the shell line after a leading `!`.
    /// Shell lines always wait for confirmation, destructive git commands when
    /// they run in more than one repo.
    pub fn start_run(&mut self, command: &str, repos: Vec<PathBuf>) {
        let command = strip_git_prefix(command.trim()).trim();
        if repos.is_empty() || command.is_empty() {
            return;
        }
        if let Some(line) = shell_line(command) {
            if !line.is_empty() {
                self.confirm = Some(PendingRun {
                    command: command.to_owned(),
                    job: Job::Shell(line.to_owned()),
                    repos,
                });
            }
            return;
        }
        let args = match shell_words::split(command) {
            Ok(args) => args,
            Err(e) => {
                self.toast(format!("Cannot parse command: {e}"));
                return;
            }
        };
        let destructive = repos.len() > 1 && is_destructive(&args);
        let pending = PendingRun {
            command: command.to_owned(),
            job: Job::Git(args),
            repos,
        };
        if destructive {
            self.confirm = Some(pending);
        } else {
            self.execute(pending);
        }
    }

    fn execute(&mut self, run: PendingRun) {
        let id = self.log.start(&run.command, &run.repos);
        for repo in &run.repos {
            self.engine.run_command(id, repo, run.job.clone());
        }
    }

    pub fn confirm_run(&mut self) {
        if let Some(run) = self.confirm.take() {
            self.execute(run);
        }
    }

    pub fn cancel_run(&mut self) {
        self.confirm = None;
    }

    /// Enter in the CLI: record history, run in all marked repos.
    pub fn run_cli_line(&mut self, line: &str) {
        let repos = self.marked_repos();
        let line = strip_git_prefix(line.trim());
        if line.is_empty() || shell_line(line) == Some("") || repos.is_empty() {
            return;
        }
        self.history.push(line);
        if let Some(paths) = &self.paths
            && let Err(e) = store::save_history(&paths.history_file, &self.history)
        {
            let message = e.to_string();
            self.toast(format!("Cannot save history: {message}"));
        }
        self.start_run(line, repos);
    }

    /// Card button `index` on one card.
    pub fn run_button(&mut self, index: usize, repo: &Path) {
        if let Some(button) = self.config.buttons.get(index).cloned() {
            self.start_run(&button.args, vec![repo.to_path_buf()]);
        }
    }

    /// Toolbar button `index` on all marked cards.
    pub fn run_button_on_marked(&mut self, index: usize) {
        if let Some(button) = self.config.buttons.get(index).cloned() {
            let repos = self.marked_repos();
            self.start_run(&button.args, repos);
        }
    }

    /// Apply all worker events.
    pub fn handle_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.handle_event(event);
        }
    }

    fn card_mut(&mut self, repo: &Path) -> Option<&mut Card> {
        self.cards.iter_mut().find(|c| c.path == repo)
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Probed { repo, result } => {
                if let Some(card) = self.card_mut(&repo) {
                    card.probe = Some(result);
                }
            }
            Event::Fetched { repo, error } => {
                if let Some(card) = self.card_mut(&repo) {
                    card.fetch_error = error;
                }
            }
            Event::Section { run, repo, state } => {
                if let Some(outcome) = state.outcome()
                    && let Some(card) = self.card_mut(&repo)
                {
                    card.last_command = Some(outcome);
                }
                self.log.update(run, &repo, state);
            }
            Event::Commands { all, builtins } => {
                self.vocab.subcommands = all;
                self.builtins = builtins.into_iter().collect();
            }
            Event::Options { command, options } => {
                self.vocab.options.insert(command, options);
            }
        }
    }

    fn any_dir(&self) -> PathBuf {
        self.cards
            .first()
            .map(|c| c.path.clone())
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    /// Completion asks for git's option list of `command`.
    pub fn request_options(&mut self, command: &str) {
        if self.vocab.options.contains_key(command) || self.options_requested.contains(command) {
            return;
        }
        if !self.commands_requested || self.vocab.subcommands.is_empty() {
            // Builtins not known yet; asked again on the next keystroke.
            return;
        }
        if self.builtins.contains(command) {
            self.options_requested.insert(command.to_owned());
            let dir = self
                .marked_repos()
                .into_iter()
                .next()
                .unwrap_or_else(|| self.any_dir());
            self.engine.load_options(dir, command.to_owned());
        } else {
            // Scripts and aliases do not support --git-completion-helper.
            self.vocab.options.insert(command.to_owned(), Vec::new());
        }
    }

    /// Union of the refs of all marked repos, for completion values.
    pub fn repo_values(&self) -> RepoValues {
        RepoValues::union(
            self.cards
                .iter()
                .filter(|c| self.marked.contains(&c.path))
                .filter_map(|c| c.snapshot().map(|s| &s.refs)),
        )
    }

    /// Periodic work: timers, config changes, expired toasts.
    /// Returns how long until the next timer fires.
    pub fn tick(&mut self, now: Instant) -> Duration {
        if !self.commands_requested {
            self.commands_requested = true;
            self.engine.load_commands(self.any_dir());
        }
        let fetch_interval = Duration::from_secs(self.config.effective_fetch_interval_secs());
        let due = self.schedule.tick(now, fetch_interval);
        if due != Due::Nothing {
            let repos: Vec<PathBuf> = self.cards.iter().map(|c| c.path.clone()).collect();
            for repo in repos {
                match due {
                    Due::Fetch => self.engine.fetch(&repo),
                    _ => self.engine.poll(&repo),
                };
            }
        }
        let concurrency = self.config.effective_concurrency();
        if concurrency != self.applied_concurrency {
            self.applied_concurrency = concurrency;
            self.engine.set_concurrency(concurrency);
        }
        self.persist();
        self.toasts.retain(|t| t.until > now);
        self.schedule.until_next(now, fetch_interval)
    }

    /// Write the config file when it changed (never after a load error).
    fn persist(&mut self) {
        if self.config == self.saved_config || self.config_error.is_some() {
            return;
        }
        self.saved_config = self.config.clone();
        let Some(paths) = &self.paths else {
            return;
        };
        if let Err(e) = store::save_config(&paths.config_file, &self.config) {
            let message = e.to_string();
            self.toast(format!("Cannot save config: {message}"));
        }
    }

    pub fn config_file(&self) -> Option<&Path> {
        self.paths.as_ref().map(|p| p.config_file.as_path())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::application::engine::FETCH_ARGS;
    use crate::domain::completion::RepoRefs;
    use crate::domain::output::{ProcessOutput, SectionState};
    use crate::domain::status::{Counts, Head, Status};
    use std::io;
    use std::sync::Mutex;

    /// Backend that records commands and returns a fixed snapshot per repo.
    #[derive(Default)]
    pub struct FakeBackend {
        pub runs: Mutex<Vec<(PathBuf, Vec<String>)>>,
        pub shell_runs: Mutex<Vec<(PathBuf, String)>>,
        pub snapshot: Mutex<Option<Snapshot>>,
    }

    pub fn snapshot(counts: Counts, upstream: Upstream, branches: &[&str]) -> Snapshot {
        Snapshot {
            status: Status {
                head: Head::Branch("main".into()),
                upstream,
                counts,
                changed_paths: vec![],
            },
            state: RepoState::Normal,
            refs: RepoRefs {
                branches: branches.iter().map(|s| s.to_string()).collect(),
                ..RepoRefs::default()
            },
            tag: None,
        }
    }

    impl Backend for FakeBackend {
        fn probe(&self, _dir: &Path) -> Result<Snapshot, ProbeError> {
            self.snapshot
                .lock()
                .unwrap()
                .clone()
                .ok_or(ProbeError::NotARepo)
        }
        fn run(&self, dir: &Path, args: &[String]) -> io::Result<ProcessOutput> {
            self.runs
                .lock()
                .unwrap()
                .push((dir.to_path_buf(), args.to_vec()));
            Ok(ProcessOutput {
                exit_code: Some(0),
                stdout: format!("ran {}", args.join(" ")),
                stderr: String::new(),
            })
        }
        fn fetch(&self, dir: &Path) -> io::Result<ProcessOutput> {
            let args: Vec<String> = FETCH_ARGS.iter().map(|s| s.to_string()).collect();
            self.run(dir, &args)
        }
        fn run_shell(&self, dir: &Path, line: &str) -> io::Result<ProcessOutput> {
            self.shell_runs
                .lock()
                .unwrap()
                .push((dir.to_path_buf(), line.to_owned()));
            Ok(ProcessOutput {
                exit_code: Some(0),
                stdout: format!("shell {line}"),
                stderr: String::new(),
            })
        }
        fn list_commands(&self, _dir: &Path) -> (Vec<String>, Vec<String>) {
            (
                vec!["pull".into(), "push".into(), "status".into()],
                vec!["pull".into(), "push".into(), "status".into()],
            )
        }
        fn options(&self, _dir: &Path, _command: &str) -> Vec<String> {
            vec!["--rebase".into()]
        }
    }

    pub fn app_with(backend: Arc<FakeBackend>, paths: Option<Paths>) -> App {
        App::new(
            Ok(Config::default()),
            paths,
            History::default(),
            backend,
            Arc::new(|| {}),
        )
    }

    /// Wait until `done` holds, applying events meanwhile.
    pub fn settle(app: &mut App, done: impl Fn(&App) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done(app) {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
            app.handle_events();
        }
    }

    fn repo_dirs(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = names
            .iter()
            .map(|n| {
                let p = dir.path().join(n);
                std::fs::create_dir_all(p.join(".git")).unwrap();
                p.canonicalize().unwrap()
            })
            .collect();
        (dir, paths)
    }

    #[test]
    fn add_scans_sorts_and_reports_duplicates() {
        let (dir, paths) = repo_dirs(&["zeta", "alpha", "org/beta"]);
        let mut app = app_with(Arc::default(), None);
        app.add_folder(dir.path());
        let names: Vec<_> = app.cards.iter().map(|c| c.path.clone()).collect();
        assert_eq!(
            names,
            vec![paths[1].clone(), paths[2].clone(), paths[0].clone()]
        );
        assert_eq!(app.config.repos, names);
        assert!(app.toasts.is_empty());

        app.add_folder(&paths[0]);
        assert_eq!(app.cards.len(), 3);
        assert_eq!(app.toasts.last().unwrap().text, "1 already on board");
    }

    #[test]
    fn folder_picker_result_is_taken_once() {
        let (dir, paths) = repo_dirs(&["picked"]);
        let mut app = app_with(Arc::default(), None);
        let folder = dir.path().to_path_buf();
        app.open_folder_picker(move || Some(folder));
        // A second click while the dialog is open is ignored.
        app.open_folder_picker(|| panic!("second dialog opened"));
        let deadline = Instant::now() + Duration::from_secs(5);
        let picked = loop {
            if let Some(p) = app.take_picked_folder() {
                break p;
            }
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
        };
        app.add_folder(&picked);
        assert_eq!(app.config.repos, paths);
        assert_eq!(app.take_picked_folder(), None);

        app.open_folder_picker(|| None);
        while app.picker.is_some() {
            assert!(Instant::now() < deadline, "timed out");
            assert_eq!(app.take_picked_folder(), None);
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn add_empty_folder_toasts() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with(Arc::default(), None);
        app.add_folder(dir.path());
        assert!(app.cards.is_empty());
        assert!(app.toasts[0].text.starts_with("No git repository found"));
    }

    #[test]
    fn remove_and_remove_marked() {
        let (dir, paths) = repo_dirs(&["a", "b", "c"]);
        let mut app = app_with(Arc::default(), None);
        app.add_folder(dir.path());
        app.marked.insert(paths[0].clone());
        app.marked.insert(paths[1].clone());
        app.remove(&paths[0]);
        assert_eq!(app.cards.len(), 2);
        assert!(!app.marked.contains(&paths[0]));
        app.remove_marked();
        assert_eq!(app.config.repos, vec![paths[2].clone()]);
        assert!(app.marked.is_empty());
        // Files on disk are untouched.
        assert!(paths.iter().all(|p| p.exists()));
    }

    #[test]
    fn mark_filters() {
        let mut app = app_with(Arc::default(), None);
        let dirty = Counts {
            modified: 1,
            ..Counts::default()
        };
        let behind = Upstream::Tracking {
            ahead: 0,
            behind: 2,
        };
        for (name, snap) in [
            ("clean", snapshot(Counts::default(), Upstream::None, &[])),
            ("dirty", snapshot(dirty, Upstream::None, &[])),
            ("behind", snapshot(Counts::default(), behind, &[])),
        ] {
            let mut card = Card::new(PathBuf::from(format!("/{name}")));
            card.probe = Some(Ok(snap));
            app.cards.push(card);
        }
        app.mark_dirty();
        assert_eq!(app.marked_repos(), vec![PathBuf::from("/dirty")]);
        app.mark_behind();
        assert_eq!(app.marked_repos(), vec![PathBuf::from("/behind")]);
        app.mark_all();
        assert_eq!(app.marked.len(), 3);
        app.mark_none();
        assert!(app.marked.is_empty());
    }

    #[test]
    fn destructive_bulk_needs_confirmation_single_does_not() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        let repos = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        app.start_run("reset --hard", repos.clone());
        assert!(app.confirm.is_some());
        assert!(app.log.is_empty());
        app.cancel_run();
        assert!(app.confirm.is_none());

        app.start_run("reset --hard", vec![PathBuf::from("/a")]);
        assert!(app.confirm.is_none());
        app.start_run("reset --hard", repos);
        app.confirm_run();
        settle(&mut app, |_| backend.runs.lock().unwrap().len() == 3);
        assert_eq!(app.log.len(), 2);
    }

    #[test]
    fn cli_runs_in_marked_repos_and_records_history() {
        let backend = Arc::new(FakeBackend::default());
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path(), dir.path());
        let mut app = app_with(backend.clone(), Some(paths.clone()));
        app.cards = vec![Card::new("/a".into()), Card::new("/b".into())];
        app.run_cli_line("status");
        assert!(app.log.is_empty(), "nothing marked: nothing runs");
        app.marked.insert("/b".into());
        app.run_cli_line("log --format='%h %s'");
        settle(&mut app, |a| a.cards[1].last_command == Some(true));
        let runs = backend.runs.lock().unwrap().clone();
        assert_eq!(
            runs,
            vec![(
                PathBuf::from("/b"),
                vec!["log".to_owned(), "--format=%h %s".to_owned()]
            )]
        );
        assert_eq!(app.history.entries(), &["log --format='%h %s'"]);
        assert_eq!(store::load_history(&paths.history_file), app.history);
        let run = app.log.runs().next().unwrap();
        assert!(
            matches!(&run.sections[0].state, SectionState::Done(o) if o.stdout == "ran log --format=%h %s")
        );
    }

    #[test]
    fn typed_git_prefix_is_dropped_from_run_and_history() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = vec![Card::new("/a".into())];
        app.marked.insert("/a".into());
        app.run_cli_line("git pull --rebase");
        settle(&mut app, |a| a.cards[0].last_command == Some(true));
        assert_eq!(
            backend.runs.lock().unwrap().clone(),
            vec![(
                PathBuf::from("/a"),
                vec!["pull".to_owned(), "--rebase".to_owned()]
            )]
        );
        assert_eq!(app.history.entries(), &["pull --rebase"]);
        assert_eq!(app.log.runs().next().unwrap().command, "pull --rebase");
    }

    #[test]
    fn shell_lines_always_need_confirmation_and_keep_their_bang() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = vec![Card::new("/a".into())];
        app.marked.insert("/a".into());
        app.run_cli_line("! ls | wc -l");
        // One repo is enough: a shell line can do anything.
        let pending = app.confirm.clone().unwrap();
        assert_eq!(pending.command, "! ls | wc -l");
        assert_eq!(pending.job, Job::Shell("ls | wc -l".into()));
        assert!(app.log.is_empty());
        assert_eq!(app.history.entries(), &["! ls | wc -l"]);
        app.confirm_run();
        settle(&mut app, |a| a.cards[0].last_command == Some(true));
        assert_eq!(
            backend.shell_runs.lock().unwrap().clone(),
            vec![(PathBuf::from("/a"), "ls | wc -l".to_owned())]
        );
        assert!(backend.runs.lock().unwrap().is_empty());
        assert_eq!(app.log.runs().next().unwrap().command, "! ls | wc -l");

        app.run_cli_line("!   ");
        assert!(app.confirm.is_none(), "an empty shell line runs nothing");
        assert_eq!(app.history.entries().len(), 1);
    }

    #[test]
    fn shell_button_asks_too() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = vec![Card::new("/a".into())];
        app.config.buttons = vec![crate::domain::config::ButtonDef::new("make", "!make test")];
        app.run_button(0, Path::new("/a"));
        assert_eq!(
            app.confirm.as_ref().map(|p| &p.job),
            Some(&Job::Shell("make test".into()))
        );
        app.cancel_run();
        assert!(backend.shell_runs.lock().unwrap().is_empty());
    }

    #[test]
    fn unparsable_command_toasts() {
        let mut app = app_with(Arc::default(), None);
        app.start_run("log 'unclosed", vec![PathBuf::from("/a")]);
        assert!(app.log.is_empty());
        assert!(app.toasts[0].text.starts_with("Cannot parse command"));
    }

    #[test]
    fn buttons_run_on_card_or_marked() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = vec![Card::new("/a".into()), Card::new("/b".into())];
        app.run_button(2, Path::new("/a"));
        app.marked
            .extend([PathBuf::from("/a"), PathBuf::from("/b")]);
        app.run_button_on_marked(0);
        settle(&mut app, |_| backend.runs.lock().unwrap().len() == 3);
        let mut runs = backend.runs.lock().unwrap().clone();
        runs.sort();
        assert_eq!(
            runs,
            vec![
                (
                    PathBuf::from("/a"),
                    vec!["pull".to_owned(), "--ff-only".to_owned()]
                ),
                (PathBuf::from("/a"), vec!["push".to_owned()]),
                (
                    PathBuf::from("/b"),
                    vec!["pull".to_owned(), "--ff-only".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn tick_polls_fetches_and_persists_config() {
        let backend = Arc::new(FakeBackend::default());
        *backend.snapshot.lock().unwrap() =
            Some(snapshot(Counts::default(), Upstream::None, &["main"]));
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path(), dir.path());
        let mut app = app_with(backend.clone(), Some(paths.clone()));
        app.cards = vec![Card::new("/a".into())];
        app.config.concurrency = 3;
        app.tick(Instant::now());
        settle(&mut app, |a| {
            a.cards[0].snapshot().is_some() && !a.vocab.subcommands.is_empty()
        });
        let runs = backend.runs.lock().unwrap().clone();
        assert_eq!(
            runs,
            vec![(
                PathBuf::from("/a"),
                vec!["fetch".to_owned(), "--all".to_owned(), "--prune".to_owned()]
            )]
        );
        assert_eq!(
            store::load_config(&paths.config_file).unwrap().concurrency,
            3
        );
    }

    #[test]
    fn config_error_disables_saving() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::in_dirs(dir.path(), dir.path());
        let mut app = App::new(
            Err("broken".into()),
            Some(paths.clone()),
            History::default(),
            Arc::new(FakeBackend::default()),
            Arc::new(|| {}),
        );
        app.config.concurrency = 9;
        app.tick(Instant::now());
        assert!(!paths.config_file.exists());
    }

    #[test]
    fn options_requested_once_for_builtins_only() {
        let mut app = app_with(Arc::default(), None);
        app.tick(Instant::now());
        settle(&mut app, |a| !a.vocab.subcommands.is_empty());
        app.request_options("pull");
        app.request_options("pull");
        settle(&mut app, |a| a.vocab.options.contains_key("pull"));
        assert_eq!(app.vocab.options["pull"], vec!["--rebase".to_owned()]);
        app.request_options("my-alias");
        assert_eq!(app.vocab.options["my-alias"], Vec::<String>::new());
    }

    #[test]
    fn repo_values_from_marked_only() {
        let mut app = app_with(Arc::default(), None);
        for (name, branches) in [("/a", vec!["main", "x"]), ("/b", vec!["main"])] {
            let mut card = Card::new(name.into());
            card.probe = Some(Ok(snapshot(Counts::default(), Upstream::None, &branches)));
            app.cards.push(card);
        }
        app.marked.insert("/b".into());
        let values = app.repo_values();
        assert_eq!(values.total, 1);
        assert_eq!(values.branches.keys().collect::<Vec<_>>(), vec!["main"]);
    }
}
