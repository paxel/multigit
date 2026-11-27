//! A commit of one repository applied to another as a patch: the same
//! message and author, on the branch the other has checked out.

use crate::domain::output::ProcessOutput;
use crate::infrastructure::git::GitRunner;
use std::io;
use std::path::{Path, PathBuf};

fn failed(message: impl Into<String>) -> ProcessOutput {
    ProcessOutput {
        exit_code: Some(1),
        stdout: String::new(),
        stderr: format!("{}\n", message.into()),
    }
}

/// The folder git keeps a running `am` or `rebase` in, in `dir`.
fn git_path(runner: &GitRunner, dir: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    let out = runner.query(dir, &["rev-parse", "--git-path", name])?;
    if !out.success() {
        return Ok(None);
    }
    Ok(Some(dir.join(out.stdout.trim())))
}

/// Apply commit `hash` of the repository at `source` to the one at `dir`:
/// `git format-patch` there, `git am --3way` here. When it does not apply,
/// the `am` is aborted and the repository is as it was. A repository in
/// the middle of an `am` or a rebase is left alone, and a merge commit,
/// which has no patch, is not applied.
pub fn apply_commit(
    runner: &GitRunner,
    dir: &Path,
    source: &Path,
    hash: &str,
) -> io::Result<ProcessOutput> {
    if hash.is_empty() || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(failed(format!("not a commit hash: {hash}")));
    }
    let parents = runner.query(source, &["rev-list", "--parents", "-n", "1", hash])?;
    if parents.stdout.split_whitespace().count() > 2 {
        return Ok(failed("a merge commit has no patch to apply"));
    }
    let patch = runner.query(
        source,
        &["format-patch", "-1", "--stdout", "--no-signature", hash],
    )?;
    if !patch.success() || patch.stdout.trim().is_empty() {
        let why = patch.stderr.trim();
        return Ok(failed(format!("cannot make a patch of {hash}: {why}")));
    }
    for busy in ["rebase-apply", "rebase-merge"] {
        if git_path(runner, dir, busy)?.is_some_and(|p| p.exists()) {
            return Ok(failed("an am or a rebase is in progress here: left alone"));
        }
    }
    let Some(file) = git_path(runner, dir, "multigit-apply.patch")? else {
        return Ok(failed("not a git repository"));
    };
    std::fs::write(&file, &patch.stdout)?;
    let file_arg = file.to_string_lossy().into_owned();
    let applied = runner.run(dir, &["am", "--3way", "--quiet", &file_arg]);
    let _ = std::fs::remove_file(&file);
    let mut applied = applied?;
    if !applied.success() {
        // Nothing was in progress before, so a session now is this am's.
        // Without one (a dirty index stops `am` before it starts) there is
        // nothing to abort, and nothing was touched.
        let started = git_path(runner, dir, "rebase-apply")?.is_some_and(|p| p.exists());
        let note = if !started {
            "does not apply: the repository is as it was"
        } else if runner.run(dir, &["am", "--abort"])?.success() {
            "does not apply: aborted, the repository is as it was"
        } else {
            "does not apply, and `git am --abort` failed too"
        };
        applied.stderr.push_str(&format!("{note}\n"));
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

    #[test]
    fn a_commit_lands_elsewhere_with_its_message_and_author_or_not_at_all() {
        let runner = isolated_runner();
        let source = TestRepo::new();
        source.commit("a.txt", "one\ntwo\nthree\n");
        source.write("a.txt", "one\nTWO\nthree\n");
        source.git(&["commit", "-q", "-am", "Fix two"]);
        let hash = source.git(&["rev-parse", "HEAD"]).trim().to_owned();

        // A repository with the same file takes it.
        let target = TestRepo::new();
        target.commit("a.txt", "one\ntwo\nthree\n");
        let out = apply_commit(&runner, target.path(), source.path(), &hash).unwrap();
        assert!(out.success(), "{}", out.stderr);
        assert_eq!(
            target.git(&["log", "-1", "--format=%s %an"]).trim(),
            "Fix two Test"
        );
        assert!(!target.path().join(".git/multigit-apply.patch").exists());

        // One whose file differs too much is left as it was.
        let other = TestRepo::new();
        other.commit("a.txt", "something else\n");
        let before = other.git(&["rev-parse", "HEAD"]);
        let out = apply_commit(&runner, other.path(), source.path(), &hash).unwrap();
        assert!(!out.success());
        assert!(out.stderr.contains("aborted"), "{}", out.stderr);
        assert_eq!(other.git(&["rev-parse", "HEAD"]), before);
        assert!(!other.path().join(".git/rebase-apply").exists());
        assert_eq!(other.git(&["status", "--porcelain"]), "");

        // Local changes to the patched file, staged, unstaged and new,
        // are all still there after it did not apply.
        let dirty = TestRepo::new();
        dirty.commit("a.txt", "one\ntwo\nthree\n");
        dirty.write("a.txt", "one\nstaged\nthree\n");
        dirty.git(&["add", "a.txt"]);
        dirty.write("a.txt", "one\nstaged\nthree\nunstaged\n");
        dirty.write("new.txt", "untracked\n");
        let status = dirty.git(&["status", "--porcelain"]);
        let out = apply_commit(&runner, dirty.path(), source.path(), &hash).unwrap();
        assert!(!out.success());
        assert_eq!(dirty.git(&["status", "--porcelain"]), status);
        assert!(!dirty.git(&["diff", "--cached", "--stat"]).is_empty());
        let worktree = std::fs::read_to_string(dirty.path().join("a.txt")).unwrap();
        assert_eq!(worktree, "one\nstaged\nthree\nunstaged\n");
        assert!(dirty.path().join("new.txt").exists());
        assert!(!dirty.path().join(".git/rebase-apply").exists());

        // One in the middle of an am is not touched.
        std::fs::create_dir(other.path().join(".git/rebase-apply")).unwrap();
        let out = apply_commit(&runner, other.path(), source.path(), &hash).unwrap();
        assert!(out.stderr.contains("left alone"), "{}", out.stderr);
        assert!(other.path().join(".git/rebase-apply").exists());

        // A merge commit has no patch.
        source.git(&["checkout", "-q", "-b", "side", "HEAD~1"]);
        source.commit("b.txt", "b\n");
        source.git(&["checkout", "-q", "-"]);
        source.git(&["merge", "-q", "--no-edit", "side"]);
        let merge = source.git(&["rev-parse", "HEAD"]).trim().to_owned();
        let out = apply_commit(&runner, target.path(), source.path(), &merge).unwrap();
        assert!(out.stderr.contains("merge commit"), "{}", out.stderr);
    }
}
