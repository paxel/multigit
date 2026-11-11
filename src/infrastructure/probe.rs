//! Reads the state of one repository for its card (read-only, no network).

use crate::domain::completion::RepoRefs;
use crate::domain::output::ProcessOutput;
use crate::domain::repo::{ProbeError, Snapshot};
use crate::domain::status::{RepoState, parse_porcelain_v2};
use crate::infrastructure::git::GitRunner;
use std::path::{Path, PathBuf};

fn first_error_line(out: &ProcessOutput) -> String {
    out.stderr
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("git failed")
        .to_owned()
}

fn query(runner: &GitRunner, dir: &Path, args: &[&str]) -> Result<ProcessOutput, ProbeError> {
    let out = runner
        .query(dir, args)
        .map_err(|e| ProbeError::Git(e.to_string()))?;
    if out.success() {
        Ok(out)
    } else {
        Err(ProbeError::Git(first_error_line(&out)))
    }
}

/// Status, in-progress operation and refs of the repository at `dir`.
#[tracing::instrument(skip_all, fields(dir = %dir.display()))]
pub fn probe(runner: &GitRunner, dir: &Path) -> Result<Snapshot, ProbeError> {
    if !dir.is_dir() {
        return Err(ProbeError::PathMissing);
    }
    let git_dir = query(runner, dir, &["rev-parse", "--absolute-git-dir"])
        .map_err(|_| ProbeError::NotARepo)?;
    let git_dir = PathBuf::from(git_dir.stdout.trim());

    let status_out = query(runner, dir, &["status", "--porcelain=v2", "--branch"])?;
    let status =
        parse_porcelain_v2(&status_out.stdout).map_err(|e| ProbeError::Git(e.to_string()))?;

    let refs_out = query(
        runner,
        dir,
        &[
            "for-each-ref",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let remotes_out = query(runner, dir, &["remote"])?;
    let mut refs = parse_refs(&refs_out.stdout);
    refs.remotes = remotes_out.stdout.lines().map(str::to_owned).collect();
    refs.files = status.changed_paths.clone();

    // Fails without a reachable tag (and on an unborn branch): no tag, not an
    // error. Not asked at all in a repository without tags, where describe
    // would walk the whole history on every refresh.
    let tag = if refs.tags.is_empty() {
        None
    } else {
        query(runner, dir, &["describe", "--tags", "--abbrev=0"])
            .ok()
            .map(|out| out.stdout.trim().to_owned())
            .filter(|t| !t.is_empty())
    };

    Ok(Snapshot {
        status,
        state: repo_state(&git_dir),
        refs,
        tag,
    })
}

/// Rebase and merge are only visible as files in the git dir.
fn repo_state(git_dir: &Path) -> RepoState {
    if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
        RepoState::Rebasing
    } else if git_dir.join("MERGE_HEAD").exists() {
        RepoState::Merging
    } else {
        RepoState::Normal
    }
}

/// Sort full ref names into branches, remote branches and tags.
pub fn parse_refs(output: &str) -> RepoRefs {
    let mut refs = RepoRefs::default();
    for line in output.lines() {
        if let Some(b) = line.strip_prefix("refs/heads/") {
            refs.branches.push(b.to_owned());
        } else if let Some(r) = line.strip_prefix("refs/remotes/") {
            if !r.ends_with("/HEAD") {
                refs.remote_branches.push(r.to_owned());
            }
        } else if let Some(t) = line.strip_prefix("refs/tags/") {
            refs.tags.push(t.to_owned());
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::status::{Head, Upstream};
    use crate::infrastructure::testutil::{TestRepo, isolated_runner};

    #[test]
    fn missing_path_and_not_a_repo() {
        let runner = isolated_runner();
        assert_eq!(
            probe(&runner, Path::new("/does/not/exist")),
            Err(ProbeError::PathMissing)
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(probe(&runner, dir.path()), Err(ProbeError::NotARepo));
    }

    #[test]
    fn fresh_repo_is_unborn_without_upstream() {
        let repo = TestRepo::new();
        repo.write("new.txt", "x");
        let snap = probe(&isolated_runner(), repo.path()).unwrap();
        assert_eq!(snap.status.head, Head::Unborn("main".into()));
        assert_eq!(snap.status.upstream, Upstream::None);
        assert_eq!(snap.status.counts.untracked, 1);
        assert_eq!(snap.refs.files, vec!["new.txt".to_owned()]);
        assert_eq!(snap.state, RepoState::Normal);
        assert_eq!(snap.tag, None);
    }

    #[test]
    fn tag_is_the_nearest_one_reachable_from_head() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        repo.commit("a.txt", "1");
        assert_eq!(probe(&runner, repo.path()).unwrap().tag, None);

        // Lightweight tags count; commits after the tag still show it.
        repo.git(&["tag", "v1"]);
        repo.commit("a.txt", "2");
        assert_eq!(
            probe(&runner, repo.path()).unwrap().tag.as_deref(),
            Some("v1")
        );

        // A tag on another branch is not reachable from HEAD.
        repo.git(&["switch", "-q", "-c", "side"]);
        repo.commit("b.txt", "s");
        repo.git(&["tag", "side-tag"]);
        repo.git(&["switch", "-q", "main"]);
        assert_eq!(
            probe(&runner, repo.path()).unwrap().tag.as_deref(),
            Some("v1")
        );

        repo.git(&["tag", "-a", "v2", "-m", "release"]);
        assert_eq!(
            probe(&runner, repo.path()).unwrap().tag.as_deref(),
            Some("v2")
        );
    }

    #[test]
    fn ahead_behind_against_remote() {
        let remote = TestRepo::bare();
        let a = TestRepo::clone_of(&remote);
        a.commit("one.txt", "1");
        a.git(&["push", "-q", "-u", "origin", "main"]);
        let b = TestRepo::clone_of(&remote);
        a.commit("two.txt", "2");
        a.git(&["push", "-q"]);
        b.commit("local.txt", "l");
        b.git(&["fetch", "-q"]);
        b.git(&["tag", "v1"]);

        let snap = probe(&isolated_runner(), b.path()).unwrap();
        assert_eq!(snap.status.head, Head::Branch("main".into()));
        assert_eq!(
            snap.status.upstream,
            Upstream::Tracking {
                ahead: 1,
                behind: 1
            }
        );
        assert!(snap.status.counts.is_clean());
        assert_eq!(snap.refs.branches, vec!["main".to_owned()]);
        assert_eq!(snap.refs.remote_branches, vec!["origin/main".to_owned()]);
        assert_eq!(snap.refs.remotes, vec!["origin".to_owned()]);
        assert_eq!(snap.refs.tags, vec!["v1".to_owned()]);
    }

    #[test]
    fn detached_and_merging() {
        let repo = TestRepo::new();
        repo.commit("a.txt", "base\n");
        repo.git(&["switch", "-q", "-c", "other"]);
        repo.commit("a.txt", "other\n");
        repo.git(&["switch", "-q", "main"]);
        repo.commit("a.txt", "main\n");
        let runner = isolated_runner();
        // A conflicting merge leaves MERGE_HEAD and an unmerged path.
        let merge = runner.run(repo.path(), &["merge", "other"]).unwrap();
        assert!(!merge.success());
        let snap = probe(&runner, repo.path()).unwrap();
        assert_eq!(snap.state, RepoState::Merging);
        assert_eq!(snap.status.counts.conflicts, 1);

        repo.git(&["merge", "--abort"]);
        repo.git(&["switch", "-q", "--detach", "HEAD~1"]);
        let snap = probe(&runner, repo.path()).unwrap();
        assert!(matches!(snap.status.head, Head::Detached(ref s) if s.len() == 7));
    }

    #[test]
    fn parse_refs_skips_remote_head() {
        let refs = parse_refs(
            "refs/heads/main\nrefs/remotes/origin/HEAD\nrefs/remotes/origin/main\nrefs/tags/v1\n",
        );
        assert_eq!(refs.branches, vec!["main"]);
        assert_eq!(refs.remote_branches, vec!["origin/main"]);
        assert_eq!(refs.tags, vec!["v1"]);
    }
}
