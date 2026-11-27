//! Staging in the repository view: what is staged of each changed file,
//! and the git commands that stage, unstage and discard files and folders.

use crate::domain::details::{ChangeKind, FileChange};
use std::collections::BTreeMap;

/// The two status letters of a changed file, as `git status` has them:
/// the index against the last commit, and the working tree against the
/// index. `.` is "no change"; an untracked file is `?` twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileState {
    pub index: char,
    pub worktree: char,
}

impl FileState {
    const UNTRACKED: Self = Self {
        index: '?',
        worktree: '?',
    };

    /// The letters of a file whose state was not read: changed in the
    /// working tree the way `kind` says.
    pub fn guessed(kind: ChangeKind) -> Self {
        Self {
            index: '.',
            worktree: kind.letter(),
        }
    }

    /// Git does not track the file yet.
    pub fn untracked(self) -> bool {
        self == Self::UNTRACKED
    }

    /// Something of the file is staged.
    pub fn staged(self) -> bool {
        !matches!(self.index, '.' | '?')
    }

    /// Something of the file is not staged: a change in the working tree,
    /// a new file git does not track, or a conflict.
    pub fn unstaged(self) -> bool {
        self.worktree != '.' || self.index == 'U'
    }

    /// The two letters as the list shows them: a blank for "no change".
    pub fn letters(self) -> String {
        let shown = |c: char| if c == '.' { ' ' } else { c };
        format!("{}{}", shown(self.index), shown(self.worktree))
    }
}

/// The two letters of each changed file in the output of
/// `git status --porcelain=v2`, by path. A renamed file's old name is gone
/// from the index.
pub fn parse_states(porcelain_v2: &str) -> BTreeMap<String, FileState> {
    let mut states = BTreeMap::new();
    for line in porcelain_v2.lines() {
        let Some((marker, rest)) = line.split_once(' ') else {
            continue;
        };
        let fields = match marker {
            "1" => 7,
            "2" => 8,
            "u" => 9,
            "?" => {
                states.insert(rest.to_owned(), FileState::UNTRACKED);
                continue;
            }
            _ => continue,
        };
        let Some(paths) = rest.splitn(fields + 1, ' ').nth(fields) else {
            continue;
        };
        let mut xy = rest.chars();
        let state = FileState {
            index: xy.next().unwrap_or('.'),
            worktree: xy.next().unwrap_or('.'),
        };
        match paths.split_once('\t') {
            Some((path, old)) => {
                states.insert(path.to_owned(), state);
                if state.index == 'R' {
                    let gone = FileState {
                        index: 'D',
                        worktree: '.',
                    };
                    states.insert(old.to_owned(), gone);
                }
            }
            None => {
                states.insert(paths.to_owned(), state);
            }
        }
    }
    states
}

/// How much of a file, or of the files in a folder, is staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staged {
    Nothing,
    Partly,
    All,
}

/// The files of `files` a row stands for: the file with that path, or
/// every file in a folder, whose path ends with `/`.
pub fn files_of<'a>(files: &'a [FileChange], path: &str) -> Vec<&'a FileChange> {
    files
        .iter()
        .filter(|f| {
            if path.ends_with('/') {
                f.path.starts_with(path)
            } else {
                f.path == path
            }
        })
        .collect()
}

/// How much of `paths` is staged; a path without a state counts as not
/// staged.
pub fn staged(states: &BTreeMap<String, FileState>, paths: &[&str]) -> Staged {
    let (mut any, mut all) = (false, true);
    for path in paths {
        let state = states.get(*path);
        let staged = state.is_some_and(|s| s.staged());
        let rest = state.is_none_or(|s| s.unstaged());
        any |= staged;
        all &= staged && !rest;
    }
    match (any, all && !paths.is_empty()) {
        (_, true) => Staged::All,
        (true, false) => Staged::Partly,
        (false, false) => Staged::Nothing,
    }
}

/// What can be done to the changes of a file or a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAct {
    Stage,
    Unstage,
    /// Back to the last commit: staged and unstaged changes go, and a new
    /// file is deleted.
    DiscardAll,
    /// Back to what is staged: the changes in the working tree go, and a
    /// file git does not track is deleted.
    DiscardUnstaged,
}

impl FileAct {
    /// As a menu names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stage => "Stage",
            Self::Unstage => "Unstage",
            Self::DiscardAll => "Discard all changes",
            Self::DiscardUnstaged => "Discard unstaged changes",
        }
    }

    /// It throws work away and must be confirmed.
    pub fn discards(self) -> bool {
        matches!(self, Self::DiscardAll | Self::DiscardUnstaged)
    }
}

/// `git` with paths taken as they are: a `*` or `[` in a file name is no
/// pattern.
fn literal(command: &[&str], paths: &[&str]) -> Vec<String> {
    let mut args = vec!["--literal-pathspecs".to_owned()];
    args.extend(command.iter().map(|a| (*a).to_owned()));
    args.push("--".to_owned());
    args.extend(paths.iter().map(|p| (*p).to_owned()));
    args
}

/// The git commands, one after the other, that do `act` to the files at
/// `paths`. Empty when there is nothing to do.
pub fn file_steps(
    act: FileAct,
    states: &BTreeMap<String, FileState>,
    paths: &[&str],
) -> Vec<Vec<String>> {
    let state = |p: &&str| states.get(*p).copied();
    let untracked: Vec<&str> = paths
        .iter()
        .filter(|p| state(p).is_some_and(FileState::untracked))
        .copied()
        .collect();
    let tracked: Vec<&str> = paths
        .iter()
        .filter(|p| !state(p).is_some_and(FileState::untracked))
        .copied()
        .collect();
    let mut steps = Vec::new();
    match act {
        FileAct::Stage => steps.push(literal(&["add", "--all"], paths)),
        FileAct::Unstage => {
            let staged: Vec<&str> = tracked
                .iter()
                .filter(|p| state(p).is_some_and(FileState::staged))
                .copied()
                .collect();
            if !staged.is_empty() {
                // `reset` works before the first commit too.
                steps.push(literal(&["reset", "--quiet"], &staged));
            }
        }
        FileAct::DiscardAll => {
            // Files the last commit has go back to it; files only the
            // index has go away.
            let (known, added): (Vec<&str>, Vec<&str>) = tracked
                .iter()
                .partition(|p| !state(p).is_some_and(|s| s.index == 'A'));
            if !known.is_empty() {
                steps.push(literal(
                    &["restore", "--source=HEAD", "--staged", "--worktree"],
                    &known,
                ));
            }
            if !added.is_empty() {
                steps.push(literal(&["rm", "--force", "--quiet"], &added));
            }
            if !untracked.is_empty() {
                steps.push(literal(&["clean", "--force", "--quiet"], &untracked));
            }
        }
        FileAct::DiscardUnstaged => {
            let changed: Vec<&str> = tracked
                .iter()
                .filter(|p| state(p).is_some_and(|s| s.worktree != '.'))
                .copied()
                .collect();
            if !changed.is_empty() {
                steps.push(literal(&["restore", "--worktree"], &changed));
            }
            if !untracked.is_empty() {
                steps.push(literal(&["clean", "--force", "--quiet"], &untracked));
            }
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(xy: &str) -> FileState {
        let mut c = xy.chars();
        FileState {
            index: c.next().unwrap(),
            worktree: c.next().unwrap(),
        }
    }

    #[test]
    fn the_two_letters_of_each_file_are_read_from_the_status() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let status = format!(
            "# branch.head main\n\
             1 .M N... 100644 100644 100644 {oid} {oid} src/b file.rs\n\
             1 MM N... 100644 100644 100644 {oid} {oid} both.rs\n\
             1 A. N... 000000 100644 100644 {oid} {oid} new.rs\n\
             2 R. N... 100644 100644 100644 {oid} {oid} R100 renamed.rs\told.rs\n\
             u UU N... 100644 100644 100644 100644 {oid} {oid} {oid} clash.rs\n\
             ? notes.txt\n! ignored.log\n"
        );
        let states = parse_states(&status);
        let letters: Vec<(&str, String)> = states
            .iter()
            .map(|(p, s)| (p.as_str(), s.letters()))
            .collect();
        assert_eq!(
            letters,
            [
                ("both.rs", "MM".to_string()),
                ("clash.rs", "UU".to_string()),
                ("new.rs", "A ".to_string()),
                ("notes.txt", "??".to_string()),
                ("old.rs", "D ".to_string()),
                ("renamed.rs", "R ".to_string()),
                ("src/b file.rs", " M".to_string()),
            ]
        );
        assert!(states["notes.txt"].untracked());
        assert!(states["clash.rs"].unstaged());
    }

    #[test]
    fn a_file_or_folder_is_staged_all_partly_or_not() {
        let states: BTreeMap<String, FileState> = [
            ("a/staged.rs", "M."),
            ("a/both.rs", "MM"),
            ("b/unstaged.rs", ".M"),
            ("b/new.txt", "??"),
        ]
        .into_iter()
        .map(|(p, s)| (p.to_string(), state(s)))
        .collect();
        assert_eq!(staged(&states, &["a/staged.rs"]), Staged::All);
        assert_eq!(staged(&states, &["a/both.rs"]), Staged::Partly);
        assert_eq!(staged(&states, &["b/new.txt"]), Staged::Nothing);
        assert_eq!(
            staged(&states, &["a/staged.rs", "a/both.rs"]),
            Staged::Partly
        );
        assert_eq!(
            staged(&states, &["b/unstaged.rs", "b/new.txt"]),
            Staged::Nothing
        );
        assert_eq!(staged(&states, &[]), Staged::Nothing);

        let files: Vec<FileChange> = states
            .keys()
            .map(|p| FileChange {
                path: p.clone(),
                kind: ChangeKind::Modified,
            })
            .collect();
        let paths = |row: &str| -> Vec<String> {
            files_of(&files, row)
                .iter()
                .map(|f| f.path.clone())
                .collect()
        };
        assert_eq!(paths("a/"), ["a/both.rs", "a/staged.rs"]);
        assert_eq!(paths("b/new.txt"), ["b/new.txt"]);
        assert!(paths("b").is_empty(), "a folder ends with a slash");
    }

    #[test]
    fn each_act_becomes_git_commands_with_literal_paths() {
        let states: BTreeMap<String, FileState> = [
            ("old.rs", "M."),
            ("both.rs", "MM"),
            ("work.rs", ".M"),
            ("added.rs", "A."),
            ("new*.txt", "??"),
        ]
        .into_iter()
        .map(|(p, s)| (p.to_string(), state(s)))
        .collect();
        let all = ["old.rs", "both.rs", "work.rs", "added.rs", "new*.txt"];
        let lines = |act| -> Vec<String> {
            file_steps(act, &states, &all)
                .iter()
                .map(|s| s.join(" "))
                .collect()
        };
        assert_eq!(
            lines(FileAct::Stage),
            ["--literal-pathspecs add --all -- old.rs both.rs work.rs added.rs new*.txt"]
        );
        assert_eq!(
            lines(FileAct::Unstage),
            ["--literal-pathspecs reset --quiet -- old.rs both.rs added.rs"]
        );
        assert_eq!(
            lines(FileAct::DiscardAll),
            [
                "--literal-pathspecs restore --source=HEAD --staged --worktree -- old.rs both.rs work.rs",
                "--literal-pathspecs rm --force --quiet -- added.rs",
                "--literal-pathspecs clean --force --quiet -- new*.txt",
            ]
        );
        assert_eq!(
            lines(FileAct::DiscardUnstaged),
            [
                "--literal-pathspecs restore --worktree -- both.rs work.rs",
                "--literal-pathspecs clean --force --quiet -- new*.txt",
            ]
        );
        assert!(file_steps(FileAct::Unstage, &states, &["work.rs"]).is_empty());
        assert!(FileAct::DiscardAll.discards() && !FileAct::Stage.discards());
    }
}
