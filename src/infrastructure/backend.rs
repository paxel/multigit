//! The engine's [`Backend`] port implemented over the system git.

use crate::application::engine::Backend;
use crate::domain::details::{Details, DiffTarget, LOG_PAGE, LogEntry};
use crate::domain::output::ProcessOutput;
use crate::domain::repo::{ProbeError, Snapshot};
use crate::infrastructure::details;
use crate::infrastructure::git::GitRunner;
use crate::infrastructure::probe::{self, Probed};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// After this many cheap probes of a repository in a row the next one reads
/// everything again: with a refresh every 10 seconds, a branch or tag made
/// outside the app shows within a minute.
const LIGHT_PROBES_IN_A_ROW: u32 = 5;

/// What the last probe of a repository found, and how many cheap probes
/// have built on it.
#[derive(Debug)]
struct Remembered {
    probed: Probed,
    light_probes: u32,
}

#[derive(Debug, Default)]
pub struct GitBackend {
    runner: GitRunner,
    remembered: Mutex<HashMap<PathBuf, Remembered>>,
}

impl GitBackend {
    pub fn new(runner: GitRunner) -> Self {
        Self {
            runner,
            remembered: Mutex::default(),
        }
    }

    fn remembered(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Remembered>> {
        self.remembered.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The cheap probe, when one is allowed and HEAD has not moved.
    fn light_probe(&self, dir: &Path) -> Option<Snapshot> {
        let (previous, light_probes) = {
            let remembered = self.remembered();
            let entry = remembered.get(dir)?;
            if entry.light_probes >= LIGHT_PROBES_IN_A_ROW {
                return None;
            }
            (entry.probed.clone(), entry.light_probes)
        };
        let probed = probe::probe_light(&self.runner, dir, &previous)?;
        let snapshot = probed.snapshot.clone();
        self.remembered().insert(
            dir.to_path_buf(),
            Remembered {
                probed,
                light_probes: light_probes + 1,
            },
        );
        Some(snapshot)
    }

    fn words(&self, dir: &Path, args: &[&str]) -> Vec<String> {
        match self.runner.query(dir, args) {
            Ok(out) if out.success() => out.stdout.split_whitespace().map(str::to_owned).collect(),
            _ => Vec::new(),
        }
    }
}

impl Backend for GitBackend {
    /// One `git status` while nothing says more changed; everything after a
    /// fetch or a command (`fresh`), when HEAD moved, and every sixth time.
    fn probe(&self, dir: &Path, fresh: bool) -> Result<Snapshot, ProbeError> {
        if !fresh && let Some(snapshot) = self.light_probe(dir) {
            return Ok(snapshot);
        }
        match probe::probe_full(&self.runner, dir) {
            Ok(probed) => {
                let snapshot = probed.snapshot.clone();
                let entry = Remembered {
                    probed,
                    light_probes: 0,
                };
                self.remembered().insert(dir.to_path_buf(), entry);
                Ok(snapshot)
            }
            Err(e) => {
                self.remembered().remove(dir);
                Err(e)
            }
        }
    }

    fn run(&self, dir: &Path, args: &[String]) -> io::Result<ProcessOutput> {
        self.runner.run(dir, args)
    }

    fn fetch(&self, dir: &Path) -> io::Result<ProcessOutput> {
        self.runner.background_fetch(dir)
    }

    fn run_shell(&self, dir: &Path, line: &str) -> io::Result<ProcessOutput> {
        self.runner.run_shell(dir, line)
    }

    fn list_commands(&self, dir: &Path) -> (Vec<String>, Vec<String>) {
        (
            self.words(dir, &["--list-cmds=main,others,alias,nohelpers"]),
            self.words(dir, &["--list-cmds=builtins"]),
        )
    }

    fn options(&self, dir: &Path, command: &str) -> Vec<String> {
        self.words(dir, &[command, "--git-completion-helper"])
    }

    fn details(&self, dir: &Path, commits: usize) -> Result<Details, String> {
        details::details(&self.runner, dir, commits)
    }

    fn log(&self, dir: &Path, skip: usize) -> Result<Vec<LogEntry>, String> {
        details::log(&self.runner, dir, skip, LOG_PAGE)
    }

    fn diff(&self, dir: &Path, target: &DiffTarget) -> Result<String, String> {
        details::diff(&self.runner, dir, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

    /// A backend whose git is a script that notes every call in `log`.
    #[cfg(unix)]
    fn logging_backend(dir: &Path) -> (GitBackend, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let log = dir.join("calls.log");
        let script = dir.join("git-logged");
        let text = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec git \"$@\"\n",
            log.display()
        );
        std::fs::write(&script, text).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (
            GitBackend::new(isolated_runner().with_program(&script)),
            log,
        )
    }

    /// The git subcommands called since the last look, then forgotten.
    #[cfg(unix)]
    fn calls(log: &Path) -> Vec<String> {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        let _ = std::fs::remove_file(log);
        let known = [
            "rev-parse",
            "status",
            "for-each-ref",
            "remote",
            "describe",
            "log",
        ];
        text.lines()
            .filter_map(|l| l.split(' ').find(|w| known.contains(w)))
            .map(str::to_owned)
            .collect()
    }

    #[cfg(unix)]
    #[test]
    fn an_idle_repository_costs_one_git_call_per_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = TestRepo::new();
        repo.commit("a.txt", "1");
        repo.git(&["tag", "v1"]);
        let (backend, log) = logging_backend(tmp.path());
        let full = [
            "rev-parse",
            "status",
            "for-each-ref",
            "remote",
            "describe",
            "log",
        ];

        let first = backend.probe(repo.path(), false).unwrap();
        assert_eq!(calls(&log), full, "nothing remembered yet");
        assert_eq!(first.tags, ["v1"]);

        // Five refreshes with one call each; they still see the working tree.
        repo.write("a.txt", "2");
        repo.git(&["tag", "v2"]);
        for _ in 0..5 {
            let snap = backend.probe(repo.path(), false).unwrap();
            assert_eq!(calls(&log), ["status"]);
            assert_eq!(snap.status.counts.modified, 1);
            assert_eq!(
                snap.tags,
                ["v1"],
                "the new tag waits for the next full probe"
            );
        }
        // The sixth reads everything: the tag made outside shows.
        let snap = backend.probe(repo.path(), false).unwrap();
        assert_eq!(calls(&log), full);
        assert_eq!(snap.tags, ["v2", "v1"]);

        // After a fetch or a command nothing remembered is used.
        backend.probe(repo.path(), true).unwrap();
        assert_eq!(calls(&log), full);

        // HEAD moved: the status notices, then everything is read.
        repo.git(&["commit", "-q", "-am", "two"]);
        let snap = backend.probe(repo.path(), false).unwrap();
        let mut expected = vec!["status"];
        expected.extend(full);
        assert_eq!(calls(&log), expected);
        assert_eq!(snap.commit.unwrap().subject, "two");
        assert!(snap.tags.is_empty() || snap.tags == ["v2", "v1"]);

        // A repository that went away is reported and forgotten.
        std::fs::remove_dir_all(repo.path().join(".git")).unwrap();
        assert_eq!(backend.probe(repo.path(), false), Err(ProbeError::NotARepo));
        assert!(backend.remembered().is_empty());
    }

    #[test]
    fn lists_commands_and_options() {
        let repo = TestRepo::new();
        let backend = GitBackend::new(isolated_runner());
        let (all, builtins) = backend.list_commands(repo.path());
        assert!(all.iter().any(|c| c == "pull"));
        assert!(builtins.iter().any(|c| c == "status"));
        let options = backend.options(repo.path(), "pull");
        assert!(options.iter().any(|o| o == "--rebase"), "{options:?}");
        assert!(backend.options(repo.path(), "not-a-command").is_empty());
    }
}
