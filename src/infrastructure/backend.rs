//! The engine's [`Backend`] port implemented over the system git.

use crate::application::engine::Backend;
use crate::domain::output::ProcessOutput;
use crate::domain::repo::{ProbeError, Snapshot};
use crate::infrastructure::git::GitRunner;
use crate::infrastructure::probe;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct GitBackend {
    runner: GitRunner,
}

impl GitBackend {
    pub fn new(runner: GitRunner) -> Self {
        Self { runner }
    }

    fn words(&self, dir: &Path, args: &[&str]) -> Vec<String> {
        match self.runner.query(dir, args) {
            Ok(out) if out.success() => out.stdout.split_whitespace().map(str::to_owned).collect(),
            _ => Vec::new(),
        }
    }
}

impl Backend for GitBackend {
    fn probe(&self, dir: &Path) -> Result<Snapshot, ProbeError> {
        probe::probe(&self.runner, dir)
    }

    fn run(&self, dir: &Path, args: &[String]) -> io::Result<ProcessOutput> {
        self.runner.run(dir, args)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

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
