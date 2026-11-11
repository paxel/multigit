//! What the board knows about one repository after a probe.

use crate::domain::completion::RepoRefs;
use crate::domain::status::{RepoState, Status};

/// Result of one successful status probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub status: Status,
    pub state: RepoState,
    pub refs: RepoRefs,
    /// Nearest tag reachable from HEAD (lightweight tags included); `None`
    /// without one.
    pub tag: Option<String>,
}

/// Why a repository could not be probed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProbeError {
    #[error("path missing")]
    PathMissing,
    #[error("not a git repo")]
    NotARepo,
    #[error("{0}")]
    Git(String),
}

/// A failed background fetch, as the card keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    /// The one line the card shows; see [`summarize_fetch_error`].
    pub summary: String,
    /// Everything git printed, for the clipboard.
    pub output: String,
}

impl FetchError {
    /// From what the failed `git fetch` printed.
    pub fn from_output(stdout: &str, stderr: &str) -> Self {
        let output = [stdout.trim_end(), stderr.trim_end()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            summary: summarize_fetch_error(stderr),
            output,
        }
    }

    /// From a fetch that could not be started.
    pub fn from_message(message: String) -> Self {
        Self {
            summary: message.clone(),
            output: message,
        }
    }

    /// The card's line: `Error: ` and the summary without git's own
    /// `error:` or `fatal:` in front.
    pub fn card_text(&self) -> String {
        let summary = self
            .summary
            .strip_prefix("error:")
            .or_else(|| self.summary.strip_prefix("fatal:"))
            .unwrap_or(&self.summary)
            .trim_start();
        format!("Error: {summary}")
    }
}

/// Short card text for a failed fetch: "auth failed" for credential problems,
/// otherwise git's first `error:` or `fatal:` line. A fetch that fails in part
/// prints its normal `From <url>` lines first, so the first line it printed is
/// only used when it marked none as an error.
pub fn summarize_fetch_error(stderr: &str) -> String {
    const AUTH_MARKERS: &[&str] = &[
        "Permission denied",
        "Authentication failed",
        "could not read Username",
        "could not read Password",
        "terminal prompts disabled",
        "Host key verification failed",
    ];
    if AUTH_MARKERS.iter().any(|m| stderr.contains(m)) {
        return "auth failed".to_owned();
    }
    let mut lines = stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    lines
        .clone()
        .find(|l| l.starts_with("error:") || l.starts_with("fatal:"))
        .or_else(|| lines.next())
        .unwrap_or("fetch failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_error_keeps_all_output_and_shortens_the_card_line() {
        let error = FetchError::from_output("out\n", "From x\nfatal: no such remote\n");
        assert_eq!(error.summary, "fatal: no such remote");
        assert_eq!(error.output, "out\nFrom x\nfatal: no such remote");
        assert_eq!(error.card_text(), "Error: no such remote");
        let auth = FetchError::from_output("", "Permission denied (publickey).");
        assert_eq!(auth.card_text(), "Error: auth failed");
        let spawn = FetchError::from_message("No such file".into());
        assert_eq!(
            (spawn.card_text().as_str(), spawn.output.as_str()),
            ("Error: No such file", "No such file")
        );
    }

    #[test]
    fn auth_errors_are_summarized() {
        let ssh = "git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.";
        assert_eq!(summarize_fetch_error(ssh), "auth failed");
        let https = "fatal: could not read Username for 'https://x': terminal prompts disabled";
        assert_eq!(summarize_fetch_error(https), "auth failed");
    }

    #[test]
    fn other_errors_use_first_line() {
        assert_eq!(
            summarize_fetch_error("\nfatal: unable to access 'x': Could not resolve host\n"),
            "fatal: unable to access 'x': Could not resolve host"
        );
        assert_eq!(summarize_fetch_error(""), "fetch failed");
    }

    #[test]
    fn partial_failure_shows_the_error_line_not_the_progress_before_it() {
        let stderr = "From github.com:o/r\n   1a2b3c4..5d6e7f8  main -> origin/main\n\
                      error: cannot lock ref 'refs/remotes/origin/a/b': 'refs/remotes/origin/a' exists\n\
                      ! [new branch] a/b -> origin/a/b (unable to update local ref)\n";
        assert_eq!(
            summarize_fetch_error(stderr),
            "error: cannot lock ref 'refs/remotes/origin/a/b': 'refs/remotes/origin/a' exists"
        );
        assert_eq!(summarize_fetch_error("From x\n ! [rejected] a\n"), "From x");
    }
}
