//! Reads the state of one repository for its card (read-only, no network).

use crate::domain::completion::RepoRefs;
use crate::domain::output::ProcessOutput;
use crate::domain::repo::{DatedRef, DatedRefs, ProbeError, Snapshot, tags_on_commit_of};
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
            REF_FORMAT,
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let remotes_out = query(runner, dir, &["remote"])?;
    let (mut refs, dated) = parse_refs(&refs_out.stdout);
    refs.remotes = remotes_out.stdout.lines().map(str::to_owned).collect();
    refs.files = status.changed_paths.clone();

    // Fails without a reachable tag (and on an unborn branch): no tag, not an
    // error. Not asked at all in a repository without tags, where describe
    // would walk the whole history on every refresh.
    let nearest = if refs.tags.is_empty() {
        None
    } else {
        query(runner, dir, &["describe", "--tags", "--abbrev=0"])
            .ok()
            .map(|out| out.stdout.trim().to_owned())
            .filter(|t| !t.is_empty())
    };
    let tags = nearest.map_or_else(Vec::new, |tag| tags_on_commit_of(&tag, &dated.tags));

    Ok(Snapshot {
        status,
        state: repo_state(&git_dir),
        refs,
        dated,
        tags,
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

/// One line per ref, tab separated: full name, object, the commit an
/// annotated tag points at (empty otherwise), and the date the ref's object
/// was made (the tag's own date, or the commit's).
const REF_FORMAT: &str =
    "--format=%(refname)%09%(objectname)%09%(*objectname)%09%(creatordate:unix)";

/// Sort the lines of [`REF_FORMAT`] into branches, remote branches and tags:
/// their names for completion, and with commit and date for the card.
pub fn parse_refs(output: &str) -> (RepoRefs, DatedRefs) {
    let mut refs = RepoRefs::default();
    let mut dated = DatedRefs::default();
    for line in output.lines() {
        let mut fields = line.split('\t');
        let full_name = fields.next().unwrap_or_default();
        let object = fields.next().unwrap_or_default();
        let peeled = fields.next().unwrap_or_default();
        let date = fields.next().and_then(|d| d.parse().ok()).unwrap_or(0);
        let entry = |name: &str| DatedRef {
            name: name.to_owned(),
            commit: if peeled.is_empty() { object } else { peeled }.to_owned(),
            date,
        };
        if let Some(b) = full_name.strip_prefix("refs/heads/") {
            refs.branches.push(b.to_owned());
            dated.branches.push(entry(b));
        } else if let Some(r) = full_name.strip_prefix("refs/remotes/") {
            if !r.ends_with("/HEAD") {
                refs.remote_branches.push(r.to_owned());
                dated.remote_branches.push(entry(r));
            }
        } else if let Some(t) = full_name.strip_prefix("refs/tags/") {
            refs.tags.push(t.to_owned());
            dated.tags.push(entry(t));
        }
    }
    (refs, dated)
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
        assert!(snap.tags.is_empty());
    }

    #[test]
    fn tag_is_the_nearest_one_reachable_from_head() {
        let repo = TestRepo::new();
        let runner = isolated_runner();
        repo.commit("a.txt", "1");
        assert!(probe(&runner, repo.path()).unwrap().tags.is_empty());

        // Lightweight tags count; commits after the tag still show it.
        repo.git(&["tag", "v1"]);
        repo.commit("a.txt", "2");
        assert_eq!(probe(&runner, repo.path()).unwrap().tags, ["v1"]);

        // A tag on another branch is not reachable from HEAD.
        repo.git(&["switch", "-q", "-c", "side"]);
        repo.commit("b.txt", "s");
        repo.git(&["tag", "side-tag"]);
        repo.git(&["switch", "-q", "main"]);
        assert_eq!(probe(&runner, repo.path()).unwrap().tags, ["v1"]);

        // Every tag of that commit shows, annotated or not, highest version
        // first; the tag of the older commit does not.
        repo.git(&["tag", "-a", "v2", "-m", "release"]);
        repo.git(&["tag", "latest"]);
        repo.git(&["tag", "-a", "v10", "-m", "release"]);
        assert_eq!(
            probe(&runner, repo.path()).unwrap().tags,
            ["v10", "v2", "latest"]
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
        let (refs, dated) = parse_refs(
            "refs/heads/main\taaa\t\t100\nrefs/remotes/origin/HEAD\taaa\t\t100\n\
             refs/remotes/origin/main\taaa\t\t100\nrefs/tags/v1\tttt\tbbb\t200\nrefs/tags/light\tbbb\t\t150\n",
        );
        assert_eq!(refs.branches, vec!["main"]);
        assert_eq!(refs.remote_branches, vec!["origin/main"]);
        assert_eq!(refs.tags, vec!["v1", "light"]);
        assert_eq!(dated.branches[0].commit, "aaa");
        assert_eq!(dated.remote_branches[0].date, 100);
        // An annotated tag carries the commit it points at and its own date.
        let tags: Vec<_> = dated
            .tags
            .iter()
            .map(|t| (t.name.as_str(), t.commit.as_str(), t.date))
            .collect();
        assert_eq!(tags, [("v1", "bbb", 200), ("light", "bbb", 150)]);
    }
}
