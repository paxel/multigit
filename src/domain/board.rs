//! Which cards the board shows, and in which order.

use crate::domain::naming::{board_order, title};
use crate::domain::repo::Snapshot;
use crate::domain::status::{Head, RepoState, Upstream};
use std::cmp::Ordering;
use std::path::Path;

/// What the board is sorted by. Every key sorts ascending; cards without a
/// value for the key come last, and equal cards are in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Path,
    Branch,
    Tag,
    DirtyFirst,
    BehindFirst,
}

impl SortKey {
    /// The key the sort button switches to next.
    pub fn next(self) -> Self {
        match self {
            Self::Name => Self::Path,
            Self::Path => Self::Branch,
            Self::Branch => Self::Tag,
            Self::Tag => Self::DirtyFirst,
            Self::DirtyFirst => Self::BehindFirst,
            Self::BehindFirst => Self::Name,
        }
    }

    /// As the sort button names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Path => "path",
            Self::Branch => "branch",
            Self::Tag => "tag",
            Self::DirtyFirst => "dirty first",
            Self::BehindFirst => "behind first",
        }
    }
}

/// A card as sorting sees it; no snapshot for a card that is loading or
/// greyed out.
#[derive(Debug, Clone, Copy)]
pub struct Sorted<'a> {
    pub path: &'a Path,
    pub snapshot: Option<&'a Snapshot>,
}

impl Sorted<'_> {
    fn branch(&self) -> Option<String> {
        match &self.snapshot?.status.head {
            Head::Branch(b) | Head::Unborn(b) => Some(b.to_lowercase()),
            Head::Detached(_) => None,
        }
    }

    fn tag(&self) -> Option<String> {
        self.snapshot?.tag.as_ref().map(|t| t.to_lowercase())
    }

    fn dirty(&self) -> bool {
        self.snapshot
            .is_some_and(|s| !s.status.counts.is_clean() || s.state != RepoState::Normal)
    }

    fn behind(&self) -> bool {
        self.snapshot.is_some_and(
            |s| matches!(s.status.upstream, Upstream::Tracking { behind, .. } if behind > 0),
        )
    }
}

/// Values in order, a missing one after all present ones.
fn missing_last<T: Ord>(a: Option<T>, b: Option<T>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// Board order of two cards under `key`.
pub fn compare(key: SortKey, a: Sorted, b: Sorted) -> Ordering {
    let by_key = match key {
        SortKey::Name => Ordering::Equal,
        SortKey::Path => a.path.cmp(b.path),
        SortKey::Branch => missing_last(a.branch(), b.branch()),
        SortKey::Tag => missing_last(a.tag(), b.tag()),
        SortKey::DirtyFirst => b.dirty().cmp(&a.dirty()),
        SortKey::BehindFirst => b.behind().cmp(&a.behind()),
    };
    by_key.then_with(|| board_order(a.path, b.path))
}

/// Whether the filter text lets the card of `path` show: its folder name
/// contains the text, case ignored. An empty or blank filter shows all.
pub fn matches_filter(path: &Path, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    needle.is_empty() || title(path).to_lowercase().contains(&needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::completion::RepoRefs;
    use crate::domain::status::{Counts, Status};
    use std::path::PathBuf;

    fn snap(branch: Option<&str>, tag: Option<&str>, modified: u32, behind: u32) -> Snapshot {
        Snapshot {
            status: Status {
                head: match branch {
                    Some(b) => Head::Branch(b.into()),
                    None => Head::Detached("1a2b3c4".into()),
                },
                upstream: Upstream::Tracking { ahead: 0, behind },
                counts: Counts {
                    modified,
                    ..Counts::default()
                },
                changed_paths: vec![],
            },
            state: RepoState::Normal,
            refs: RepoRefs::default(),
            tag: tag.map(str::to_owned),
        }
    }

    /// Folder names of the cards sorted by `key`.
    fn order(key: SortKey, cards: &[(&str, Option<Snapshot>)]) -> Vec<String> {
        let paths: Vec<PathBuf> = cards.iter().map(|c| PathBuf::from(c.0)).collect();
        let mut sorted: Vec<Sorted> = paths
            .iter()
            .zip(cards)
            .map(|(path, card)| Sorted {
                path,
                snapshot: card.1.as_ref(),
            })
            .collect();
        sorted.sort_by(|a, b| compare(key, *a, *b));
        sorted.iter().map(|s| title(s.path)).collect()
    }

    #[test]
    fn sort_keys_cycle_through_all_six_and_back() {
        let mut key = SortKey::default();
        let mut labels = Vec::new();
        for _ in 0..6 {
            labels.push(key.label());
            key = key.next();
        }
        assert_eq!(
            labels,
            [
                "name",
                "path",
                "branch",
                "tag",
                "dirty first",
                "behind first"
            ]
        );
        assert_eq!(key, SortKey::Name);
    }

    #[test]
    fn cards_sort_by_key_with_missing_values_last_and_ties_by_name() {
        let cards = [
            ("/z/alpha", Some(snap(Some("main"), None, 0, 0))),
            ("/a/Beta", Some(snap(Some("Feature"), Some("v2"), 1, 0))),
            ("/m/gamma", Some(snap(None, Some("v1"), 0, 3))),
            ("/b/delta", None),
            ("/c/epsilon", Some(snap(Some("main"), Some("v1"), 1, 3))),
        ];
        let sorted = |key| order(key, &cards);
        assert_eq!(
            sorted(SortKey::Name),
            ["alpha", "Beta", "delta", "epsilon", "gamma"]
        );
        assert_eq!(
            sorted(SortKey::Path),
            ["Beta", "delta", "epsilon", "gamma", "alpha"]
        );
        // Detached and greyed-out cards have no branch.
        assert_eq!(
            sorted(SortKey::Branch),
            ["Beta", "alpha", "epsilon", "delta", "gamma"]
        );
        assert_eq!(
            sorted(SortKey::Tag),
            ["epsilon", "gamma", "Beta", "alpha", "delta"]
        );
        assert_eq!(
            sorted(SortKey::DirtyFirst),
            ["Beta", "epsilon", "alpha", "delta", "gamma"]
        );
        assert_eq!(
            sorted(SortKey::BehindFirst),
            ["epsilon", "gamma", "alpha", "Beta", "delta"]
        );
    }

    #[test]
    fn filter_is_a_case_blind_substring_of_the_folder_name() {
        let path = Path::new("/home/etl/MultiGit");
        assert!(matches_filter(path, ""));
        assert!(matches_filter(path, "  "));
        assert!(matches_filter(path, "git"));
        assert!(matches_filter(path, " MULTI "));
        assert!(!matches_filter(path, "etl"), "parents do not count");
        assert!(!matches_filter(path, "mgt"), "no fuzzy match");
    }
}
