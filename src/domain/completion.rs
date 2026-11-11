//! CLI completion: candidates for the word under the cursor and the Tab behaviour.

use crate::domain::hints::{self, ValueKind};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Subcommand names and per-command option lists as reported by git itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// From `git --list-cmds=...`.
    pub subcommands: Vec<String>,
    /// From `git <cmd> --git-completion-helper`; a missing key means "not asked yet".
    pub options: HashMap<String, Vec<String>>,
}

/// Refs and paths of one repository, used to build completion values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoRefs {
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<String>,
    pub tags: Vec<String>,
    pub files: Vec<String>,
}

/// For each value, in how many of the marked repos it exists.
pub type Counted = BTreeMap<String, usize>;

/// Union of the refs of all marked repos.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoValues {
    /// Number of marked repos.
    pub total: usize,
    pub branches: Counted,
    pub remote_branches: Counted,
    pub remotes: Counted,
    pub tags: Counted,
    pub files: Counted,
}

impl RepoValues {
    /// Union over the given repos, counting how many repos have each value.
    pub fn union<'a, I: IntoIterator<Item = &'a RepoRefs>>(repos: I) -> Self {
        fn add(target: &mut Counted, values: &[String]) {
            let unique: BTreeSet<&String> = values.iter().collect();
            for v in unique {
                *target.entry(v.clone()).or_default() += 1;
            }
        }
        let mut result = Self::default();
        for r in repos {
            result.total += 1;
            add(&mut result.branches, &r.branches);
            add(&mut result.remote_branches, &r.remote_branches);
            add(&mut result.remotes, &r.remotes);
            add(&mut result.tags, &r.tags);
            add(&mut result.files, &r.files);
        }
        result
    }
}

/// One entry of the completion popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub description: Option<String>,
    /// `in n/m` when the value exists only in some marked repos.
    pub tag: Option<String>,
    /// Whether accepting the candidate appends a space (not for `--opt=`).
    pub append_space: bool,
}

impl Candidate {
    fn plain(text: &str, description: Option<&str>) -> Self {
        Self {
            text: text.to_owned(),
            description: description.map(str::to_owned),
            tag: None,
            append_space: !text.ends_with('='),
        }
    }

    /// Text inserted when the candidate is accepted.
    pub fn insertion(&self) -> String {
        if self.append_space {
            format!("{} ", self.text)
        } else {
            self.text.clone()
        }
    }
}

/// Result of completing the word that ends at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Byte offset where the word being completed starts.
    pub word_start: usize,
    /// The part of the word already typed.
    pub partial: String,
    pub candidates: Vec<Candidate>,
    /// Subcommand whose git option list is not known yet and should be fetched.
    pub missing_options_for: Option<String>,
}

/// Length of a leading `git` word plus the whitespace after it; 0 when absent.
pub fn git_prefix_len(line: &str) -> usize {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix("git") else {
        return 0;
    };
    if !rest.starts_with(char::is_whitespace) {
        return 0;
    }
    line.len() - rest.trim_start().len()
}

/// The line without a leading `git` word.
pub fn strip_git_prefix(line: &str) -> &str {
    &line[git_prefix_len(line)..]
}

/// Split the text before the cursor into finished words and the partial word.
fn split_words(before_cursor: &str) -> (Vec<&str>, usize, &str) {
    let word_start = before_cursor
        .rfind(char::is_whitespace)
        .map(|i| i + before_cursor[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let words = before_cursor[..word_start].split_whitespace().collect();
    (words, word_start, &before_cursor[word_start..])
}

/// Compute completion candidates for the word ending at `cursor` (byte offset).
pub fn complete(line: &str, cursor: usize, vocab: &Vocabulary, values: &RepoValues) -> Completion {
    // Shell syntax cannot be completed reliably: no candidates after `!`.
    if crate::domain::command::shell_line(line).is_some() {
        return Completion {
            word_start: cursor.min(line.len()),
            partial: String::new(),
            candidates: Vec::new(),
            missing_options_for: None,
        };
    }
    // A typed leading `git` is ignored (the label already says git).
    let skip = git_prefix_len(line);
    if skip > 0 && cursor >= skip {
        let mut c = complete(&line[skip..], cursor - skip, vocab, values);
        c.word_start += skip;
        return c;
    }
    let before = &line[..cursor.min(line.len())];
    let (words, word_start, partial) = split_words(before);
    let mut missing_options_for = None;

    let candidates = match words.split_first() {
        None => subcommand_candidates(vocab, partial),
        Some((cmd, rest)) if partial.starts_with('-') && !rest.contains(&"--") => {
            if !vocab.options.contains_key(*cmd) {
                missing_options_for = Some((*cmd).to_owned());
            }
            option_candidates(cmd, vocab, partial)
        }
        Some((cmd, rest)) => value_candidates(cmd, rest, values, partial),
    };

    Completion {
        word_start,
        partial: partial.to_owned(),
        candidates,
        missing_options_for,
    }
}

fn subcommand_candidates(vocab: &Vocabulary, partial: &str) -> Vec<Candidate> {
    let mut names: BTreeSet<&str> = vocab.subcommands.iter().map(String::as_str).collect();
    names.extend(hints::HINTS.iter().map(|h| h.name));
    names
        .into_iter()
        .filter(|n| n.starts_with(partial))
        .map(|n| Candidate::plain(n, hints::hint(n).map(|h| h.description)))
        .collect()
}

fn option_candidates(cmd: &str, vocab: &Vocabulary, partial: &str) -> Vec<Candidate> {
    let hinted: &[(&str, &str)] = hints::hint(cmd).map_or(&[], |h| h.options);
    let mut options: BTreeMap<&str, Option<&str>> =
        hinted.iter().map(|(o, d)| (*o, Some(*d))).collect();
    let show_negations = partial.starts_with("--no");
    for opt in vocab.options.get(cmd).into_iter().flatten() {
        let negation = opt.starts_with("--no-");
        if opt != "--" && (show_negations || !negation) {
            options.entry(opt.as_str()).or_insert(None);
        }
    }
    options
        .into_iter()
        .filter(|(o, _)| o.starts_with(partial))
        .map(|(o, d)| Candidate::plain(o, d))
        .collect()
}

fn counted_candidates(sources: &[&Counted], total: usize, partial: &str) -> Vec<Candidate> {
    let mut merged: BTreeMap<&str, usize> = BTreeMap::new();
    for source in sources {
        for (value, count) in source.iter() {
            let entry = merged.entry(value.as_str()).or_default();
            *entry = (*entry).max(*count);
        }
    }
    merged
        .into_iter()
        .filter(|(v, _)| v.starts_with(partial))
        .map(|(v, count)| Candidate {
            tag: (count < total).then(|| format!("in {count}/{total}")),
            ..Candidate::plain(v, None)
        })
        .collect()
}

fn value_candidates(
    cmd: &str,
    rest: &[&str],
    values: &RepoValues,
    partial: &str,
) -> Vec<Candidate> {
    if rest.contains(&"--") {
        return counted_candidates(&[&values.files], values.total, partial);
    }
    let positional = rest.iter().filter(|w| !w.starts_with('-')).count();
    let kind = hints::hint(cmd).map_or(ValueKind::Nothing, |h| h.values);
    kind_candidates(kind, positional, values, partial)
}

fn kind_candidates(
    kind: ValueKind,
    positional: usize,
    values: &RepoValues,
    partial: &str,
) -> Vec<Candidate> {
    let total = values.total;
    let local = &values.branches;
    let remote = &values.remote_branches;
    match kind {
        ValueKind::Branches => counted_candidates(&[local], total, partial),
        ValueKind::BranchesAndRemote => counted_candidates(&[local, remote], total, partial),
        ValueKind::Refs => counted_candidates(&[local, remote, &values.tags], total, partial),
        ValueKind::Remotes => counted_candidates(&[&values.remotes], total, partial),
        ValueKind::RemoteThenBranch if positional == 0 => {
            counted_candidates(&[&values.remotes], total, partial)
        }
        ValueKind::RemoteThenBranch => counted_candidates(&[local], total, partial),
        ValueKind::Tags => counted_candidates(&[&values.tags], total, partial),
        ValueKind::Files => counted_candidates(&[&values.files], total, partial),
        ValueKind::Subcommands(subs, _) if positional == 0 => subs
            .iter()
            .filter(|(s, _)| s.starts_with(partial))
            .map(|(s, d)| Candidate::plain(s, Some(d)))
            .collect(),
        ValueKind::Subcommands(_, then) => kind_candidates(*then, positional - 1, values, partial),
        ValueKind::Nothing => Vec::new(),
    }
}

/// What pressing Tab does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabAction {
    /// No candidates.
    Nothing,
    /// Replace `partial` with `text` (single match, auto-insert enabled).
    Insert(String),
    /// Open the popup; optionally first extend `partial` to the common prefix.
    Popup { extend_to: Option<String> },
}

/// Shell-like Tab: one match inserts inline (if enabled), several extend to the
/// common prefix and open the popup.
pub fn tab_action(completion: &Completion, auto_insert_single: bool) -> TabAction {
    match completion.candidates.as_slice() {
        [] => TabAction::Nothing,
        [only] if auto_insert_single => TabAction::Insert(only.insertion()),
        // With auto-insert off, a single match must not be typed in for the user.
        [_] => TabAction::Popup { extend_to: None },
        many => {
            let prefix = common_prefix(many.iter().map(|c| c.text.as_str()));
            let extend_to = (prefix.len() > completion.partial.len()).then_some(prefix);
            TabAction::Popup { extend_to }
        }
    }
}

fn common_prefix<'a, I: Iterator<Item = &'a str>>(mut words: I) -> String {
    let Some(first) = words.next() else {
        return String::new();
    };
    let mut prefix = first.to_owned();
    for w in words {
        let shared: usize = prefix
            .char_indices()
            .zip(w.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, a), _)| i + a.len_utf8());
        prefix.truncate(shared);
    }
    prefix
}

/// Replace the word `[word_start, cursor)` in `line` with `text`.
/// Returns the new line and the new cursor position (byte offset).
pub fn replace_word(line: &str, word_start: usize, cursor: usize, text: &str) -> (String, usize) {
    let cursor = cursor.min(line.len());
    let mut out = String::with_capacity(line.len() + text.len());
    out.push_str(&line[..word_start]);
    out.push_str(text);
    let new_cursor = out.len();
    out.push_str(&line[cursor..]);
    (out, new_cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocab() -> Vocabulary {
        let mut options = HashMap::new();
        options.insert(
            "pull".to_owned(),
            vec![
                "--rebase".to_owned(),
                "--no-rebase".to_owned(),
                "--quiet".to_owned(),
                "--".to_owned(),
            ],
        );
        Vocabulary {
            subcommands: vec!["pull".into(), "push".into(), "prune".into(), "st".into()],
            options,
        }
    }

    fn refs(branches: &[&str], remotes: &[&str]) -> RepoRefs {
        RepoRefs {
            branches: branches.iter().map(|s| s.to_string()).collect(),
            remotes: remotes.iter().map(|s| s.to_string()).collect(),
            ..RepoRefs::default()
        }
    }

    fn texts(c: &Completion) -> Vec<&str> {
        c.candidates.iter().map(|c| c.text.as_str()).collect()
    }

    fn at_end(line: &str, values: &RepoValues) -> Completion {
        complete(line, line.len(), &vocab(), values)
    }

    #[test]
    fn subcommands_include_git_list_and_hints() {
        let c = at_end("pu", &RepoValues::default());
        assert_eq!(texts(&c), vec!["pull", "push"]);
        assert_eq!(c.word_start, 0);
        assert_eq!(c.partial, "pu");
        assert!(c.candidates[0].description.is_some());
        assert!(texts(&at_end("", &RepoValues::default())).contains(&"st"));
    }

    #[test]
    fn shell_lines_get_no_completion() {
        for line in ["!pu", "! git pull --r", "  !"] {
            let c = at_end(line, &RepoValues::default());
            assert!(c.candidates.is_empty(), "{line}");
            assert!(c.missing_options_for.is_none());
            assert_eq!(tab_action(&c, true), TabAction::Nothing);
        }
    }

    #[test]
    fn options_merge_helper_and_hints_hide_negations() {
        let c = at_end("pull --r", &RepoValues::default());
        assert_eq!(texts(&c), vec!["--rebase"]);
        assert_eq!(c.word_start, 5);
        assert!(c.missing_options_for.is_none());
        let c = at_end("pull --no", &RepoValues::default());
        assert_eq!(texts(&c), vec!["--no-rebase"]);
        let c = at_end("pull --q", &RepoValues::default());
        assert_eq!(texts(&c), vec!["--quiet"]);
        assert_eq!(c.candidates[0].description, None);
    }

    #[test]
    fn options_unknown_command_requests_lookup() {
        let c = at_end("log --one", &RepoValues::default());
        assert_eq!(texts(&c), vec!["--oneline"]);
        assert_eq!(c.missing_options_for.as_deref(), Some("log"));
    }

    #[test]
    fn option_with_equals_has_no_trailing_space() {
        let c = at_end("log --auth", &RepoValues::default());
        assert_eq!(c.candidates[0].insertion(), "--author=");
        let c = at_end("log --onel", &RepoValues::default());
        assert_eq!(c.candidates[0].insertion(), "--oneline ");
    }

    #[test]
    fn branch_values_tagged_when_not_in_all() {
        let repos = [refs(&["main", "feature-x"], &[]), refs(&["main"], &[])];
        let values = RepoValues::union(&repos);
        let c = at_end("switch ", &values);
        assert_eq!(texts(&c), vec!["feature-x", "main"]);
        assert_eq!(c.candidates[0].tag.as_deref(), Some("in 1/2"));
        assert_eq!(c.candidates[1].tag, None);
    }

    #[test]
    fn push_completes_remote_then_branch() {
        let values = RepoValues::union(&[refs(&["main"], &["origin", "upstream"])]);
        assert_eq!(texts(&at_end("push ", &values)), vec!["origin", "upstream"]);
        assert_eq!(texts(&at_end("push -u o", &values)), vec!["origin"]);
        assert_eq!(texts(&at_end("push origin ", &values)), vec!["main"]);
    }

    #[test]
    fn subcommand_style_commands() {
        let values = RepoValues::union(&[refs(&[], &["origin"])]);
        assert_eq!(texts(&at_end("stash p", &values)), vec!["pop", "push"]);
        assert_eq!(texts(&at_end("remote remove ", &values)), vec!["origin"]);
    }

    #[test]
    fn double_dash_completes_files() {
        let repos = [RepoRefs {
            files: vec!["a.txt".into(), "b.txt".into()],
            ..RepoRefs::default()
        }];
        let values = RepoValues::union(&repos);
        assert_eq!(texts(&at_end("checkout -- a", &values)), vec!["a.txt"]);
        assert_eq!(texts(&at_end("checkout -- -", &values)), Vec::<&str>::new());
    }

    #[test]
    fn unknown_command_has_no_values() {
        let values = RepoValues::union(&[refs(&["main"], &[])]);
        assert!(at_end("frobnicate ", &values).candidates.is_empty());
    }

    #[test]
    fn completes_word_at_cursor_not_line_end() {
        let values = RepoValues::union(&[refs(&["main"], &["origin"])]);
        let line = "push o main";
        let c = complete(line, 6, &vocab(), &values);
        assert_eq!(c.word_start, 5);
        assert_eq!(texts(&c), vec!["origin"]);
    }

    #[test]
    fn leading_git_is_ignored() {
        assert_eq!(strip_git_prefix("git pull --rebase"), "pull --rebase");
        assert_eq!(strip_git_prefix("  git   status"), "status");
        assert_eq!(strip_git_prefix("gitk"), "gitk");
        assert_eq!(strip_git_prefix("git"), "git");
        assert_eq!(strip_git_prefix("pull"), "pull");
        assert_eq!(strip_git_prefix("log --grep git"), "log --grep git");

        let c = at_end("git pu", &RepoValues::default());
        assert_eq!(texts(&c), vec!["pull", "push"]);
        assert_eq!(c.word_start, 4);
        let c = at_end("git pull --r", &RepoValues::default());
        assert_eq!(texts(&c), vec!["--rebase"]);
        assert_eq!(c.word_start, 9);
        assert_eq!(
            replace_word("git pull --r", c.word_start, 12, "--rebase "),
            ("git pull --rebase ".into(), 18)
        );
    }

    #[test]
    fn tab_single_match_inserts_or_opens_popup() {
        let c = at_end("reba", &RepoValues::default());
        assert_eq!(tab_action(&c, true), TabAction::Insert("rebase ".into()));
        assert_eq!(tab_action(&c, false), TabAction::Popup { extend_to: None });
    }

    #[test]
    fn tab_many_extends_common_prefix() {
        let c = at_end("p", &RepoValues::default());
        assert_eq!(tab_action(&c, true), TabAction::Popup { extend_to: None });
        let c = at_end("pull --r", &RepoValues::default());
        assert_eq!(tab_action(&c, true), TabAction::Insert("--rebase ".into()));
        let values = RepoValues::union(&[refs(&["feature-a", "feature-b"], &[])]);
        let c = at_end("switch f", &values);
        assert_eq!(
            tab_action(&c, true),
            TabAction::Popup {
                extend_to: Some("feature-".into())
            }
        );
    }

    #[test]
    fn tab_nothing() {
        let c = at_end("zzz", &RepoValues::default());
        assert_eq!(tab_action(&c, true), TabAction::Nothing);
    }

    #[test]
    fn replace_word_keeps_tail() {
        assert_eq!(
            replace_word("push o main", 5, 6, "origin "),
            ("push origin  main".into(), 12)
        );
        assert_eq!(replace_word("pu", 0, 2, "pull "), ("pull ".into(), 5));
    }

    #[test]
    fn common_prefix_unicode() {
        assert_eq!(common_prefix(["äbc", "äbd"].into_iter()), "äb");
        assert_eq!(common_prefix(["x", "y"].into_iter()), "");
    }
}
