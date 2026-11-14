//! The Graph tab of the repository view: all branches as a condensed graph.
//! Only the commits that matter are rows of their own (branch tips, tags,
//! merges, and commits where branches part); a run of plain commits between
//! them is one row that unfolds.

use std::collections::{HashMap, HashSet};

/// A commit as the graph reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub subject: String,
}

/// What a name in the repository is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

/// A branch or tag: its short name, the commit it is on, and when that
/// commit (or the tag) was made, in seconds since 1970.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRef {
    pub name: String,
    pub kind: RefKind,
    pub hash: String,
    pub time: i64,
}

/// Everything the Graph tab holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphData {
    /// Children before parents, the commits of a line of history together.
    pub commits: Vec<GraphCommit>,
    /// The branches drawn, and the tags.
    pub refs: Vec<GraphRef>,
    /// How many old remote branches are left out.
    pub hidden: usize,
    /// The history is longer than what was read.
    pub cut: bool,
}

impl GraphData {
    pub fn holds(&self, hash: &str) -> bool {
        self.commits.iter().any(|c| c.hash == hash)
    }
}

/// How many commits the graph reads at most.
pub const GRAPH_LIMIT: usize = 3000;
/// A remote branch nobody has locally whose last commit is older than this
/// many days is left out of the graph until asked for.
pub const OLD_DAYS: i64 = 90;
/// The `--format` of `for-each-ref` for the graph: name, commit (for an
/// annotated tag the commit it points to comes third), time.
pub const REF_FORMAT: &str =
    "--format=%(refname)%09%(objectname)%09%(*objectname)%09%(creatordate:unix)";
/// The `--format` of the graph's log: hash, parents, author, subject.
pub const GRAPH_FORMAT: &str = "--format=%H%x09%P%x09%an%x09%s";

/// The branches and tags in the output of `for-each-ref` with
/// [`REF_FORMAT`]. A remote's `HEAD` is no branch.
pub fn parse_refs(output: &str) -> Vec<GraphRef> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let full = fields.next()?;
            let object = fields.next()?;
            let peeled = fields.next().unwrap_or_default();
            let time = fields.next().and_then(|t| t.parse().ok()).unwrap_or(0);
            let (kind, name) = if let Some(name) = full.strip_prefix("refs/heads/") {
                (RefKind::Local, name)
            } else if let Some(name) = full.strip_prefix("refs/remotes/") {
                (RefKind::Remote, name)
            } else {
                (RefKind::Tag, full.strip_prefix("refs/tags/")?)
            };
            if kind == RefKind::Remote && name.ends_with("/HEAD") {
                return None;
            }
            let hash = if peeled.is_empty() { object } else { peeled };
            Some(GraphRef {
                name: name.to_owned(),
                kind,
                hash: hash.to_owned(),
                time,
            })
        })
        .collect()
}

/// The commits in the output of `git log` with [`GRAPH_FORMAT`].
pub fn parse_graph_log(output: &str) -> Vec<GraphCommit> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            Some(GraphCommit {
                hash: fields.next().filter(|h| !h.is_empty())?.to_owned(),
                parents: fields
                    .next()?
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect(),
                author: fields.next()?.to_owned(),
                subject: fields.next().unwrap_or_default().to_owned(),
            })
        })
        .collect()
}

/// The branch a remote branch belongs to: `origin/main` is `main`.
fn local_name(remote: &str) -> &str {
    remote.split_once('/').map_or(remote, |(_, name)| name)
}

/// The names the graph draws, and how many it leaves out: every local
/// branch, every tag, and the remote branches but for those that nobody
/// has locally and that are older than [`OLD_DAYS`], unless `with_old`.
pub fn drawn_refs(refs: Vec<GraphRef>, now: i64, with_old: bool) -> (Vec<GraphRef>, usize) {
    let locals: HashSet<String> = refs
        .iter()
        .filter(|r| r.kind == RefKind::Local)
        .map(|r| r.name.clone())
        .collect();
    let old = |r: &GraphRef| {
        r.kind == RefKind::Remote
            && !locals.contains(local_name(&r.name))
            && now - r.time > OLD_DAYS * 86_400
    };
    let hidden = refs.iter().filter(|r| old(r)).count();
    if with_old {
        return (refs, hidden);
    }
    (refs.into_iter().filter(|r| !old(r)).collect(), hidden)
}

/// What a label on a commit names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Local,
    Remote,
    /// A local branch and its remote branch on the same commit.
    Both,
    Tag,
}

/// A name on a commit of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub text: String,
    pub kind: LabelKind,
    /// The checked-out branch.
    pub head: bool,
}

/// The labels of every commit that has some. A remote branch on the same
/// commit as its local branch shares the local one's label (`main =
/// origin`). `head` is the checked-out branch.
pub fn labels(refs: &[GraphRef], head: Option<&str>) -> HashMap<String, Vec<Label>> {
    let mut by_commit: HashMap<String, Vec<Label>> = HashMap::new();
    let on = |kind| refs.iter().filter(move |r| r.kind == kind);
    for local in on(RefKind::Local) {
        let remotes: Vec<&str> = on(RefKind::Remote)
            .filter(|r| r.hash == local.hash && local_name(&r.name) == local.name)
            .filter_map(|r| r.name.split_once('/').map(|(remote, _)| remote))
            .collect();
        let label = Label {
            text: if remotes.is_empty() {
                local.name.clone()
            } else {
                format!("{} = {}", local.name, remotes.join(", "))
            },
            kind: if remotes.is_empty() {
                LabelKind::Local
            } else {
                LabelKind::Both
            },
            head: head == Some(local.name.as_str()),
        };
        by_commit.entry(local.hash.clone()).or_default().push(label);
    }
    for remote in on(RefKind::Remote) {
        let shared =
            on(RefKind::Local).any(|l| l.hash == remote.hash && l.name == local_name(&remote.name));
        if !shared {
            by_commit
                .entry(remote.hash.clone())
                .or_default()
                .push(Label {
                    text: remote.name.clone(),
                    kind: LabelKind::Remote,
                    head: false,
                });
        }
    }
    for tag in on(RefKind::Tag) {
        by_commit.entry(tag.hash.clone()).or_default().push(Label {
            text: tag.name.clone(),
            kind: LabelKind::Tag,
            head: false,
        });
    }
    by_commit
}

/// What a row of the graph shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    Commit {
        hash: String,
        author: String,
        subject: String,
        labels: Vec<Label>,
        merge: bool,
    },
    /// A run of plain commits, folded. `first` is the hash of its newest.
    Fold { count: usize, first: String },
}

/// A row of the graph, with the lines to draw in it. Lanes are counted
/// from the left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// The lane the row's dot is in.
    pub lane: usize,
    /// A line comes into the dot from above.
    pub up: bool,
    /// A line leaves the dot downwards.
    pub down: bool,
    /// Lanes whose lines pass by.
    pub through: Vec<usize>,
    /// Lanes whose lines end in the dot, coming from above.
    pub into: Vec<usize>,
    /// Lanes that lines from the dot go to, downwards: the other parents
    /// of a merge.
    pub out: Vec<usize>,
    pub kind: RowKind,
}

impl GraphRow {
    /// One more than the rightmost lane the row draws in.
    pub fn width(&self) -> usize {
        let others = self.through.iter().chain(&self.into).chain(&self.out);
        others.copied().fold(self.lane, usize::max) + 1
    }
}

/// The rows of the graph. A commit is a row of its own when it has a label,
/// is a merge, or is where branches part; two or more plain commits in a
/// row become one fold, unless the hash of the newest is in `unfolded`.
pub fn graph_rows(
    commits: &[GraphCommit],
    labels: &HashMap<String, Vec<Label>>,
    unfolded: &HashSet<String>,
) -> Vec<GraphRow> {
    let known: HashSet<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    let mut children: HashMap<&str, usize> = HashMap::new();
    for parent in commits.iter().flat_map(|c| &c.parents) {
        *children.entry(parent.as_str()).or_default() += 1;
    }
    // Which commit each lane waits for.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows: Vec<(GraphRow, bool)> = Vec::with_capacity(commits.len());
    for commit in commits {
        let hash = commit.hash.as_str();
        let waiting: Vec<usize> = (0..lanes.len())
            .filter(|i| lanes.get(*i) == Some(&Some(hash)))
            .collect();
        let lane = waiting.first().copied().unwrap_or_else(|| free(&mut lanes));
        let into: Vec<usize> = waiting.iter().skip(1).copied().collect();
        for ended in &into {
            set(&mut lanes, *ended, None);
        }
        set(&mut lanes, lane, None);
        let through: Vec<usize> = (0..lanes.len())
            .filter(|i| lanes.get(*i).is_some_and(Option::is_some))
            .collect();
        let mut out = Vec::new();
        let mut down = false;
        for (nth, parent) in commit.parents.iter().enumerate() {
            let parent = parent.as_str();
            if !known.contains(parent) {
                continue;
            }
            if nth == 0 {
                set(&mut lanes, lane, Some(parent));
                down = true;
            } else if let Some(there) = lanes.iter().position(|l| *l == Some(parent)) {
                out.push(there);
            } else {
                let new = free(&mut lanes);
                set(&mut lanes, new, Some(parent));
                out.push(new);
            }
        }
        let own_labels = labels.get(hash).cloned().unwrap_or_default();
        let merge = commit.parents.len() > 1;
        let parts = children.get(hash).copied().unwrap_or(0) > 1;
        let plain = own_labels.is_empty() && !merge && !parts && into.is_empty();
        let row = GraphRow {
            lane,
            up: !waiting.is_empty(),
            down,
            through,
            into,
            out,
            kind: RowKind::Commit {
                hash: commit.hash.clone(),
                author: commit.author.clone(),
                subject: commit.subject.clone(),
                labels: own_labels,
                merge,
            },
        };
        rows.push((row, plain));
    }
    fold(rows, unfolded)
}

/// The first lane nothing waits in; a new one when all are taken.
fn free(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(lane) => lane,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

fn set<'a>(lanes: &mut [Option<&'a str>], lane: usize, waits_for: Option<&'a str>) {
    if let Some(slot) = lanes.get_mut(lane) {
        *slot = waits_for;
    }
}

/// Runs of two or more plain rows in one lane as one row each.
fn fold(rows: Vec<(GraphRow, bool)>, unfolded: &HashSet<String>) -> Vec<GraphRow> {
    let mut out: Vec<GraphRow> = Vec::new();
    let mut run: Vec<GraphRow> = Vec::new();
    let close = |run: &mut Vec<GraphRow>, out: &mut Vec<GraphRow>| {
        let first = match run.first().map(|r| &r.kind) {
            Some(RowKind::Commit { hash, .. }) => hash.clone(),
            _ => String::new(),
        };
        if run.len() < 2 || unfolded.contains(&first) {
            out.append(run);
            return;
        }
        let count = run.len();
        let down = run.last().is_some_and(|r| r.down);
        if let Some(mut folded) = run.drain(..).next() {
            folded.down = down;
            folded.kind = RowKind::Fold { count, first };
            out.push(folded);
        }
    };
    for (row, plain) in rows {
        let continues = plain && run.last().is_none_or(|last| last.lane == row.lane);
        if !continues {
            close(&mut run, &mut out);
        }
        if plain {
            run.push(row);
        } else {
            out.push(row);
        }
    }
    close(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> GraphCommit {
        GraphCommit {
            hash: hash.into(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: "Ada".into(),
            subject: format!("subject of {hash}"),
        }
    }

    fn a_ref(name: &str, kind: RefKind, hash: &str, time: i64) -> GraphRef {
        GraphRef {
            name: name.into(),
            kind,
            hash: hash.into(),
            time,
        }
    }

    fn label(text: &str, kind: LabelKind) -> Label {
        Label {
            text: text.into(),
            kind,
            head: false,
        }
    }

    /// Each row as `lane hash-or-fold` with its lines.
    fn drawn(rows: &[GraphRow]) -> Vec<String> {
        rows.iter()
            .map(|r| {
                let what = match &r.kind {
                    RowKind::Commit { hash, merge, .. } => {
                        format!("{hash}{}", if *merge { " merge" } else { "" })
                    }
                    RowKind::Fold { count, first } => format!("fold {count} from {first}"),
                };
                let lines = format!(
                    "{}{} through{:?} into{:?} out{:?}",
                    if r.up { "^" } else { " " },
                    if r.down { "v" } else { " " },
                    r.through,
                    r.into,
                    r.out
                );
                format!("{} {what} {lines}", r.lane)
            })
            .collect()
    }

    #[test]
    fn names_and_commits_are_read_from_git_output() {
        let refs = parse_refs(
            "refs/heads/main\taaaa\t\t1700000000\n\
             refs/heads/feature/x\tbbbb\t\t1700000100\n\
             refs/remotes/origin/main\taaaa\t\t1700000000\n\
             refs/remotes/origin/HEAD\taaaa\t\t1700000000\n\
             refs/tags/v1\tcccc\t\t1600000000\n\
             refs/tags/annotated\ttagobject\tdddd\t1650000000\n\
             refs/stash\teeee\t\t1\nnonsense\n",
        );
        assert_eq!(
            refs,
            [
                a_ref("main", RefKind::Local, "aaaa", 1_700_000_000),
                a_ref("feature/x", RefKind::Local, "bbbb", 1_700_000_100),
                a_ref("origin/main", RefKind::Remote, "aaaa", 1_700_000_000),
                a_ref("v1", RefKind::Tag, "cccc", 1_600_000_000),
                a_ref("annotated", RefKind::Tag, "dddd", 1_650_000_000),
            ]
        );
        let commits = parse_graph_log("aaaa\tbbbb cccc\tAda\tMerge\ta tab\nbbbb\t\tBob\t\n\n\tx\n");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].parents, ["bbbb", "cccc"]);
        assert_eq!(commits[0].subject, "Merge\ta tab");
        assert!(commits[1].parents.is_empty());
        assert_eq!(
            (commits[1].author.as_str(), commits[1].subject.as_str()),
            ("Bob", "")
        );
    }

    #[test]
    fn old_remote_branches_nobody_has_locally_are_left_out_until_asked_for() {
        let day = 86_400;
        let now = 1_000 * day;
        let refs = vec![
            a_ref("main", RefKind::Local, "a", now - 400 * day),
            a_ref("origin/main", RefKind::Remote, "a", now - 400 * day),
            a_ref("origin/fresh", RefKind::Remote, "b", now - 89 * day),
            a_ref("origin/stale", RefKind::Remote, "c", now - 91 * day),
            a_ref("fork/ancient", RefKind::Remote, "d", now - 900 * day),
            a_ref("v0.1", RefKind::Tag, "e", now - 900 * day),
        ];
        let names =
            |refs: &[GraphRef]| -> Vec<String> { refs.iter().map(|r| r.name.clone()).collect() };
        let (shown, hidden) = drawn_refs(refs.clone(), now, false);
        assert_eq!(hidden, 2);
        assert_eq!(
            names(&shown),
            ["main", "origin/main", "origin/fresh", "v0.1"]
        );
        let (all, hidden) = drawn_refs(refs.clone(), now, true);
        assert_eq!((all.len(), hidden), (6, 2));
        assert_eq!(drawn_refs(Vec::new(), now, false), (Vec::new(), 0));
    }

    #[test]
    fn a_remote_branch_on_its_local_branch_shares_the_label() {
        let refs = [
            a_ref("main", RefKind::Local, "a", 0),
            a_ref("origin/main", RefKind::Remote, "a", 0),
            a_ref("fork/main", RefKind::Remote, "a", 0),
            a_ref("work", RefKind::Local, "b", 0),
            a_ref("origin/work", RefKind::Remote, "c", 0),
            a_ref("origin/theirs", RefKind::Remote, "a", 0),
            a_ref("v1", RefKind::Tag, "a", 0),
        ];
        let labels = labels(&refs, Some("work"));
        assert_eq!(
            labels["a"],
            [
                label("main = origin, fork", LabelKind::Both),
                label("origin/theirs", LabelKind::Remote),
                label("v1", LabelKind::Tag),
            ]
        );
        assert_eq!(
            labels["b"],
            [Label {
                text: "work".into(),
                kind: LabelKind::Local,
                head: true
            }]
        );
        assert_eq!(labels["c"], [label("origin/work", LabelKind::Remote)]);
        assert_eq!(labels.len(), 3);
    }

    #[test]
    fn plain_runs_fold_and_branches_get_lanes() {
        // main: m3 m2 m1 base b0; feature: f2 f1 base.
        let commits = [
            commit("m3", &["m2"]),
            commit("m2", &["m1"]),
            commit("m1", &["base"]),
            commit("f2", &["f1"]),
            commit("f1", &["base"]),
            commit("base", &["b0"]),
            commit("b0", &["gone"]),
        ];
        let mut labels = HashMap::new();
        labels.insert("m3".to_owned(), vec![label("main", LabelKind::Local)]);
        labels.insert("f2".to_owned(), vec![label("feature", LabelKind::Local)]);
        let rows = graph_rows(&commits, &labels, &HashSet::new());
        assert_eq!(
            drawn(&rows),
            [
                "0 m3  v through[] into[] out[]",
                "0 fold 2 from m2 ^v through[] into[] out[]",
                "1 f2  v through[0] into[] out[]",
                "1 f1 ^v through[0] into[] out[]",
                "0 base ^v through[] into[1] out[]",
                // Its parent was not read: the line ends.
                "0 b0 ^  through[] into[] out[]",
            ]
        );
        assert_eq!(rows.iter().map(GraphRow::width).max(), Some(2));
        match &rows[0].kind {
            RowKind::Commit {
                labels, subject, ..
            } => {
                assert_eq!(labels[0].text, "main");
                assert_eq!(subject, "subject of m3");
            }
            other => panic!("{other:?}"),
        }
        // Unfolded, the run is its commits again.
        let unfolded = HashSet::from(["m2".to_owned()]);
        let rows = graph_rows(&commits, &labels, &unfolded);
        assert_eq!(rows.len(), 7);
        assert_eq!(drawn(&rows)[1], "0 m2 ^v through[] into[] out[]");
        assert_eq!(drawn(&rows)[2], "0 m1 ^v through[] into[] out[]");
    }

    #[test]
    fn a_merge_sends_a_line_to_its_other_parent_and_long_runs_fold_whole() {
        // main: tip, merge(m1, s2), m1, root; side: s2, s1, root.
        let commits = [
            commit("tip", &["merge"]),
            commit("merge", &["m1", "s2"]),
            commit("m1", &["root"]),
            commit("s2", &["s1"]),
            commit("s1", &["s0"]),
            commit("s0", &["root"]),
            commit("root", &[]),
        ];
        let mut labels = HashMap::new();
        labels.insert("tip".to_owned(), vec![label("main", LabelKind::Local)]);
        let rows = graph_rows(&commits, &labels, &HashSet::new());
        assert_eq!(
            drawn(&rows),
            [
                "0 tip  v through[] into[] out[]",
                "0 merge merge ^v through[] into[] out[1]",
                "0 m1 ^v through[1] into[] out[]",
                "1 fold 3 from s2 ^v through[0] into[] out[]",
                "0 root ^  through[] into[1] out[]",
            ]
        );
        // A history without any name: one fold.
        let line: Vec<_> = (0..40)
            .map(|i| commit(&format!("c{i}"), &[&format!("c{}", i + 1)]))
            .collect();
        let rows = graph_rows(&line, &HashMap::new(), &HashSet::new());
        // The parent of the last one was not read: no line leaves the fold.
        assert_eq!(
            drawn(&rows),
            ["0 fold 40 from c0    through[] into[] out[]"]
        );
        assert!(graph_rows(&[], &HashMap::new(), &HashSet::new()).is_empty());
        // A merge of something already waited for goes to that lane.
        let both = [
            commit("a", &["x"]),
            commit("b", &["y", "x"]),
            commit("y", &["x"]),
            commit("x", &[]),
        ];
        let rows = graph_rows(&both, &HashMap::new(), &HashSet::new());
        assert_eq!(drawn(&rows)[1], "1 b merge  v through[0] into[] out[0]");
        assert_eq!(drawn(&rows)[3], "0 x ^  through[] into[1] out[]");
        let data = GraphData {
            commits: both.to_vec(),
            ..GraphData::default()
        };
        assert!(data.holds("y") && !data.holds("nope"));
    }
}
