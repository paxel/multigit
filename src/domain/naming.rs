//! Card titles, path lines and board order.

use std::cmp::Ordering;
use std::path::{Component, Path};

/// The card title: the last path component (folder name).
pub fn title(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Board order: alphabetical by folder name (case-insensitive), ties broken by full path.
pub fn board_order(a: &Path, b: &Path) -> Ordering {
    title(a)
        .to_lowercase()
        .cmp(&title(b).to_lowercase())
        .then_with(|| a.cmp(b))
}

/// The card's path line: the folders above the repository, as many trailing
/// ones as `fits` accepts, with a leading `…/` when some are cut. The nearest
/// parent is always shown; a repository at the root gets `/`.
pub fn parent_line(path: &Path, fits: impl Fn(&str) -> bool) -> String {
    let Some(parent) = path.parent() else {
        return "/".to_owned();
    };
    let full = parent.to_string_lossy().into_owned();
    if fits(&full) {
        return full;
    }
    let parts: Vec<String> = parent
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let mut shown = String::new();
    for len in 1..=parts.len() {
        let tail = parts
            .get(parts.len().saturating_sub(len)..)
            .unwrap_or_default();
        let candidate = format!("\u{2026}/{}", tail.join("/"));
        if len > 1 && !fits(&candidate) {
            break;
        }
        shown = candidate;
    }
    if shown.is_empty() { full } else { shown }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn parent_line_shows_the_whole_parent_when_it_fits() {
        assert_eq!(
            parent_line(Path::new("/home/u/dev/multigit"), |_| true),
            "/home/u/dev"
        );
        assert_eq!(parent_line(Path::new("/"), |_| true), "/");
        assert_eq!(parent_line(Path::new("/repo"), |_| false), "/");
    }

    #[test]
    fn parent_line_keeps_the_nearest_parents_that_fit() {
        let path = Path::new("/home/u/dev/multigit");
        assert_eq!(
            parent_line(path, |s| s.chars().count() <= 7),
            "\u{2026}/u/dev"
        );
        // The nearest parent stays even when it does not fit.
        assert_eq!(parent_line(path, |_| false), "\u{2026}/dev");
    }

    #[test]
    fn board_order_is_case_insensitive_then_path() {
        let mut p = paths(&["/b/Zed", "/a/api", "/z/Api", "/c/beta"]);
        p.sort_by(|a, b| board_order(a, b));
        assert_eq!(p, paths(&["/a/api", "/z/Api", "/c/beta", "/b/Zed"]));
    }
}
