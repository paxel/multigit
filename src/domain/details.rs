//! What the read-only repository view shows: changed files, the last
//! commits, the branches, and the diff of the file or commit picked.

/// How a file differs from the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChangeKind {
    Conflict,
    Staged,
    Modified,
    Untracked,
}

impl ChangeKind {
    /// As the file list names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::Staged => "staged",
            Self::Modified => "modified",
            Self::Untracked => "untracked",
        }
    }
}

/// A changed file. A file with staged and with unstaged changes is two.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// A commit in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// How long ago, as git words it ("3 days ago").
    pub age: String,
    pub subject: String,
}

/// A branch in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: String,
    /// It is the checked-out one.
    pub current: bool,
}

/// The three views of the repository view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RepoTab {
    /// The local changes.
    Files,
    /// The history of the checked-out branch.
    Commits,
    /// All branches.
    Graph,
}

impl RepoTab {
    pub const ALL: [Self; 3] = [Self::Files, Self::Commits, Self::Graph];

    /// As its tab names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Commits => "Commits",
            Self::Graph => "Graph",
        }
    }

    /// The tab a repository opens on: its changes when it has some, its
    /// history when it is clean.
    pub fn opening(dirty: bool) -> Self {
        if dirty { Self::Files } else { Self::Commits }
    }
}

/// Where the user was in the repository view: a tab and what was picked in
/// it.
pub type Place = (RepoTab, Option<DiffTarget>);

/// The places looked at, in order, with the one shown now: what Back and
/// Forward step through, as in a browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trail<T> {
    steps: Vec<T>,
    at: usize,
}

impl<T: PartialEq> Trail<T> {
    pub fn new(first: T) -> Self {
        Self {
            steps: vec![first],
            at: 0,
        }
    }

    /// Go to a new place: what Forward led to is forgotten. Going to where
    /// one already is, is no step.
    pub fn visit(&mut self, step: T) {
        if self.steps.get(self.at) == Some(&step) {
            return;
        }
        self.steps.truncate(self.at + 1);
        self.steps.push(step);
        self.at = self.steps.len() - 1;
    }

    pub fn can_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_forward(&self) -> bool {
        self.at + 1 < self.steps.len()
    }

    /// Step back; the place now shown, or `None` at the first one.
    pub fn back(&mut self) -> Option<&T> {
        if !self.can_back() {
            return None;
        }
        self.at -= 1;
        self.steps.get(self.at)
    }

    /// Step forward; the place now shown, or `None` at the last one.
    pub fn forward(&mut self) -> Option<&T> {
        if !self.can_forward() {
            return None;
        }
        self.at += 1;
        self.steps.get(self.at)
    }
}

/// Everything the lists of the view hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub files: Vec<FileChange>,
    pub commits: Vec<LogEntry>,
    pub branches: Vec<BranchEntry>,
}

/// What the diff pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DiffTarget {
    File(FileChange),
    /// A commit by its full hash.
    Commit(String),
}

impl Details {
    /// Whether the lists still hold `target`: a selection outlives a reload
    /// only then.
    pub fn holds(&self, target: &DiffTarget) -> bool {
        match target {
            DiffTarget::File(file) => self.files.contains(file),
            DiffTarget::Commit(hash) => self.commits.iter().any(|c| &c.hash == hash),
        }
    }
}

/// How many commits the list shows.
pub const LOG_LIMIT: usize = 100;
/// The `--format` of the commit list: hash, short hash, author, age and
/// subject, tab separated.
pub const LOG_FORMAT: &str = "--format=%H%x09%h%x09%an%x09%ar%x09%s";
/// Lines of a diff the pane shows; longer ones are cut.
pub const DIFF_LINE_LIMIT: usize = 5000;

/// The changed files in the output of `git status --porcelain=v2`:
/// conflicts first, then staged, modified and untracked, each by path.
pub fn parse_changes(porcelain_v2: &str) -> Vec<FileChange> {
    let mut files = Vec::new();
    let mut add = |path: &str, kind| {
        files.push(FileChange {
            path: path.to_owned(),
            kind,
        });
    };
    for line in porcelain_v2.lines() {
        let Some((marker, rest)) = line.split_once(' ') else {
            continue;
        };
        // Fields before the path: `1` has 7, `2` has 8 (and a tab plus the
        // old path after it), `u` has 9; the first is the XY state.
        let fields = match marker {
            "1" => 7,
            "2" => 8,
            "u" => 9,
            "?" => {
                add(rest, ChangeKind::Untracked);
                continue;
            }
            _ => continue,
        };
        let Some(path) = rest.splitn(fields + 1, ' ').nth(fields) else {
            continue;
        };
        let path = path.split('\t').next().unwrap_or(path);
        if marker == "u" {
            add(path, ChangeKind::Conflict);
            continue;
        }
        let mut xy = rest.chars();
        if xy.next().is_some_and(|x| x != '.') {
            add(path, ChangeKind::Staged);
        }
        if xy.next().is_some_and(|y| y != '.') {
            add(path, ChangeKind::Modified);
        }
    }
    files.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
    files
}

/// The commits in the output of `git log` with [`LOG_FORMAT`].
pub fn parse_log(output: &str) -> Vec<LogEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\t');
            Some(LogEntry {
                hash: fields.next().filter(|h| !h.is_empty())?.to_owned(),
                short: fields.next()?.to_owned(),
                author: fields.next()?.to_owned(),
                age: fields.next()?.to_owned(),
                subject: fields.next().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

/// The branches named in `names` (one per line), the checked-out one marked.
pub fn parse_branches(names: &str, current: Option<&str>) -> Vec<BranchEntry> {
    names
        .lines()
        .filter(|name| !name.is_empty())
        .map(|name| BranchEntry {
            name: name.to_owned(),
            current: current == Some(name),
        })
        .collect()
}

/// A diff for the pane: at most [`DIFF_LINE_LIMIT`] lines, with a last line
/// that says how many were left out.
pub fn cut_diff(diff: &str) -> String {
    let total = diff.lines().count();
    if total <= DIFF_LINE_LIMIT {
        return diff.to_owned();
    }
    let mut shown: Vec<&str> = diff.lines().take(DIFF_LINE_LIMIT).collect();
    let note = format!("\u{2026} {} more lines not shown", total - DIFF_LINE_LIMIT);
    shown.push(&note);
    shown.join("\n")
}

/// How a line of a diff is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Added,
    Removed,
    /// `@@ … @@`, and the `diff`, `index`, `---`, `+++` lines above a hunk.
    Header,
    Plain,
}

pub fn line_kind(line: &str) -> LineKind {
    const HEADERS: [&str; 5] = ["+++", "---", "@@", "diff ", "index "];
    if HEADERS.iter().any(|h| line.starts_with(h)) {
        LineKind::Header
    } else if line.starts_with('+') {
        LineKind::Added
    } else if line.starts_with('-') {
        LineKind::Removed
    } else {
        LineKind::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, kind: ChangeKind) -> FileChange {
        FileChange {
            path: path.into(),
            kind,
        }
    }

    #[test]
    fn changed_files_are_read_from_the_status_by_kind() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let status = format!(
            "# branch.oid {oid}\n# branch.head main\n\
             1 .M N... 100644 100644 100644 {oid} {oid} src/b file.rs\n\
             1 MM N... 100644 100644 100644 {oid} {oid} both.rs\n\
             1 A. N... 000000 100644 100644 {oid} {oid} new.rs\n\
             2 R. N... 100644 100644 100644 {oid} {oid} R100 renamed.rs\told.rs\n\
             u UU N... 100644 100644 100644 100644 {oid} {oid} {oid} clash.rs\n\
             ? notes.txt\n! ignored.log\n"
        );
        use ChangeKind::{Conflict, Modified, Staged, Untracked};
        assert_eq!(
            parse_changes(&status),
            [
                file("clash.rs", Conflict),
                file("both.rs", Staged),
                file("new.rs", Staged),
                file("renamed.rs", Staged),
                file("both.rs", Modified),
                file("src/b file.rs", Modified),
                file("notes.txt", Untracked),
            ]
        );
        assert!(parse_changes("# branch.head main\n1 .M\nnonsense\n").is_empty());
        assert_eq!(Untracked.label(), "untracked");
        assert_eq!(Conflict.label(), "conflict");
        assert_eq!((Staged.label(), Modified.label()), ("staged", "modified"));
    }

    #[test]
    fn commits_and_branches_are_read_from_git_output() {
        let log = "aaaa\taa\tAda\t3 days ago\tFix a\tb\nbbbb\tbb\tBob\t2 weeks ago\t\n\nbroken\n";
        let commits = parse_log(log);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "Fix a\tb");
        assert_eq!(
            (commits[1].author.as_str(), commits[1].age.as_str()),
            ("Bob", "2 weeks ago")
        );
        assert_eq!(commits[1].subject, "");

        let branches = parse_branches("main\nfeature/x\n\n", Some("feature/x"));
        assert_eq!(branches.len(), 2);
        assert!(!branches[0].current && branches[1].current);
        assert!(parse_branches("main\n", None).iter().all(|b| !b.current));
    }

    #[test]
    fn tabs_are_named_and_a_repository_opens_on_its_changes_or_its_history() {
        assert_eq!(
            RepoTab::ALL.map(RepoTab::label),
            ["Files", "Commits", "Graph"]
        );
        assert_eq!(RepoTab::opening(true), RepoTab::Files);
        assert_eq!(RepoTab::opening(false), RepoTab::Commits);
    }

    #[test]
    fn back_and_forward_step_through_the_places_like_a_browser() {
        let mut trail = Trail::new(1);
        assert!(!trail.can_back() && !trail.can_forward());
        assert_eq!(trail.back(), None);
        assert_eq!(trail.forward(), None);
        trail.visit(1);
        assert!(!trail.can_back(), "the same place again is no step");
        trail.visit(2);
        trail.visit(3);
        assert!(trail.can_back() && !trail.can_forward());
        assert_eq!(trail.back(), Some(&2));
        assert_eq!(trail.back(), Some(&1));
        assert_eq!(trail.back(), None);
        assert_eq!(trail.forward(), Some(&2));
        // A new place from the middle forgets what Forward led to.
        trail.visit(9);
        assert!(!trail.can_forward());
        assert_eq!(trail.back(), Some(&2));
        assert_eq!(trail.forward(), Some(&9));
    }

    #[test]
    fn a_selection_outlives_a_reload_only_while_the_lists_hold_it() {
        let details = Details {
            files: vec![file("a.rs", ChangeKind::Modified)],
            commits: parse_log("aaaa\taa\tAda\tnow\tone\n"),
            branches: vec![],
        };
        assert!(details.holds(&DiffTarget::File(file("a.rs", ChangeKind::Modified))));
        assert!(!details.holds(&DiffTarget::File(file("a.rs", ChangeKind::Staged))));
        assert!(details.holds(&DiffTarget::Commit("aaaa".into())));
        assert!(!details.holds(&DiffTarget::Commit("cccc".into())));
    }

    #[test]
    fn long_diffs_are_cut_with_a_note_and_lines_have_kinds() {
        let short = "a\nb\n";
        assert_eq!(cut_diff(short), short);
        let long: String = (0..DIFF_LINE_LIMIT + 7)
            .map(|i| format!("line {i}\n"))
            .collect();
        let cut = cut_diff(&long);
        assert_eq!(cut.lines().count(), DIFF_LINE_LIMIT + 1);
        assert_eq!(
            cut.lines().last().unwrap(),
            "\u{2026} 7 more lines not shown"
        );

        assert_eq!(line_kind("+added"), LineKind::Added);
        assert_eq!(line_kind("-removed"), LineKind::Removed);
        assert_eq!(line_kind(" context"), LineKind::Plain);
        for header in [
            "@@ -1 +1 @@",
            "+++ b/a",
            "--- a/a",
            "diff --git a/a b/a",
            "index 1..2",
        ] {
            assert_eq!(line_kind(header), LineKind::Header, "{header}");
        }
    }
}
