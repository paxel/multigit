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

/// Settings a repository's own config may not choose for the background fetch,
/// because each makes git run a program: hooks, the file system monitor, and
/// the `ext::` transport, whose remote URL is a command line.
const BACKGROUND_FETCH_CONFIG: &[&str] = &[
    "core.hooksPath=/dev/null",
    "core.fsmonitor=false",
    "protocol.ext.allow=never",
];

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
    /// Queries run automatically and all the time, so the program a repository
    /// names in `core.fsmonitor` is not run for them.
    #[tracing::instrument(skip_all, fields(dir = %dir.display()))]
    pub fn query<S: AsRef<OsStr>>(&self, dir: &Path, args: &[S]) -> io::Result<ProcessOutput> {
        let mut cmd = self.command(dir);
        cmd.args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
            .args(args);
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

    /// The background fetch: what `git fetch --all --prune` does, without the
    /// programs a repository's config can make a fetch run (see
    /// [`BACKGROUND_FETCH_CONFIG`], and `remote.<name>.uploadpack`). It runs
    /// every minute without the user doing anything, so a prepared repository
    /// folder must not get its commands executed through it. A fetch the user
    /// starts goes through [`Self::run`] and keeps hooks and the rest.
    ///
    /// Each remote is fetched on its own: `--upload-pack`, which overrides the
    /// configured program, does not reach the per-remote fetches of `--all`.
    /// Like `--all`, a failing remote does not stop the others; the result is
    /// the first failure, or the last fetch.
    #[tracing::instrument(skip_all, fields(dir = %dir.display()))]
    pub fn background_fetch(&self, dir: &Path) -> io::Result<ProcessOutput> {
        let ssh = self.batch_ssh_command(dir);
        let mut last = ProcessOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        let mut failed = None;
        for remote in self.remotes_to_fetch(dir)? {
            let mut cmd = self.command(dir);
            if let Some(ssh) = &ssh {
                cmd.env("GIT_SSH_COMMAND", ssh);
            }
            for setting in BACKGROUND_FETCH_CONFIG {
                cmd.args(["-c", setting]);
            }
            cmd.args(["fetch", "--prune", "--upload-pack=git-upload-pack", "--"])
                .arg(&remote);
            last = Self::output(cmd)?;
            if !last.success() && failed.is_none() {
                failed = Some(last.clone());
            }
        }
        Ok(failed.unwrap_or(last))
    }

    /// The remotes `fetch --all` covers: all but those with `skipFetchAll` set.
    fn remotes_to_fetch(&self, dir: &Path) -> io::Result<Vec<String>> {
        let skipped = self.query(
            dir,
            &[
                "config",
                "--type=bool",
                "--get-regexp",
                r"^remote\..*\.skipfetchall$",
            ],
        )?;
        let skipped: Vec<&str> = skipped
            .stdout
            .lines()
            .filter_map(|line| line.strip_suffix(" true"))
            .filter_map(|key| key.strip_prefix("remote."))
            .filter_map(|key| key.strip_suffix(".skipfetchall"))
            .collect();
        let remotes = self.query(dir, &["remote"])?;
        Ok(remotes
            .stdout
            .lines()
            .filter(|r| !r.is_empty() && !skipped.contains(r))
            .map(str::to_owned)
            .collect())
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
    use crate::application::engine::FETCH_ARGS;
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

    /// An executable script at `path` that creates `marker` when it runs, then
    /// runs `then` (shell code).
    #[cfg(unix)]
    fn marker_script(path: &Path, marker: &Path, then: &str) {
        use std::os::unix::fs::PermissionsExt;
        let script = format!("#!/bin/sh\n: > '{}'\n{then}\n", marker.display());
        std::fs::write(path, script).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn queries_do_not_run_the_repository_fsmonitor() {
        let repo = TestRepo::new();
        repo.commit("a.txt", "a");
        let marker = repo.path().join(".git/fsmonitor-ran");
        let script = repo.path().join(".git/fsmonitor.sh");
        marker_script(&script, &marker, "exit 1");
        repo.git(&["config", "core.fsmonitor", &script.to_string_lossy()]);
        let runner = isolated_runner();

        // The same status as a user command runs it: the setting is live.
        runner.run(repo.path(), &["status"]).unwrap();
        assert!(marker.exists(), "fsmonitor did not run for a user command");
        std::fs::remove_file(&marker).unwrap();

        let out = runner
            .query(repo.path(), &["status", "--porcelain=v2", "--branch"])
            .unwrap();
        assert!(out.success(), "{out:?}");
        assert!(!marker.exists(), "fsmonitor ran for an automatic query");
    }

    /// A bare remote with one pushed commit, the clone that pushes to it and a
    /// second clone that is behind after `advance`.
    #[cfg(unix)]
    fn remote_pusher_and_clone() -> (TestRepo, TestRepo, TestRepo) {
        let remote = TestRepo::bare();
        let pusher = TestRepo::clone_of(&remote);
        pusher.commit("one.txt", "1");
        pusher.git(&["push", "-q", "-u", "origin", "main"]);
        let clone = TestRepo::clone_of(&remote);
        (remote, pusher, clone)
    }

    /// Push a new commit, so the next fetch of the other clone updates a ref.
    #[cfg(unix)]
    fn advance(pusher: &TestRepo, file: &str) -> String {
        pusher.commit(file, file);
        pusher.git(&["push", "-q"]);
        pusher.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    #[cfg(unix)]
    fn fetch_as_user(repo: &TestRepo) -> ProcessOutput {
        isolated_runner().run(repo.path(), FETCH_ARGS).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_runs_no_hooks_but_a_user_fetch_does() {
        let (_remote, pusher, clone) = remote_pusher_and_clone();
        let marker = clone.path().join(".git/hook-ran");
        marker_script(
            &clone.path().join(".git/hooks/reference-transaction"),
            &marker,
            "exit 0",
        );

        let head = advance(&pusher, "two.txt");
        let out = isolated_runner().background_fetch(clone.path()).unwrap();
        assert!(out.success(), "{out:?}");
        assert_eq!(clone.git(&["rev-parse", "origin/main"]).trim(), head);
        assert!(!marker.exists(), "a hook ran for the background fetch");

        advance(&pusher, "three.txt");
        assert!(fetch_as_user(&clone).success());
        assert!(marker.exists(), "hooks must still run for a user fetch");
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_covers_every_remote_but_the_skipped_ones() {
        let (remote, pusher, clone) = remote_pusher_and_clone();
        let url = remote.path().to_string_lossy().into_owned();
        clone.git(&["remote", "add", "second", &url]);
        clone.git(&["remote", "add", "skipped", &url]);
        clone.git(&["config", "remote.skipped.skipFetchAll", "yes"]);
        let head = advance(&pusher, "two.txt");

        let out = isolated_runner().background_fetch(clone.path()).unwrap();
        assert!(out.success(), "{out:?}");
        for name in ["origin", "second"] {
            let fetched = clone.git(&["rev-parse", &format!("{name}/main")]);
            assert_eq!(fetched.trim(), head, "{name}");
        }
        let refs = clone.git(&["for-each-ref", "refs/remotes/skipped"]);
        assert_eq!(refs, "", "a skipFetchAll remote was fetched");
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_without_remotes_succeeds_and_a_failure_is_reported() {
        let repo = TestRepo::new();
        repo.commit("a.txt", "a");
        let runner = isolated_runner();
        assert!(runner.background_fetch(repo.path()).unwrap().success());

        repo.git(&["remote", "add", "gone", "/does/not/exist.git"]);
        let out = runner.background_fetch(repo.path()).unwrap();
        assert!(!out.success());
        assert!(!out.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_continues_after_a_failing_remote() {
        let (_remote, pusher, clone) = remote_pusher_and_clone();
        // Sorts before `origin`, so it is fetched (and fails) first.
        clone.git(&["remote", "add", "broken", "/does/not/exist.git"]);
        let head = advance(&pusher, "two.txt");
        let out = isolated_runner().background_fetch(clone.path()).unwrap();
        assert!(!out.success(), "the failure is the result");
        assert_eq!(clone.git(&["rev-parse", "origin/main"]).trim(), head);
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_does_not_run_an_ext_remote() {
        let (_remote, _pusher, clone) = remote_pusher_and_clone();
        let marker = clone.path().join(".git/ext-ran");
        let script = clone.path().join(".git/ext.sh");
        marker_script(&script, &marker, "exit 1");
        clone.git(&[
            "remote",
            "add",
            "evil",
            &format!("ext::{}", script.display()),
        ]);
        clone.git(&["config", "protocol.ext.allow", "always"]);

        // The repository's own config allows it: a user fetch runs the command.
        fetch_as_user(&clone);
        assert!(
            marker.exists(),
            "the ext remote did not run for a user fetch"
        );
        std::fs::remove_file(&marker).unwrap();

        isolated_runner().background_fetch(clone.path()).unwrap();
        assert!(
            !marker.exists(),
            "the ext remote ran for the background fetch"
        );
    }

    #[cfg(unix)]
    #[test]
    fn background_fetch_ignores_a_configured_uploadpack() {
        let (remote, pusher, clone) = remote_pusher_and_clone();
        let marker = clone.path().join(".git/uploadpack-ran");
        let script = clone.path().join(".git/uploadpack.sh");
        marker_script(&script, &marker, "exec git-upload-pack \"$@\"");
        // Two remotes: `fetch --all` then fetches each in a child process.
        clone.git(&["remote", "add", "second", &remote.path().to_string_lossy()]);
        for name in ["origin", "second"] {
            let key = format!("remote.{name}.uploadpack");
            clone.git(&["config", &key, &script.to_string_lossy()]);
        }

        advance(&pusher, "two.txt");
        assert!(fetch_as_user(&clone).success());
        assert!(marker.exists(), "uploadpack did not run for a user fetch");
        std::fs::remove_file(&marker).unwrap();

        let head = advance(&pusher, "three.txt");
        let out = isolated_runner().background_fetch(clone.path()).unwrap();
        assert!(out.success(), "{out:?}");
        assert_eq!(clone.git(&["rev-parse", "origin/main"]).trim(), head);
        assert!(!marker.exists(), "uploadpack ran for the background fetch");
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
