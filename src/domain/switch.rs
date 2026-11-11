//! What the right-click menus on a card's branch and tag offer.

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
