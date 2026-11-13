//! What the read-only repository view shows: changed files, the last
//! commits, the branches, and the diff of the file or commit picked.

use std::collections::HashSet;

/// How a file differs from the last commit. Staging plays no part: a file
/// is compared as it lies in the folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChangeKind {
    Conflict,
    /// New: staged, or not tracked yet.
    Added,
    Modified,
    Deleted,
}

impl ChangeKind {
    /// As a tooltip names it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Deleted => "deleted",
        }
    }

    /// The letter in front of the file's name.
    pub fn letter(self) -> char {
        match self {
            Self::Conflict => '!',
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
        }
    }
}

/// A changed file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

/// A row of the tree of changed files: a folder or a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeRow {
    /// How deep it sits; 0 at the top.
    pub depth: usize,
    /// What the row shows: a file's name, or a folder's, which is several
    /// folders in one (`src/ui`) where each holds nothing but the next.
    pub name: String,
    /// A folder's path with a `/` at its end, or the file's path.
    pub path: String,
    /// The file; `None` for a folder.
    pub file: Option<FileChange>,
    /// A folder that shows what is in it.
    pub open: bool,
}

#[derive(Default)]
struct Folder<'a> {
    folders: std::collections::BTreeMap<&'a str, Folder<'a>>,
    files: Vec<(&'a str, &'a FileChange)>,
}

impl<'a> Folder<'a> {
    fn rows(&self, at: &str, depth: usize, closed: &HashSet<String>, out: &mut Vec<TreeRow>) {
        for (name, folder) in &self.folders {
            // A folder that holds nothing but one folder is shown with it.
            let (mut name, mut folder) = ((*name).to_owned(), folder);
            while folder.files.is_empty() && folder.folders.len() == 1 {
                let Some((inner, next)) = folder.folders.iter().next() else {
                    break;
                };
                name = format!("{name}/{inner}");
                folder = next;
            }
            let path = format!("{at}{name}/");
            let open = !closed.contains(&path);
            out.push(TreeRow {
                depth,
                name,
                path: path.clone(),
                file: None,
                open,
            });
            if open {
                folder.rows(&path, depth + 1, closed, out);
            }
        }
        let mut files = self.files.clone();
        files.sort_by_key(|(name, _)| *name);
        for (name, file) in files {
            out.push(TreeRow {
                depth,
                name: name.to_owned(),
                path: file.path.clone(),
                file: Some(file.clone()),
                open: false,
            });
        }
    }
}

/// The changed files as the rows of a tree: folders before files, each in
/// name order; what lies in a folder of `closed` is left out.
pub fn tree_rows(files: &[FileChange], closed: &HashSet<String>) -> Vec<TreeRow> {
    let mut root = Folder::default();
    for file in files {
        let mut folder = &mut root;
        let mut parts: Vec<&str> = file.path.split('/').collect();
        let name = parts.pop().unwrap_or_default();
        for part in parts {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.push((name, file));
    }
    let mut rows = Vec::new();
    root.rows("", 0, closed, &mut rows);
    rows
}

/// A commit in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub hash: String,
    pub short: String,
    pub author: String,
    /// When it was written, in seconds since 1970.
    pub time: i64,
    /// The same as a date with the time of day.
    pub date: String,
    /// The tags on it.
    pub tags: Vec<String>,
    pub subject: String,
}

/// How long ago `time` was at `now`, in a few characters: "now", "12m",
/// "3h", "5d", "2mo", "4y".
pub fn short_age(now: i64, time: i64) -> String {
    let seconds = now.saturating_sub(time).max(0);
    let (minutes, hours, days) = (seconds / 60, seconds / 3600, seconds / 86_400);
    match () {
        () if minutes < 1 => "now".to_owned(),
        () if hours < 1 => format!("{minutes}m"),
        () if days < 1 => format!("{hours}h"),
        () if days < 60 => format!("{days}d"),
        () if days < 730 => format!("{}mo", days / 30),
        () => format!("{}y", days / 365),
    }
}

/// An author in two letters: the first of the first and of the last word of
/// the name, or the first two of a single word.
pub fn initials(author: &str) -> String {
    let words: Vec<&str> = author
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let letters: Vec<char> = match (words.first(), words.last()) {
        (Some(first), Some(last)) if words.len() > 1 => {
            first.chars().take(1).chain(last.chars().take(1)).collect()
        }
        (Some(only), _) => only.chars().take(2).collect(),
        _ => vec!['?'],
    };
    letters.into_iter().flat_map(char::to_uppercase).collect()
}

/// The place of an author's color on the color wheel, 0 to 1: the same for
/// the same name everywhere, and far apart for most pairs of names.
pub fn author_hue(author: &str) -> f32 {
    // FNV-1a.
    let hash = author.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    (hash % 360) as f32 / 360.0
}

/// A commit as the right pane shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitDetail {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub date: String,
    /// The branches and tags on it.
    pub refs: Vec<String>,
    /// The whole message.
    pub message: String,
    /// What it changed, against its first parent.
    pub files: Vec<FileChange>,
}

/// The `--format` a commit is read with for the right pane: hash, author,
/// mail address, date, the names on it and the message, each ended by a
/// NUL. With `-z --name-status` the changed files follow in the same way.
pub const COMMIT_FORMAT: &str = "--format=%H%x00%an%x00%ae%x00%ad%x00%D%x00%B%x00";

/// The commit in the output of `git show -z --name-status` with
/// [`COMMIT_FORMAT`].
pub fn parse_commit(raw: &str) -> Option<CommitDetail> {
    let mut fields = raw.split('\0');
    let hash = fields.next()?.trim().to_owned();
    if hash.is_empty() {
        return None;
    }
    let author = fields.next()?.to_owned();
    let email = fields.next()?.to_owned();
    let date = fields.next()?.to_owned();
    let refs = fields
        .next()?
        .split(", ")
        .map(|name| {
            name.trim_start_matches("HEAD -> ")
                .trim_start_matches("tag: ")
        })
        .filter(|name| !name.is_empty() && *name != "HEAD")
        .map(str::to_owned)
        .collect();
    let message = fields.next()?.trim_end().to_owned();
    let mut files = Vec::new();
    // git puts a NUL of its own between the message and the files, so an
    // empty field comes first; no status and no path is ever empty.
    let mut fields = fields.filter(|field| !field.trim().is_empty());
    while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
        let kind = match status.trim().chars().next() {
            Some('A') => ChangeKind::Added,
            Some('D') => ChangeKind::Deleted,
            Some('U') => ChangeKind::Conflict,
            _ => ChangeKind::Modified,
        };
        files.push(FileChange {
            path: path.to_owned(),
            kind,
        });
    }
    Some(CommitDetail {
        hash,
        author,
        email,
        date,
        refs,
        message,
        files,
    })
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
    /// What a commit did to one file.
    CommitFile {
        hash: String,
        path: String,
    },
}

impl Details {
    /// Whether the lists still hold `target`: a selection outlives a reload
    /// only then.
    pub fn holds(&self, target: &DiffTarget) -> bool {
        match target {
            DiffTarget::File(file) => self.files.contains(file),
            DiffTarget::Commit(hash) => self.commits.iter().any(|c| &c.hash == hash),
            // Never picked by itself: it is shown inside its commit.
            DiffTarget::CommitFile { .. } => false,
        }
    }
}

/// How many commits the list reads at a time.
pub const LOG_PAGE: usize = 100;
/// The `--format` of the commit list: hash, short hash, author, time, date,
/// the names on the commit and its subject, tab separated.
pub const LOG_FORMAT: &str = "--format=%H%x09%h%x09%an%x09%at%x09%ad%x09%D%x09%s";
/// Of the names on a commit the list reads the tags only.
pub const LOG_NAMES: &str = "--decorate-refs=refs/tags/";
/// What git writes before the name of a tag among the names on a commit.
const TAG_MARK: &str = "tag: ";
/// How the list's and the pane's dates are written.
pub const LOG_DATE: &str = "--date=format:%Y-%m-%d %H:%M";
/// Lines of a diff the pane shows; longer ones are cut.
pub const DIFF_LINE_LIMIT: usize = 5000;

/// The changed files in the output of `git status --porcelain=v2`, each
/// once, by path.
pub fn parse_changes(porcelain_v2: &str) -> Vec<FileChange> {
    let mut kinds: std::collections::BTreeMap<String, ChangeKind> = Default::default();
    let mut add = |path: &str, kind: ChangeKind| {
        kinds
            .entry(path.to_owned())
            .and_modify(|known| {
                // Deleted in the index and there again as a new file.
                if *known != kind && *known != ChangeKind::Conflict {
                    *known = ChangeKind::Modified;
                }
            })
            .or_insert(kind);
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
                add(rest, ChangeKind::Added);
                continue;
            }
            _ => continue,
        };
        let Some(paths) = rest.splitn(fields + 1, ' ').nth(fields) else {
            continue;
        };
        let (path, old) = match paths.split_once('\t') {
            Some((path, old)) => (path, Some(old)),
            None => (paths, None),
        };
        if marker == "u" {
            add(path, ChangeKind::Conflict);
            continue;
        }
        let mut xy = rest.chars();
        let (x, y) = (xy.next().unwrap_or('.'), xy.next().unwrap_or('.'));
        match (x, y) {
            // Added and gone again: the last commit never had it.
            ('A', 'D') => {}
            (_, 'D') | ('D', _) => add(path, ChangeKind::Deleted),
            ('A', _) => add(path, ChangeKind::Added),
            // Renamed or copied: the new name is a new file, and a renamed
            // file's old name is gone.
            ('R' | 'C', _) => {
                add(path, ChangeKind::Added);
                if let Some(old) = old.filter(|_| x == 'R') {
                    add(old, ChangeKind::Deleted);
                }
            }
            _ => add(path, ChangeKind::Modified),
        }
    }
    kinds
        .into_iter()
        .map(|(path, kind)| FileChange { path, kind })
        .collect()
}

/// The commits in the output of `git log` with [`LOG_FORMAT`].
pub fn parse_log(output: &str) -> Vec<LogEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(7, '\t');
            Some(LogEntry {
                hash: fields.next().filter(|h| !h.is_empty())?.to_owned(),
                short: fields.next()?.to_owned(),
                author: fields.next()?.to_owned(),
                time: fields.next()?.parse().ok()?,
                date: fields.next()?.to_owned(),
                tags: fields
                    .next()?
                    .split(", ")
                    .filter_map(|name| name.strip_prefix(TAG_MARK))
                    .map(str::to_owned)
                    .collect(),
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

/// What the right pane shows for a changed file, line by line: the content
/// of an added file, the deleted content of a deleted one, and the diff of
/// any other.
pub fn file_lines(kind: ChangeKind, diff: &str) -> Vec<(LineKind, &str)> {
    let (sign, shown_as) = match kind {
        ChangeKind::Added => ('+', LineKind::Plain),
        ChangeKind::Deleted => ('-', LineKind::Removed),
        ChangeKind::Modified | ChangeKind::Conflict => {
            return diff.lines().map(|l| (line_kind(l), l)).collect();
        }
    };
    let mut lines = diff.lines().skip_while(|l| !l.starts_with("@@")).peekable();
    if lines.peek().is_none() {
        // No text to show (a binary file, an empty one): what git says
        // about it, without the lines that only name the file.
        const NAMING: [&str; 7] = [
            "diff ",
            "index ",
            "--- ",
            "+++ ",
            "new file mode",
            "deleted file mode",
            "old mode",
        ];
        return diff
            .lines()
            .filter(|l| !l.is_empty() && !NAMING.iter().any(|n| l.starts_with(n)))
            .map(|l| (LineKind::Plain, l))
            .collect();
    }
    lines
        .filter(|l| !l.starts_with("@@") && !l.starts_with('\\'))
        .map(|l| (shown_as, l.strip_prefix(sign).unwrap_or(l)))
        .collect()
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
    fn changed_files_are_read_from_the_status_once_each_against_the_last_commit() {
        let oid = "0123456789abcdef0123456789abcdef01234567";
        let status = format!(
            "# branch.oid {oid}\n# branch.head main\n\
             1 .M N... 100644 100644 100644 {oid} {oid} src/b file.rs\n\
             1 MM N... 100644 100644 100644 {oid} {oid} both.rs\n\
             1 A. N... 000000 100644 100644 {oid} {oid} new.rs\n\
             1 AM N... 000000 100644 100644 {oid} {oid} new2.rs\n\
             1 AD N... 000000 100644 000000 {oid} {oid} came-and-went.rs\n\
             1 D. N... 100644 000000 000000 {oid} {oid} gone.rs\n\
             1 .D N... 100644 100644 000000 {oid} {oid} lost.rs\n\
             1 D. N... 100644 000000 000000 {oid} {oid} back.rs\n\
             2 R. N... 100644 100644 100644 {oid} {oid} R100 renamed.rs\told.rs\n\
             2 C. N... 100644 100644 100644 {oid} {oid} C100 copy.rs\torigin.rs\n\
             u UU N... 100644 100644 100644 100644 {oid} {oid} {oid} clash.rs\n\
             ? notes.txt\n? back.rs\n! ignored.log\n"
        );
        use ChangeKind::{Added, Conflict, Deleted, Modified};
        assert_eq!(
            parse_changes(&status),
            [
                file("back.rs", Modified),
                file("both.rs", Modified),
                file("clash.rs", Conflict),
                file("copy.rs", Added),
                file("gone.rs", Deleted),
                file("lost.rs", Deleted),
                file("new.rs", Added),
                file("new2.rs", Added),
                file("notes.txt", Added),
                file("old.rs", Deleted),
                file("renamed.rs", Added),
                file("src/b file.rs", Modified),
            ]
        );
        assert!(parse_changes("# branch.head main\n1 .M\nnonsense\n").is_empty());
        let all = [Conflict, Added, Modified, Deleted];
        assert_eq!(
            all.map(ChangeKind::label),
            ["conflict", "added", "modified", "deleted"]
        );
        assert_eq!(all.map(ChangeKind::letter), ['!', 'A', 'M', 'D']);
    }

    #[test]
    fn changed_files_make_a_tree_with_folders_first_and_single_folders_joined() {
        use ChangeKind::{Added, Deleted, Modified};
        let files = [
            file("zeta.txt", Modified),
            file("src/ui/view.rs", Modified),
            file("src/ui/app.rs", Added),
            file("src/main.rs", Deleted),
            file("docs/agents/deep/a.md", Added),
            file("alpha.txt", Added),
        ];
        let shape = |closed: &[&str]| -> Vec<String> {
            let closed = closed.iter().map(|c| c.to_string()).collect();
            tree_rows(&files, &closed)
                .iter()
                .map(|r| {
                    let mark = match (&r.file, r.open) {
                        (Some(f), _) => f.kind.letter(),
                        (None, true) => 'v',
                        (None, false) => '>',
                    };
                    format!("{}{mark} {}", "  ".repeat(r.depth), r.name)
                })
                .collect()
        };
        assert_eq!(
            shape(&[]),
            [
                "v docs/agents/deep",
                "  A a.md",
                "v src",
                "  v ui",
                "    A app.rs",
                "    M view.rs",
                "  D main.rs",
                "A alpha.txt",
                "M zeta.txt",
            ]
        );
        assert_eq!(
            shape(&["src/ui/", "docs/agents/deep/"]),
            [
                "> docs/agents/deep",
                "v src",
                "  > ui",
                "  D main.rs",
                "A alpha.txt",
                "M zeta.txt",
            ]
        );
        assert_eq!(shape(&["src/"]).len(), 5);
        let rows = tree_rows(&files, &HashSet::new());
        assert_eq!(rows[0].path, "docs/agents/deep/");
        assert_eq!(rows[3].path, "src/ui/");
        assert_eq!(rows[4].path, "src/ui/app.rs");
        assert_eq!(rows[4].file, Some(file("src/ui/app.rs", Added)));
        assert!(tree_rows(&[], &HashSet::new()).is_empty());
    }

    #[test]
    fn the_pane_shows_content_for_an_added_file_and_what_is_gone_for_a_deleted_one() {
        use ChangeKind::{Added, Conflict, Deleted, Modified};
        let added = "diff --git a/n b/n\nnew file mode 100644\nindex 0..1\n--- /dev/null\n+++ b/n\n@@ -0,0 +1,2 @@\n+first\n++plus\n\\ No newline at end of file\n";
        assert_eq!(
            file_lines(Added, added),
            [(LineKind::Plain, "first"), (LineKind::Plain, "+plus")]
        );
        let deleted = "diff --git a/n b/n\ndeleted file mode 100644\n--- a/n\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-gone\n--dash\n";
        assert_eq!(
            file_lines(Deleted, deleted),
            [(LineKind::Removed, "gone"), (LineKind::Removed, "-dash")]
        );
        let modified = "--- a/n\n+++ b/n\n@@ -1 +1 @@\n-old\n+new\n same\n";
        let kinds: Vec<_> = file_lines(Modified, modified).iter().map(|l| l.0).collect();
        use LineKind::{Header, Plain, Removed};
        assert_eq!(
            kinds,
            [Header, Header, Header, Removed, LineKind::Added, Plain]
        );
        assert_eq!(file_lines(Conflict, modified).len(), 6);
        // Nothing to show as text: what git says, without the naming lines.
        let binary = "diff --git a/i.png b/i.png\nnew file mode 100644\nindex 0..1\nBinary files /dev/null and b/i.png differ\n";
        assert_eq!(
            file_lines(Added, binary),
            [(LineKind::Plain, "Binary files /dev/null and b/i.png differ")]
        );
        assert!(
            file_lines(
                Added,
                "diff --git a/e b/e\nnew file mode 100644\nindex 0..0\n"
            )
            .is_empty()
        );
        assert!(file_lines(Deleted, "").is_empty());
    }

    #[test]
    fn commits_and_branches_are_read_from_git_output() {
        let log = "aaaa\taa\tAda\t1700000000\t2023-11-14 22:13\t\tFix a\tb\n\
                   bbbb\tbb\tBob\t1600000000\t2020-09-13 12:26\tHEAD -> main, tag: v1, tag: v0.9, origin/main\t\n\nbroken\n\
                   cccc\tcc\tCy\tnot a time\tdate\t\tsubject\n";
        let commits = parse_log(log);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "Fix a\tb");
        assert_eq!(commits[0].time, 1_700_000_000);
        assert_eq!(
            (commits[1].author.as_str(), commits[1].date.as_str()),
            ("Bob", "2020-09-13 12:26")
        );
        assert_eq!(commits[1].subject, "");
        // Of the names on a commit only the tags are kept.
        assert!(commits[0].tags.is_empty());
        assert_eq!(commits[1].tags, ["v1", "v0.9"]);

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
    fn ages_are_short_and_authors_have_initials_and_a_color_of_their_own() {
        let age = |seconds: i64| short_age(1_000_000_000, 1_000_000_000 - seconds);
        assert_eq!(age(0), "now");
        assert_eq!(age(59), "now");
        assert_eq!(age(60), "1m");
        assert_eq!(age(3599), "59m");
        assert_eq!(age(3 * 3600 + 5), "3h");
        assert_eq!(age(5 * 86_400), "5d");
        assert_eq!(age(59 * 86_400), "59d");
        assert_eq!(age(61 * 86_400), "2mo");
        assert_eq!(age(700 * 86_400), "23mo");
        assert_eq!(age(4 * 365 * 86_400 + 86_400), "4y");
        assert_eq!(age(-500), "now", "a clock that is behind");

        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("patrick"), "PA");
        assert_eq!(initials("Jean-Luc van der Berg"), "JB");
        assert_eq!(initials("dependabot[bot]"), "DB");
        assert_eq!(initials("x"), "X");
        assert_eq!(initials("  "), "?");
        assert_eq!(initials("émile zola"), "ÉZ");

        let hue = author_hue("Ada Lovelace");
        assert_eq!(hue, author_hue("Ada Lovelace"));
        assert!((0.0..1.0).contains(&hue));
        let names = ["Ada", "Bob", "Cy", "Dee", "Eve", "Fay", "Gus", "Hal"];
        let mut hues: Vec<u32> = names
            .iter()
            .map(|n| (author_hue(n) * 360.0) as u32)
            .collect();
        hues.sort_unstable();
        hues.dedup();
        assert!(hues.len() >= 7, "{hues:?}");
    }

    #[test]
    fn a_commit_is_read_with_its_names_message_and_files() {
        let raw = "abcd1234\0Ada L\0ada@example.org\x002026-11-12 20:03\0HEAD -> main, tag: v1.0, origin/main\0Fix it\n\nA longer\nexplanation.\n\0\nM\0src/a b.rs\0A\0new.rs\0D\0old.rs\0T\0link\0";
        let commit = parse_commit(raw).unwrap();
        assert_eq!(commit.hash, "abcd1234");
        assert_eq!(
            (commit.author.as_str(), commit.email.as_str()),
            ("Ada L", "ada@example.org")
        );
        assert_eq!(commit.date, "2026-11-12 20:03");
        assert_eq!(commit.refs, ["main", "v1.0", "origin/main"]);
        assert_eq!(commit.message, "Fix it\n\nA longer\nexplanation.");
        let files: Vec<_> = commit
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.kind))
            .collect();
        use ChangeKind::{Added, Deleted, Modified};
        assert_eq!(
            files,
            [
                ("src/a b.rs", Modified),
                ("new.rs", Added),
                ("old.rs", Deleted),
                ("link", Modified)
            ]
        );
        // As git writes it: a NUL of its own before the files.
        let real = parse_commit("abcd\0A\0a@b\0d\0\0msg\n\0\0A\0b.txt\0M\0c d.txt\0").unwrap();
        assert_eq!(
            real.files,
            [file("b.txt", Added), file("c d.txt", Modified)]
        );
        // No names, no files: a commit that changed nothing, with HEAD
        // detached on it.
        let bare = parse_commit("abcd\0A\0a@b\0d\0HEAD\0msg\n\0").unwrap();
        assert!(bare.refs.is_empty() && bare.files.is_empty());
        assert_eq!(bare.message, "msg");
        assert_eq!(parse_commit(""), None);
        assert_eq!(parse_commit("abcd\0only an author"), None);
    }

    #[test]
    fn a_selection_outlives_a_reload_only_while_the_lists_hold_it() {
        let details = Details {
            files: vec![file("a.rs", ChangeKind::Modified)],
            commits: parse_log("aaaa\taa\tAda\t1\tthen\t\tone\n"),
            branches: vec![],
        };
        let inside = DiffTarget::CommitFile {
            hash: "aaaa".into(),
            path: "a.rs".into(),
        };
        assert!(!details.holds(&inside));
        assert!(details.holds(&DiffTarget::File(file("a.rs", ChangeKind::Modified))));
        assert!(!details.holds(&DiffTarget::File(file("a.rs", ChangeKind::Deleted))));
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
