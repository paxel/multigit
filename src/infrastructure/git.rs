//! Runs the system `git` without a terminal: no prompts, no pager, no editor.

use crate::domain::command::shell_program;
use crate::domain::output::ProcessOutput;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Editor stand-in: git runs it through `sh -c '<editor> "$@"'`, so it prints a
/// clear message and fails instead of waiting for an editor that never appears.
pub const EDITOR_STUB: &str = "printf '%s\\n' 'multigit: this command needs an editor, which is not available here (use -m or similar)' >&2; false";

/// SSH option that makes ssh fail instead of asking for a password or passphrase.
const BATCH_MODE: &str = "-o BatchMode=yes";

/// Spawns `git` processes with the non-interactive environment.
#[derive(Debug, Clone)]
pub struct GitRunner {
    program: OsString,
    extra_env: Vec<(OsString, OsString)>,
}

impl Default for GitRunner {
    fn default() -> Self {
        Self {
            program: OsString::from("git"),
            extra_env: Vec::new(),
        }
    }
}

impl GitRunner {
    /// Add an environment variable to every spawned process (used by tests to
    /// isolate from the user's git config).
    pub fn with_env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    fn env_value(&self, key: &str) -> Option<OsString> {
        self.extra_env
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var_os(key))
    }

    fn command(&self, dir: &Path) -> Command {
        self.command_for(&self.program, dir)
    }

    fn command_for(&self, program: &OsStr, dir: &Path) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            // An empty GIT_ASKPASS stops git from falling back to core.askPass/SSH_ASKPASS.
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS_REQUIRE", "never")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("GIT_EDITOR", EDITOR_STUB)
            .env("GIT_SEQUENCE_EDITOR", EDITOR_STUB);
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        cmd
    }

    fn output(mut cmd: Command) -> io::Result<ProcessOutput> {
        let out = cmd.output()?;
        Ok(ProcessOutput {
            exit_code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    /// Read-only query: `git --no-optional-locks <args>`, never touches the network.
    #[tracing::instrument(skip_all, fields(dir = %dir.display()))]
    pub fn query<S: AsRef<OsStr>>(&self, dir: &Path, args: &[S]) -> io::Result<ProcessOutput> {
        let mut cmd = self.command(dir);
        cmd.arg("--no-optional-locks").args(args);
        Self::output(cmd)
    }

    /// A user command (button, CLI, background fetch), with SSH forced into batch mode.
    #[tracing::instrument(skip_all, fields(dir = %dir.display()))]
    pub fn run<S: AsRef<OsStr>>(&self, dir: &Path, args: &[S]) -> io::Result<ProcessOutput> {
        let mut cmd = self.command(dir);
        if let Some(ssh) = self.batch_ssh_command(dir) {
            cmd.env("GIT_SSH_COMMAND", ssh);
        }
        cmd.args(args);
        Self::output(cmd)
    }

    /// The shell `!` lines run in: `$SHELL`, or `sh` when it is unset or empty.
    pub fn shell(&self) -> OsString {
        shell_program(self.env_value("SHELL"))
    }

    /// A `!` line: `<shell> -c <line>` in `dir`, with the same non-interactive
    /// environment as git commands, so git inside the line never prompts either.
    #[tracing::instrument(skip_all, fields(dir = %dir.display()))]
    pub fn run_shell(&self, dir: &Path, line: &str) -> io::Result<ProcessOutput> {
        let mut cmd = self.command_for(&self.shell(), dir);
        if let Some(ssh) = self.batch_ssh_command(dir) {
            cmd.env("GIT_SSH_COMMAND", ssh);
        }
        cmd.arg("-c").arg(line);
        Self::output(cmd)
    }

    /// The ssh command git would use, with BatchMode appended. Keeps an existing
    /// `GIT_SSH_COMMAND` or `core.sshCommand`; returns `None` when `GIT_SSH` is set,
    /// since overriding it would change which program git runs.
    fn batch_ssh_command(&self, dir: &Path) -> Option<OsString> {
        if let Some(existing) = self.env_value("GIT_SSH_COMMAND") {
            return Some(append_batch_mode(&existing));
        }
        if self.env_value("GIT_SSH").is_some() {
            return None;
        }
        let configured = self
            .query(dir, &["config", "--get", "core.sshCommand"])
            .ok()
            .filter(ProcessOutput::success)
            .map(|o| o.stdout.trim().to_owned())
            .filter(|s| !s.is_empty());
        Some(append_batch_mode(OsStr::new(
            configured.as_deref().unwrap_or("ssh"),
        )))
    }
}

fn append_batch_mode(ssh: &OsStr) -> OsString {
    let mut cmd = ssh.to_owned();
    cmd.push(" ");
    cmd.push(BATCH_MODE);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

    #[test]
    fn query_runs_git_in_dir() {
        let repo = TestRepo::new();
        let out = isolated_runner()
            .query(repo.path(), &["rev-parse", "--is-inside-work-tree"])
            .unwrap();
        assert!(out.success(), "{out:?}");
        assert_eq!(out.stdout.trim(), "true");
    }

    #[test]
    fn editor_commands_fail_with_message() {
        let repo = TestRepo::new();
        repo.write("a.txt", "a");
        repo.git(&["add", "a.txt"]);
        let out = isolated_runner().run(repo.path(), &["commit"]).unwrap();
        assert!(!out.success());
        assert!(
            out.stderr
                .contains("multigit: this command needs an editor"),
            "{out:?}"
        );
    }

    #[test]
    fn ssh_command_gets_batch_mode() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        let plain = runner.batch_ssh_command(repo.path());
        assert_eq!(plain, Some(OsString::from("ssh -o BatchMode=yes")));

        repo.git(&["config", "core.sshCommand", "ssh -i /k"]);
        let configured = runner.batch_ssh_command(repo.path());
        assert_eq!(
            configured,
            Some(OsString::from("ssh -i /k -o BatchMode=yes"))
        );

        let env = runner.clone().with_env("GIT_SSH_COMMAND", "myssh");
        assert_eq!(
            env.batch_ssh_command(repo.path()),
            Some(OsString::from("myssh -o BatchMode=yes"))
        );
    }

    #[test]
    fn ssh_command_reaches_git() {
        let repo = TestRepo::new();
        // A shell alias prints the GIT_SSH_COMMAND that git passes to its children.
        let out = isolated_runner()
            .run(
                repo.path(),
                &[
                    "-c",
                    "alias.showssh=!printf %s \"$GIT_SSH_COMMAND\"",
                    "showssh",
                ],
            )
            .unwrap();
        assert!(out.success(), "{out:?}");
        assert_eq!(out.stdout, "ssh -o BatchMode=yes");
    }

    #[test]
    fn shell_line_runs_in_repo_without_prompts() {
        let repo = TestRepo::new();
        let runner = isolated_runner().with_env("SHELL", "/bin/sh");
        let out = runner
            .run_shell(
                repo.path(),
                "printf '%s|%s' \"$(pwd -P)\" \"$GIT_TERMINAL_PROMPT\"; echo oops >&2; exit 3",
            )
            .unwrap();
        let dir = repo.path().canonicalize().unwrap();
        assert_eq!(out.stdout, format!("{}|0", dir.display()));
        assert_eq!(out.stderr, "oops\n");
        assert_eq!(out.exit_code, Some(3));
    }

    #[test]
    fn shell_is_login_shell_or_sh() {
        let runner = isolated_runner();
        assert_eq!(
            runner.clone().with_env("SHELL", "/usr/bin/fish").shell(),
            OsString::from("/usr/bin/fish")
        );
        assert_eq!(runner.with_env("SHELL", "").shell(), OsString::from("sh"));
    }

    #[test]
    fn missing_shell_is_io_error() {
        let repo = TestRepo::new();
        let runner = isolated_runner().with_env("SHELL", "/does/not/exist/sh");
        assert!(runner.run_shell(repo.path(), "true").is_err());
    }

    #[test]
    fn missing_dir_is_io_error() {
        let runner = isolated_runner();
        assert!(
            runner
                .query(Path::new("/does/not/exist"), &["status"])
                .is_err()
        );
    }
}
