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
