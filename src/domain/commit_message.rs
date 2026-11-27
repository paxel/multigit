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
}
