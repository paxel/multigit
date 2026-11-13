//! Reads what the repository view shows (read-only, no network). The view
//! reloads by itself when its repository changes, so these calls follow the
//! rule of the automatic ones: no program the repository's own config names
//! is run for a diff or a signature.

use crate::domain::details::{
    COMMIT_FORMAT, ChangeKind, Details, DiffTarget, LOG_DATE, LOG_FORMAT, LOG_NAMES, LOG_PAGE,
    LogEntry, cut_diff, parse_branches, parse_changes, parse_log,
};
use crate::domain::graph::{
    GRAPH_FORMAT, GRAPH_LIMIT, GraphData, REF_FORMAT, RefKind, drawn_refs, parse_graph_log,
    parse_refs,
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

/// Commits of the checked-out branch, newest first: `count` of them after
/// the first `skip`.
pub fn log(
    runner: &GitRunner,
    dir: &Path,
    skip: usize,
    count: usize,
) -> Result<Vec<LogEntry>, String> {
    let (skip, count) = (format!("--skip={skip}"), format!("-{count}"));
    let args = [
        "log",
        &count,
        &skip,
        "--no-show-signature",
        LOG_NAMES,
        LOG_DATE,
        LOG_FORMAT,
    ];
    stdout(runner, dir, &args).map(|out| parse_log(&out))
}

/// The graph of all branches of the repository at `dir`, as it is at `now`
/// (seconds since 1970); `with_old` draws the old remote branches too.
pub fn graph(
    runner: &GitRunner,
    dir: &Path,
    now: i64,
    with_old: bool,
) -> Result<GraphData, String> {
    let names = stdout(
        runner,
        dir,
        &[
            "for-each-ref",
            REF_FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let (refs, hidden) = drawn_refs(parse_refs(&names), now, with_old);
    // The history of every branch drawn; tags only name commits in it.
    let mut tips: Vec<&str> = refs
        .iter()
        .filter(|r| r.kind != RefKind::Tag)
        .map(|r| r.hash.as_str())
        .collect();
    tips.sort_unstable();
    tips.dedup();
    if tips.is_empty() {
        return Ok(GraphData {
            hidden,
            ..GraphData::default()
        });
    }
    // One more than is shown tells whether the history goes on.
    let limit = format!("-{}", GRAPH_LIMIT + 1);
    let mut args = vec![
        "log",
        "--topo-order",
        &limit,
        "--no-show-signature",
        GRAPH_FORMAT,
    ];
    args.extend(tips);
    args.push("--");
    let mut commits = parse_graph_log(&stdout(runner, dir, &args)?);
    let cut = commits.len() > GRAPH_LIMIT;
    commits.truncate(GRAPH_LIMIT);
    Ok(GraphData {
        commits,
        refs,
        hidden,
        cut,
    })
}

/// The lists of the view for the repository at `dir`, with at least the
/// first `commits` commits.
pub fn details(runner: &GitRunner, dir: &Path, commits: usize) -> Result<Details, String> {
    // Every untracked file by itself, not a new folder as one entry: the
    // tree shows files.
    let status = stdout(
        runner,
        dir,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
        ],
    )?;
    let current = match parse_porcelain_v2(&status).map(|s| s.head) {
        Ok(Head::Branch(name) | Head::Unborn(name)) => Some(name),
        _ => None,
    };
    // Fails before the first commit: no commits then.
    let commits = log(runner, dir, 0, commits.max(LOG_PAGE)).unwrap_or_default();
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
        commits,
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

/// What a commit changed is what differs from its first parent, for a merge
/// too; a renamed file is one that went and one that came, as in the tree.
const COMMIT_CHANGES: [&str; 2] = ["--diff-merges=first-parent", "--no-renames"];

/// `hash`, when it is one: nothing else may stand where git expects a
/// commit.
fn a_hash(hash: &str) -> Result<&str, String> {
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("not a commit hash: {hash}"));
    }
    Ok(hash)
}

/// How the file differs from the last commit, staged or not. A file git
/// does not track yet, and any new file before the first commit, is
/// compared with nothing.
fn file_diff(
    runner: &GitRunner,
    dir: &Path,
    path: &str,
    kind: ChangeKind,
) -> Result<String, String> {
    let mut args = vec!["diff"];
    args.extend(PLAIN_DIFF);
    args.extend(["HEAD", "--", path]);
    let out = query(runner, dir, &args)?;
    let new = kind == ChangeKind::Added;
    match (out.success(), new) {
        (true, false) => Ok(out.stdout),
        (true, true) if !out.stdout.is_empty() => Ok(out.stdout),
        (_, true) => untracked_diff(runner, dir, path),
        (false, false) => Err(error_of(&out)),
    }
}

/// The diff of `target` in the repository at `dir`, cut to what the pane
/// shows.
pub fn diff(runner: &GitRunner, dir: &Path, target: &DiffTarget) -> Result<String, String> {
    let shown = match target {
        DiffTarget::File(file) => file_diff(runner, dir, &file.path, file.kind)?,
        // The commit and the names of its files, for `parse_commit`.
        DiffTarget::Commit(hash) => {
            let mut args = vec!["show", "--no-show-signature", "-z", "--name-status"];
            args.extend(COMMIT_CHANGES);
            args.extend([LOG_DATE, COMMIT_FORMAT, a_hash(hash)?]);
            return stdout(runner, dir, &args);
        }
        DiffTarget::CommitFile { hash, path } => {
            let mut args = vec!["show", "--no-show-signature", "--format=", "--patch"];
            args.extend(COMMIT_CHANGES);
            args.extend(PLAIN_DIFF);
            args.extend([a_hash(hash)?, "--", path]);
            stdout(runner, dir, &args)?
        }
    };
    Ok(cut_diff(&shown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::details::FileChange;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};
    use std::collections::HashSet;

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
        let fresh = details(&runner, repo.path(), 0).unwrap();
        assert_eq!(fresh.files.len(), 1);
        assert!(fresh.commits.is_empty() && fresh.branches.is_empty());

        repo.commit("a.txt", "one\n");
        repo.commit("b.txt", "two\n");
        repo.git(&["branch", "side"]);
        repo.write("a.txt", "one\nchanged\n");
        repo.write("b.txt", "staged\n");
        repo.git(&["add", "b.txt"]);
        repo.write("new file.txt", "new\n");
        // A new folder is listed by its files.
        std::fs::create_dir_all(repo.path().join("dir/sub")).unwrap();
        repo.write("dir/sub/one.txt", "first\n");
        let d = details(&runner, repo.path(), 0).unwrap();
        let files: Vec<_> = d.files.iter().map(|f| (f.path.as_str(), f.kind)).collect();
        assert_eq!(
            files,
            [
                ("a.txt", ChangeKind::Modified),
                ("b.txt", ChangeKind::Modified),
                ("dir/sub/one.txt", ChangeKind::Added),
                ("new file.txt", ChangeKind::Added)
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
        assert!(details(&runner, dir.path(), 0).is_err(), "not a repository");
    }

    #[test]
    fn the_log_is_read_in_pages_and_a_merge_shows_what_it_brought() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        for i in 0..5 {
            repo.commit(&format!("f{i}.txt"), "x\n");
        }
        let subjects = |entries: Vec<LogEntry>| -> Vec<String> {
            entries.into_iter().map(|e| e.subject).collect()
        };
        let all = log(&runner, repo.path(), 0, 100).unwrap();
        assert_eq!(all.len(), 5);
        assert!(all[0].time > 1_600_000_000, "{}", all[0].time);
        assert_eq!(
            all[0].date.len(),
            "2026-11-12 20:03".len(),
            "{}",
            all[0].date
        );
        assert_eq!(
            subjects(log(&runner, repo.path(), 0, 2).unwrap()),
            ["f4.txt", "f3.txt"]
        );
        assert_eq!(
            subjects(log(&runner, repo.path(), 2, 2).unwrap()),
            ["f2.txt", "f1.txt"]
        );
        assert_eq!(
            subjects(log(&runner, repo.path(), 4, 2).unwrap()),
            ["f0.txt"]
        );
        assert!(log(&runner, repo.path(), 9, 2).unwrap().is_empty());
        // The tags on a commit are read with it; branch names are not.
        assert!(all.iter().all(|entry| entry.tags.is_empty()));
        repo.git(&["tag", "v1"]);
        repo.git(&["tag", "-a", "-m", "annotated", "v1-annotated"]);
        repo.git(&["branch", "other"]);
        let tagged = log(&runner, repo.path(), 0, 2).unwrap();
        assert_eq!(tagged[0].tags, ["v1", "v1-annotated"]);
        assert!(tagged[1].tags.is_empty());
        repo.git(&["tag", "-d", "v1", "v1-annotated"]);
        repo.git(&["branch", "-D", "other"]);
        // The lists hold as many commits as were asked for, a page at least.
        assert_eq!(details(&runner, repo.path(), 3).unwrap().commits.len(), 5);

        // A merge shows what it brought into the branch it was made on.
        repo.git(&["checkout", "-q", "-b", "side"]);
        repo.commit("side.txt", "from the side\n");
        repo.git(&["checkout", "-q", "main"]);
        repo.commit("main.txt", "on main\n");
        repo.git(&["merge", "-q", "--no-ff", "-m", "Merge side", "side"]);
        let head = repo.git(&["rev-parse", "HEAD"]);
        let raw = diff(
            &runner,
            repo.path(),
            &DiffTarget::Commit(head.trim().into()),
        )
        .unwrap();
        let merge = crate::domain::details::parse_commit(&raw).unwrap();
        assert_eq!(merge.message, "Merge side");
        let paths: Vec<_> = merge.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["side.txt"]);
        let inside = DiffTarget::CommitFile {
            hash: head.trim().into(),
            path: "side.txt".into(),
        };
        let brought = diff(&runner, repo.path(), &inside).unwrap();
        assert!(brought.contains("+from the side"), "{brought}");
    }

    #[test]
    fn the_graph_reads_all_branches_their_names_and_leaves_old_remote_ones_out() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let empty = graph(&runner, repo.path(), now, false).unwrap();
        assert_eq!(empty, GraphData::default(), "no commit yet");

        repo.commit("a.txt", "one\n");
        repo.git(&["tag", "v1"]);
        repo.git(&["tag", "-a", "-m", "annotated", "v1-annotated"]);
        let root = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.commit("b.txt", "two\n");
        repo.git(&["checkout", "-q", "-b", "side", &root]);
        repo.commit("c.txt", "three\n");
        let side = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        repo.git(&["checkout", "-q", "main"]);
        let main = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
        // Remote branches without a remote: names under refs/remotes.
        repo.git(&["update-ref", "refs/remotes/origin/main", &main]);
        repo.git(&["update-ref", "refs/remotes/origin/theirs", &side]);
        repo.git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);

        let data = graph(&runner, repo.path(), now, false).unwrap();
        let hashes: HashSet<&str> = data.commits.iter().map(|c| c.hash.as_str()).collect();
        assert_eq!(hashes.len(), 3);
        assert!(hashes.contains(root.as_str()) && hashes.contains(side.as_str()));
        assert_eq!(data.commits.last().unwrap().hash, root, "parents come last");
        let of = |hash: &str| data.commits.iter().find(|c| c.hash == hash).unwrap();
        assert_eq!(of(&main).parents, std::slice::from_ref(&root));
        assert_eq!(of(&main).subject, "b.txt");
        assert_eq!(of(&main).author, "Test");
        let mut names: Vec<_> = data
            .refs
            .iter()
            .map(|r| (r.name.as_str(), r.hash.as_str()))
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            [
                ("main", main.as_str()),
                ("origin/main", main.as_str()),
                ("origin/theirs", side.as_str()),
                ("side", side.as_str()),
                ("v1", root.as_str()),
                ("v1-annotated", root.as_str()),
            ]
        );
        assert_eq!((data.hidden, data.cut), (0, false));

        // A hundred days on, the remote branch nobody has locally is old.
        let later = now + 100 * 86_400;
        let aged = graph(&runner, repo.path(), later, false).unwrap();
        assert_eq!(aged.hidden, 1);
        assert!(aged.refs.iter().all(|r| r.name != "origin/theirs"));
        assert_eq!(aged.commits.len(), 3, "its commit is still on side");
        let all = graph(&runner, repo.path(), later, true).unwrap();
        assert_eq!(all.hidden, 1);
        assert!(all.refs.iter().any(|r| r.name == "origin/theirs"));
        // Without the local branch its commit goes with it.
        repo.git(&["branch", "-D", "side"]);
        let gone = graph(&runner, repo.path(), later, false).unwrap();
        assert_eq!(gone.commits.len(), 2);
        assert_eq!(
            graph(&runner, repo.path(), later, true)
                .unwrap()
                .commits
                .len(),
            3
        );

        let dir = tempfile::tempdir().unwrap();
        assert!(graph(&runner, dir.path(), now, false).is_err());
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
        // Staged or not, a file is compared with the last commit.
        let staged = diff(&runner, repo.path(), &file("b.txt", ChangeKind::Modified)).unwrap();
        assert!(
            staged.contains("-two") && staged.contains("+staged"),
            "{staged}"
        );
        repo.write("b.txt", "staged\nand more\n");
        let both = diff(&runner, repo.path(), &file("b.txt", ChangeKind::Modified)).unwrap();
        assert!(
            both.contains("-two") && both.contains("+staged") && both.contains("+and more"),
            "{both}"
        );
        // A name like an option is still a file.
        let untracked = diff(&runner, repo.path(), &file("-new.txt", ChangeKind::Added)).unwrap();
        assert!(untracked.contains("+brand new"), "{untracked}");
        // A new file that is staged, and a deleted one.
        repo.write("added.txt", "staged new\n");
        repo.git(&["add", "added.txt"]);
        let added = diff(&runner, repo.path(), &file("added.txt", ChangeKind::Added)).unwrap();
        assert!(added.contains("+staged new"), "{added}");
        std::fs::remove_file(repo.path().join("a.txt")).unwrap();
        let deleted = diff(&runner, repo.path(), &file("a.txt", ChangeKind::Deleted)).unwrap();
        assert!(
            deleted.contains("-one") && deleted.contains("deleted file"),
            "{deleted}"
        );
        repo.write("a.txt", "one\nchanged\n");

        let head = repo.git(&["rev-parse", "HEAD"]);
        let head = head.trim();
        let raw = diff(&runner, repo.path(), &DiffTarget::Commit(head.into())).unwrap();
        let commit = crate::domain::details::parse_commit(&raw).unwrap();
        assert_eq!(commit.hash, head);
        assert_eq!(commit.author, "Test");
        assert_eq!(commit.message, "b.txt");
        assert_eq!(commit.refs, ["main"]);
        assert_eq!(
            commit.date.len(),
            "2026-11-12 20:03".len(),
            "{}",
            commit.date
        );
        assert_eq!(
            commit.files,
            [FileChange {
                path: "b.txt".into(),
                kind: ChangeKind::Added
            }]
        );
        let inside = |path: &str| DiffTarget::CommitFile {
            hash: head.into(),
            path: path.into(),
        };
        let one = diff(&runner, repo.path(), &inside("b.txt")).unwrap();
        assert!(one.contains("+two") && !one.contains("Author"), "{one}");
        assert_eq!(diff(&runner, repo.path(), &inside("a.txt")).unwrap(), "");
        let bad = DiffTarget::CommitFile {
            hash: "--output=/tmp/x".into(),
            path: "b.txt".into(),
        };
        assert!(diff(&runner, repo.path(), &bad).is_err());

        // Before the first commit there is nothing to compare with: a new
        // file shows whole.
        let unborn = TestRepo::new();
        unborn.write("first.txt", "hello\n");
        let first = diff(
            &runner,
            unborn.path(),
            &file("first.txt", ChangeKind::Added),
        )
        .unwrap();
        assert!(first.contains("+hello"), "{first}");
        assert!(
            diff(
                &runner,
                unborn.path(),
                &file("first.txt", ChangeKind::Modified)
            )
            .is_err()
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
        diff(&runner, repo.path(), &file("a.bin", ChangeKind::Modified)).unwrap();
        repo.write("new.bin", "x\n");
        diff(&runner, repo.path(), &file("new.bin", ChangeKind::Added)).unwrap();
        diff(
            &runner,
            repo.path(),
            &DiffTarget::Commit(head.trim().into()),
        )
        .unwrap();
        let inside = DiffTarget::CommitFile {
            hash: head.trim().into(),
            path: "a.bin".into(),
        };
        assert!(
            diff(&runner, repo.path(), &inside)
                .unwrap()
                .contains("+one")
        );
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

        let d = details(&runner, repo.path(), 0).unwrap();
        assert_eq!(d.commits[0].hash, hash);
        let modified = diff(&runner, repo.path(), &file("a.bin", ChangeKind::Modified)).unwrap();
        assert!(modified.contains("+two"), "{modified}");
        repo.git(&["add", "a.bin"]);
        diff(&runner, repo.path(), &file("a.bin", ChangeKind::Modified)).unwrap();
        repo.write("new.bin", "x\n");
        diff(&runner, repo.path(), &file("new.bin", ChangeKind::Added)).unwrap();
        let shown = diff(&runner, repo.path(), &DiffTarget::Commit(hash.clone())).unwrap();
        assert!(shown.contains("a.bin"), "{shown}");
        let inside = DiffTarget::CommitFile {
            hash,
            path: "a.bin".into(),
        };
        assert!(
            diff(&runner, repo.path(), &inside)
                .unwrap()
                .contains("+one")
        );
        log(&runner, repo.path(), 0, 5).unwrap();
        assert_eq!(
            graph(&runner, repo.path(), 0, true).unwrap().commits.len(),
            1
        );
        for (name, marker) in [
            ("diff.external", &external_ran),
            ("textconv", &textconv_ran),
            ("gpg.program", &gpg_ran),
        ] {
            assert!(!marker.exists(), "the view ran the repository's {name}");
        }
    }
}
