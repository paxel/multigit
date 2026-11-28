//! Checking out a branch or a commit from the Graph tab.

use crate::domain::graph::{GraphRef, RefKind};

/// One way to check out a row of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    /// As a menu names it: `Check out main`, `Check out commit 1a2b3c4`.
    pub label: String,
    /// What it checks out, for the stash message and the questions.
    pub what: String,
    /// The git arguments.
    pub args: Vec<String>,
    /// It is a branch, not the bare commit.
    pub branch: bool,
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// The ways to check out the commit `hash`: each branch on it, then the
/// commit itself (on no branch). A remote branch checks out the local one
/// of its name, which git makes tracking it when there is none; a remote
/// branch whose local one is on the same commit is left out, and so is
/// the branch that is checked out (`head`).
pub fn checkouts(refs: &[GraphRef], hash: &str, head: Option<&str>) -> Vec<Checkout> {
    let local = |name: &str| {
        refs.iter()
            .find(|r| r.kind == RefKind::Local && r.name == name)
    };
    let mut ways = Vec::new();
    for r in refs.iter().filter(|r| r.hash == hash) {
        match r.kind {
            RefKind::Tag => {}
            RefKind::Local if Some(r.name.as_str()) != head => ways.push(Checkout {
                label: format!("Check out {}", r.name),
                what: r.name.clone(),
                args: words(&["switch", "--no-guess", &r.name]),
                branch: true,
            }),
            RefKind::Local => {}
            RefKind::Remote => {
                let Some((_, name)) = r.name.split_once('/') else {
                    continue;
                };
                let existing = local(name);
                if existing.is_some_and(|l| l.hash == hash) || Some(name) == head {
                    continue;
                }
                let args = match existing {
                    Some(_) => words(&["switch", "--no-guess", name]),
                    None => words(&["switch", "--track", &r.name]),
                };
                ways.push(Checkout {
                    label: format!("Check out {}", r.name),
                    what: name.to_owned(),
                    args,
                    branch: true,
                });
            }
        }
    }
    let short: String = hash.chars().take(7).collect();
    ways.push(Checkout {
        label: format!("Check out commit {short}"),
        what: short,
        args: words(&["switch", "--detach", hash]),
        branch: false,
    });
    ways
}

/// git refused to check out because local changes would be lost.
pub fn refused_for_changes(error: &str) -> bool {
    error.contains("would be overwritten") || error.contains("Please commit your changes or stash")
}

/// The command that stashes everything before checking out `what`.
pub fn stash_before(what: &str) -> Vec<String> {
    let message = format!("multigit: before checking out {what}");
    words(&[
        "stash",
        "push",
        "--include-untracked",
        "--message",
        &message,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, kind: RefKind, hash: &str) -> GraphRef {
        GraphRef {
            name: name.into(),
            kind,
            hash: hash.into(),
            time: 0,
        }
    }

    #[test]
    fn a_row_checks_out_its_branches_or_its_commit() {
        let refs = [
            named("main", RefKind::Local, "aaaa"),
            named("origin/main", RefKind::Remote, "aaaa"),
            named("side", RefKind::Local, "aaaa"),
            named("origin/feature/x", RefKind::Remote, "aaaa"),
            named("origin/old", RefKind::Remote, "aaaa"),
            named("old", RefKind::Local, "bbbb"),
            named("v1", RefKind::Tag, "aaaa"),
        ];
        let ways = checkouts(&refs, "aaaa1234", Some("main"));
        assert_eq!(ways.len(), 1, "no branch on that commit");
        let ways = checkouts(&refs, "aaaa", Some("main"));
        let shown: Vec<(&str, String)> = ways
            .iter()
            .map(|w| (w.label.as_str(), w.args.join(" ")))
            .collect();
        assert_eq!(
            shown,
            [
                ("Check out side", "switch --no-guess side".to_string()),
                (
                    "Check out origin/feature/x",
                    "switch --track origin/feature/x".to_string()
                ),
                ("Check out origin/old", "switch --no-guess old".to_string()),
                ("Check out commit aaaa", "switch --detach aaaa".to_string()),
            ]
        );
        assert!(ways.last().is_some_and(|w| !w.branch));
        assert_eq!(ways[1].what, "feature/x");
    }

    #[test]
    fn a_refusal_for_local_changes_is_told_apart() {
        assert!(refused_for_changes(
            "error: Your local changes to the following files would be overwritten by checkout:"
        ));
        assert!(!refused_for_changes("fatal: invalid reference: x"));
        assert_eq!(
            stash_before("main").join(" "),
            "stash push --include-untracked --message multigit: before checking out main"
        );
    }
}
