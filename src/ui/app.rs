//! Board state and the actions the UI triggers on it (no drawing here).

use crate::application::desktop::Desktop;
use crate::application::engine::{Backend, Engine, Event, Notify, ProbeCause};
use crate::application::music::Music;
use crate::application::schedule::{Due, Schedule};
use crate::domain::audio::{self, SAMPLE_RATE, Sound};
use crate::domain::board::{SortKey, Sorted, compare, is_ahead, matches_filter};
use crate::domain::command::{Job, display, shell_line};
use crate::domain::completion::{RepoValues, Vocabulary, strip_git_prefix};
use crate::domain::config::{Config, Theme};
use crate::domain::destructive::is_destructive;
use crate::domain::details::{Details, DiffTarget, LogEntry, Place, RepoTab, Trail};
use crate::domain::favorites::{FavoriteDraft, active_after_delete, forget};
use crate::domain::graph::GraphData;
use crate::domain::history::History;
use crate::domain::kawaii;
use crate::domain::naming::{board_order, title};
use crate::domain::output::OutputLog;
use crate::domain::repo::{FetchError, ProbeError, Snapshot};
use crate::domain::status::{RepoState, Upstream};
use crate::infrastructure::audio::decode;
use crate::infrastructure::desktop::{scratch_dir, write_scratch_file};
use crate::infrastructure::scan::scan;
use crate::infrastructure::store::{self, Paths};
use crate::ui::cli::CliState;
use std::collections::{BTreeSet, HashMap, HashSet};
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

    /// As sorting sees the card.
    fn sorted(&self) -> Sorted<'_> {
        Sorted {
            path: &self.path,
            snapshot: self.snapshot(),
        }
    }

    /// Has changes or an operation in progress.
    pub fn is_dirty(&self) -> bool {
        self.snapshot()
            .is_some_and(|s| !s.status.counts.is_clean() || s.state != RepoState::Normal)
    }

    /// Has unpushed commits.
    pub fn is_ahead(&self) -> bool {
        self.snapshot().is_some_and(is_ahead)
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

/// The open read-only view of one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoView {
    pub repo: PathBuf,
    /// The tab that is shown.
    pub tab: RepoTab,
    /// The lists; `None` until they were read the first time.
    pub details: Option<Result<Details, String>>,
    /// The file or commit picked in the shown tab; its diff is shown.
    pub selected: Option<DiffTarget>,
    /// What is picked in the tabs that are not shown.
    pub kept: HashMap<RepoTab, DiffTarget>,
    /// What Back and Forward step through.
    pub trail: Trail<Place>,
    /// The folders of the file tree that are folded, by path.
    pub closed: HashSet<String>,
    /// The files of the picked commit that are unfolded, by path, each
    /// with its diff; `None` while it is read.
    pub unfolded: HashMap<String, Option<Result<String, String>>>,
    /// The graph of all branches; `None` until the Graph tab asked for it.
    pub graph: Option<Result<GraphData, String>>,
    /// The folded runs of the graph that were unfolded, each by the hash
    /// of its newest commit.
    pub graph_open: HashSet<String>,
    /// The graph draws the old remote branches too.
    pub with_old: bool,
    /// The history goes on after the commits the list holds.
    pub more_commits: bool,
    /// The next page of commits is being read.
    pub loading_commits: bool,
    /// The diff of `selected`; `None` while it is read.
    pub diff: Option<Result<String, String>>,
}

impl RepoView {
    /// Whether what was read still has `target`: a file among the changed
    /// ones, a commit in the list or in the graph.
    pub fn holds(&self, target: &DiffTarget) -> bool {
        let listed = matches!(&self.details, Some(Ok(details)) if details.holds(target));
        let drawn = match (target, &self.graph) {
            (DiffTarget::Commit(hash), Some(Ok(graph))) => graph.holds(hash),
            _ => false,
        };
        listed || drawn
    }

    /// Take off the picks that what was read no longer has, in the shown
    /// tab and in the others.
    fn drop_what_is_gone(&mut self) {
        if self.selected.as_ref().is_some_and(|t| !self.holds(t)) {
            self.selected = None;
            self.diff = None;
            self.unfolded.clear();
        }
        let mut kept = std::mem::take(&mut self.kept);
        kept.retain(|_, target| self.holds(target));
        self.kept = kept;
    }
}

/// What the settings window shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsPage {
    #[default]
    Settings,
    About,
    Licenses,
}

/// The open dialog that makes a favorite or changes one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteDialog {
    pub draft: FavoriteDraft,
    /// The favorite that is changed; `None` makes a new one.
    pub editing: Option<usize>,
    /// The repositories a new favorite starts with.
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
    /// Marks of all cards, also of the ones the filter hides.
    pub marked: BTreeSet<PathBuf>,
    /// Text of the filter field; not saved.
    pub filter: String,
    /// What the board is sorted by; not saved.
    pub sort: SortKey,
    /// The favorite whose repositories alone are shown, by its position in
    /// the config; not saved.
    pub active_favorite: Option<usize>,
    pub favorite_dialog: Option<FavoriteDialog>,
    pub repo_view: Option<RepoView>,
    engine: Engine,
    desktop: Arc<dyn Desktop>,
    /// Where files for other programs are written; removed on exit.
    pub scratch: PathBuf,
    events: Receiver<Event>,
    schedule: Schedule,
    applied_concurrency: usize,
    pub log: OutputLog,
    /// The output panel is folded away; not saved.
    pub output_hidden: bool,
    /// The last run the open output panel showed.
    output_seen: Option<u64>,
    pub history: History,
    pub cli: CliState,
    pub vocab: Vocabulary,
    builtins: HashSet<String>,
    options_requested: HashSet<String>,
    commands_requested: bool,
    pub toasts: Vec<Toast>,
    pub copy_flash: Option<CopyFlash>,
    /// The clock of the Kawaii theme's motion, in seconds; it runs only
    /// while the window has focus.
    pub kawaii_clock: f64,
    /// Seconds a change in a repository still keeps the Kawaii theme moving
    /// in a window without focus.
    kawaii_awake: f32,
    /// Cards that swing because they were just marked, with the clock time
    /// of the mark.
    wobbles: HashMap<PathBuf, f64>,
    pub confirm: Option<PendingRun>,
    pub settings_open: bool,
    pub settings_page: SettingsPage,
    /// The app icon for the About page, once it was drawn.
    pub icon: Option<egui::TextureHandle>,
    notify: Notify,
    /// Folder dialog running on its own thread; yields the picked folder.
    picker: Option<Receiver<Option<PathBuf>>>,
    /// File dialog for a sound, running on its own thread.
    sound_picker: Option<(Sound, Receiver<Option<PathBuf>>)>,
    /// When each sound last played.
    sound_played: HashMap<Sound, Instant>,
    /// For each sound, the file and volume its WAV in the scratch folder
    /// was made from.
    sound_made: HashMap<Sound, (Option<PathBuf>, u8)>,
    /// The missing player was reported already.
    player_missing_said: bool,
    /// The Kawaii theme's music while it plays.
    music: Option<Music>,
    /// A mouse button is down. A volume slider that is still dragged does
    /// not retune the music; the volume is taken when it is let go.
    pub pointer_held: bool,
    /// The music ended with an error; it is not started again until it was
    /// switched off once.
    music_failed: bool,
}

impl App {
    /// `config` is the result of loading the config file.
    pub fn new(
        config: Result<Config, String>,
        paths: Option<Paths>,
        history: History,
        backend: Arc<dyn Backend>,
        desktop: Arc<dyn Desktop>,
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
            filter: String::new(),
            sort: SortKey::default(),
            active_favorite: None,
            favorite_dialog: None,
            repo_view: None,
            engine,
            desktop,
            scratch: scratch_dir(),
            events,
            schedule: Schedule::default(),
            log: OutputLog::default(),
            output_hidden: false,
            output_seen: None,
            history,
            cli: CliState::default(),
            vocab: Vocabulary::default(),
            builtins: HashSet::new(),
            options_requested: HashSet::new(),
            commands_requested: false,
            toasts: Vec::new(),
            copy_flash: None,
            kawaii_clock: 0.0,
            kawaii_awake: 0.0,
            wobbles: HashMap::new(),
            confirm: None,
            settings_open: false,
            settings_page: SettingsPage::default(),
            icon: None,
            notify,
            picker: None,
            sound_picker: None,
            sound_played: HashMap::new(),
            sound_made: HashMap::new(),
            player_missing_said: false,
            music: None,
            pointer_held: false,
            music_failed: false,
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

    /// Run a (blocking) file dialog for the user's own `sound` on its own
    /// thread. Ignored while one is open.
    pub fn open_sound_picker<F>(&mut self, sound: Sound, pick: F)
    where
        F: FnOnce() -> Option<PathBuf> + Send + 'static,
    {
        if self.sound_picker.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let notify = self.notify.clone();
        std::thread::spawn(move || {
            let _ = tx.send(pick());
            notify();
        });
        self.sound_picker = Some((sound, rx));
    }

    /// Take the file picked in the sound dialog, once it closed, as that
    /// sound's file.
    pub fn take_picked_sound(&mut self) {
        let Some((sound, rx)) = &self.sound_picker else {
            return;
        };
        let sound = *sound;
        match rx.try_recv() {
            Ok(file) => {
                self.sound_picker = None;
                if file.is_some() {
                    self.config.set_sound_file(sound, file);
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.sound_picker = None,
        }
    }

    pub fn music_plays(&self) -> bool {
        self.music.is_some()
    }

    /// The volume the playing music was last given.
    pub fn music_volume_playing(&self) -> Option<u8> {
        self.music.as_ref().map(Music::volume)
    }

    /// Start, stop or tune the music so it fits the settings: it plays in
    /// the Kawaii theme with music switched on, and nowhere else.
    pub fn sync_music(&mut self) {
        if !(self.is_kawaii() && self.config.music) {
            self.music = None;
            self.music_failed = false;
            return;
        }
        match &self.music {
            Some(music) => {
                if !self.pointer_held {
                    music.set_volume(self.config.music_volume);
                }
                if let Some(error) = music.take_error() {
                    self.music = None;
                    self.music_failed = true;
                    self.toast(format!("Cannot play music: {error}"));
                }
            }
            None if !self.music_failed => {
                let dir = self.scratch.clone();
                let write = Arc::new(move |wav: &[u8]| write_scratch_file(&dir, "music.wav", wav));
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |d| d.as_nanos() as u64);
                let volume = self.config.music_volume;
                self.music = Some(Music::start(self.desktop.clone(), write, volume, seed));
            }
            None => {}
        }
    }

    /// The WAV of `sound` in the scratch folder, made again when the
    /// user's file or the volume changed. A file that cannot be read is
    /// said once and the app's own tone is used.
    fn sound_wav(&mut self, sound: Sound) -> std::io::Result<PathBuf> {
        let made_from = (
            self.config.sound_file(sound).cloned(),
            self.config.sound_volume,
        );
        let file = self.scratch.join(sound.file_name());
        if self.sound_made.get(&sound) == Some(&made_from) && file.exists() {
            return Ok(file);
        }
        let (samples, rate) = match made_from.0.as_deref().map(decode) {
            Some(Ok(decoded)) => (decoded.samples, decoded.rate),
            Some(Err(e)) => {
                let name = made_from.0.as_deref().map(title).unwrap_or_default();
                self.toast(format!("Cannot read {name}: {e}"));
                (sound.default_samples(), SAMPLE_RATE)
            }
            None => (sound.default_samples(), SAMPLE_RATE),
        };
        let wav = audio::wav_bytes(&samples, rate, made_from.1);
        let file = write_scratch_file(&self.scratch, sound.file_name(), wav)?;
        self.sound_made.insert(sound, made_from);
        Ok(file)
    }

    /// Play `sound` now (the play button in Settings, and news on a card).
    pub fn play_sound(&mut self, sound: Sound) {
        let played = self
            .sound_wav(sound)
            .and_then(|file| self.desktop.play(&file));
        if let Err(e) = played
            && !std::mem::replace(&mut self.player_missing_said, true)
        {
            self.toast(format!("Cannot play sounds: {e}"));
        }
    }

    /// News on a card at `now`: play each of `sounds` unless it played a
    /// moment ago. Nothing while sounds are switched off.
    pub fn sound_news(&mut self, sounds: &[Sound], now: Instant) {
        if !self.config.sounds {
            return;
        }
        for &sound in sounds {
            let last = self.sound_played.get(&sound);
            let since = last.map(|last| now.saturating_duration_since(*last).as_secs_f64());
            if audio::may_play(since) {
                self.sound_played.insert(sound, now);
                self.play_sound(sound);
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
        // Picking a hidden repository itself, not a folder above it, asks
        // for it by name: that unhides it.
        let picked_itself = found.len() == 1 && folder.join(".git").exists();
        let (mut duplicates, mut hidden) = (0, 0);
        for path in found {
            if self.cards.iter().any(|c| c.path == path) {
                duplicates += 1;
            } else if self.config.hidden.contains(&path) && !picked_itself {
                hidden += 1;
            } else {
                self.config.hidden.retain(|h| *h != path);
                self.put_on_board(path);
            }
        }
        self.sync_repos_to_config();
        if duplicates > 0 {
            self.toast(format!("{duplicates} already on board"));
        }
        if hidden > 0 {
            self.toast(format!("{hidden} hidden, left out (Settings unhides)"));
        }
    }

    /// A new card for `path`, in board order, probed at once.
    fn put_on_board(&mut self, path: PathBuf) {
        self.engine.poll(&path);
        self.cards.push(Card::new(path));
        self.cards.sort_by(|a, b| board_order(&a.path, &b.path));
    }

    /// Take the cards of `paths` off the board for good: nothing on disk
    /// changes, and favorites forget them.
    pub fn remove_cards(&mut self, paths: &[PathBuf]) {
        self.cards.retain(|c| !paths.contains(&c.path));
        self.marked.retain(|p| !paths.contains(p));
        forget(&mut self.config.favorites, paths);
        self.sync_repos_to_config();
    }

    /// Take the cards of `paths` off the board but remember them: a scan of
    /// their parent folder leaves them out, nothing runs on them, and they
    /// keep their favorites.
    pub fn hide_cards(&mut self, paths: &[PathBuf]) {
        self.cards.retain(|c| !paths.contains(&c.path));
        self.marked.retain(|p| !paths.contains(p));
        for path in paths {
            if !self.config.hidden.contains(path) {
                self.config.hidden.push(path.clone());
            }
        }
        self.config.hidden.sort();
        self.sync_repos_to_config();
    }

    /// Put a hidden repository back on the board.
    pub fn unhide(&mut self, path: &Path) {
        if !self.config.hidden.iter().any(|h| h == path) {
            return;
        }
        self.config.hidden.retain(|h| h != path);
        if !self.cards.iter().any(|c| c.path == path) {
            self.put_on_board(path.to_path_buf());
        }
        self.sync_repos_to_config();
    }

    pub fn unhide_all(&mut self) {
        for path in std::mem::take(&mut self.config.hidden) {
            if !self.cards.iter().any(|c| c.path == path) {
                self.put_on_board(path);
            }
        }
        self.sync_repos_to_config();
    }

    /// Open the read-only view of `repo` and read its lists. It opens on
    /// the changes of a repository that has some, else on its history.
    pub fn open_repo_view(&mut self, repo: &Path) {
        let dirty = self
            .cards
            .iter()
            .find(|c| c.path == repo)
            .and_then(Card::snapshot)
            .is_some_and(|s| !s.status.counts.is_clean());
        let tab = RepoTab::opening(dirty);
        self.repo_view = Some(RepoView {
            repo: repo.to_path_buf(),
            tab,
            details: None,
            selected: None,
            kept: HashMap::new(),
            trail: Trail::new((tab, None)),
            closed: HashSet::new(),
            unfolded: HashMap::new(),
            graph: None,
            graph_open: HashSet::new(),
            with_old: false,
            more_commits: true,
            loading_commits: false,
            diff: None,
        });
        self.engine.load_details(repo.to_path_buf(), 0);
    }

    pub fn close_repo_view(&mut self) {
        self.repo_view = None;
    }

    /// Read the lists of the open view again.
    pub fn refresh_repo_view(&mut self) {
        if let Some(view) = &self.repo_view {
            // As many commits as the list holds, so it does not shrink.
            let commits = view
                .details
                .as_ref()
                .and_then(|d| d.as_ref().ok())
                .map_or(0, |d| d.commits.len());
            self.engine.load_details(view.repo.clone(), commits);
            if view.graph.is_some() || view.tab == RepoTab::Graph {
                self.engine.load_graph(view.repo.clone(), view.with_old);
            }
        }
    }

    /// Show `place` in the open view: its tab, and its pick with the diff.
    /// A pick the lists no longer hold is left out.
    fn show_place(&mut self, (tab, target): Place) {
        let Some(view) = &mut self.repo_view else {
            return;
        };
        if let Some(old) = view.selected.take() {
            view.kept.insert(view.tab, old);
        }
        view.kept.remove(&tab);
        let held = target.filter(|t| view.holds(t));
        view.tab = tab;
        if tab == RepoTab::Graph && view.graph.is_none() {
            self.engine.load_graph(view.repo.clone(), view.with_old);
        }
        view.selected = held.clone();
        view.diff = None;
        view.unfolded.clear();
        if let Some(target) = held {
            self.engine.load_diff(view.repo.clone(), target);
        }
    }

    /// A click on a file or commit in the view: show its diff.
    pub fn select_in_repo_view(&mut self, target: DiffTarget) {
        if let Some(view) = &mut self.repo_view {
            let place = (view.tab, Some(target.clone()));
            view.trail.visit(place);
            view.selected = Some(target.clone());
            view.diff = None;
            view.unfolded.clear();
            self.engine.load_diff(view.repo.clone(), target);
        }
    }

    /// A click on a file of the picked commit: unfold its diff, or fold it.
    pub fn toggle_commit_file(&mut self, path: &str) {
        let Some(view) = &mut self.repo_view else {
            return;
        };
        let Some(DiffTarget::Commit(hash)) = &view.selected else {
            return;
        };
        if view.unfolded.remove(path).is_some() {
            return;
        }
        view.unfolded.insert(path.to_owned(), None);
        let target = DiffTarget::CommitFile {
            hash: hash.clone(),
            path: path.to_owned(),
        };
        self.engine.load_diff(view.repo.clone(), target);
    }

    /// The commit list was scrolled to its end: read the next page, once.
    pub fn load_more_commits(&mut self) {
        let Some(view) = &mut self.repo_view else {
            return;
        };
        let Some(Ok(details)) = &view.details else {
            return;
        };
        if view.more_commits && !view.loading_commits {
            view.loading_commits = true;
            self.engine
                .load_log(view.repo.clone(), details.commits.len());
        }
    }

    /// A page of commits was read: it goes to the end of the list when it
    /// is the page that follows it.
    fn commits_loaded(
        &mut self,
        repo: &Path,
        skip: usize,
        result: Result<Vec<LogEntry>, String>,
        more: bool,
    ) {
        let Some(view) = self.repo_view.as_mut().filter(|v| v.repo == repo) else {
            return;
        };
        let Some(Ok(details)) = &mut view.details else {
            return;
        };
        if details.commits.len() != skip {
            return;
        }
        view.loading_commits = false;
        match result {
            Ok(page) => {
                details.commits.extend(page);
                view.more_commits = more;
            }
            Err(error) => {
                view.more_commits = false;
                self.toast(format!("Cannot read more commits: {error}"));
            }
        }
    }

    /// The graph of the open view was read.
    fn graph_loaded(&mut self, repo: &Path, result: Result<GraphData, String>) {
        let Some(view) = self.repo_view.as_mut().filter(|v| v.repo == repo) else {
            return;
        };
        view.graph = Some(result);
        view.drop_what_is_gone();
    }

    /// A click on a folded run of the graph: show its commits.
    pub fn unfold_graph_run(&mut self, first: &str) {
        if let Some(view) = &mut self.repo_view {
            view.graph_open.insert(first.to_owned());
        }
    }

    /// A click on the line that counts the old remote branches left out of
    /// the graph: draw them too.
    pub fn show_old_branches(&mut self) {
        if let Some(view) = &mut self.repo_view
            && !view.with_old
        {
            view.with_old = true;
            self.engine.load_graph(view.repo.clone(), true);
        }
    }

    /// A click on a folder of the file tree: fold it, or unfold it.
    pub fn toggle_repo_folder(&mut self, path: &str) {
        if let Some(view) = &mut self.repo_view
            && !view.closed.remove(path)
        {
            view.closed.insert(path.to_owned());
        }
    }

    /// A click on a tab of the view: show it, with what was picked in it.
    pub fn switch_repo_tab(&mut self, tab: RepoTab) {
        let Some(view) = &mut self.repo_view else {
            return;
        };
        if view.tab == tab {
            return;
        }
        let place = (tab, view.kept.get(&tab).cloned());
        view.trail.visit(place.clone());
        self.show_place(place);
    }

    /// Back in the view: the place looked at before this one.
    pub fn repo_view_back(&mut self) {
        let place = self
            .repo_view
            .as_mut()
            .and_then(|v| v.trail.back().cloned());
        if let Some(place) = place {
            self.show_place(place);
        }
    }

    /// Forward in the view: the place Back was pressed on.
    pub fn repo_view_forward(&mut self) {
        let place = self
            .repo_view
            .as_mut()
            .and_then(|v| v.trail.forward().cloned());
        if let Some(place) = place {
            self.show_place(place);
        }
    }

    /// The lists of the open view were read. A selection the lists no
    /// longer hold goes; one they still hold gets its diff read again, and
    /// the old diff stays on screen until the new one is there.
    fn repo_view_loaded(&mut self, repo: &Path, result: Result<Details, String>) {
        let Some(view) = self.repo_view.as_mut().filter(|v| v.repo == repo) else {
            return;
        };
        view.details = Some(result);
        view.drop_what_is_gone();
        // A page asked for before this reload no longer fits the list.
        view.loading_commits = false;
        if let Some(Ok(details)) = &view.details {
            let page = crate::domain::details::LOG_PAGE;
            view.more_commits = details.commits.len() >= page && details.commits.len() % page == 0;
        }
        if let Some(target) = view.selected.clone() {
            self.engine.load_diff(repo.to_path_buf(), target);
        }
    }

    pub fn is_kawaii(&self) -> bool {
        self.config.theme == Theme::Kawaii
    }

    /// A click on a card: mark it, or take the mark off. In the Kawaii theme
    /// a card that gets its mark swings.
    pub fn toggle_mark(&mut self, path: PathBuf) {
        if self.marked.remove(&path) {
            return;
        }
        if self.is_kawaii() {
            self.wobbles.insert(path.clone(), self.kawaii_clock);
        }
        self.marked.insert(path);
    }

    /// One frame of the Kawaii theme's clock; `true` while it runs: with
    /// focus, and for a few seconds after a repository changed.
    pub fn advance_kawaii(&mut self, dt: f32, focused: bool) -> bool {
        if !self.is_kawaii() {
            self.wobbles.clear();
            self.kawaii_awake = 0.0;
            return false;
        }
        let running = focused || self.kawaii_awake > 0.0;
        self.kawaii_clock = kawaii::advance(self.kawaii_clock, dt, running);
        self.kawaii_awake = kawaii::awake_after(self.kawaii_awake, dt, focused);
        let clock = self.kawaii_clock;
        self.wobbles
            .retain(|_, marked_at| kawaii::wobble(clock, *marked_at).is_some());
        running
    }

    /// Something on a card changed: the Kawaii theme moves for a while, also
    /// in a window without focus.
    fn wake_kawaii(&mut self) {
        if self.is_kawaii() {
            self.kawaii_awake = kawaii::WAKE_SECS;
        }
    }

    /// How far the Kawaii theme moves the card at position `index` of the
    /// board right now: right and down, in points. Nothing in other themes.
    pub fn card_offset(&self, path: &Path, index: usize) -> (f32, f32) {
        if !self.is_kawaii() {
            return (0.0, 0.0);
        }
        let swing = self
            .wobbles
            .get(path)
            .and_then(|marked_at| kawaii::wobble(self.kawaii_clock, *marked_at));
        (swing.unwrap_or(0.0), kawaii::bob(self.kawaii_clock, index))
    }

    /// What a right click on the card of `clicked` acts on: that card, or,
    /// when it is marked, all marked cards the board shows.
    pub fn menu_targets(&self, clicked: &Path) -> Vec<PathBuf> {
        if self.marked.contains(clicked) {
            self.marked_repos()
        } else {
            vec![clicked.to_path_buf()]
        }
    }

    pub fn add_to_favorite(&mut self, index: usize, repos: &[PathBuf]) {
        if let Some(favorite) = self.config.favorites.get_mut(index) {
            favorite.add(repos);
        }
    }

    pub fn remove_from_favorite(&mut self, index: usize, repos: &[PathBuf]) {
        if let Some(favorite) = self.config.favorites.get_mut(index) {
            favorite.remove(repos);
        }
    }

    /// Open the dialog for a new favorite of `repos`.
    pub fn new_favorite(&mut self, repos: Vec<PathBuf>) {
        self.favorite_dialog = Some(FavoriteDialog {
            draft: FavoriteDraft::default(),
            editing: None,
            repos,
        });
    }

    /// Open the dialog that changes the favorite at `index`.
    pub fn edit_favorite(&mut self, index: usize) {
        if let Some(favorite) = self.config.favorites.get(index) {
            self.favorite_dialog = Some(FavoriteDialog {
                draft: FavoriteDraft::of(favorite),
                editing: Some(index),
                repos: Vec::new(),
            });
        }
    }

    /// Save of the favorite dialog; a draft without a name keeps it open.
    pub fn save_favorite(&mut self) {
        let Some(dialog) = self.favorite_dialog.take_if(|d| d.draft.is_valid()) else {
            return;
        };
        match dialog.editing {
            Some(index) => {
                if let Some(favorite) = self.config.favorites.get_mut(index) {
                    dialog.draft.apply(favorite);
                }
            }
            None => self
                .config
                .favorites
                .push(dialog.draft.create(dialog.repos)),
        }
    }

    /// Delete the favorite at `index`; its repositories stay on the board.
    pub fn delete_favorite(&mut self, index: usize) {
        if index < self.config.favorites.len() {
            self.config.favorites.remove(index);
            self.active_favorite = active_after_delete(self.active_favorite, index);
        }
    }

    /// A click on a favorite's button: show only its repositories, or all
    /// again when it was the active one.
    pub fn toggle_favorite(&mut self, index: usize) {
        self.active_favorite = (self.active_favorite != Some(index)).then_some(index);
    }

    /// Whether the board shows this card.
    pub fn shows(&self, card: &Card) -> bool {
        let in_active = self
            .active_favorite
            .and_then(|i| self.config.favorites.get(i))
            .is_none_or(|f| f.contains(&card.path));
        in_active && matches_filter(&card.path, &self.filter)
    }

    /// The cards the board shows, in the order of the sort key. Everything that acts on
    /// "the marked cards" acts on the marked ones among these: a hidden card
    /// keeps its mark but is never run in, marked, unmarked or removed.
    pub fn shown_cards(&self) -> Vec<&Card> {
        let mut shown: Vec<&Card> = self.cards.iter().filter(|c| self.shows(c)).collect();
        // `cards` is in name order already.
        if self.sort != SortKey::Name {
            shown.sort_by(|a, b| compare(self.sort, a.sorted(), b.sorted()));
        }
        shown
    }

    fn shown_paths(&self) -> BTreeSet<PathBuf> {
        self.shown_cards().iter().map(|c| c.path.clone()).collect()
    }

    /// A click on a card's fetch error: its whole output for the clipboard;
    /// the error leaves the card until a fetch fails again.
    pub fn take_fetch_error(&mut self, path: &Path) -> Option<String> {
        let error = self.card_mut(path)?.fetch_error.take()?;
        Some(error.output)
    }

    fn mark_where(&mut self, keep: impl Fn(&Card) -> bool) {
        let shown = self.shown_paths();
        let picked: Vec<PathBuf> = self
            .shown_cards()
            .into_iter()
            .filter(|c| keep(c))
            .map(|c| c.path.clone())
            .collect();
        self.marked.retain(|p| !shown.contains(p));
        self.marked.extend(picked);
    }

    pub fn mark_all(&mut self) {
        self.mark_where(|_| true);
    }
    pub fn mark_none(&mut self) {
        self.mark_where(|_| false);
    }
    pub fn mark_dirty(&mut self) {
        self.mark_where(Card::is_dirty);
    }
    pub fn mark_behind(&mut self) {
        self.mark_where(Card::is_behind);
    }
    pub fn mark_ahead(&mut self) {
        self.mark_where(Card::is_ahead);
    }

    /// The marked repositories the board shows, in board order.
    pub fn marked_repos(&self) -> Vec<PathBuf> {
        self.shown_cards()
            .into_iter()
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

    /// A link of the About page: a web page or a mail in the system's
    /// program for it.
    pub fn open_url(&mut self, url: &str) {
        if let Err(e) = self.desktop.open_url(url) {
            self.toast(format!("Cannot open {url}: {e}"));
        }
    }

    /// The card's terminal button: a terminal window in the repository.
    pub fn open_terminal(&mut self, repo: &Path) {
        if let Err(e) = self.desktop.open_terminal(repo, &self.config.terminal) {
            self.toast(format!("Cannot open a terminal: {e}"));
        }
    }

    /// Write the output of run `id` (of all its repositories, or of `repo`
    /// alone) to a text file and open it with the system's default program.
    /// Nothing happens while that output is not complete.
    pub fn open_output(&mut self, id: u64, repo: Option<&Path>) {
        let Some(run) = self.log.run(id).filter(|r| r.is_done(repo)) else {
            return;
        };
        let text = run.text(&display(&run.command), repo);
        let name = match repo {
            Some(repo) => format!("output-{id}-{}.txt", title(repo)),
            None => format!("output-{id}.txt"),
        };
        let opened = write_scratch_file(&self.scratch, &name, &text)
            .and_then(|file| self.desktop.open_file(&file));
        if let Err(e) = opened {
            self.toast(format!("Cannot open the output: {e}"));
        }
    }

    /// The open output panel shows every run: none is news any more.
    pub fn output_shown(&mut self) {
        self.output_seen = self.log.last().map(|r| r.id);
    }

    /// For the mark on the button of the hidden output panel: the outcome of
    /// the last run when the panel has not shown it. `Some(None)` = running.
    pub fn unseen_outcome(&self) -> Option<Option<bool>> {
        let last = self.log.last()?;
        (self.output_seen != Some(last.id)).then(|| last.outcome())
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
            Event::Probed {
                repo,
                result,
                cause,
            } => {
                let mut changed = false;
                let mut sounds = Vec::new();
                if let Some(card) = self.card_mut(&repo) {
                    let new = Some(*result);
                    // The first probe of a card is no change.
                    changed = card.probe.is_some() && card.probe != new;
                    // What the user's own command did is no news to them.
                    if cause != ProbeCause::Command
                        && let (Some(Ok(before)), Some(Ok(after))) = (&card.probe, &new)
                    {
                        sounds = audio::news(before, after);
                    }
                    card.probe = new;
                }
                self.sound_news(&sounds, Instant::now());
                if changed {
                    self.wake_kawaii();
                    // The open view of that repository follows its card.
                    if self.repo_view.as_ref().is_some_and(|v| v.repo == repo) {
                        self.refresh_repo_view();
                    }
                }
            }
            Event::Fetched { repo, error } => {
                let mut appeared = false;
                if let Some(card) = self.card_mut(&repo) {
                    appeared = error.is_some() && card.fetch_error != error;
                    card.fetch_error = error;
                }
                if appeared {
                    self.wake_kawaii();
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
            Event::Details { repo, result } => self.repo_view_loaded(&repo, result),
            Event::Log {
                repo,
                skip,
                result,
                more,
            } => self.commits_loaded(&repo, skip, result, more),
            Event::Graph { repo, result } => self.graph_loaded(&repo, result),
            Event::Diff {
                repo,
                target,
                result,
            } => {
                // A diff that is no longer asked for is dropped.
                let Some(view) = self.repo_view.as_mut().filter(|v| v.repo == repo) else {
                    return;
                };
                if view.selected.as_ref() == Some(&target) {
                    view.diff = Some(result);
                } else if let DiffTarget::CommitFile { hash, path } = target
                    && view.selected == Some(DiffTarget::Commit(hash))
                    && let Some(slot) = view.unfolded.get_mut(&path)
                {
                    *slot = Some(result);
                }
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
            self.shown_cards()
                .into_iter()
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
        self.sync_music();
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

impl Drop for App {
    /// End the music's player, then take the files written for other
    /// programs away again.
    fn drop(&mut self) {
        self.music = None;
        let _ = std::fs::remove_dir_all(&self.scratch);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::application::engine::FETCH_ARGS;
    use crate::domain::output::{ProcessOutput, SectionState};
    use crate::domain::status::{Counts, Head, Status};
    use std::io;
    use std::sync::Mutex;

    /// Backend that records commands and returns a fixed snapshot per repo.
    #[derive(Default)]
    pub struct FakeBackend {
        pub runs: Mutex<Vec<(PathBuf, Vec<String>)>>,
        pub shell_runs: Mutex<Vec<(PathBuf, String)>>,
        /// What the repository view gets as lists; an error while `None`.
        pub details: Mutex<Option<Details>>,
        /// How often the lists were read.
        pub details_read: Mutex<usize>,
        /// What the Graph tab gets; an error while `None`. Asked for the
        /// old remote branches too, it has `origin/ancient` on its last
        /// commit.
        pub graph: Mutex<Option<GraphData>>,
        /// How often the graph was read.
        pub graphs_read: Mutex<usize>,
        /// How many commits the history has; 0 leaves the commits of
        /// `details` as they are, `usize::MAX` makes reading a page fail.
        pub log_length: Mutex<usize>,
        pub snapshot: Mutex<Option<Snapshot>>,
    }

    pub fn snapshot(counts: Counts, upstream: Upstream, branches: &[&str]) -> Snapshot {
        let mut snap = Snapshot::of(Status {
            head: Head::Branch("main".into()),
            upstream,
            counts,
            changed_paths: vec![],
        });
        snap.refs.branches = branches.iter().map(|s| s.to_string()).collect();
        // Later in the list is newer.
        snap.dated.branches = branches
            .iter()
            .zip(0..)
            .map(|(name, date)| crate::domain::repo::DatedRef {
                name: name.to_string(),
                commit: String::new(),
                date,
            })
            .collect();
        snap
    }

    /// Commit number `i` of a made-up history, newest first.
    pub fn fake_commit(i: usize) -> LogEntry {
        LogEntry {
            hash: format!("{i:040x}"),
            short: format!("{i:07x}"),
            author: "Ada Lovelace".into(),
            time: 1_700_000_000 - i as i64 * 3600,
            date: "2023-11-14 22:13".into(),
            subject: format!("Commit number {i}"),
        }
    }

    impl Backend for FakeBackend {
        fn probe(&self, _dir: &Path, _fresh: bool) -> Result<Snapshot, ProbeError> {
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
        fn details(&self, _dir: &Path, commits: usize) -> Result<Details, String> {
            *self.details_read.lock().unwrap() += 1;
            let mut details = self.details.lock().unwrap().clone();
            // A history as long as `log_length`, when that is set.
            if let Some(details) = &mut details
                && let length = *self.log_length.lock().unwrap()
                && length > 0
            {
                let count = commits.max(crate::domain::details::LOG_PAGE).min(length);
                details.commits = (0..count).map(fake_commit).collect();
            }
            details.ok_or_else(|| "not a git repository".to_owned())
        }
        fn graph(&self, _dir: &Path, with_old: bool) -> Result<GraphData, String> {
            *self.graphs_read.lock().unwrap() += 1;
            let mut graph = self.graph.lock().unwrap().clone();
            if let Some(graph) = &mut graph
                && with_old
            {
                graph.refs.push(crate::domain::graph::GraphRef {
                    name: "origin/ancient".into(),
                    kind: crate::domain::graph::RefKind::Remote,
                    hash: graph
                        .commits
                        .last()
                        .map(|c| c.hash.clone())
                        .unwrap_or_default(),
                    time: 0,
                });
            }
            graph.ok_or_else(|| "no graph".to_owned())
        }
        fn log(&self, _dir: &Path, skip: usize) -> Result<Vec<LogEntry>, String> {
            let length = *self.log_length.lock().unwrap();
            if length == usize::MAX {
                return Err("history unreadable".to_owned());
            }
            let end = length.min(skip + crate::domain::details::LOG_PAGE);
            Ok((skip..end).map(fake_commit).collect())
        }
        fn diff(&self, _dir: &Path, target: &DiffTarget) -> Result<String, String> {
            Ok(match target {
                DiffTarget::File(file) => match file.kind {
                    crate::domain::details::ChangeKind::Added => {
                        "+++ b/new\n@@ -0,0 +1 @@\n+fresh line".to_owned()
                    }
                    crate::domain::details::ChangeKind::Deleted => {
                        "--- a/old\n@@ -1 +0,0 @@\n-gone line".to_owned()
                    }
                    _ => format!("--- a/{0}\n+++ b/{0}\n-old\n+new", file.path),
                },
                DiffTarget::Commit(hash) => format!(
                    "{hash}\0Ada Lovelace\0ada@example.org\x002026-11-12 20:03\0HEAD -> main, tag: v9\0Fix the thing\n\nBecause it was broken.\n\0\nM\0src/main.rs\0A\0docs/new.md\0"
                ),
                DiffTarget::CommitFile { path, .. } => {
                    format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-before\n+after in {path}")
                }
            })
        }
    }

    /// Desktop that opens nothing and records what it was asked to open.
    #[derive(Default)]
    pub struct FakeDesktop {
        pub files: Mutex<Vec<PathBuf>>,
        pub urls: Mutex<Vec<String>>,
        /// The size of every music passage handed to the player.
        pub music: Mutex<Vec<usize>>,
        /// Every WAV handed to the player, as it was at that moment.
        pub played: Mutex<Vec<(PathBuf, Vec<u8>)>>,
        /// Folder and configured command of every terminal asked for.
        pub terminals: Mutex<Vec<(PathBuf, String)>>,
        pub fail: bool,
    }

    impl Desktop for FakeDesktop {
        fn open_file(&self, file: &Path) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no opener"));
            }
            self.files.lock().unwrap().push(file.to_path_buf());
            Ok(())
        }
        fn play(&self, file: &Path) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no audio player found"));
            }
            let wav = std::fs::read(file)?;
            self.played.lock().unwrap().push((file.to_path_buf(), wav));
            Ok(())
        }
        fn play_wait(&self, file: &Path, stop: &std::sync::atomic::AtomicBool) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no audio player found"));
            }
            self.music.lock().unwrap().push(std::fs::read(file)?.len());
            // The passage "plays" until the music is stopped.
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(())
        }
        fn open_url(&self, url: &str) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no browser"));
            }
            self.urls.lock().unwrap().push(url.to_owned());
            Ok(())
        }
        fn open_terminal(&self, dir: &Path, configured: &str) -> io::Result<()> {
            if self.fail {
                return Err(io::Error::other("no terminal"));
            }
            let asked = (dir.to_path_buf(), configured.to_owned());
            self.terminals.lock().unwrap().push(asked);
            Ok(())
        }
    }

    pub fn app_with(backend: Arc<FakeBackend>, paths: Option<Paths>) -> App {
        app_on(backend, Arc::default(), paths)
    }

    /// An app on fakes. Its scratch folder is one no other test or app uses.
    pub fn app_on(
        backend: Arc<FakeBackend>,
        desktop: Arc<FakeDesktop>,
        paths: Option<Paths>,
    ) -> App {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut app = App::new(
            Ok(Config::default()),
            paths,
            History::default(),
            backend,
            desktop,
            Arc::new(|| {}),
        );
        app.scratch = scratch_dir().join(format!("test-{n}"));
        app
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
    fn removed_cards_leave_the_board_and_their_marks() {
        let (dir, paths) = repo_dirs(&["a", "b", "c"]);
        let mut app = app_with(Arc::default(), None);
        app.add_folder(dir.path());
        app.marked.insert(paths[0].clone());
        app.marked.insert(paths[1].clone());
        app.remove_cards(&paths[..1]);
        assert_eq!(app.cards.len(), 2);
        assert!(!app.marked.contains(&paths[0]));
        let marked = app.marked_repos();
        app.remove_cards(&marked);
        assert_eq!(app.config.repos, vec![paths[2].clone()]);
        assert!(app.marked.is_empty());
        // Files on disk are untouched.
        assert!(paths.iter().all(|p| p.exists()));
        // A removed repository comes back with its folder.
        app.add_folder(dir.path());
        assert_eq!(app.cards.len(), 3);
    }

    #[test]
    fn hidden_repositories_stay_away_until_they_are_unhidden() {
        let (dir, paths) = repo_dirs(&["a", "b", "c"]);
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.add_folder(dir.path());
        app.new_favorite(vec![paths[0].clone(), paths[1].clone()]);
        app.favorite_dialog.as_mut().unwrap().draft.name = "f".into();
        app.save_favorite();
        app.marked.insert(paths[0].clone());

        app.hide_cards(&paths[..2]);
        app.hide_cards(&paths[..1]);
        assert_eq!(app.config.hidden, paths[..2].to_vec(), "each once");
        assert_eq!(app.config.repos, vec![paths[2].clone()]);
        assert!(app.marked.is_empty());
        assert_eq!(
            app.config.favorites[0].repos.len(),
            2,
            "favorites keep them"
        );

        // The parent folder again: the hidden ones are left out, and said so.
        app.toasts.clear();
        app.add_folder(dir.path());
        assert_eq!(app.cards.len(), 1);
        let texts: Vec<_> = app.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "1 already on board",
                "2 hidden, left out (Settings unhides)"
            ]
        );

        // Nothing runs on a hidden repository: no card, so no poll or fetch.
        let fetch_interval = Duration::from_secs(app.config.effective_fetch_interval_secs());
        app.tick(Instant::now() + fetch_interval + Duration::from_secs(1));
        settle(&mut app, |a| a.cards[0].probe.is_some());
        assert!(backend.runs.lock().unwrap().iter().all(|r| r.0 == paths[2]));

        // Picking a hidden repository itself brings it back.
        app.add_folder(&paths[0]);
        assert_eq!(app.config.hidden, vec![paths[1].clone()]);
        assert_eq!(app.cards.len(), 2);

        app.unhide(&paths[1]);
        app.unhide(Path::new("/not/hidden"));
        assert!(app.config.hidden.is_empty());
        let on_board: Vec<_> = app.cards.iter().map(|c| c.path.clone()).collect();
        assert_eq!(on_board, paths, "in board order");
        assert_eq!(app.config.repos, paths);

        app.hide_cards(&paths);
        assert!(app.cards.is_empty());
        app.unhide_all();
        assert_eq!(app.config.repos, paths);
        assert!(app.config.hidden.is_empty());
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
        let ahead = Upstream::Tracking {
            ahead: 1,
            behind: 0,
        };
        for (name, snap) in [
            ("ahead", snapshot(Counts::default(), ahead, &[])),
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
        app.mark_ahead();
        assert_eq!(app.marked_repos(), vec![PathBuf::from("/ahead")]);
        app.mark_all();
        assert_eq!(app.marked.len(), 4);
        app.mark_none();
        assert!(app.marked.is_empty());
    }

    #[test]
    fn cards_hidden_by_the_filter_keep_their_mark_but_nothing_touches_them() {
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = ["/x/etl-load", "/x/etl-store", "/x/web"]
            .into_iter()
            .map(|p| Card::new(p.into()))
            .collect();
        app.mark_all();
        app.filter = "ETL".into();
        let etl = vec![PathBuf::from("/x/etl-load"), PathBuf::from("/x/etl-store")];
        assert_eq!(app.marked_repos(), etl);

        // Command line and toolbar buttons run in the shown cards only.
        app.run_cli_line("status");
        app.run_button_on_marked(2);
        settle(&mut app, |_| backend.runs.lock().unwrap().len() == 4);
        let ran: BTreeSet<PathBuf> = backend
            .runs
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.0.clone())
            .collect();
        assert_eq!(ran, etl.iter().cloned().collect());

        // Marking touches the shown cards only: the hidden mark stays.
        app.mark_none();
        assert_eq!(app.marked, BTreeSet::from([PathBuf::from("/x/web")]));
        app.marked.clear();
        app.mark_all();
        assert_eq!(app.marked, etl.iter().cloned().collect());

        // Removing the marked cards leaves a marked card the filter hides.
        app.marked.insert("/x/web".into());
        let marked = app.marked_repos();
        app.remove_cards(&marked);
        assert_eq!(app.config.repos, vec![PathBuf::from("/x/web")]);
        app.filter.clear();
        assert_eq!(app.marked_repos(), vec![PathBuf::from("/x/web")]);
    }

    #[test]
    fn output_opens_as_a_text_file_that_goes_away_with_the_app() {
        let backend = Arc::new(FakeBackend::default());
        let desktop = Arc::new(FakeDesktop::default());
        let mut app = app_on(backend.clone(), desktop.clone(), None);
        app.cards = vec![Card::new("/x/a".into()), Card::new("/x/b".into())];
        app.start_run("status", vec!["/x/a".into(), "/x/b".into()]);
        // Not while it runs: the gate of the fake is open, so wait for it.
        settle(&mut app, |a| a.log.last().unwrap().outcome().is_some());
        app.open_output(0, None);
        app.open_output(0, Some(Path::new("/x/b")));
        app.open_output(7, None);
        let files = desktop.files.lock().unwrap().clone();
        assert_eq!(
            files,
            vec![
                app.scratch.join("output-0.txt"),
                app.scratch.join("output-0-b.txt")
            ]
        );
        assert_eq!(
            std::fs::read_to_string(&files[0]).unwrap(),
            "git status\n\n== /x/a (exit 0) ==\nran status\n\n== /x/b (exit 0) ==\nran status\n"
        );
        assert_eq!(
            std::fs::read_to_string(&files[1]).unwrap(),
            "git status\n\n== /x/b (exit 0) ==\nran status\n"
        );
        assert!(app.toasts.is_empty());
        let scratch = app.scratch.clone();
        drop(app);
        assert!(!scratch.exists());
    }

    #[test]
    fn a_failing_opener_is_reported() {
        let desktop = Arc::new(FakeDesktop {
            fail: true,
            ..FakeDesktop::default()
        });
        let mut app = app_on(Arc::default(), desktop, None);
        app.start_run("status", vec!["/x/a".into()]);
        settle(&mut app, |a| a.log.last().unwrap().outcome().is_some());
        app.open_output(0, None);
        assert_eq!(
            app.toasts.last().unwrap().text,
            "Cannot open the output: no opener"
        );
        app.open_url("https://example.org");
        assert_eq!(
            app.toasts.last().unwrap().text,
            "Cannot open https://example.org: no browser"
        );
        app.open_terminal(Path::new("/x/a"));
        assert_eq!(
            app.toasts.last().unwrap().text,
            "Cannot open a terminal: no terminal"
        );
    }

    #[test]
    fn favorites_group_cards_and_the_active_one_hides_the_rest() {
        let mut app = app_with(Arc::default(), None);
        app.cards = ["/x/etl-load", "/x/etl-store", "/x/web"]
            .into_iter()
            .map(|p| Card::new(p.into()))
            .collect();
        let etl = vec![PathBuf::from("/x/etl-load"), PathBuf::from("/x/etl-store")];

        // An unmarked card is the target alone; a marked one brings the marked.
        assert_eq!(
            app.menu_targets(Path::new("/x/web")),
            vec![PathBuf::from("/x/web")]
        );
        app.marked.extend(etl.iter().cloned());
        assert_eq!(
            app.menu_targets(Path::new("/x/web")),
            vec![PathBuf::from("/x/web")]
        );
        assert_eq!(app.menu_targets(Path::new("/x/etl-load")), etl);

        app.new_favorite(etl.clone());
        app.save_favorite();
        assert!(app.favorite_dialog.is_some(), "no name: the dialog stays");
        app.favorite_dialog.as_mut().unwrap().draft.name = "ETL".into();
        app.save_favorite();
        assert!(app.favorite_dialog.is_none());
        assert_eq!(app.config.favorites[0].repos, etl);

        app.new_favorite(vec![]);
        app.favorite_dialog.as_mut().unwrap().draft.name = "Front".into();
        app.save_favorite();
        app.add_to_favorite(1, &[PathBuf::from("/x/web"), PathBuf::from("/x/etl-load")]);
        app.add_to_favorite(9, &etl);

        app.toggle_favorite(1);
        let shown = |app: &App| -> Vec<PathBuf> {
            app.shown_cards().iter().map(|c| c.path.clone()).collect()
        };
        assert_eq!(
            shown(&app),
            vec![PathBuf::from("/x/etl-load"), PathBuf::from("/x/web")]
        );
        // The hidden marked card is not run in; the text filter narrows further.
        assert_eq!(app.marked_repos(), vec![PathBuf::from("/x/etl-load")]);
        app.filter = "web".into();
        assert_eq!(shown(&app), vec![PathBuf::from("/x/web")]);
        app.filter.clear();
        app.toggle_favorite(1);
        assert_eq!(shown(&app).len(), 3);

        app.edit_favorite(1);
        app.edit_favorite(9);
        let dialog = app.favorite_dialog.as_mut().unwrap();
        assert_eq!(
            (dialog.editing, dialog.draft.name.as_str()),
            (Some(1), "Front")
        );
        dialog.draft.color = "pink".into();
        app.save_favorite();
        assert_eq!(app.config.favorites[1].color, "pink");
        assert_eq!(
            app.config.favorites[1].repos.len(),
            2,
            "the repositories stay"
        );

        app.remove_from_favorite(1, &[PathBuf::from("/x/web")]);
        app.remove_from_favorite(9, &etl);
        assert_eq!(
            app.config.favorites[1].repos,
            vec![PathBuf::from("/x/etl-load")]
        );

        // A card that leaves the board leaves its favorites.
        app.remove_cards(&[PathBuf::from("/x/etl-load")]);
        assert_eq!(
            app.config.favorites[0].repos,
            vec![PathBuf::from("/x/etl-store")]
        );
        assert!(app.config.favorites[1].repos.is_empty());
        app.remove_cards(&[PathBuf::from("/x/etl-store")]);
        assert!(app.config.favorites[0].repos.is_empty());

        // Deleting keeps the active favorite the same one.
        app.toggle_favorite(1);
        app.delete_favorite(0);
        app.delete_favorite(9);
        assert_eq!(app.active_favorite, Some(0));
        assert_eq!(app.config.favorites[0].name, "Front");
        app.delete_favorite(0);
        assert_eq!(app.active_favorite, None);
        assert_eq!(shown(&app), vec![PathBuf::from("/x/web")]);
    }

    #[test]
    fn kawaii_moves_cards_only_in_its_theme_and_only_with_focus() {
        let mut app = app_with(Arc::default(), None);
        let path = PathBuf::from("/x/a");
        // Other themes: no clock, no motion, no swing on a mark.
        assert!(!app.advance_kawaii(0.05, true));
        app.toggle_mark(path.clone());
        assert_eq!(app.card_offset(&path, 1), (0.0, 0.0));
        assert_eq!(app.kawaii_clock, 0.0);
        app.toggle_mark(path.clone());
        assert!(app.marked.is_empty());

        app.config.theme = Theme::Kawaii;
        assert!(app.advance_kawaii(0.05, true));
        app.toggle_mark(path.clone());
        assert!(app.advance_kawaii(0.05, true));
        let (swing, bob) = app.card_offset(&path, 1);
        assert!(swing != 0.0 && bob != 0.0, "{swing} {bob}");
        assert_eq!(
            app.card_offset(Path::new("/x/b"), 1).0,
            0.0,
            "only the marked card swings"
        );

        // Without focus the clock stands still: the same picture every frame.
        let frozen = (app.kawaii_clock, app.card_offset(&path, 1));
        for _ in 0..5 {
            assert!(!app.advance_kawaii(0.05, false));
        }
        assert_eq!((app.kawaii_clock, app.card_offset(&path, 1)), frozen);

        // The swing ends; taking the mark off starts none.
        for _ in 0..20 {
            app.advance_kawaii(0.05, true);
        }
        assert_eq!(app.card_offset(&path, 1).0, 0.0);
        app.toggle_mark(path.clone());
        assert_eq!(app.card_offset(&path, 1).0, 0.0);
    }

    #[test]
    fn a_change_on_a_card_wakes_the_kawaii_theme_without_focus() {
        let mut app = app_with(Arc::default(), None);
        app.config.theme = Theme::Kawaii;
        app.cards = vec![Card::new("/x/a".into())];
        let clean = snapshot(Counts::default(), Upstream::None, &["main"]);
        let dirty = snapshot(
            Counts {
                modified: 1,
                ..Counts::default()
            },
            Upstream::None,
            &["main"],
        );
        let probed = |snap: &Snapshot| Event::Probed {
            repo: "/x/a".into(),
            result: Box::new(Ok(snap.clone())),
            cause: ProbeCause::Poll,
        };
        // The first probe and an unchanged one wake nothing.
        app.handle_event(probed(&clean));
        app.handle_event(probed(&clean));
        assert!(!app.advance_kawaii(0.033, false));

        // A change runs it for five seconds, then it stands still again.
        app.handle_event(probed(&dirty));
        let mut frames = 0;
        while app.advance_kawaii(0.033, false) {
            frames += 1;
        }
        assert!((150..=154).contains(&frames), "{frames}");
        let clock = app.kawaii_clock;
        assert!(clock > 4.9);
        assert!(!app.advance_kawaii(0.033, false));
        assert_eq!(app.kawaii_clock, clock);

        // So does a fetch error that appears; one that goes away does not.
        let fetched = |error: Option<&str>| Event::Fetched {
            repo: "/x/a".into(),
            error: error.map(|e| FetchError::from_message(e.into())),
        };
        app.handle_event(fetched(Some("auth failed")));
        assert!(app.advance_kawaii(0.033, false));
        // Focus uses the rest up: no burst left over when it is lost again.
        assert!(app.advance_kawaii(0.033, true));
        assert!(!app.advance_kawaii(0.033, false));
        app.handle_event(fetched(Some("auth failed")));
        app.handle_event(fetched(None));
        assert!(!app.advance_kawaii(0.033, false));

        // Other themes are not woken.
        app.config.theme = Theme::Dark;
        app.handle_event(probed(&clean));
        assert!(!app.advance_kawaii(0.033, false));
        app.config.theme = Theme::Kawaii;
        assert!(!app.advance_kawaii(0.033, false));
    }

    #[test]
    fn repository_view_loads_its_lists_follows_the_card_and_keeps_a_held_selection() {
        use crate::domain::details::{ChangeKind, FileChange, parse_log};
        let backend = Arc::new(FakeBackend::default());
        let mut app = app_with(backend.clone(), None);
        app.cards = vec![Card::new("/x/a".into()), Card::new("/x/b".into())];
        let file = |path: &str| FileChange {
            path: path.into(),
            kind: ChangeKind::Modified,
        };
        let lists = |files: &[&str]| Details {
            files: files.iter().map(|f| file(f)).collect(),
            commits: parse_log("aaaa\taa\tAda\t1\tthen\tone\n"),
            branches: vec![],
        };
        let view = |app: &App| app.repo_view.clone().unwrap();
        let read = || *backend.details_read.lock().unwrap();

        // A repository that cannot be read shows the error.
        app.open_repo_view(Path::new("/x/a"));
        assert_eq!(view(&app).details, None);
        settle(&mut app, |a| view(a).details.is_some());
        assert_eq!(view(&app).details, Some(Err("not a git repository".into())));

        *backend.details.lock().unwrap() = Some(lists(&["one.rs", "two.rs"]));
        app.refresh_repo_view();
        settle(&mut app, |a| matches!(view(a).details, Some(Ok(_))));

        let target = DiffTarget::File(file("two.rs"));
        app.select_in_repo_view(target.clone());
        assert_eq!(view(&app).diff, None);
        settle(&mut app, |a| view(a).diff.is_some());
        assert!(view(&app).diff.unwrap().unwrap().contains("+++ b/two.rs"));
        // A diff nobody asks for any more is dropped.
        app.handle_event(Event::Diff {
            repo: "/x/a".into(),
            target: DiffTarget::Commit("aaaa".into()),
            result: Ok("stale".into()),
        });
        app.handle_event(Event::Diff {
            repo: "/x/b".into(),
            target: target.clone(),
            result: Ok("other repository".into()),
        });
        assert!(view(&app).diff.unwrap().unwrap().contains("two.rs"));

        // The card of the open repository changes: the lists are read again
        // and the selection, still in them, stays with its diff.
        let probed = |repo: &str, modified| Event::Probed {
            repo: repo.into(),
            result: Box::new(Ok(snapshot(
                Counts {
                    modified,
                    ..Counts::default()
                },
                Upstream::None,
                &["main"],
            ))),
            cause: ProbeCause::Poll,
        };
        for repo in ["/x/a", "/x/b"] {
            app.handle_event(probed(repo, 0));
        }
        let before = read();
        app.handle_event(probed("/x/b", 1));
        assert_eq!(read(), before, "another card's change reads nothing");
        app.handle_event(probed("/x/a", 1));
        settle(&mut app, |_| read() == before + 1);
        app.handle_events();
        assert_eq!(view(&app).selected, Some(target));
        assert!(
            view(&app).diff.is_some(),
            "the old diff stays until the new one is in"
        );

        // The selected file is gone after a reload: so is the selection.
        *backend.details.lock().unwrap() = Some(lists(&["one.rs"]));
        app.refresh_repo_view();
        settle(&mut app, |a| view(a).selected.is_none());
        assert_eq!(view(&app).diff, None);

        // Tabs keep what was picked in them; Back and Forward step
        // through tabs and picks.
        assert_eq!(
            view(&app).tab,
            RepoTab::Commits,
            "no changes known: history"
        );
        let commit = DiffTarget::Commit("aaaa".into());
        let one = DiffTarget::File(file("one.rs"));
        app.select_in_repo_view(commit.clone());
        app.switch_repo_tab(RepoTab::Files);
        assert_eq!(
            (view(&app).tab, view(&app).selected),
            (RepoTab::Files, None)
        );
        app.switch_repo_tab(RepoTab::Files);
        app.select_in_repo_view(one.clone());
        app.switch_repo_tab(RepoTab::Commits);
        assert_eq!(view(&app).selected, Some(commit.clone()), "kept in its tab");
        assert_eq!(view(&app).diff, None);
        settle(&mut app, |a| view(a).diff.is_some());
        app.switch_repo_tab(RepoTab::Files);
        assert_eq!(view(&app).selected, Some(one.clone()));
        // Commits+aaaa, Files, Files+one, Commits+aaaa, Files+one.
        assert!(!view(&app).trail.can_forward());
        app.repo_view_back();
        assert_eq!(
            (view(&app).tab, view(&app).selected),
            (RepoTab::Commits, Some(commit.clone()))
        );
        app.repo_view_back();
        assert_eq!(view(&app).selected, Some(one.clone()));
        app.repo_view_back();
        assert_eq!(
            (view(&app).tab, view(&app).selected),
            (RepoTab::Files, None)
        );
        app.repo_view_forward();
        assert_eq!(view(&app).selected, Some(one.clone()));
        // A place whose pick is gone shows its tab without a pick.
        *backend.details.lock().unwrap() = Some(lists(&[]));
        app.refresh_repo_view();
        settle(&mut app, |a| view(a).selected.is_none());
        app.repo_view_back();
        app.repo_view_forward();
        assert_eq!(
            (view(&app).tab, view(&app).selected),
            (RepoTab::Files, None)
        );
        for _ in 0..9 {
            app.repo_view_back();
        }
        assert_eq!(view(&app).tab, RepoTab::Commits);
        assert!(!view(&app).trail.can_back());
        app.repo_view_forward();

        // Folders of the tree fold and unfold.
        app.toggle_repo_folder("src/");
        assert!(view(&app).closed.contains("src/"));
        app.toggle_repo_folder("src/");
        assert!(view(&app).closed.is_empty());

        // A repository with changes opens on them.
        app.handle_event(probed("/x/b", 2));
        app.open_repo_view(Path::new("/x/b"));
        assert_eq!(view(&app).tab, RepoTab::Files);
        app.open_repo_view(Path::new("/x/a"));
        settle(&mut app, |a| view(a).details.is_some());

        // Lists of a repository whose view is not open are dropped.
        app.handle_event(Event::Details {
            repo: "/x/b".into(),
            result: Err("other".into()),
        });
        assert!(matches!(view(&app).details, Some(Ok(_))));
        app.close_repo_view();
        app.refresh_repo_view();
        app.select_in_repo_view(DiffTarget::Commit("aaaa".into()));
        app.switch_repo_tab(RepoTab::Graph);
        app.toggle_repo_folder("src/");
        app.repo_view_back();
        app.repo_view_forward();
        app.handle_event(Event::Details {
            repo: "/x/a".into(),
            result: Err("closed".into()),
        });
        assert!(app.repo_view.is_none());
    }

    #[test]
    fn commits_come_in_pages_and_a_commit_unfolds_its_files() {
        let backend = Arc::new(FakeBackend::default());
        *backend.details.lock().unwrap() = Some(Details::default());
        *backend.log_length.lock().unwrap() = 230;
        let mut app = app_with(backend.clone(), None);
        let view = |app: &App| app.repo_view.clone().unwrap();
        let commits = |app: &App| view(app).details.unwrap().unwrap().commits.len();
        app.load_more_commits();
        app.toggle_commit_file("a.rs");
        app.open_repo_view(Path::new("/x/a"));
        app.load_more_commits();
        assert!(!view(&app).loading_commits, "nothing to add to yet");
        settle(&mut app, |a| view(a).details.is_some());
        assert_eq!(commits(&app), 100);
        assert!(view(&app).more_commits);

        // The next page is asked for once, however often the end shows.
        app.load_more_commits();
        app.load_more_commits();
        assert!(view(&app).loading_commits);
        settle(&mut app, |a| !view(a).loading_commits);
        assert_eq!(commits(&app), 200);
        // A page that does not follow the list is dropped.
        app.handle_event(Event::Log {
            repo: "/x/a".into(),
            skip: 100,
            result: Ok(vec![fake_commit(1)]),
            more: true,
        });
        app.handle_event(Event::Log {
            repo: "/x/other".into(),
            skip: 200,
            result: Ok(vec![fake_commit(1)]),
            more: true,
        });
        assert_eq!(commits(&app), 200);
        app.load_more_commits();
        settle(&mut app, |a| !view(a).loading_commits);
        assert_eq!(commits(&app), 230);
        assert!(!view(&app).more_commits, "a short page ends the history");
        app.load_more_commits();
        assert!(!view(&app).loading_commits);
        // A reload reads as many as the list held.
        app.refresh_repo_view();
        let read = *backend.details_read.lock().unwrap();
        settle(&mut app, |_| *backend.details_read.lock().unwrap() > read);
        app.handle_events();
        assert_eq!(commits(&app), 230);
        assert!(!view(&app).more_commits);

        // A file of the picked commit unfolds with its diff and folds.
        let hash = fake_commit(3).hash;
        app.toggle_commit_file("src/main.rs");
        assert!(view(&app).unfolded.is_empty(), "no commit picked");
        app.select_in_repo_view(DiffTarget::Commit(hash.clone()));
        settle(&mut app, |a| view(a).diff.is_some());
        app.toggle_commit_file("src/main.rs");
        assert_eq!(view(&app).unfolded.get("src/main.rs"), Some(&None));
        settle(&mut app, |a| view(a).unfolded["src/main.rs"].is_some());
        let diff = view(&app).unfolded["src/main.rs"].clone().unwrap().unwrap();
        assert!(diff.contains("+after in src/main.rs"), "{diff}");
        assert!(view(&app).diff.unwrap().unwrap().contains("Ada Lovelace"));
        // The diff of a file of another commit, or of a folded file, is
        // dropped.
        for (hash, path) in [("ffff", "src/main.rs"), (hash.as_str(), "other.rs")] {
            app.handle_event(Event::Diff {
                repo: "/x/a".into(),
                target: DiffTarget::CommitFile {
                    hash: hash.into(),
                    path: path.into(),
                },
                result: Ok("stale".into()),
            });
        }
        assert_eq!(view(&app).unfolded.len(), 1);
        assert!(
            !view(&app).unfolded["src/main.rs"]
                .clone()
                .unwrap()
                .unwrap()
                .contains("stale")
        );
        app.toggle_commit_file("src/main.rs");
        assert!(view(&app).unfolded.is_empty());
        // Another pick folds everything.
        app.toggle_commit_file("docs/new.md");
        app.select_in_repo_view(DiffTarget::Commit(fake_commit(4).hash));
        assert!(view(&app).unfolded.is_empty());

        // A page that cannot be read ends the list with a word about it.
        *backend.log_length.lock().unwrap() = 100;
        app.open_repo_view(Path::new("/x/a"));
        settle(&mut app, |a| view(a).details.is_some());
        *backend.log_length.lock().unwrap() = usize::MAX;
        app.load_more_commits();
        settle(&mut app, |a| !view(a).loading_commits);
        assert!(!view(&app).more_commits);
        assert_eq!(app.toasts.len(), 1);
        assert!(app.toasts[0].text.contains("history unreadable"));
    }

    #[test]
    fn the_graph_is_read_for_its_tab_and_keeps_its_pick_while_it_holds_it() {
        use crate::domain::graph::GraphCommit;
        let backend = Arc::new(FakeBackend::default());
        *backend.details.lock().unwrap() = Some(Details::default());
        let commit = |hash: &str| GraphCommit {
            hash: hash.into(),
            parents: vec![],
            author: String::new(),
            subject: String::new(),
        };
        let mut app = app_with(backend.clone(), None);
        let view = |app: &App| app.repo_view.clone().unwrap();
        let read = || *backend.graphs_read.lock().unwrap();
        app.unfold_graph_run("aa");
        app.show_old_branches();
        app.open_repo_view(Path::new("/x/a"));
        settle(&mut app, |a| view(a).details.is_some());
        app.refresh_repo_view();
        settle(&mut app, |_| *backend.details_read.lock().unwrap() == 2);
        assert_eq!(read(), 0, "nobody looked at the graph yet");

        // The tab reads the graph, once; an unreadable one shows why.
        app.switch_repo_tab(RepoTab::Graph);
        assert_eq!(view(&app).graph, None);
        settle(&mut app, |a| view(a).graph.is_some());
        assert_eq!(view(&app).graph, Some(Err("no graph".into())));
        *backend.graph.lock().unwrap() = Some(GraphData {
            commits: vec![commit("aa"), commit("bb")],
            ..GraphData::default()
        });
        app.switch_repo_tab(RepoTab::Commits);
        app.switch_repo_tab(RepoTab::Graph);
        assert_eq!(read(), 1);
        app.refresh_repo_view();
        settle(&mut app, |a| matches!(view(a).graph, Some(Ok(_))));
        assert_eq!(read(), 2);

        // A commit only the graph has can be picked and outlives a reload.
        let target = DiffTarget::Commit("bb".into());
        app.select_in_repo_view(target.clone());
        app.refresh_repo_view();
        settle(&mut app, |_| read() == 3);
        app.handle_events();
        assert_eq!(view(&app).selected, Some(target.clone()));
        app.unfold_graph_run("aa");
        assert!(view(&app).graph_open.contains("aa"));

        // The old remote branches are asked for once.
        app.show_old_branches();
        app.show_old_branches();
        assert!(view(&app).with_old);
        settle(
            &mut app,
            |a| matches!(&view(a).graph, Some(Ok(graph)) if graph.refs.len() == 1),
        );
        assert_eq!(read(), 4);

        // The pick goes when its commit left the graph, in whatever tab.
        app.switch_repo_tab(RepoTab::Files);
        *backend.graph.lock().unwrap() = Some(GraphData {
            commits: vec![commit("aa")],
            ..GraphData::default()
        });
        app.refresh_repo_view();
        settle(&mut app, |_| read() == 5);
        app.handle_events();
        app.switch_repo_tab(RepoTab::Graph);
        assert_eq!(view(&app).selected, None);
        // A graph of a repository whose view is not open is dropped.
        app.handle_event(Event::Graph {
            repo: "/x/other".into(),
            result: Err("other".into()),
        });
        assert!(matches!(view(&app).graph, Some(Ok(_))));
    }

    /// An app with sounds on, one card, and what its fake player was given.
    fn sound_app(fail: bool) -> (App, Arc<FakeDesktop>) {
        let desktop = Arc::new(FakeDesktop {
            fail,
            ..FakeDesktop::default()
        });
        let mut app = app_on(Arc::default(), desktop.clone(), None);
        app.config.sounds = true;
        app.cards = vec![Card::new("/x/a".into()), Card::new("/x/b".into())];
        (app, desktop)
    }

    fn counts_event(repo: &str, ahead: u32, behind: u32, cause: ProbeCause) -> Event {
        let upstream = Upstream::Tracking { ahead, behind };
        Event::Probed {
            repo: repo.into(),
            result: Box::new(Ok(snapshot(Counts::default(), upstream, &["main"]))),
            cause,
        }
    }

    #[test]
    fn new_commits_beep_and_boop_once_per_round_and_not_for_own_commands() {
        let (mut app, desktop) = sound_app(false);
        let played = || -> Vec<String> {
            let played = desktop.played.lock().unwrap();
            played.iter().map(|p| title(&p.0)).collect()
        };
        // First probes: no news.
        for repo in ["/x/a", "/x/b"] {
            app.handle_event(counts_event(repo, 0, 0, ProbeCause::Poll));
        }
        assert!(played().is_empty());

        // A background fetch brings remote commits to both cards: one beep.
        app.handle_event(counts_event("/x/a", 0, 2, ProbeCause::Fetch));
        app.handle_event(counts_event("/x/b", 0, 1, ProbeCause::Fetch));
        assert_eq!(played(), ["beep.wav"]);
        // A local commit made elsewhere: the boop has its own quiet time.
        app.handle_event(counts_event("/x/a", 1, 2, ProbeCause::Poll));
        assert_eq!(played(), ["beep.wav", "boop.wav"]);
        // The user's own command changed the counts: silent.
        app.handle_event(counts_event("/x/b", 3, 4, ProbeCause::Command));
        // Counts going down: silent.
        app.handle_event(counts_event("/x/a", 0, 0, ProbeCause::Poll));
        assert_eq!(played().len(), 2);

        // After the quiet time the same sound plays again.
        let later = Instant::now() + Duration::from_secs_f64(audio::QUIET_SECS + 0.1);
        app.sound_news(&[Sound::Beep], later);
        app.sound_news(&[Sound::Beep], later + Duration::from_secs(1));
        assert_eq!(played(), ["beep.wav", "boop.wav", "beep.wav"]);

        // Switched off: nothing.
        app.config.sounds = false;
        app.sound_news(&[Sound::Boop], later + Duration::from_secs(60));
        assert_eq!(played().len(), 3);
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn sound_file_follows_volume_and_the_users_file() {
        let (mut app, desktop) = sound_app(false);
        let last = || desktop.played.lock().unwrap().last().unwrap().1.clone();
        app.play_sound(Sound::Beep);
        let default_wav = last();
        assert_eq!(
            default_wav,
            audio::wav_bytes(&Sound::Beep.default_samples(), SAMPLE_RATE, 60)
        );
        app.config.sound_volume = 10;
        app.play_sound(Sound::Beep);
        assert_ne!(last(), default_wav, "made again at the new volume");

        // The user's own file: decoded and written as WAV at the volume.
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("own.wav");
        std::fs::write(&own, audio::wav_bytes(&[0.5; 400], 8000, 100)).unwrap();
        app.config.set_sound_file(Sound::Beep, Some(own));
        app.config.sound_volume = 100;
        app.play_sound(Sound::Beep);
        assert_eq!(last().len(), 44 + 800);
        assert_eq!(u32::from_le_bytes(last()[24..28].try_into().unwrap()), 8000);

        // A file that is no sound: said once, and the app's tone plays.
        let bad = dir.path().join("bad.mp3");
        std::fs::write(&bad, "text").unwrap();
        app.config.set_sound_file(Sound::Boop, Some(bad));
        app.play_sound(Sound::Boop);
        app.play_sound(Sound::Boop);
        assert_eq!(
            last(),
            audio::wav_bytes(&Sound::Boop.default_samples(), SAMPLE_RATE, 100)
        );
        let toasts: Vec<_> = app.toasts.iter().map(|t| t.text.clone()).collect();
        assert_eq!(toasts.len(), 1, "{toasts:?}");
        assert!(toasts[0].starts_with("Cannot read bad.mp3:"), "{toasts:?}");
    }

    #[test]
    fn a_missing_player_is_said_once_and_the_picker_sets_the_file() {
        let (mut app, _desktop) = sound_app(true);
        app.play_sound(Sound::Beep);
        app.play_sound(Sound::Boop);
        let toasts: Vec<_> = app.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(toasts, ["Cannot play sounds: no audio player found"]);

        app.open_sound_picker(Sound::Boop, || Some(PathBuf::from("/s/boop.ogg")));
        app.open_sound_picker(Sound::Beep, || panic!("second dialog opened"));
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.config.boop_file.is_none() {
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(5));
            app.take_picked_sound();
        }
        assert_eq!(app.config.boop_file, Some(PathBuf::from("/s/boop.ogg")));
        assert_eq!(app.config.beep_file, None);
        // A cancelled dialog changes nothing.
        app.open_sound_picker(Sound::Boop, || None);
        std::thread::sleep(Duration::from_millis(50));
        app.take_picked_sound();
        app.take_picked_sound();
        assert_eq!(app.config.boop_file, Some(PathBuf::from("/s/boop.ogg")));
    }

    #[test]
    fn music_plays_in_the_kawaii_theme_with_music_on_and_nowhere_else() {
        let desktop = Arc::new(FakeDesktop::default());
        let mut app = app_on(Arc::default(), desktop.clone(), None);
        let wait_for_passage = |n: usize| {
            let deadline = Instant::now() + Duration::from_secs(60);
            while desktop.music.lock().unwrap().len() < n {
                assert!(Instant::now() < deadline, "no music");
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        app.config.music = true;
        app.sync_music();
        assert!(!app.music_plays(), "not in another theme");
        app.config.theme = Theme::Kawaii;
        app.config.music = false;
        app.sync_music();
        assert!(!app.music_plays(), "not while switched off");

        app.config.music = true;
        app.sync_music();
        app.sync_music();
        assert!(app.music_plays());
        wait_for_passage(1);
        // A slider that is still dragged changes nothing yet; let go, the
        // passage goes on at the new volume in a player of its own.
        app.config.music_volume = 5;
        app.pointer_held = true;
        app.sync_music();
        assert_eq!(app.music_volume_playing(), Some(40));
        app.pointer_held = false;
        app.sync_music();
        assert_eq!(app.music_volume_playing(), Some(5));
        wait_for_passage(2);
        let passages = desktop.music.lock().unwrap().clone();
        assert!(passages[1] <= passages[0], "the rest: {passages:?}");

        // Another theme ends it; back in Kawaii it starts again.
        app.config.theme = Theme::Dark;
        app.sync_music();
        assert!(!app.music_plays());
        app.config.theme = Theme::Kawaii;
        app.sync_music();
        wait_for_passage(3);
        assert_eq!(
            desktop.music.lock().unwrap().len(),
            3,
            "one player at a time"
        );
        assert!(app.toasts.is_empty());
        // The app going away ends the player.
        drop(app);
    }

    #[test]
    fn music_without_a_player_is_said_once_and_not_tried_again() {
        let desktop = Arc::new(FakeDesktop {
            fail: true,
            ..FakeDesktop::default()
        });
        let mut app = app_on(Arc::default(), desktop, None);
        app.config.theme = Theme::Kawaii;
        app.config.music = true;
        app.sync_music();
        let deadline = Instant::now() + Duration::from_secs(60);
        while app.toasts.is_empty() {
            assert!(Instant::now() < deadline, "no error");
            std::thread::sleep(Duration::from_millis(5));
            app.sync_music();
        }
        for _ in 0..5 {
            app.sync_music();
        }
        assert!(!app.music_plays());
        let toasts: Vec<_> = app.toasts.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(toasts, ["Cannot play music: no audio player found"]);
        // Switched off and on again, it is tried again.
        app.config.music = false;
        app.sync_music();
        app.config.music = true;
        app.sync_music();
        assert!(app.music_plays());
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
            Arc::new(FakeDesktop::default()),
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
