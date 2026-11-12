//! Reads what the repository view shows (read-only, no network). The view
//! reloads by itself when its repository changes, so these calls follow the
//! rule of the automatic ones: no program the repository's own config names
//! is run for a diff or a signature.

use crate::domain::details::{
    ChangeKind, Details, DiffTarget, LOG_FORMAT, LOG_LIMIT, cut_diff, parse_branches,
    parse_changes, parse_log,
};
use crate::domain::output::ProcessOutput;
use crate::domain::status::{Head, parse_porcelain_v2};
use crate::infrastructure::git::GitRunner;
use std::path::Path;

/// Options of every diff: no external diff program, no textconv filter, no
/// colors.
const PLAIN_DIFF: [&str; 3] = ["--no-ext-diff", "--no-textconv", "--no-color"];

fn error_of(out: &ProcessOutput) -> String {
    out.stderr
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("git failed")
        .to_owned()
}

fn query(runner: &GitRunner, dir: &Path, args: &[&str]) -> Result<ProcessOutput, String> {
    runner.query(dir, args).map_err(|e| e.to_string())
}

fn stdout(runner: &GitRunner, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = query(runner, dir, args)?;
    if out.success() {
        Ok(out.stdout)
    } else {
        Err(error_of(&out))
    }
}

/// The lists of the view for the repository at `dir`.
pub fn details(runner: &GitRunner, dir: &Path) -> Result<Details, String> {
    let status = stdout(runner, dir, &["status", "--porcelain=v2", "--branch"])?;
    let current = match parse_porcelain_v2(&status).map(|s| s.head) {
        Ok(Head::Branch(name) | Head::Unborn(name)) => Some(name),
        _ => None,
    };
    // Fails before the first commit: no commits then.
    let limit = format!("-{LOG_LIMIT}");
    let log = stdout(
        runner,
        dir,
        &["log", &limit, "--no-show-signature", LOG_FORMAT],
    )
    .unwrap_or_default();
    let branches = stdout(
        runner,
        dir,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)",
            "refs/heads",
        ],
    )?;
    Ok(Details {
        files: parse_changes(&status),
        commits: parse_log(&log),
        branches: parse_branches(&branches, current.as_deref()),
    })
}

/// An untracked file as a diff against nothing. `--no-index` exits with 1
/// when the two differ, which is every time.
fn untracked_diff(runner: &GitRunner, dir: &Path, path: &str) -> Result<String, String> {
    let mut args = vec!["diff"];
    args.extend(PLAIN_DIFF);
    args.extend(["--no-index", "--", "/dev/null", path]);
    let out = query(runner, dir, &args)?;
    if out.success() || out.exit_code == Some(1) {
        Ok(out.stdout)
    } else {
        Err(error_of(&out))
    }
}

/// The diff of `target` in the repository at `dir`, cut to what the pane
/// shows.
pub fn diff(runner: &GitRunner, dir: &Path, target: &DiffTarget) -> Result<String, String> {
    // Status names a new folder as one entry, `dir/`: show its files.
    if let DiffTarget::File(file) = target
        && file.kind == ChangeKind::Untracked
    {
        if !file.path.ends_with('/') {
            return untracked_diff(runner, dir, &file.path).map(|d| cut_diff(&d));
        }
        let listed = stdout(
            runner,
            dir,
            &[
                "ls-files",
                "--others",
                "--exclude-standard",
                "--",
                &file.path,
            ],
        )?;
        let mut all = String::new();
        for path in listed.lines() {
            all.push_str(&untracked_diff(runner, dir, path)?);
            // Enough is read once the pane is full.
            if all.lines().count() > crate::domain::details::DIFF_LINE_LIMIT {
                break;
            }
        }
        return Ok(cut_diff(&all));
    }
    let mut args: Vec<&str> = Vec::new();
    match target {
        DiffTarget::Commit(hash) => {
            if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("not a commit hash: {hash}"));
            }
            args.extend(["show", "--no-show-signature", "--stat", "--patch"]);
            args.extend(PLAIN_DIFF);
            args.push(hash);
        }
        DiffTarget::File(file) => {
            args.push("diff");
            args.extend(PLAIN_DIFF);
            match file.kind {
                ChangeKind::Staged => args.push("--cached"),
                ChangeKind::Modified | ChangeKind::Conflict => {}
                // Handled above.
                ChangeKind::Untracked => {}
            }
            args.extend(["--", &file.path]);
        }
    }
    let out = query(runner, dir, &args)?;
    if out.success() {
        Ok(cut_diff(&out.stdout))
    } else {
        Err(error_of(&out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::details::FileChange;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

    fn file(path: &str, kind: ChangeKind) -> DiffTarget {
        DiffTarget::File(FileChange {
            path: path.into(),
            kind,
        })
    }

    #[test]
    fn lists_changed_files_commits_and_branches() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        // Before the first commit: files, no commits, no branches yet.
        repo.write("a.txt", "one\n");
        let fresh = details(&runner, repo.path()).unwrap();
        assert_eq!(fresh.files.len(), 1);
        assert!(fresh.commits.is_empty() && fresh.branches.is_empty());

        repo.commit("a.txt", "one\n");
        repo.commit("b.txt", "two\n");
        repo.git(&["branch", "side"]);
        repo.write("a.txt", "one\nchanged\n");
        repo.write("b.txt", "staged\n");
        repo.git(&["add", "b.txt"]);
        repo.write("new file.txt", "new\n");
        let d = details(&runner, repo.path()).unwrap();
        let files: Vec<_> = d.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(
            files,
            [
                ("b.txt", ChangeKind::Staged),
                ("a.txt", ChangeKind::Modified),
                ("new file.txt", ChangeKind::Untracked)
            ]
        );
        let subjects: Vec<_> = d.commits.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(subjects, ["b.txt", "a.txt"]);
        assert_eq!(d.commits[0].author, "Test");
        assert_eq!(d.commits[0].hash, repo.git(&["rev-parse", "HEAD"]).trim());
        let mut branches: Vec<_> = d
            .branches
            .iter()
            .map(|b| (b.name.as_str(), b.current))
            .collect();
        branches.sort_unstable();
        assert_eq!(branches, [("main", true), ("side", false)]);

        let dir = tempfile::tempdir().unwrap();
        assert!(details(&runner, dir.path()).is_err(), "not a repository");
    }

    #[test]
    fn diffs_of_files_and_commits() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        repo.commit("a.txt", "one\n");
        repo.commit("b.txt", "two\n");
        repo.write("a.txt", "one\nchanged\n");
        repo.write("b.txt", "staged\n");
        repo.git(&["add", "b.txt"]);
        repo.write("-new.txt", "brand new\n");

        let modified = diff(&runner, repo.path(), &file("a.txt", ChangeKind::Modified)).unwrap();
        assert!(
            modified.contains("+changed") && !modified.contains("staged"),
            "{modified}"
        );
        let staged = diff(&runner, repo.path(), &file("b.txt", ChangeKind::Staged)).unwrap();
        assert!(
            staged.contains("-two") && staged.contains("+staged"),
            "{staged}"
        );
        // A name like an option is still a file.
        let untracked = diff(
            &runner,
            repo.path(),
            &file("-new.txt", ChangeKind::Untracked),
        )
        .unwrap();
        assert!(untracked.contains("+brand new"), "{untracked}");

        let head = repo.git(&["rev-parse", "HEAD"]);
        let commit = diff(
            &runner,
            repo.path(),
            &DiffTarget::Commit(head.trim().into()),
        )
        .unwrap();
        assert!(
            commit.contains("b.txt") && commit.contains("+two"),
            "{commit}"
        );
        assert!(commit.contains("Author: Test"), "{commit}");

        // A new folder is one untracked entry; its files show as one diff.
        std::fs::create_dir(repo.path().join("dir")).unwrap();
        repo.write("dir/one.txt", "first\n");
        repo.write("dir/two.txt", "second\n");
        let d = details(&runner, repo.path()).unwrap();
        let folder = d.files.iter().find(|f| f.path == "dir/").unwrap();
        let both = diff(&runner, repo.path(), &DiffTarget::File(folder.clone())).unwrap();
        assert!(
            both.contains("+first") && both.contains("+second"),
            "{both}"
        );

        for bad in ["", "--output=/tmp/x", "HEAD"] {
            assert!(
                diff(&runner, repo.path(), &DiffTarget::Commit(bad.into())).is_err(),
                "{bad}"
            );
        }
        let missing = "0123456789abcdef0123456789abcdef01234567";
        assert!(diff(&runner, repo.path(), &DiffTarget::Commit(missing.into())).is_err());
    }

    /// A diff driver with a command, set for a file type by an attribute: a
    /// user's `git diff` runs it, the view does not.
    #[cfg(unix)]
    #[test]
    fn the_view_runs_no_diff_driver_command() {
        use std::os::unix::fs::PermissionsExt;
        let repo = TestRepo::new();
        let runner = isolated_runner();
        repo.commit("a.bin", "one\n");
        let git_dir = repo.path().join(".git");
        let marker = git_dir.join("driver-ran");
        let driver = git_dir.join("driver.sh");
        let text = format!("#!/bin/sh\n: > '{}'\nexit 0\n", marker.display());
        std::fs::write(&driver, text).unwrap();
        std::fs::set_permissions(&driver, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(git_dir.join("info/attributes"), "*.bin diff=drv\n").unwrap();
        repo.git(&["config", "diff.drv.command", &driver.to_string_lossy()]);
        repo.write("a.bin", "one\ntwo\n");

        runner.run(repo.path(), &["diff"]).unwrap();
        assert!(marker.exists(), "the driver did not run for a user command");
        std::fs::remove_file(&marker).unwrap();

        let head = repo.git(&["rev-parse", "HEAD"]);
        diff(&runner, repo.path(), &file("a.bin", ChangeKind::Modified)).unwrap();
        repo.git(&["add", "a.bin"]);
        diff(&runner, repo.path(), &file("a.bin", ChangeKind::Staged)).unwrap();
        repo.write("new.bin", "x\n");
        diff(
            &runner,
            repo.path(),
            &file("new.bin", ChangeKind::Untracked),
        )
        .unwrap();
        diff(
            &runner,
            repo.path(),
            &DiffTarget::Commit(head.trim().into()),
        )
        .unwrap();
        assert!(
            !marker.exists(),
            "the view ran the repository's diff driver"
        );
    }

    /// The repository names an external diff program, a textconv filter and
    /// a gpg program. A user's own `git diff` and `git log` run them; the
    /// view does not.
    #[cfg(unix)]
    #[test]
    fn the_view_runs_no_program_the_repository_names() {
        use std::os::unix::fs::PermissionsExt;
        let repo = TestRepo::new();
        let runner = isolated_runner();
        repo.commit("a.bin", "one\n");
        let git_dir = repo.path().join(".git");
        let script = |name: &str, body: &str| {
            let marker = git_dir.join(format!("{name}-ran"));
            let path = git_dir.join(format!("{name}.sh"));
            let text = format!("#!/bin/sh\n: > '{}'\n{body}\n", marker.display());
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            (path.to_string_lossy().into_owned(), marker)
        };
        let (external, external_ran) = script("external", "exit 0");
        let (textconv, textconv_ran) = script("textconv", "cat \"$1\"");
        let (gpg, gpg_ran) = script("gpg", "exit 1");

        // HEAD with a signature header, as in the probe's test.
        let plain = repo.git(&["cat-file", "commit", "HEAD"]);
        let (headers, message) = plain.split_once("\n\n").unwrap();
        let signed = format!(
            "{headers}\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n abcd\n -----END PGP SIGNATURE-----\n\n{message}"
        );
        let object = git_dir.join("signed-commit");
        std::fs::write(&object, signed).unwrap();
        let hash = repo.git(&[
            "hash-object",
            "-t",
            "commit",
            "-w",
            &object.to_string_lossy(),
        ]);
        let hash = hash.trim().to_owned();
        repo.git(&["update-ref", "refs/heads/main", &hash]);

        std::fs::write(git_dir.join("info/attributes"), "*.bin diff=conv\n").unwrap();
        repo.git(&["config", "diff.conv.textconv", &textconv]);
        repo.git(&["config", "gpg.program", &gpg]);
        repo.git(&["config", "log.showSignature", "true"]);
        repo.write("a.bin", "one\ntwo\n");

        // The settings are live: a user's commands run all three.
        runner.run(repo.path(), &["diff"]).unwrap();
        assert!(
            textconv_ran.exists(),
            "textconv did not run for a user command"
        );
        repo.git(&["config", "diff.external", &external]);
        runner.run(repo.path(), &["diff"]).unwrap();
        assert!(
            external_ran.exists(),
            "diff.external did not run for a user command"
        );
        runner.run(repo.path(), &["log", "-1"]).unwrap();
        assert!(
            gpg_ran.exists(),
            "gpg.program did not run for a user command"
        );
        for marker in [&external_ran, &textconv_ran, &gpg_ran] {
            std::fs::remove_file(marker).unwrap();
        }

        let d = details(&runner, repo.path()).unwrap();
        assert_eq!(d.commits[0].hash, hash);
        let modified = diff(&runner, repo.path(), &file("a.bin", ChangeKind::Modified)).unwrap();
        assert!(modified.contains("+two"), "{modified}");
        repo.git(&["add", "a.bin"]);
        diff(&runner, repo.path(), &file("a.bin", ChangeKind::Staged)).unwrap();
        repo.write("new.bin", "x\n");
        diff(
            &runner,
            repo.path(),
            &file("new.bin", ChangeKind::Untracked),
        )
        .unwrap();
        let shown = diff(&runner, repo.path(), &DiffTarget::Commit(hash)).unwrap();
        assert!(shown.contains("a.bin"), "{shown}");
        for (name, marker) in [
            ("diff.external", &external_ran),
            ("textconv", &textconv_ran),
            ("gpg.program", &gpg_ran),
        ] {
            assert!(!marker.exists(), "the view ran the repository's {name}");
        }
    }
}
