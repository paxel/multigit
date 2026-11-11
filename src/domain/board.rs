//! Which cards the board shows.

use crate::domain::naming::title;
use std::path::Path;

/// Whether the filter text lets the card of `path` show: its folder name
/// contains the text, case ignored. An empty or blank filter shows all.
pub fn matches_filter(path: &Path, filter: &str) -> bool {
    let needle = filter.trim().to_lowercase();
    needle.is_empty() || title(path).to_lowercase().contains(&needle)
}

#[cfg(test)]
mod tests {
    use super::*;

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
