//! A folder typed instead of picked in a dialog: `~` for the home folder,
//! Tab to complete it.

use std::path::{Path, PathBuf};

/// `text` with a leading `~` as `home`, relative to `cwd`.
pub fn expand(text: &str, home: Option<&Path>, cwd: &Path) -> PathBuf {
    let text = text.trim();
    let path = match (text.strip_prefix('~'), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(text),
    };
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

/// Tab on a typed folder: the one folder whose name starts with the last
/// part, with a `/` after it, or the longest start all of them share.
/// Folders starting with a dot are offered only when the part does too.
/// `None` when nothing fits.
pub fn complete(text: &str, home: Option<&Path>, cwd: &Path) -> Option<String> {
    let (dir_text, part) = match text.rfind('/') {
        Some(at) => (text.get(..=at)?, text.get(at + 1..)?),
        None => ("", text),
    };
    let dir = expand(if dir_text.is_empty() { "." } else { dir_text }, home, cwd);
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(part))
        .filter(|name| part.starts_with('.') || !name.starts_with('.'))
        .collect();
    names.sort();
    let shared = match names.as_slice() {
        [] => return None,
        [only] => format!("{only}/"),
        [first, rest @ ..] => {
            let mut prefix = first.clone();
            for name in rest {
                while !name.starts_with(&prefix) {
                    prefix.pop();
                }
            }
            prefix
        }
    };
    Some(format!("{dir_text}{shared}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilde_is_the_home_folder_and_relative_paths_start_here() {
        let home = Path::new("/home/me");
        let here = Path::new("/work");
        assert_eq!(expand("~", Some(home), here), PathBuf::from("/home/me"));
        assert_eq!(
            expand("~/src", Some(home), here),
            PathBuf::from("/home/me/src")
        );
        assert_eq!(expand("src", Some(home), here), PathBuf::from("/work/src"));
        assert_eq!(expand("/opt", Some(home), here), PathBuf::from("/opt"));
        assert_eq!(
            expand("~other", Some(home), here),
            PathBuf::from("/work/~other")
        );
    }

    #[test]
    fn tab_completes_one_folder_or_what_several_share() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["projects", "pictures", "music", ".config"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        std::fs::write(dir.path().join("pinned.txt"), "").unwrap();
        let base = format!("{}/", dir.path().display());
        let complete = |typed: &str| complete(typed, None, dir.path());
        assert_eq!(
            complete(&format!("{base}mu")),
            Some(format!("{base}music/"))
        );
        assert_eq!(complete(&format!("{base}p")), Some(format!("{base}p")));
        assert_eq!(
            complete(&format!("{base}pr")),
            Some(format!("{base}projects/"))
        );
        assert_eq!(
            complete(&format!("{base}.c")),
            Some(format!("{base}.config/"))
        );
        assert_eq!(complete(&format!("{base}x")), None);
        assert_eq!(
            complete("mu"),
            Some("music/".to_owned()),
            "relative to here"
        );
    }
}
