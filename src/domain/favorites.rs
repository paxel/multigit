//! Favorites: named groups of repositories.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The colors a favorite can have, by name, as red, green and blue. They
/// read on a light and on a dark background.
pub const PALETTE: [(&str, [u8; 3]); 12] = [
    ("red", [0xd6, 0x45, 0x45]),
    ("orange", [0xe0, 0x7b, 0x39]),
    ("amber", [0xd9, 0xa5, 0x14]),
    ("lime", [0x7f, 0xb0, 0x3a]),
    ("green", [0x2e, 0x9e, 0x5b]),
    ("teal", [0x1f, 0x9e, 0x9e]),
    ("cyan", [0x2f, 0xa3, 0xd6]),
    ("blue", [0x3b, 0x6f, 0xd6]),
    ("indigo", [0x5b, 0x55, 0xc9]),
    ("purple", [0x94, 0x50, 0xc8]),
    ("pink", [0xd2, 0x55, 0x9a]),
    ("grey", [0x7a, 0x80, 0x88]),
];

/// The emojis a favorite can have: ones the app's font draws.
pub const EMOJIS: [&str; 24] = [
    "\u{2b50}",
    "\u{2764}",
    "\u{1f525}",
    "\u{1f680}",
    "\u{1f41b}",
    "\u{1f527}",
    "\u{1f4e6}",
    "\u{1f310}",
    "\u{1f512}",
    "\u{26a1}",
    "\u{1f3af}",
    "\u{1f4ca}",
    "\u{1f3e0}",
    "\u{2601}",
    "\u{1f3a8}",
    "\u{1f4f1}",
    "\u{2699}",
    "\u{1f433}",
    "\u{1f40d}",
    "\u{2615}",
    "\u{1f4a1}",
    "\u{1f4c1}",
    "\u{1f511}",
    "\u{1f3b5}",
];

/// Red, green and blue of the palette color `name`; grey for an unknown name
/// (a hand-edited config file).
pub fn color_rgb(name: &str) -> [u8; 3] {
    PALETTE
        .iter()
        .find(|(n, _)| *n == name)
        .map_or([0x7a, 0x80, 0x88], |(_, rgb)| *rgb)
}

/// Whether black text reads better than white on this color.
pub fn dark_text_on(rgb: [u8; 3]) -> bool {
    let [r, g, b] = rgb.map(f32::from);
    0.299 * r + 0.587 * g + 0.114 * b > 150.0
}

/// A named group of repositories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favorite {
    pub name: String,
    /// A name from [`PALETTE`].
    pub color: String,
    /// One of [`EMOJIS`], or none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(default)]
    pub repos: Vec<PathBuf>,
}

impl Favorite {
    pub fn contains(&self, repo: &Path) -> bool {
        self.repos.iter().any(|r| r == repo)
    }

    /// Emoji and name, as buttons and menus show the favorite.
    pub fn label(&self) -> String {
        match &self.emoji {
            Some(emoji) => format!("{emoji} {}", self.name),
            None => self.name.clone(),
        }
    }

    /// Put `repos` into the favorite; the ones already in it stay once.
    pub fn add(&mut self, repos: &[PathBuf]) {
        for repo in repos {
            if !self.contains(repo) {
                self.repos.push(repo.clone());
            }
        }
    }

    /// Take `repos` out of the favorite.
    pub fn remove(&mut self, repos: &[PathBuf]) {
        self.repos.retain(|r| !repos.contains(r));
    }
}

/// What the dialog for a new or changed favorite holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteDraft {
    pub name: String,
    pub color: String,
    pub emoji: Option<String>,
}

impl Default for FavoriteDraft {
    fn default() -> Self {
        Self {
            name: String::new(),
            color: PALETTE[0].0.to_owned(),
            emoji: None,
        }
    }
}

impl FavoriteDraft {
    /// The draft that changes `favorite`.
    pub fn of(favorite: &Favorite) -> Self {
        Self {
            name: favorite.name.clone(),
            color: favorite.color.clone(),
            emoji: favorite.emoji.clone(),
        }
    }

    /// A favorite needs a name.
    pub fn is_valid(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// A new favorite of `repos`.
    pub fn create(&self, repos: Vec<PathBuf>) -> Favorite {
        Favorite {
            name: self.name.trim().to_owned(),
            color: self.color.clone(),
            emoji: self.emoji.clone(),
            repos,
        }
    }

    /// Give `favorite` this name, color and emoji; its repositories stay.
    pub fn apply(&self, favorite: &mut Favorite) {
        favorite.name = self.name.trim().to_owned();
        favorite.color = self.color.clone();
        favorite.emoji = self.emoji.clone();
    }
}

/// The favorites, by position, that hold `repo`.
pub fn favorites_of(favorites: &[Favorite], repo: &Path) -> Vec<usize> {
    favorites
        .iter()
        .enumerate()
        .filter(|(_, f)| f.contains(repo))
        .map(|(i, _)| i)
        .collect()
}

/// Take `repos` out of every favorite: their cards left the board.
pub fn forget(favorites: &mut [Favorite], repos: &[PathBuf]) {
    for favorite in favorites {
        favorite.remove(repos);
    }
}

/// The active favorite after the one at `deleted` is gone.
pub fn active_after_delete(active: Option<usize>, deleted: usize) -> Option<usize> {
    match active {
        Some(a) if a == deleted => None,
        Some(a) if a > deleted => Some(a - 1),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn palette_has_twelve_distinct_named_colors() {
        let mut names: Vec<_> = PALETTE.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 12);
        assert_eq!(color_rgb("blue"), [0x3b, 0x6f, 0xd6]);
        assert_eq!(color_rgb("no such color"), color_rgb("grey"));
        assert!(dark_text_on(color_rgb("amber")));
        assert!(!dark_text_on(color_rgb("indigo")));
        let mut emojis = EMOJIS.to_vec();
        emojis.sort_unstable();
        emojis.dedup();
        assert_eq!(emojis.len(), EMOJIS.len());
    }

    #[test]
    fn draft_creates_and_changes_a_favorite() {
        let mut draft = FavoriteDraft::default();
        assert!(!draft.is_valid());
        assert_eq!(draft.color, "red");
        draft.name = "  ETL ".into();
        draft.emoji = Some(EMOJIS[3].into());
        assert!(draft.is_valid());
        let mut favorite = draft.create(paths(&["/a"]));
        assert_eq!(favorite.name, "ETL");
        assert_eq!(favorite.label(), format!("{} ETL", EMOJIS[3]));
        assert_eq!(FavoriteDraft::of(&favorite).name, "ETL");

        let change = FavoriteDraft {
            name: "Load".into(),
            color: "teal".into(),
            emoji: None,
        };
        change.apply(&mut favorite);
        assert_eq!(favorite.label(), "Load");
        assert_eq!(favorite.color, "teal");
        assert_eq!(favorite.repos, paths(&["/a"]), "the repositories stay");
    }

    #[test]
    fn repositories_join_once_and_leave() {
        let mut favorites = vec![
            FavoriteDraft::default().create(paths(&["/a", "/b"])),
            FavoriteDraft::default().create(paths(&["/b"])),
        ];
        favorites[1].add(&paths(&["/b", "/c", "/c"]));
        assert_eq!(favorites[1].repos, paths(&["/b", "/c"]));
        assert_eq!(favorites_of(&favorites, Path::new("/b")), [0, 1]);
        assert_eq!(favorites_of(&favorites, Path::new("/c")), [1]);
        assert!(favorites_of(&favorites, Path::new("/x")).is_empty());
        favorites[0].remove(&paths(&["/a", "/x"]));
        assert_eq!(favorites[0].repos, paths(&["/b"]));
        forget(&mut favorites, &paths(&["/b"]));
        assert!(favorites[0].repos.is_empty(), "an empty favorite stays");
        assert_eq!(favorites[1].repos, paths(&["/c"]));
    }

    #[test]
    fn active_favorite_follows_a_delete() {
        assert_eq!(active_after_delete(Some(2), 2), None);
        assert_eq!(active_after_delete(Some(2), 0), Some(1));
        assert_eq!(active_after_delete(Some(0), 2), Some(0));
        assert_eq!(active_after_delete(None, 0), None);
    }
}
