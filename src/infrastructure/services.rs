//! Reads the chips of a card through `gh` and `glab`, the command line
//! tools of GitHub and GitLab, as the user has them installed and logged
//! in. multigit keeps no token of its own.

use crate::domain::output::ProcessOutput;
use crate::domain::services::{
    ChipQuery, Forge, ServiceChip, encoded, failed_chip, github_pr, github_run, gitlab_mr,
    gitlab_pipeline, tool_chip,
};
use std::collections::BTreeSet;
use std::io;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// Runs a command line tool and returns what it printed.
pub trait Tools: Send + Sync {
    fn run(&self, program: &str, args: &[String]) -> io::Result<ProcessOutput>;
}

/// The tools of the system, without a terminal: they never ask anything.
#[derive(Debug, Default)]
pub struct SystemTools;

impl Tools for SystemTools {
    fn run(&self, program: &str, args: &[String]) -> io::Result<ProcessOutput> {
        let out = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_PROMPT", "1")
            .env("GLAB_CHECK_UPDATE", "false")
            .env("NO_COLOR", "1")
            .output()?;
        Ok(ProcessOutput {
            exit_code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// Whether a tool can speak for a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Login {
    Yes,
    NotLoggedIn,
    Missing,
}

fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

/// The chip reader: the tools and the hosts they were found logged into.
#[derive(Debug, Default)]
pub struct ChipReader<T: Tools> {
    tools: T,
    /// Logins once found stay found; a missing one is asked again next time.
    logged_in: Mutex<BTreeSet<(&'static str, String)>>,
}

impl<T: Tools> ChipReader<T> {
    pub fn new(tools: T) -> Self {
        Self {
            tools,
            logged_in: Mutex::default(),
        }
    }

    fn login(&self, forge: Forge, host: &str) -> Login {
        let key = (forge.cli(), host.to_owned());
        let mut known = self.logged_in.lock().unwrap_or_else(|p| p.into_inner());
        if known.contains(&key) {
            return Login::Yes;
        }
        let args = words(&["auth", "status", "--hostname", host]);
        match self.tools.run(forge.cli(), &args) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Login::Missing,
            Err(_) => Login::NotLoggedIn,
            Ok(out) if out.success() => {
                known.insert(key);
                Login::Yes
            }
            Ok(_) => Login::NotLoggedIn,
        }
    }

    /// The chips of a card: on github.com and gitlab.com a grey chip when
    /// the tool is missing or not logged in, on another host only when a
    /// tool is logged into it.
    pub fn read(&self, query: &ChipQuery) -> Vec<ServiceChip> {
        for (forge, on) in [(Forge::GitHub, query.github), (Forge::GitLab, query.gitlab)] {
            if !on {
                continue;
            }
            let public = query.host == forge.public_host();
            match self.login(forge, &query.host) {
                Login::Yes => {
                    return match forge {
                        Forge::GitHub => self.github(query),
                        Forge::GitLab => self.gitlab(query),
                    };
                }
                Login::Missing if public => return vec![tool_chip(forge, false)],
                Login::NotLoggedIn if public => return vec![tool_chip(forge, true)],
                _ => {}
            }
        }
        Vec::new()
    }

    /// Run a tool and turn its output into a chip; `none` is what the
    /// tool says when there is nothing to show.
    fn chip(
        &self,
        what: &str,
        program: &str,
        args: &[String],
        none: &str,
        parse: fn(&str) -> Result<Option<ServiceChip>, String>,
    ) -> Option<ServiceChip> {
        match self.tools.run(program, args) {
            Err(e) => Some(failed_chip(what, &e.to_string())),
            Ok(out) if out.success() => {
                parse(&out.stdout).unwrap_or_else(|e| Some(failed_chip(what, &e)))
            }
            Ok(out) if !none.is_empty() && out.stderr.contains(none) => None,
            Ok(out) => Some(failed_chip(what, &out.stderr)),
        }
    }

    fn github(&self, query: &ChipQuery) -> Vec<ServiceChip> {
        let repo = format!("{}/{}", query.host, query.path);
        let pr = words(&[
            "pr",
            "view",
            &query.branch,
            "--repo",
            &repo,
            "--json",
            "number,url,state,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup",
        ]);
        let run = words(&[
            "run",
            "list",
            "--repo",
            &repo,
            "--branch",
            &query.branch,
            // Enough for every workflow of the newest commit.
            "--limit",
            "20",
            "--json",
            "status,conclusion,url,workflowName,headSha",
        ]);
        [
            self.chip("PR", "gh", &pr, "no pull requests found", github_pr),
            self.chip("CI", "gh", &run, "", github_run),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn gitlab(&self, query: &ChipQuery) -> Vec<ServiceChip> {
        let project = encoded(&query.path);
        let branch = encoded(&query.branch);
        let api = |endpoint: String| words(&["api", "--hostname", &query.host, &endpoint]);
        let mr = api(format!(
            "projects/{project}/merge_requests?source_branch={branch}&state=opened&per_page=1"
        ));
        let pipeline = api(format!(
            "projects/{project}/pipelines?ref={branch}&per_page=1"
        ));
        [
            self.chip("MR", "glab", &mr, "", gitlab_mr),
            self.chip("CI", "glab", &pipeline, "", gitlab_pipeline),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::Light;

    /// Tools that answer from a list: a program and the start of its
    /// arguments, and what it prints; `None` for a tool that is not there.
    #[derive(Default)]
    struct Fake {
        answers: Vec<(&'static str, &'static str, Option<ProcessOutput>)>,
        calls: Mutex<Vec<String>>,
    }

    fn ok(stdout: &str) -> Option<ProcessOutput> {
        Some(ProcessOutput {
            exit_code: Some(0),
            stdout: stdout.to_owned(),
            stderr: String::new(),
        })
    }

    fn failed(stderr: &str) -> Option<ProcessOutput> {
        Some(ProcessOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: stderr.to_owned(),
        })
    }

    impl Tools for Fake {
        fn run(&self, program: &str, args: &[String]) -> io::Result<ProcessOutput> {
            let line = args.join(" ");
            self.calls.lock().unwrap().push(format!("{program} {line}"));
            let answer = self
                .answers
                .iter()
                .find(|(p, start, _)| *p == program && line.starts_with(start));
            match answer {
                Some((_, _, Some(out))) => Ok(out.clone()),
                Some((_, _, None)) => Err(io::Error::from(io::ErrorKind::NotFound)),
                None => Ok(failed("unexpected").unwrap()),
            }
        }
    }

    fn query(host: &str) -> ChipQuery {
        ChipQuery {
            host: host.into(),
            path: "o/r".into(),
            branch: "feature/x".into(),
            github: true,
            gitlab: true,
        }
    }

    #[test]
    fn github_gives_the_pr_and_the_run_of_the_branch() {
        let fake = Fake {
            answers: vec![
                ("gh", "auth status --hostname github.com", ok("")),
                (
                    "gh",
                    "pr view feature/x --repo github.com/o/r",
                    ok(r#"{"number":4,"state":"OPEN","url":"u"}"#),
                ),
                (
                    "gh",
                    "run list --repo github.com/o/r --branch feature/x",
                    ok(
                        r#"[{"status":"completed","conclusion":"failure","url":"r","workflowName":"CI"}]"#,
                    ),
                ),
            ],
            ..Fake::default()
        };
        let reader = ChipReader::new(fake);
        let chips = reader.read(&query("github.com"));
        let shown: Vec<(&str, Light)> = chips.iter().map(|c| (c.text.as_str(), c.light)).collect();
        assert_eq!(shown, [("PR #4", Light::Green), ("CI", Light::Red)]);
        // The login is asked once.
        reader.read(&query("github.com"));
        let calls = reader.tools.calls.lock().unwrap().clone();
        assert_eq!(calls.iter().filter(|c| c.contains("auth")).count(), 1);
    }

    #[test]
    fn a_branch_without_pr_has_only_its_run_and_a_failed_read_says_so() {
        let fake = Fake {
            answers: vec![
                ("gh", "auth status", ok("")),
                (
                    "gh",
                    "pr view",
                    failed("no pull requests found for branch \"feature/x\""),
                ),
                ("gh", "run list", failed("HTTP 503")),
            ],
            ..Fake::default()
        };
        let chips = ChipReader::new(fake).read(&query("github.com"));
        let shown: Vec<(&str, Light)> = chips.iter().map(|c| (c.text.as_str(), c.light)).collect();
        assert_eq!(shown, [("CI ?", Light::Grey)]);
    }

    #[test]
    fn a_missing_or_logged_out_tool_is_a_grey_chip_only_on_the_public_host() {
        let missing = Fake {
            answers: vec![("gh", "auth", None), ("glab", "auth", None)],
            ..Fake::default()
        };
        let reader = ChipReader::new(missing);
        assert_eq!(reader.read(&query("github.com"))[0].text, "gh missing");
        assert_eq!(reader.read(&query("gitlab.com"))[0].text, "glab missing");
        assert!(reader.read(&query("git.example.org")).is_empty());
        let logged_out = Fake {
            answers: vec![("gh", "auth", failed("not logged in"))],
            ..Fake::default()
        };
        let chips = ChipReader::new(logged_out).read(&query("github.com"));
        assert_eq!(chips[0].text, "gh not logged in");
        // Switched off, nothing is asked.
        let off = ChipReader::new(Fake::default());
        let mut q = query("github.com");
        q.github = false;
        q.gitlab = false;
        assert!(off.read(&q).is_empty());
        assert!(off.tools.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_company_gitlab_is_found_by_the_login_of_glab() {
        let fake = Fake {
            answers: vec![
                ("gh", "auth", failed("not a known host")),
                ("glab", "auth status --hostname git.example.org", ok("")),
                (
                    "glab",
                    "api --hostname git.example.org projects/o%2Fr/merge_requests?source_branch=feature%2Fx&state=opened",
                    ok(r#"[{"iid":3,"detailed_merge_status":"mergeable","web_url":"m"}]"#),
                ),
                (
                    "glab",
                    "api --hostname git.example.org projects/o%2Fr/pipelines?ref=feature%2Fx",
                    ok("[]"),
                ),
            ],
            ..Fake::default()
        };
        let chips = ChipReader::new(fake).read(&query("git.example.org"));
        let shown: Vec<(&str, Light)> = chips.iter().map(|c| (c.text.as_str(), c.light)).collect();
        assert_eq!(shown, [("MR !3", Light::Green)]);
    }
}
