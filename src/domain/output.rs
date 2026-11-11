//! Output panel model: the last commands with one section per repository.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Commands kept in the output panel.
pub const MAX_RUNS: usize = 50;

/// Captured result of one finished git process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessOutput {
    /// `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Progress of one repository within a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SectionState {
    Queued,
    Running,
    Done(ProcessOutput),
    /// git could not be started at all.
    SpawnFailed(String),
}

impl SectionState {
    /// `Some(true)` = succeeded, `Some(false)` = failed, `None` = not finished.
    pub fn outcome(&self) -> Option<bool> {
        match self {
            SectionState::Queued | SectionState::Running => None,
            SectionState::Done(out) => Some(out.success()),
            SectionState::SpawnFailed(_) => Some(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub repo: PathBuf,
    pub state: SectionState,
}

/// One command executed in one or more repositories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: u64,
    /// The arguments as typed, without the leading `git`.
    pub command: String,
    pub sections: Vec<Section>,
}

impl Run {
    /// `None` while a repository is not done; then whether all succeeded.
    pub fn outcome(&self) -> Option<bool> {
        self.sections
            .iter()
            .map(|s| s.state.outcome())
            .try_fold(true, |all, outcome| Some(all && outcome?))
    }
}

impl Section {
    /// `exit 0`, `exit 128`, `killed`, `failed to start`; `queued` or
    /// `running` before it is done.
    pub fn status(&self) -> String {
        match &self.state {
            SectionState::Queued => "queued".to_owned(),
            SectionState::Running => "running".to_owned(),
            SectionState::Done(out) => out
                .exit_code
                .map_or("killed".to_owned(), |c| format!("exit {c}")),
            SectionState::SpawnFailed(_) => "failed to start".to_owned(),
        }
    }

    /// The section as text: a header line with path and status, then what
    /// the command printed.
    fn text(&self) -> String {
        let mut text = format!("== {} ({}) ==\n", self.repo.display(), self.status());
        let printed: Vec<&str> = match &self.state {
            SectionState::Done(out) => vec![out.stdout.trim_end(), out.stderr.trim_end()],
            SectionState::SpawnFailed(error) => vec![error.trim_end()],
            SectionState::Queued | SectionState::Running => Vec::new(),
        };
        for part in printed.into_iter().filter(|p| !p.is_empty()) {
            text.push_str(part);
            text.push('\n');
        }
        text
    }
}

impl Run {
    /// The run as a text file: the command line `first_line`, then every
    /// repository's section, or only the one of `repo`.
    pub fn text(&self, first_line: &str, repo: Option<&Path>) -> String {
        let mut text = format!("{first_line}\n");
        for section in &self.sections {
            if repo.is_none_or(|r| r == section.repo) {
                text.push('\n');
                text.push_str(&section.text());
            }
        }
        text
    }

    /// Whether the run, or with `repo` its section of that repository, is
    /// done.
    pub fn is_done(&self, repo: Option<&Path>) -> bool {
        let mut sections = self
            .sections
            .iter()
            .filter(|s| repo.is_none_or(|r| r == s.repo));
        sections.all(|s| s.state.outcome().is_some())
    }
}

/// Bounded log of runs, oldest first.
#[derive(Debug, Clone, Default)]
pub struct OutputLog {
    runs: VecDeque<Run>,
    next_id: u64,
}

impl OutputLog {
    /// Start a run; every repo begins as queued. Returns the run id.
    pub fn start(&mut self, command: &str, repos: &[PathBuf]) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.runs.push_back(Run {
            id,
            command: command.to_owned(),
            sections: repos
                .iter()
                .map(|r| Section {
                    repo: r.clone(),
                    state: SectionState::Queued,
                })
                .collect(),
        });
        while self.runs.len() > MAX_RUNS {
            self.runs.pop_front();
        }
        id
    }

    /// Update one section; unknown runs (already dropped) are ignored.
    pub fn update(&mut self, run: u64, repo: &Path, state: SectionState) {
        if let Some(section) = self
            .runs
            .iter_mut()
            .find(|r| r.id == run)
            .and_then(|r| r.sections.iter_mut().find(|s| s.repo == repo))
        {
            section.state = state;
        }
    }

    pub fn clear(&mut self) {
        self.runs.clear();
    }

    pub fn runs(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter()
    }

    pub fn run(&self, id: u64) -> Option<&Run> {
        self.runs.iter().find(|r| r.id == id)
    }

    /// The run started last.
    pub fn last(&self) -> Option<&Run> {
        self.runs.back()
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32) -> SectionState {
        SectionState::Done(ProcessOutput {
            exit_code: Some(code),
            stdout: String::new(),
            stderr: String::new(),
        })
    }

    #[test]
    fn start_update_and_outcome() {
        let mut log = OutputLog::default();
        let repos = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert!(log.last().is_none());
        let id = log.start("status", &repos);
        log.update(id, Path::new("/a"), out(0));
        assert_eq!(log.last().unwrap().outcome(), None, "/b is not done");
        log.update(id, Path::new("/b"), out(1));
        let run = log.runs().next().unwrap();
        assert_eq!(run.command, "status");
        assert_eq!(run.sections[0].state.outcome(), Some(true));
        assert_eq!(run.sections[1].state.outcome(), Some(false));
        assert_eq!(SectionState::Running.outcome(), None);
        assert_eq!(log.last().unwrap().outcome(), Some(false));
    }

    #[test]
    fn run_as_text_has_the_command_then_a_header_and_the_output_per_repository() {
        let mut log = OutputLog::default();
        let repos = vec![
            PathBuf::from("/x/a"),
            PathBuf::from("/x/b"),
            PathBuf::from("/x/c"),
        ];
        let id = log.start("status", &repos);
        let done = |code, stdout: &str, stderr: &str| {
            SectionState::Done(ProcessOutput {
                exit_code: code,
                stdout: stdout.into(),
                stderr: stderr.into(),
            })
        };
        log.update(id, Path::new("/x/a"), done(Some(0), "clean\n\n", ""));
        assert!(log.run(id).unwrap().is_done(Some(Path::new("/x/a"))));
        assert!(!log.run(id).unwrap().is_done(None));
        assert!(!log.run(id).unwrap().is_done(Some(Path::new("/x/b"))));
        log.update(id, Path::new("/x/b"), done(None, "out", "err\n"));
        log.update(
            id,
            Path::new("/x/c"),
            SectionState::SpawnFailed("no git".into()),
        );
        let run = log.run(id).unwrap();
        assert!(run.is_done(None));
        assert_eq!(
            run.text("git status", None),
            "git status\n\n== /x/a (exit 0) ==\nclean\n\n== /x/b (killed) ==\nout\nerr\n\n\
             == /x/c (failed to start) ==\nno git\n"
        );
        assert_eq!(
            run.text("git status", Some(Path::new("/x/b"))),
            "git status\n\n== /x/b (killed) ==\nout\nerr\n"
        );
        assert!(log.run(99).is_none());
        let id = log.start("push", &repos);
        assert_eq!(log.run(id).unwrap().sections[0].status(), "queued");
        log.update(id, Path::new("/x/a"), SectionState::Running);
        assert_eq!(log.run(id).unwrap().sections[0].status(), "running");
        assert_eq!(
            log.run(id)
                .unwrap()
                .text("git push", Some(Path::new("/x/a"))),
            "git push\n\n== /x/a (running) ==\n"
        );
    }

    #[test]
    fn capped_at_max_runs_and_clear() {
        let mut log = OutputLog::default();
        let repos = vec![PathBuf::from("/a")];
        for i in 0..MAX_RUNS + 5 {
            log.start(&format!("log -{i}"), &repos);
        }
        assert_eq!(log.len(), MAX_RUNS);
        assert_eq!(log.runs().next().unwrap().command, "log -5");
        // Updates to dropped runs are ignored.
        log.update(0, Path::new("/a"), out(0));
        let id = log.start("status", &repos);
        log.update(id, Path::new("/a"), out(0));
        assert_eq!(log.last().unwrap().outcome(), Some(true));
        log.clear();
        assert!(log.is_empty());
    }
}
