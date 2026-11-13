//! Background execution: polls, fetches and user commands on worker threads.
//!
//! Rules:
//! - Polls and fetches share one concurrency budget, user commands another, so
//!   background work never starves commands.
//! - One process per repository at a time: a command waits for a running
//!   poll/fetch of its repo; polls/fetches are skipped while a command for the
//!   repo is queued or running, and while another poll/fetch is still busy.
//! - After a command, the repository is probed again right away.

use crate::application::semaphore::Semaphore;
use crate::domain::command::Job;
use crate::domain::details::{Details, DiffTarget, LOG_PAGE, LogEntry};
use crate::domain::graph::GraphData;
use crate::domain::output::{ProcessOutput, SectionState};
use crate::domain::repo::{FetchError, ProbeError, Snapshot};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::thread;

/// What the automatic background fetch does, as git arguments.
pub const FETCH_ARGS: &[&str] = &["fetch", "--all", "--prune"];

/// The git operations the engine needs (port; implemented over the system git).
pub trait Backend: Send + Sync + 'static {
    /// The state of the repository for its card. `fresh` says its branches
    /// or tags may have changed (a fetch or a command just ran), so nothing
    /// remembered from an earlier probe may be used.
    fn probe(&self, dir: &Path, fresh: bool) -> Result<Snapshot, ProbeError>;
    fn run(&self, dir: &Path, args: &[String]) -> io::Result<ProcessOutput>;
    /// The automatic background fetch ([`FETCH_ARGS`]). Nobody started it, so
    /// it must not run programs the repository's own config names.
    fn fetch(&self, dir: &Path) -> io::Result<ProcessOutput>;
    /// A `!` line through the user's shell, in `dir`.
    fn run_shell(&self, dir: &Path, line: &str) -> io::Result<ProcessOutput>;
    /// All subcommand names (incl. aliases) and the names of builtins.
    fn list_commands(&self, dir: &Path) -> (Vec<String>, Vec<String>);
    /// Options git reports for a builtin via `--git-completion-helper`.
    fn options(&self, dir: &Path, command: &str) -> Vec<String>;
    /// The lists of the repository view. Read-only, and like the automatic
    /// calls it must not run programs the repository's own config names.
    /// `commits` is how many commits the list should hold at least.
    fn details(&self, dir: &Path, commits: usize) -> Result<Details, String>;
    /// The next page of the commit list after the first `skip`; same rule.
    fn log(&self, dir: &Path, skip: usize) -> Result<Vec<LogEntry>, String>;
    /// The graph of all branches; `with_old` draws the old remote branches
    /// too. Same rule.
    fn graph(&self, dir: &Path, with_old: bool) -> Result<GraphData, String>;
    /// The diff of a file or commit for the repository view; same rule.
    fn diff(&self, dir: &Path, target: &DiffTarget) -> Result<String, String>;
}

/// What made a probe run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeCause {
    /// The regular refresh.
    Poll,
    /// A background fetch just ran.
    Fetch,
    /// A command the user ran in that repository just ended.
    Command,
}

/// Messages from worker threads to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Probed {
        repo: PathBuf,
        /// Boxed: a snapshot is far bigger than the other events.
        result: Box<Result<Snapshot, ProbeError>>,
        cause: ProbeCause,
    },
    /// A background fetch finished; `error` is set when it failed.
    Fetched {
        repo: PathBuf,
        error: Option<FetchError>,
    },
    Section {
        run: u64,
        repo: PathBuf,
        state: SectionState,
    },
    Commands {
        all: Vec<String>,
        builtins: Vec<String>,
    },
    Options {
        command: String,
        options: Vec<String>,
    },
    /// The lists of the repository view were read.
    Details {
        repo: PathBuf,
        result: Result<Details, String>,
    },
    /// A further page of the commit list was read: what follows the first
    /// `skip` commits, and whether the history goes on after it.
    Log {
        repo: PathBuf,
        skip: usize,
        result: Result<Vec<LogEntry>, String>,
        more: bool,
    },
    /// The graph of the repository view was read.
    Graph {
        repo: PathBuf,
        result: Result<GraphData, String>,
    },
    /// A diff for the repository view was read.
    Diff {
        repo: PathBuf,
        target: DiffTarget,
        result: Result<String, String>,
    },
}

/// Called after every event so the UI can repaint.
pub type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Slot {
    lock: Mutex<()>,
    pending_commands: AtomicUsize,
    background_busy: AtomicBool,
}

impl Slot {
    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn try_lock(&self) -> Option<MutexGuard<'_, ()>> {
        match self.lock.try_lock() {
            Ok(guard) => Some(guard),
            Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    fn commands_pending(&self) -> bool {
        self.pending_commands.load(Ordering::SeqCst) > 0
    }
}

#[derive(Clone)]
struct Shared {
    backend: Arc<dyn Backend>,
    tx: Sender<Event>,
    notify: Notify,
}

impl Shared {
    fn send(&self, event: Event) {
        // The receiver only disappears when the app shuts down.
        let _ = self.tx.send(event);
        (self.notify)();
    }
}

/// Owns the worker budgets and per-repository slots; lives on the UI thread.
pub struct Engine {
    shared: Shared,
    commands: Arc<Semaphore>,
    background: Arc<Semaphore>,
    slots: HashMap<PathBuf, Arc<Slot>>,
}

impl Engine {
    pub fn new(
        backend: Arc<dyn Backend>,
        concurrency: usize,
        notify: Notify,
    ) -> (Self, Receiver<Event>) {
        let (tx, rx) = channel();
        let engine = Self {
            shared: Shared {
                backend,
                tx,
                notify,
            },
            commands: Arc::new(Semaphore::new(concurrency)),
            background: Arc::new(Semaphore::new(concurrency)),
            slots: HashMap::new(),
        };
        (engine, rx)
    }

    /// Change both budgets; running jobs keep their permits.
    pub fn set_concurrency(&self, concurrency: usize) {
        self.commands.set_max(concurrency);
        self.background.set_max(concurrency);
    }

    fn slot(&mut self, repo: &Path) -> Arc<Slot> {
        self.slots.entry(repo.to_path_buf()).or_default().clone()
    }

    /// Whether a command for `repo` is queued or running.
    pub fn is_busy(&self, repo: &Path) -> bool {
        self.slots.get(repo).is_some_and(|s| s.commands_pending())
    }

    /// Probe `repo` in the background. Returns `false` when skipped.
    pub fn poll(&mut self, repo: &Path) -> bool {
        self.spawn_background(repo, false)
    }

    /// Fetch, then probe `repo` in the background. Returns `false` when skipped.
    pub fn fetch(&mut self, repo: &Path) -> bool {
        self.spawn_background(repo, true)
    }

    fn spawn_background(&mut self, repo: &Path, fetch: bool) -> bool {
        let slot = self.slot(repo);
        if slot.commands_pending() || slot.background_busy.swap(true, Ordering::SeqCst) {
            return false;
        }
        let shared = self.shared.clone();
        let budget = self.background.clone();
        let repo = repo.to_path_buf();
        thread::spawn(move || {
            {
                let _permit = budget.acquire();
                // A command may have been queued while this job waited for a permit.
                let guard = (!slot.commands_pending())
                    .then(|| slot.try_lock())
                    .flatten();
                if guard.is_some() {
                    if fetch {
                        let error = match shared.backend.fetch(&repo) {
                            Ok(out) if out.success() => None,
                            Ok(out) => Some(FetchError::from_output(&out.stdout, &out.stderr)),
                            Err(e) => Some(FetchError::from_message(e.to_string())),
                        };
                        shared.send(Event::Fetched {
                            repo: repo.clone(),
                            error,
                        });
                    }
                    let result = shared.backend.probe(&repo, fetch);
                    shared.send(Event::Probed {
                        repo: repo.clone(),
                        result: Box::new(result),
                        cause: if fetch {
                            ProbeCause::Fetch
                        } else {
                            ProbeCause::Poll
                        },
                    });
                }
            }
            slot.background_busy.store(false, Ordering::SeqCst);
        });
        true
    }

    /// Run `job` (git arguments or a shell line) in `repo` as part of output run `run`.
    pub fn run_command(&mut self, run: u64, repo: &Path, job: Job) {
        let slot = self.slot(repo);
        slot.pending_commands.fetch_add(1, Ordering::SeqCst);
        let shared = self.shared.clone();
        let budget = self.commands.clone();
        let repo = repo.to_path_buf();
        thread::spawn(move || {
            {
                // Repo lock first: waiting for it must not hold a command permit.
                let _guard = slot.lock();
                let _permit = budget.acquire();
                shared.send(Event::Section {
                    run,
                    repo: repo.clone(),
                    state: SectionState::Running,
                });
                let result = match &job {
                    Job::Git(args) => shared.backend.run(&repo, args),
                    Job::Shell(line) => shared.backend.run_shell(&repo, line),
                };
                let state = match result {
                    Ok(out) => SectionState::Done(out),
                    Err(e) => SectionState::SpawnFailed(e.to_string()),
                };
                shared.send(Event::Section {
                    run,
                    repo: repo.clone(),
                    state,
                });
                let result = shared.backend.probe(&repo, true);
                shared.send(Event::Probed {
                    repo: repo.clone(),
                    result: Box::new(result),
                    cause: ProbeCause::Command,
                });
            }
            slot.pending_commands.fetch_sub(1, Ordering::SeqCst);
            (shared.notify)();
        });
    }

    /// Load subcommand names in the background (sends [`Event::Commands`]).
    pub fn load_commands(&self, dir: PathBuf) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let (all, builtins) = shared.backend.list_commands(&dir);
            shared.send(Event::Commands { all, builtins });
        });
    }

    /// Read the lists of the repository view, with at least `commits`
    /// commits (sends [`Event::Details`]).
    pub fn load_details(&self, repo: PathBuf, commits: usize) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let result = shared.backend.details(&repo, commits);
            shared.send(Event::Details { repo, result });
        });
    }

    /// Read the page of the commit list after the first `skip` commits
    /// (sends [`Event::Log`]).
    pub fn load_log(&self, repo: PathBuf, skip: usize) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let result = shared.backend.log(&repo, skip);
            let more = result.as_ref().is_ok_and(|page| page.len() >= LOG_PAGE);
            shared.send(Event::Log {
                repo,
                skip,
                result,
                more,
            });
        });
    }

    /// Read the graph of the repository view (sends [`Event::Graph`]).
    pub fn load_graph(&self, repo: PathBuf, with_old: bool) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let result = shared.backend.graph(&repo, with_old);
            shared.send(Event::Graph { repo, result });
        });
    }

    /// Read a diff for the repository view (sends [`Event::Diff`]).
    pub fn load_diff(&self, repo: PathBuf, target: DiffTarget) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let result = shared.backend.diff(&repo, &target);
            shared.send(Event::Diff {
                repo,
                target,
                result,
            });
        });
    }

    /// Load git's option list for `command` (sends [`Event::Options`]).
    pub fn load_options(&self, dir: PathBuf, command: String) {
        let shared = self.shared.clone();
        thread::spawn(move || {
            let options = shared.backend.options(&dir, &command);
            shared.send(Event::Options { command, options });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status::{Counts, Head, Status, Upstream};
    use std::sync::Condvar;
    use std::time::Duration;

    /// Fake backend: records calls; `run` blocks while the gate is closed.
    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        /// The `fresh` flag of every probe.
        fresh: Mutex<Vec<bool>>,
        gate: Mutex<bool>,
        cv: Condvar,
        fail_fetch: bool,
    }

    impl Fake {
        fn open(&self) {
            *self.gate.lock().unwrap() = true;
            self.cv.notify_all();
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn snapshot() -> Snapshot {
        Snapshot::of(Status {
            head: Head::Branch("main".into()),
            upstream: Upstream::None,
            counts: Counts::default(),
            changed_paths: vec![],
        })
    }

    impl Backend for Fake {
        fn probe(&self, dir: &Path, fresh: bool) -> Result<Snapshot, ProbeError> {
            self.fresh.lock().unwrap().push(fresh);
            self.calls
                .lock()
                .unwrap()
                .push(format!("probe {}", dir.display()));
            Ok(snapshot())
        }
        fn run(&self, dir: &Path, args: &[String]) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("run {} {}", dir.display(), args.join(" ")));
            let mut open = self.gate.lock().unwrap();
            while !*open {
                open = self.cv.wait(open).unwrap();
            }
            let failed = self.fail_fetch && args.first().is_some_and(|a| a == "fetch");
            Ok(ProcessOutput {
                exit_code: Some(if failed { 128 } else { 0 }),
                stdout: "ok".into(),
                stderr: if failed {
                    "Permission denied (publickey).".into()
                } else {
                    String::new()
                },
            })
        }
        fn fetch(&self, dir: &Path) -> io::Result<ProcessOutput> {
            let args: Vec<String> = FETCH_ARGS.iter().map(|s| s.to_string()).collect();
            self.run(dir, &args)
        }
        fn run_shell(&self, dir: &Path, line: &str) -> io::Result<ProcessOutput> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("shell {} {line}", dir.display()));
            Ok(ProcessOutput {
                exit_code: Some(0),
                stdout: "ok".into(),
                stderr: String::new(),
            })
        }
        fn list_commands(&self, _dir: &Path) -> (Vec<String>, Vec<String>) {
            (vec!["pull".into()], vec!["pull".into()])
        }
        fn options(&self, _dir: &Path, command: &str) -> Vec<String> {
            vec![format!("--{command}-opt")]
        }
        fn details(&self, dir: &Path, commits: usize) -> Result<Details, String> {
            Err(format!("details {} {commits}", dir.display()))
        }
        fn log(&self, _dir: &Path, skip: usize) -> Result<Vec<LogEntry>, String> {
            let entry = |i: usize| LogEntry {
                hash: format!("{i:x}"),
                short: String::new(),
                author: String::new(),
                time: 0,
                date: String::new(),
                subject: String::new(),
            };
            // A history of 150 commits.
            Ok((skip..150.min(skip + LOG_PAGE)).map(entry).collect())
        }
        fn diff(&self, dir: &Path, target: &DiffTarget) -> Result<String, String> {
            Ok(format!("diff {} {target:?}", dir.display()))
        }
        fn graph(&self, dir: &Path, with_old: bool) -> Result<GraphData, String> {
            Err(format!("graph {} {with_old}", dir.display()))
        }
    }

    fn engine(fake: Arc<Fake>) -> (Engine, Receiver<Event>) {
        Engine::new(fake, 2, Arc::new(|| {}))
    }

    fn recv(rx: &Receiver<Event>) -> Event {
        rx.recv_timeout(Duration::from_secs(5)).expect("event")
    }

    #[test]
    fn poll_sends_snapshot() {
        let fake = Arc::new(Fake::default());
        let (mut engine, rx) = engine(fake.clone());
        assert!(engine.poll(Path::new("/r")));
        assert_eq!(
            recv(&rx),
            Event::Probed {
                repo: "/r".into(),
                result: Box::new(Ok(snapshot())),
                cause: ProbeCause::Poll
            }
        );
    }

    #[test]
    fn command_runs_then_reprobes_and_blocks_polls() {
        let fake = Arc::new(Fake::default());
        let (mut engine, rx) = engine(fake.clone());
        let repo = Path::new("/r");
        engine.run_command(7, repo, Job::Git(vec!["push".into()]));
        assert!(engine.is_busy(repo));
        assert_eq!(
            recv(&rx),
            Event::Section {
                run: 7,
                repo: repo.into(),
                state: SectionState::Running
            }
        );
        // While the command runs, background work for this repo is skipped.
        assert!(!engine.poll(repo));
        assert!(!engine.fetch(repo));
        fake.open();
        match recv(&rx) {
            Event::Section {
                state: SectionState::Done(out),
                ..
            } => assert!(out.success()),
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(recv(&rx), Event::Probed { .. }));
        assert_eq!(fake.calls(), vec!["run /r push", "probe /r"]);
    }

    #[test]
    fn shell_job_runs_through_the_shell_then_reprobes() {
        let fake = Arc::new(Fake::default());
        let (mut engine, rx) = engine(fake.clone());
        let repo = Path::new("/r");
        engine.run_command(3, repo, Job::Shell("ls | wc -l".into()));
        assert!(matches!(
            recv(&rx),
            Event::Section {
                state: SectionState::Running,
                ..
            }
        ));
        assert!(matches!(
            recv(&rx),
            Event::Section {
                run: 3,
                state: SectionState::Done(_),
                ..
            }
        ));
        assert!(matches!(recv(&rx), Event::Probed { .. }));
        assert_eq!(fake.calls(), vec!["shell /r ls | wc -l", "probe /r"]);
    }

    #[test]
    fn command_waits_for_running_fetch() {
        let fake = Arc::new(Fake::default());
        let (mut engine, rx) = engine(fake.clone());
        let repo = Path::new("/r");
        assert!(engine.fetch(repo));
        // Give the fetch time to take the repo lock and block in `run`.
        while fake.calls().is_empty() {
            thread::sleep(Duration::from_millis(5));
        }
        engine.run_command(1, repo, Job::Git(vec!["pull".into()]));
        thread::sleep(Duration::from_millis(50));
        assert_eq!(fake.calls(), vec!["run /r fetch --all --prune"]);
        fake.open();
        let mut events = Vec::new();
        while events.len() < 5 {
            events.push(recv(&rx));
        }
        assert_eq!(
            fake.calls(),
            vec![
                "run /r fetch --all --prune",
                "probe /r",
                "run /r pull",
                "probe /r"
            ]
        );
    }

    #[test]
    fn second_poll_skipped_while_first_busy() {
        let fake = Arc::new(Fake::default());
        let (mut engine, rx) = engine(fake.clone());
        assert!(engine.fetch(Path::new("/r")));
        assert!(!engine.poll(Path::new("/r")));
        assert!(engine.poll(Path::new("/other")));
        fake.open();
        let mut events = Vec::new();
        while events.len() < 3 {
            events.push(recv(&rx));
        }
        assert!(events.contains(&Event::Fetched {
            repo: "/r".into(),
            error: None
        }));
    }

    #[test]
    fn only_a_plain_poll_may_build_on_an_earlier_probe() {
        let fake = Arc::new(Fake::default());
        fake.open();
        let (mut engine, rx) = engine(fake.clone());
        let probed = |rx: &Receiver<Event>| loop {
            if let Event::Probed { cause, .. } = recv(rx) {
                break cause;
            }
        };
        assert!(engine.poll(Path::new("/r")));
        let mut causes = vec![probed(&rx)];
        // The poll sent its result just before it is done: a fetch asked for
        // in between is skipped, so ask until it is taken.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !engine.fetch(Path::new("/r")) {
            assert!(std::time::Instant::now() < deadline, "the poll never ended");
            thread::sleep(Duration::from_millis(1));
        }
        causes.push(probed(&rx));
        engine.run_command(1, Path::new("/r"), Job::Git(vec!["push".into()]));
        causes.push(probed(&rx));
        assert_eq!(
            causes,
            [ProbeCause::Poll, ProbeCause::Fetch, ProbeCause::Command]
        );
        assert_eq!(fake.fresh.lock().unwrap().clone(), [false, true, true]);
    }

    #[test]
    fn failed_fetch_reports_auth() {
        let fake = Arc::new(Fake {
            fail_fetch: true,
            ..Fake::default()
        });
        fake.open();
        let (mut engine, rx) = engine(fake.clone());
        engine.fetch(Path::new("/r"));
        assert_eq!(
            recv(&rx),
            Event::Fetched {
                repo: "/r".into(),
                error: Some(FetchError {
                    summary: "auth failed".into(),
                    output: "ok\nPermission denied (publickey).".into(),
                })
            }
        );
    }

    #[test]
    fn vocabulary_loading() {
        let fake = Arc::new(Fake::default());
        let (engine, rx) = engine(fake);
        engine.load_commands("/".into());
        assert_eq!(
            recv(&rx),
            Event::Commands {
                all: vec!["pull".into()],
                builtins: vec!["pull".into()]
            }
        );
        engine.load_details("/r".into(), 200);
        assert_eq!(
            recv(&rx),
            Event::Details {
                repo: "/r".into(),
                result: Err("details /r 200".into())
            }
        );
        // A full page says the history goes on; a short one ends it.
        for (skip, length, goes_on) in [(0, 100, true), (100, 50, false), (150, 0, false)] {
            engine.load_log("/r".into(), skip);
            match recv(&rx) {
                Event::Log {
                    repo,
                    skip: at,
                    result: Ok(page),
                    more,
                } => {
                    assert_eq!((repo, at), (PathBuf::from("/r"), skip));
                    assert_eq!((page.len(), more), (length, goes_on));
                }
                other => panic!("{other:?}"),
            }
        }
        engine.load_graph("/r".into(), true);
        assert_eq!(
            recv(&rx),
            Event::Graph {
                repo: "/r".into(),
                result: Err("graph /r true".into())
            }
        );
        let target = DiffTarget::Commit("abc".into());
        engine.load_diff("/r".into(), target.clone());
        assert_eq!(
            recv(&rx),
            Event::Diff {
                repo: "/r".into(),
                target,
                result: Ok("diff /r Commit(\"abc\")".into())
            }
        );
        engine.load_options("/".into(), "pull".into());
        assert_eq!(
            recv(&rx),
            Event::Options {
                command: "pull".into(),
                options: vec!["--pull-opt".into()]
            }
        );
    }
}
