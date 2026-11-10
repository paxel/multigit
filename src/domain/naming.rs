//! Card titles, collision subtitles and board order.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

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

/// For each path, a subtitle that tells it apart from other entries with the same title.
///
/// Paths whose title is unique get `None`. Colliding paths get the shortest
/// `parent/.../name` suffix that is unique within the collision group; if even the
/// full path does not differ by suffix (one path is a suffix of another), the full
/// path is used.
pub fn subtitles(paths: &[PathBuf]) -> Vec<Option<String>> {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in paths.iter().enumerate() {
        groups.entry(title(p)).or_default().push(i);
    }
    let mut result = vec![None; paths.len()];
    for indices in groups.values().filter(|g| g.len() > 1) {
        for &i in indices {
            let others: Vec<&PathBuf> = indices
                .iter()
                .filter(|&&j| j != i)
                .filter_map(|&j| paths.get(j))
                .collect();
            if let (Some(path), Some(slot)) = (paths.get(i), result.get_mut(i)) {
                *slot = Some(distinguishing_suffix(path, &others));
            }
        }
    }
    result
}

fn normal_components(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect()
}

fn suffix(components: &[String], len: usize) -> &[String] {
    components
        .get(components.len().saturating_sub(len)..)
        .unwrap_or_default()
}

fn distinguishing_suffix(path: &Path, others: &[&PathBuf]) -> String {
    let own = normal_components(path);
    let others: Vec<Vec<String>> = others.iter().map(|p| normal_components(p)).collect();
    for len in 2..=own.len() {
        let candidate = suffix(&own, len);
        let clashes = others
            .iter()
            .any(|o| o.len() >= len && suffix(o, len) == candidate);
        if !clashes {
            return candidate.join("/");
        }
    }
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn unique_titles_have_no_subtitle() {
        let p = paths(&["/home/u/dev/multigit", "/home/u/dev/api"]);
        assert_eq!(subtitles(&p), vec![None, None]);
    }

    #[test]
    fn collision_uses_parent_prefix() {
        let p = paths(&["/home/u/work/api", "/home/u/side/api", "/home/u/x"]);
        assert_eq!(
            subtitles(&p),
            vec![Some("work/api".into()), Some("side/api".into()), None]
        );
    }

    #[test]
    fn collision_goes_deeper_when_parents_collide() {
        let p = paths(&["/a/work/api", "/b/work/api", "/c/side/api"]);
        assert_eq!(
            subtitles(&p),
            vec![
                Some("a/work/api".into()),
                Some("b/work/api".into()),
                Some("side/api".into())
            ]
        );
    }

    #[test]
    fn suffix_of_other_path_falls_back_to_full_path() {
        let p = paths(&["/api", "/x/api"]);
        assert_eq!(
            subtitles(&p),
            vec![Some("/api".into()), Some("x/api".into())]
        );
    }

    #[test]
    fn board_order_is_case_insensitive_then_path() {
        let mut p = paths(&["/b/Zed", "/a/api", "/z/Api", "/c/beta"]);
        p.sort_by(|a, b| board_order(a, b));
        assert_eq!(p, paths(&["/a/api", "/z/Api", "/c/beta", "/b/Zed"]));
    }
}
