//! Parsing of `git status --porcelain=v2 --branch` output into a card-ready model.

/// Which branch the work tree is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Head {
    /// On a named branch that has at least one commit.
    Branch(String),
    /// On a named branch without any commit yet.
    Unborn(String),
    /// Detached HEAD at the given commit (short form).
    Detached(String),
}

/// Relation of the current branch to its upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upstream {
    /// No upstream configured (or detached HEAD).
    None,
    /// Upstream configured, but the remote branch no longer exists.
    Gone,
    /// Upstream exists; `ahead` = unpushed commits, `behind` = unpulled commits.
    Tracking { ahead: u32, behind: u32 },
}

/// Counts of changed paths, as shown on a card (`+staged ~modified ?untracked !conflicts`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
    pub conflicts: u32,
}

impl Counts {
    /// True when no path is staged, modified, untracked or conflicted.
    pub fn is_clean(&self) -> bool {
        *self == Counts::default()
    }
}

/// An operation in progress that git reports through files in the git dir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoState {
    Normal,
    Rebasing,
    Merging,
}

/// Parsed porcelain v2 status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub head: Head,
    pub upstream: Upstream,
    /// The upstream of the branch as git names it (`origin/main`), when it
    /// has one, there or gone.
    pub tracking: Option<String>,
    pub counts: Counts,
    /// Paths that are changed, untracked or conflicted; used for CLI completion.
    pub changed_paths: Vec<String>,
}

/// Error for output that does not look like porcelain v2.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unexpected git status output: {0}")]
pub struct ParseError(pub String);

/// Parse the output of `git status --porcelain=v2 --branch` (newline separated, not `-z`).
pub fn parse_porcelain_v2(output: &str) -> Result<Status, ParseError> {
    let mut oid: Option<String> = None;
    let mut head_name: Option<String> = None;
    let mut tracking: Option<String> = None;
    let mut ahead_behind: Option<(u32, u32)> = None;
    let mut counts = Counts::default();
    let mut changed_paths = Vec::new();

    for line in output.lines().filter(|l| !l.is_empty()) {
        if let Some(header) = line.strip_prefix("# ") {
            parse_header(
                header,
                &mut oid,
                &mut head_name,
                &mut tracking,
                &mut ahead_behind,
            )?;
            continue;
        }
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        match kind {
            "1" | "2" => {
                let xy = rest.get(..2).ok_or_else(|| ParseError(line.to_owned()))?;
                let mut chars = xy.chars();
                if chars.next().is_some_and(|c| c != '.') {
                    counts.staged += 1;
                }
                if chars.next().is_some_and(|c| c != '.') {
                    counts.modified += 1;
                }
                changed_paths.push(entry_path(kind, rest));
            }
            "u" => {
                counts.conflicts += 1;
                changed_paths.push(entry_path(kind, rest));
            }
            "?" => {
                counts.untracked += 1;
                changed_paths.push(rest.to_owned());
            }
            "!" => {}
            _ => return Err(ParseError(line.to_owned())),
        }
    }

    let head = match (head_name.as_deref(), oid.as_deref()) {
        (Some("(detached)"), Some(oid)) => Head::Detached(short_oid(oid)),
        (Some(name), Some("(initial)")) => Head::Unborn(name.to_owned()),
        (Some(name), _) => Head::Branch(name.to_owned()),
        (None, _) => return Err(ParseError("missing # branch.head".to_owned())),
    };
    let upstream = match (tracking.is_some(), ahead_behind) {
        (false, _) => Upstream::None,
        (true, None) => Upstream::Gone,
        (true, Some((ahead, behind))) => Upstream::Tracking { ahead, behind },
    };
    Ok(Status {
        head,
        upstream,
        tracking,
        counts,
        changed_paths,
    })
}

fn parse_header(
    header: &str,
    oid: &mut Option<String>,
    head_name: &mut Option<String>,
    tracking: &mut Option<String>,
    ahead_behind: &mut Option<(u32, u32)>,
) -> Result<(), ParseError> {
    let (key, value) = header.split_once(' ').unwrap_or((header, ""));
    match key {
        "branch.oid" => *oid = Some(value.to_owned()),
        "branch.head" => *head_name = Some(value.to_owned()),
        "branch.upstream" => *tracking = Some(value.to_owned()),
        "branch.ab" => {
            let bad = || ParseError(header.to_owned());
            let (a, b) = value.split_once(' ').ok_or_else(bad)?;
            let ahead = a.strip_prefix('+').and_then(|n| n.parse().ok());
            let behind = b.strip_prefix('-').and_then(|n| n.parse().ok());
            match (ahead, behind) {
                (Some(ahead), Some(behind)) => *ahead_behind = Some((ahead, behind)),
                _ => return Err(bad()),
            }
        }
        // Other headers (e.g. stash) are not needed on a card.
        _ => {}
    }
    Ok(())
}

/// Extract the path from a changed entry. Field counts before the path:
/// `1` has 7, `2` has 8 (path is followed by a tab and the original path), `u` has 9.
fn entry_path(kind: &str, rest: &str) -> String {
    let fields_before_path = match kind {
        "1" => 7,
        "2" => 8,
        _ => 9,
    };
    let path = rest
        .splitn(fields_before_path + 1, ' ')
        .last()
        .unwrap_or("");
    path.split('\t').next().unwrap_or(path).to_owned()
}

fn short_oid(oid: &str) -> String {
    oid.chars().take(7).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn clean_branch_with_upstream() {
        let out = format!(
            "# branch.oid {OID}\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -3\n"
        );
        let status = parse_porcelain_v2(&out).unwrap();
        assert_eq!(status.head, Head::Branch("main".into()));
        assert_eq!(
            status.upstream,
            Upstream::Tracking {
                ahead: 2,
                behind: 3
            }
        );
        assert!(status.counts.is_clean());
    }

    #[test]
    fn upstream_without_ab_is_gone() {
        let out =
            format!("# branch.oid {OID}\n# branch.head topic\n# branch.upstream origin/topic\n");
        let gone = parse_porcelain_v2(&out).unwrap();
        assert_eq!(gone.upstream, Upstream::Gone);
        assert_eq!(gone.tracking.as_deref(), Some("origin/topic"));
        let none = parse_porcelain_v2(&format!("# branch.oid {OID}\n# branch.head topic\n"));
        assert_eq!(none.unwrap().tracking, None);
    }

    #[test]
    fn no_upstream() {
        let out = format!("# branch.oid {OID}\n# branch.head topic\n");
        assert_eq!(parse_porcelain_v2(&out).unwrap().upstream, Upstream::None);
    }

    #[test]
    fn detached_head_shows_short_oid() {
        let out = format!("# branch.oid {OID}\n# branch.head (detached)\n");
        assert_eq!(
            parse_porcelain_v2(&out).unwrap().head,
            Head::Detached("0123456".into())
        );
    }

    #[test]
    fn initial_commit_is_unborn() {
        let out = "# branch.oid (initial)\n# branch.head main\n? new.txt\n";
        let status = parse_porcelain_v2(out).unwrap();
        assert_eq!(status.head, Head::Unborn("main".into()));
        assert_eq!(status.counts.untracked, 1);
        assert_eq!(status.changed_paths, vec!["new.txt".to_owned()]);
    }

    #[test]
    fn counts_staged_modified_untracked_conflicts() {
        let h = "100644 100644 100644 aaaa bbbb";
        let u = "N... 100644 100644 100644 100644 aaaa bbbb cccc";
        let out = format!(
            "# branch.oid {OID}\n# branch.head main\n\
             1 M. N... {h} staged.txt\n\
             1 .M N... {h} modified file.txt\n\
             1 MM N... {h} both.txt\n\
             2 R. N... {h} R100 new name.txt\told.txt\n\
             u UU {u} conflict.txt\n\
             ? untracked.txt\n\
             ! ignored.txt\n"
        );
        let status = parse_porcelain_v2(&out).unwrap();
        assert_eq!(
            status.counts,
            Counts {
                staged: 3,
                modified: 2,
                untracked: 1,
                conflicts: 1
            }
        );
        assert_eq!(
            status.changed_paths,
            vec![
                "staged.txt",
                "modified file.txt",
                "both.txt",
                "new name.txt",
                "conflict.txt",
                "untracked.txt"
            ]
        );
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_porcelain_v2("fatal: not a git repository").is_err());
        assert!(parse_porcelain_v2("").is_err());
    }
}
