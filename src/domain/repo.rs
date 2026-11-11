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

/// Short card text for a failed fetch: "auth failed" for credential problems,
/// otherwise the first line git printed.
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
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("fetch failed")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
