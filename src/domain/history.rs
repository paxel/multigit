//! CLI history with fish-like autosuggestion, filtered Up/Down and fuzzy search.

/// Entries kept in the history file.
pub const MAX_ENTRIES: usize = 500;

/// Command lines, oldest first, without duplicates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct History {
    entries: Vec<String>,
}

impl History {
    /// Build from stored lines (oldest first); duplicates keep their newest position.
    pub fn from_lines<I: IntoIterator<Item = String>>(lines: I) -> Self {
        let mut history = Self::default();
        for line in lines {
            history.push(&line);
        }
        history
    }

    /// Oldest first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Record an executed line: blank lines are ignored, a repeat moves to the end.
    pub fn push(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.entries.retain(|e| e != line);
        self.entries.push(line.to_owned());
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    fn newest_first(&self) -> impl Iterator<Item = &String> {
        self.entries.iter().rev()
    }

    /// Grey inline suggestion: the newest entry that starts with `typed` and is longer.
    pub fn suggestion(&self, typed: &str) -> Option<&str> {
        if typed.is_empty() {
            return None;
        }
        self.newest_first()
            .find(|e| e.len() > typed.len() && e.starts_with(typed))
            .map(String::as_str)
    }

    /// Entries containing `needle` (case-insensitive), newest first. Empty needle = all.
    pub fn containing(&self, needle: &str) -> Vec<&str> {
        let needle = needle.to_lowercase();
        self.newest_first()
            .filter(|e| e.to_lowercase().contains(&needle))
            .map(String::as_str)
            .collect()
    }

    /// Fuzzy (subsequence) search, best match first; equal scores keep newest first.
    pub fn fuzzy(&self, query: &str) -> Vec<&str> {
        let mut scored: Vec<(i64, usize, &str)> = self
            .newest_first()
            .enumerate()
            .filter_map(|(age, e)| fuzzy_score(query, e).map(|s| (s, age, e.as_str())))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }
}

/// Score `candidate` against `query` as a case-insensitive subsequence; `None` = no match.
/// Rewards consecutive characters and matches at word starts, penalises gaps.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    let query: Vec<char> = query.to_lowercase().chars().collect();
    let cand: Vec<char> = candidate.to_lowercase().chars().collect();
    let mut score = 0i64;
    let mut qi = 0;
    let mut last_match: Option<usize> = None;
    for (ci, &c) in cand.iter().enumerate() {
        let Some(&wanted) = query.get(qi) else {
            break;
        };
        if c != wanted {
            continue;
        }
        score += 1;
        let word_start = ci
            .checked_sub(1)
            .and_then(|prev| cand.get(prev))
            .is_none_or(|prev| matches!(prev, ' ' | '-' | '/' | '_' | '.' | '='));
        if word_start {
            score += 3;
        }
        match last_match {
            Some(prev) if prev + 1 == ci => score += 5,
            Some(prev) => score -= (ci - prev - 1).min(5) as i64,
            None => {}
        }
        last_match = Some(ci);
        qi += 1;
    }
    (qi == query.len()).then_some(score)
}

/// Up/Down walk through history entries containing what was typed before the first Up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HistoryNav {
    /// Text typed before navigation started; `None` = not navigating.
    needle: Option<String>,
    index: usize,
}

impl HistoryNav {
    pub fn is_active(&self) -> bool {
        self.needle.is_some()
    }

    /// Stop navigating (e.g. the user edited the line).
    pub fn reset(&mut self) {
        self.needle = None;
        self.index = 0;
    }

    /// Older entry; returns the new line text, or `None` when there is nothing older.
    pub fn up(&mut self, history: &History, current: &str) -> Option<String> {
        let (needle, next) = match &self.needle {
            None => (current.to_owned(), 0),
            Some(needle) => (needle.clone(), self.index + 1),
        };
        let entry = (*Self::matches(history, &needle).get(next)?).to_owned();
        self.needle = Some(needle);
        self.index = next;
        Some(entry)
    }

    /// Newer entry; past the newest, the originally typed text comes back.
    pub fn down(&mut self, history: &History) -> Option<String> {
        let needle = self.needle.clone()?;
        if self.index == 0 {
            self.reset();
            return Some(needle);
        }
        self.index -= 1;
        Self::matches(history, &needle)
            .get(self.index)
            .map(|e| (*e).to_owned())
    }

    /// Entries containing the needle, newest first, without the needle itself.
    fn matches<'h>(history: &'h History, needle: &str) -> Vec<&'h str> {
        history
            .containing(needle)
            .into_iter()
            .filter(|e| *e != needle)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(lines: &[&str]) -> History {
        History::from_lines(lines.iter().map(|s| s.to_string()))
    }

    #[test]
    fn push_dedups_and_ignores_blank() {
        let mut history = h(&["status", "pull", "status"]);
        history.push("   ");
        history.push(" log ");
        assert_eq!(history.entries(), &["pull", "status", "log"]);
    }

    #[test]
    fn push_caps_at_max() {
        let history = History::from_lines((0..MAX_ENTRIES + 10).map(|i| format!("log -{i}")));
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.entries()[0], "log -10");
    }

    #[test]
    fn suggestion_is_newest_prefix_match() {
        let history = h(&["pull --rebase", "push", "pull --ff-only"]);
        assert_eq!(history.suggestion("pu"), Some("pull --ff-only"));
        assert_eq!(history.suggestion("pus"), Some("push"));
        assert_eq!(history.suggestion("push"), None);
        assert_eq!(history.suggestion(""), None);
        assert_eq!(history.suggestion("x"), None);
    }

    #[test]
    fn up_down_filter_by_typed_text() {
        let history = h(&["pull --rebase", "status", "pull --ff-only", "push"]);
        let mut nav = HistoryNav::default();
        assert_eq!(nav.up(&history, "pull").as_deref(), Some("pull --ff-only"));
        assert_eq!(
            nav.up(&history, "pull --ff-only").as_deref(),
            Some("pull --rebase")
        );
        assert_eq!(nav.up(&history, "pull --rebase"), None);
        assert_eq!(nav.down(&history).as_deref(), Some("pull --ff-only"));
        assert_eq!(nav.down(&history).as_deref(), Some("pull"));
        assert!(!nav.is_active());
        assert_eq!(nav.down(&history), None);
    }

    #[test]
    fn up_with_empty_line_walks_everything() {
        let history = h(&["a", "b"]);
        let mut nav = HistoryNav::default();
        assert_eq!(nav.up(&history, "").as_deref(), Some("b"));
        assert_eq!(nav.up(&history, "b").as_deref(), Some("a"));
    }

    #[test]
    fn up_skips_entry_equal_to_typed_text() {
        let history = h(&["push --force", "push"]);
        let mut nav = HistoryNav::default();
        assert_eq!(nav.up(&history, "push").as_deref(), Some("push --force"));
    }

    #[test]
    fn up_with_no_match_stays() {
        let history = h(&["status"]);
        let mut nav = HistoryNav::default();
        assert_eq!(nav.up(&history, "zzz"), None);
    }

    #[test]
    fn fuzzy_ranks_tight_matches_first() {
        let history = h(&[
            "log --oneline -5",
            "pull --rebase",
            "push --force-with-lease",
        ]);
        assert_eq!(
            history.fuzzy("pr"),
            vec!["pull --rebase", "push --force-with-lease"]
        );
        assert_eq!(
            history.fuzzy("lo5").first().copied(),
            Some("log --oneline -5")
        );
        assert!(history.fuzzy("xyz").is_empty());
        assert_eq!(history.fuzzy("").len(), 3);
    }

    #[test]
    fn fuzzy_score_basics() {
        assert!(fuzzy_score("abc", "a_b_c").is_some());
        assert!(fuzzy_score("acb", "abc").is_none());
        assert!(fuzzy_score("pull", "pull").unwrap() > fuzzy_score("pull", "p u l l").unwrap());
    }

    #[test]
    fn fuzzy_score_counts_word_starts_including_the_first_character() {
        // p: 1 + 3 (start), u: 1 + 5 (consecutive).
        assert_eq!(fuzzy_score("pu", "pull"), Some(10));
        // l after '-' is a word start: 1 + 3, no gap.
        assert_eq!(fuzzy_score("l", "x-l"), Some(4));
        // Mid-word with a gap of one: 1 + 3, then 1 - 1.
        assert_eq!(fuzzy_score("pl", "pul"), Some(4));
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }
}
