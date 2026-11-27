//! The commit message as it is written in the commit dialog: one box whose
//! first line is the subject.

/// Characters of a subject that git tools show whole; the rest of a longer
/// one is marked.
pub const SUBJECT_LIMIT: usize = 72;

/// The message as it is committed: without the empty lines at its end, and
/// with an empty line between the subject and a body written straight
/// under it.
pub fn finish(text: &str) -> String {
    let mut lines: Vec<&str> = text.trim_end().lines().collect();
    if lines.get(1).is_some_and(|line| !line.trim().is_empty()) {
        lines.insert(1, "");
    }
    lines.join("\n")
}

/// Enter was pressed the third time in a row at the end of the text: the
/// text ends with three line breaks and the cursor (a byte position)
/// stands after them. That sends the message.
pub fn sends(text: &str, cursor: usize) -> bool {
    cursor >= text.len() && text.ends_with("\n\n\n")
}

/// The subject split where it grows too long: what git tools show whole,
/// and what is past [`SUBJECT_LIMIT`].
pub fn subject_parts(subject: &str) -> (&str, &str) {
    match subject.char_indices().nth(SUBJECT_LIMIT) {
        Some((at, _)) => subject.split_at(at),
        None => (subject, ""),
    }
}

/// Where the first ticket key in `text` stands, as a byte range: letters
/// and digits that start with a letter, a `-` and a number
/// (`[A-Za-z][A-Za-z0-9]+-\d+`, `ABC-12`, `mgit-1000`), with no letter or
/// digit right before or after it.
pub fn ticket_key(text: &str) -> Option<std::ops::Range<usize>> {
    let bytes = text.as_bytes();
    let alnum = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_alphanumeric);
    for start in 0..bytes.len() {
        let first = bytes.get(start).is_some_and(u8::is_ascii_alphabetic);
        if !first || (start > 0 && alnum(start - 1)) {
            continue;
        }
        let mut at = start + 1;
        while alnum(at) {
            at += 1;
        }
        if at - start < 2 || bytes.get(at) != Some(&b'-') {
            continue;
        }
        let digits = at + 1;
        let mut end = digits;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end > digits && !alnum(end) {
            return Some(start..end);
        }
    }
    None
}

/// What the commit dialog starts with: the ticket key of the branch, or
/// else of the previous commit, written the way the previous commit wrote
/// its key (`[ABC-9] Fix` gives `[ABC-12] `, `ABC-9: Fix` gives
/// `ABC-12: `), and as `[KEY] ` when that has none. Empty without a key.
pub fn prefill(branch: Option<&str>, previous: Option<&str>) -> String {
    let previous = previous.unwrap_or_default();
    let before = ticket_key(previous);
    let key = branch
        .and_then(|b| ticket_key(b).and_then(|r| b.get(r)))
        .or_else(|| before.clone().and_then(|r| previous.get(r)));
    let Some(key) = key else {
        return String::new();
    };
    let Some(range) = before else {
        return format!("[{key}] ");
    };
    let prefix = previous.get(..range.start).unwrap_or_default();
    let rest = previous.get(range.end..).unwrap_or_default();
    let suffix: String = rest.chars().take_while(|c| !c.is_alphanumeric()).collect();
    format!("{prefix}{key}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_gets_its_empty_line_once_and_loses_the_ones_at_its_end() {
        assert_eq!(finish("Fix it\n\n\n"), "Fix it");
        assert_eq!(finish("Fix it\nBecause."), "Fix it\n\nBecause.");
        assert_eq!(finish("Fix it\n\nBecause."), "Fix it\n\nBecause.");
        assert_eq!(
            finish("Fix it\n\nBecause.\nAnd so.\n\n"),
            "Fix it\n\nBecause.\nAnd so."
        );
        assert_eq!(finish("  \n"), "");
    }

    #[test]
    fn three_enters_at_the_end_send_and_nowhere_else() {
        let text = "Fix it\n\n\n";
        assert!(sends(text, text.len()));
        assert!(!sends(text, 3), "the cursor is not at the end");
        assert!(!sends("Fix it\n\n", 8));
        let middle = "Fix\n\n\nit";
        assert!(!sends(middle, 6));
    }

    #[test]
    fn a_long_subject_is_split_at_the_limit() {
        let long = "x".repeat(80);
        let (shown, past) = subject_parts(&long);
        assert_eq!((shown.len(), past.len()), (72, 8));
        assert_eq!(subject_parts("short"), ("short", ""));
        let wide = "\u{e4}".repeat(73);
        assert_eq!(subject_parts(&wide).1, "\u{e4}");
    }

    #[test]
    fn a_ticket_key_stands_on_its_own() {
        fn key(text: &str) -> Option<&str> {
            ticket_key(text).and_then(|r| text.get(r))
        }
        assert_eq!(key("feature/ABC-12-login"), Some("ABC-12"));
        assert_eq!(key("[mgit-1000] Keep test apps"), Some("mgit-1000"));
        assert_eq!(key("fix(PROJ2-7): x"), Some("PROJ2-7"));
        assert_eq!(key("A-1 is too short"), None);
        assert_eq!(key("xABC-1y"), None);
        assert_eq!(key("ABC-12x then DEF-3"), Some("DEF-3"));
        assert_eq!(key("feature/master/lazzygit_clone"), None);
    }

    #[test]
    fn the_prefill_takes_the_key_of_the_branch_in_the_form_of_the_last_commit() {
        let last = Some("[ABC-9] Fix logout");
        assert_eq!(prefill(Some("feature/ABC-12-login"), last), "[ABC-12] ");
        assert_eq!(prefill(Some("main"), last), "[ABC-9] ");
        assert_eq!(prefill(Some("ABC-12"), Some("ABC-9: Fix")), "ABC-12: ");
        assert_eq!(
            prefill(Some("ABC-12"), Some("fix(ABC-9): x")),
            "fix(ABC-12): "
        );
        assert_eq!(
            prefill(Some("feature/ABC-12"), Some("Fix logout")),
            "[ABC-12] "
        );
        assert_eq!(prefill(None, None), "");
        assert_eq!(prefill(Some("main"), Some("Fix logout")), "");
    }
}
