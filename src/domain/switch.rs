//! What the right-click menus on a card's branch and tag offer.

use crate::domain::remote::{Remote, short_url};
use crate::domain::repo::{DatedRef, DatedRefs};
use std::borrow::Cow;

/// One entry of a "Switch to" list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// As the list shows it.
    pub label: String,
    /// The git arguments that switch to it.
    pub command: String,
}

fn quote(name: &str) -> Cow<'_, str> {
    shell_words::quote(name)
}

/// Newest first; equal dates by name.
fn newest_first(refs: &[DatedRef]) -> Vec<&DatedRef> {
    let mut sorted: Vec<&DatedRef> = refs.iter().collect();
    sorted.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.name.cmp(&b.name)));
    sorted
}

/// A remote branch without its remote: `origin/feature/x` is `feature/x`.
fn without_remote<'a>(remote_branch: &'a str, remotes: &[String]) -> &'a str {
    remotes
        .iter()
        .find_map(|r| remote_branch.strip_prefix(r.as_str())?.strip_prefix('/'))
        .or_else(|| remote_branch.split_once('/').map(|(_, name)| name))
        .unwrap_or(remote_branch)
}

/// The branches to switch to: the local ones, then the remote ones that have
/// no local branch of their name, each group with the newest commit first. A
/// remote branch is switched to by making its local tracking branch.
pub fn branch_choices(dated: &DatedRefs, remotes: &[String]) -> Vec<Choice> {
    let mut choices: Vec<Choice> = newest_first(&dated.branches)
        .into_iter()
        .map(|b| Choice {
            label: b.name.clone(),
            command: format!("switch {}", quote(&b.name)),
        })
        .collect();
    let is_local = |name: &str| dated.branches.iter().any(|b| b.name == name);
    for remote in newest_first(&dated.remote_branches) {
        if !is_local(without_remote(&remote.name, remotes)) {
            choices.push(Choice {
                label: remote.name.clone(),
                command: format!("switch --track {}", quote(&remote.name)),
            });
        }
    }
    choices
}

/// The tags to switch to, the one made last first. A tag is checked out as a
/// detached HEAD.
pub fn tag_choices(dated: &DatedRefs) -> Vec<Choice> {
    newest_first(&dated.tags)
        .into_iter()
        .map(|t| Choice {
            label: t.name.clone(),
            command: format!("switch --detach {}", quote(&t.name)),
        })
        .collect()
}

/// One remote in the menu of a card's remote line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteChoice {
    /// As the menu shows it: the name and the short URL.
    pub label: String,
    /// The branch tracks this remote already.
    pub current: bool,
    /// The git arguments that make the branch track its namesake on this
    /// remote; `None` when the remote has no such branch, or no branch is
    /// checked out.
    pub command: Option<String>,
}

/// The remotes to move the upstream of `branch` to; `tracked` is the name
/// of the remote it tracks now. Only a remote that has a branch of the same
/// name can be picked.
pub fn remote_choices(
    branch: Option<&str>,
    tracked: Option<&str>,
    remotes: &[Remote],
    dated: &DatedRefs,
) -> Vec<RemoteChoice> {
    remotes
        .iter()
        .map(|remote| {
            let current = tracked == Some(remote.name.as_str());
            let there = branch
                .map(|branch| format!("{}/{branch}", remote.name))
                .filter(|name| dated.remote_branches.iter().any(|b| &b.name == name));
            RemoteChoice {
                label: format!("{}  {}", remote.name, short_url(&remote.url)),
                current,
                command: there
                    .filter(|_| !current)
                    .map(|name| format!("branch --set-upstream-to={}", quote(&name))),
            }
        })
        .collect()
}

/// The choices whose label contains `filter`, case ignored.
pub fn matching<'a>(choices: &'a [Choice], filter: &str) -> Vec<&'a Choice> {
    let needle = filter.trim().to_lowercase();
    choices
        .iter()
        .filter(|c| c.label.to_lowercase().contains(&needle))
        .collect()
}

/// The git arguments that make the branch `name` from the checked-out commit
/// and switch to it; `None` for a blank name.
pub fn new_branch_command(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty()).then(|| format!("switch -c {}", quote(name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dated(name: &str, date: i64) -> DatedRef {
        DatedRef {
            name: name.into(),
            commit: String::new(),
            date,
        }
    }

    fn pairs(choices: &[Choice]) -> Vec<(&str, &str)> {
        choices
            .iter()
            .map(|c| (c.label.as_str(), c.command.as_str()))
            .collect()
    }

    #[test]
    fn the_upstream_moves_only_to_a_remote_that_has_the_branch() {
        let remote = |name: &str, url: &str| Remote {
            name: name.into(),
            url: url.into(),
        };
        let remotes = [
            remote("origin", "https://example.org/team/x.git"),
            remote("fork", "git@github.com:me/x.git"),
            remote("mirror", "/srv/x.git"),
        ];
        let refs = DatedRefs {
            branches: vec![dated("main", 1), dated("my topic", 1)],
            remote_branches: vec![
                dated("origin/main", 1),
                dated("fork/main", 1),
                dated("fork/my topic", 1),
                dated("mirror/other", 1),
            ],
            tags: vec![],
        };
        let rows = |branch, tracked| -> Vec<(String, bool, Option<String>)> {
            remote_choices(branch, tracked, &remotes, &refs)
                .into_iter()
                .map(|c| (c.label, c.current, c.command))
                .collect()
        };
        let command = |name: &str| Some(format!("branch --set-upstream-to={name}"));
        assert_eq!(
            rows(Some("main"), Some("origin")),
            [
                ("origin  https example.org/team/x".to_owned(), true, None),
                (
                    "fork  ssh github.com/me/x".to_owned(),
                    false,
                    command("fork/main")
                ),
                ("mirror  file /srv/x".to_owned(), false, None),
            ]
        );
        // Without an upstream every remote that has the branch can be picked.
        let free = rows(Some("main"), None);
        assert_eq!(free[0].2, command("origin/main"));
        assert!(!free[0].1);
        // A name with a blank is one argument.
        assert_eq!(
            rows(Some("my topic"), None)[1].2.as_deref(),
            Some("branch --set-upstream-to='fork/my topic'")
        );
        // No branch checked out: nothing to move.
        assert!(rows(None, None).iter().all(|row| row.2.is_none()));
    }

    #[test]
    fn branches_are_local_then_remote_newest_first_without_doubles() {
        let refs = DatedRefs {
            branches: vec![dated("main", 10), dated("feature/x", 30), dated("old", 10)],
            remote_branches: vec![
                dated("origin/main", 50),
                dated("origin/feature/y", 20),
                dated("up/stream/topic", 40),
                dated("up/stream/feature/x", 60),
            ],
            tags: vec![],
        };
        let remotes = vec!["origin".to_owned(), "up/stream".to_owned()];
        assert_eq!(
            pairs(&branch_choices(&refs, &remotes)),
            [
                ("feature/x", "switch feature/x"),
                ("main", "switch main"),
                ("old", "switch old"),
                ("up/stream/topic", "switch --track up/stream/topic"),
                ("origin/feature/y", "switch --track origin/feature/y"),
            ]
        );
        // Without the list of remotes the first part of the name is the remote.
        let guessed = branch_choices(&refs, &[]);
        assert!(guessed.iter().any(|c| c.label == "up/stream/feature/x"));
        assert!(!guessed.iter().any(|c| c.label == "origin/main"));
    }

    #[test]
    fn tags_are_newest_first_and_checked_out_detached() {
        let refs = DatedRefs {
            tags: vec![dated("v1", 10), dated("it's", 30), dated("v2", 20)],
            ..DatedRefs::default()
        };
        assert_eq!(
            pairs(&tag_choices(&refs)),
            [
                ("it's", "switch --detach 'it'\\''s'"),
                ("v2", "switch --detach v2"),
                ("v1", "switch --detach v1"),
            ]
        );
        assert!(tag_choices(&DatedRefs::default()).is_empty());
    }

    #[test]
    fn filter_and_new_branch() {
        let refs = DatedRefs {
            branches: vec![dated("main", 1), dated("Feature-A", 2)],
            ..DatedRefs::default()
        };
        let choices = branch_choices(&refs, &[]);
        assert_eq!(matching(&choices, " feat ").len(), 1);
        assert_eq!(matching(&choices, "").len(), 2);
        assert!(matching(&choices, "zzz").is_empty());
        assert_eq!(
            new_branch_command(" fix/it "),
            Some("switch -c fix/it".into())
        );
        assert_eq!(new_branch_command("a b"), Some("switch -c 'a b'".into()));
        assert_eq!(new_branch_command("  "), None);
    }
}
