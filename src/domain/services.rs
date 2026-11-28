//! The chips of a card for the services around its repository: the pull
//! request of the checked-out branch and its last CI run, read through the
//! command line tools of GitHub (`gh`) and GitLab (`glab`).

use serde_json::Value;

/// How a chip is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Light {
    /// Done and fine: mergeable, the run passed.
    Green,
    /// Not done yet: checks running, a review asked for, a draft.
    Orange,
    /// Something failed: a check, a conflict, changes requested.
    Red,
    /// No status: the tool is missing, or it says nothing that has one.
    Grey,
}

/// One chip on a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceChip {
    /// What the chip says: `PR #12`, `CI`, `gh missing`.
    pub text: String,
    pub light: Light,
    /// What a click opens; `None` for a chip that only tells something.
    pub url: Option<String>,
    /// The hover text.
    pub hint: String,
}

/// The code hosts the chips know.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    GitHub,
    GitLab,
}

impl Forge {
    /// The command line tool that speaks for it.
    pub fn cli(self) -> &'static str {
        match self {
            Forge::GitHub => "gh",
            Forge::GitLab => "glab",
        }
    }

    /// The host everybody knows it by.
    pub fn public_host(self) -> &'static str {
        match self {
            Forge::GitHub => "github.com",
            Forge::GitLab => "gitlab.com",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Forge::GitHub => "GitHub",
            Forge::GitLab => "GitLab",
        }
    }
}

/// What the chips of a card are read for: the remote of the branch's
/// upstream (or `origin`), its host and repository path, and the branch
/// there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChipQuery {
    pub host: String,
    pub path: String,
    pub branch: String,
    /// The services switched on in the settings.
    pub github: bool,
    pub gitlab: bool,
}

/// The grey chip of a tool that is not there or not logged in.
pub fn tool_chip(forge: Forge, installed: bool) -> ServiceChip {
    let cli = forge.cli();
    if installed {
        ServiceChip {
            text: format!("{cli} not logged in"),
            light: Light::Grey,
            url: None,
            hint: format!(
                "Run `{cli} auth login` to see the pull request and CI of {} here",
                forge.name()
            ),
        }
    } else {
        ServiceChip {
            text: format!("{cli} missing"),
            light: Light::Grey,
            url: None,
            hint: format!(
                "Install the {} CLI ({cli}) to see the pull request and CI here",
                forge.name()
            ),
        }
    }
}

/// A chip for a read that failed: grey, with the tool's first line.
pub fn failed_chip(what: &str, error: &str) -> ServiceChip {
    let line = error.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    ServiceChip {
        text: format!("{what} ?"),
        light: Light::Grey,
        url: None,
        hint: format!("Cannot read the {what}: {}", line.trim()),
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The worse of two lights; grey counts as nothing.
fn worse(a: Light, b: Light) -> Light {
    let rank = |l| match l {
        Light::Grey => 0,
        Light::Green => 1,
        Light::Orange => 2,
        Light::Red => 3,
    };
    if rank(b) > rank(a) { b } else { a }
}

/// One entry of a GitHub PR's `statusCheckRollup`: a check run with a
/// status and a conclusion, or a commit status with a state.
fn github_check(check: &Value) -> Light {
    let state = text(check, "state");
    if !state.is_empty() {
        return match state {
            "SUCCESS" => Light::Green,
            "FAILURE" | "ERROR" => Light::Red,
            _ => Light::Orange,
        };
    }
    if text(check, "status") != "COMPLETED" {
        return Light::Orange;
    }
    match text(check, "conclusion") {
        "SUCCESS" | "NEUTRAL" | "SKIPPED" => Light::Green,
        "" | "STALE" => Light::Orange,
        _ => Light::Red,
    }
}

/// The chip of the output of `gh pr view --json
/// number,url,state,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup`;
/// `None` when the PR is no longer open.
pub fn github_pr(json: &str) -> Result<Option<ServiceChip>, String> {
    let pr: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if text(&pr, "state") != "OPEN" {
        return Ok(None);
    }
    let checks = pr
        .get("statusCheckRollup")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let mut light = checks.iter().map(github_check).fold(Light::Green, worse);
    let mut why = Vec::new();
    match light {
        Light::Red => why.push("a check failed"),
        Light::Orange => why.push("checks are running"),
        _ => {}
    }
    match text(&pr, "reviewDecision") {
        "CHANGES_REQUESTED" => {
            light = worse(light, Light::Red);
            why.push("changes are requested");
        }
        "REVIEW_REQUIRED" => {
            light = worse(light, Light::Orange);
            why.push("a review is required");
        }
        _ => {}
    }
    match text(&pr, "mergeStateStatus") {
        "DIRTY" => {
            light = worse(light, Light::Red);
            why.push("it has conflicts");
        }
        "BLOCKED" | "BEHIND" | "UNKNOWN" | "UNSTABLE" => {
            light = worse(light, Light::Orange);
            why.push("it cannot be merged yet");
        }
        _ => {}
    }
    if pr.get("isDraft").and_then(Value::as_bool) == Some(true) {
        light = worse(light, Light::Orange);
        why.push("it is a draft");
    }
    let number = pr.get("number").and_then(Value::as_u64).unwrap_or(0);
    Ok(Some(ServiceChip {
        text: format!("PR #{number}"),
        light,
        url: Some(text(&pr, "url").to_owned()).filter(|u| !u.is_empty()),
        hint: hint(&format!("Pull request #{number}"), light, &why),
    }))
}

fn hint(what: &str, light: Light, why: &[&str]) -> String {
    if why.is_empty() {
        let state = match light {
            Light::Green => "fine",
            _ => "without a status",
        };
        return format!("{what}: {state}. Click to open it.");
    }
    format!("{what}: {}. Click to open it.", why.join(", "))
}

/// The light of one GitHub Actions run and how it ended.
fn github_run_light(run: &Value) -> (Light, String) {
    if text(run, "status") != "completed" {
        return (Light::Orange, "running".to_owned());
    }
    let conclusion = text(run, "conclusion");
    let light = match conclusion {
        "success" | "neutral" | "skipped" => Light::Green,
        "cancelled" | "" => Light::Orange,
        _ => Light::Red,
    };
    (light, conclusion.replace('_', " "))
}

/// The chip of the output of `gh run list --json
/// status,conclusion,url,workflowName,headSha`, newest first: all runs of
/// the newest commit together (one per workflow), the worst of them
/// decides and is what a click opens. `None` when the branch has no run.
pub fn github_run(json: &str) -> Result<Option<ServiceChip>, String> {
    let runs: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let runs = runs.as_array().map_or(&[][..], Vec::as_slice);
    let Some(newest) = runs.first() else {
        return Ok(None);
    };
    let sha = text(newest, "headSha");
    let mut light = Light::Grey;
    let mut url = None;
    let mut states = Vec::new();
    for run in runs.iter().filter(|r| text(r, "headSha") == sha) {
        let (this, state) = github_run_light(run);
        if url.is_none() || worse(light, this) != light {
            url = Some(text(run, "url").to_owned()).filter(|u| !u.is_empty());
        }
        light = worse(light, this);
        states.push(format!("{} {state}", text(run, "workflowName")));
    }
    Ok(Some(ServiceChip {
        text: "CI".to_owned(),
        light,
        url,
        hint: format!("Last runs: {}. Click to open the worst.", states.join(", ")),
    }))
}

/// The chip of GitLab's merge requests of a branch (`merge_requests?
/// source_branch=…&state=opened`); `None` when it has none open.
pub fn gitlab_mr(json: &str) -> Result<Option<ServiceChip>, String> {
    let list: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Some(mr) = list.as_array().and_then(|l| l.first()) else {
        return Ok(None);
    };
    let status = text(mr, "detailed_merge_status");
    let conflicts = mr.get("has_conflicts").and_then(Value::as_bool) == Some(true);
    let draft = mr.get("draft").and_then(Value::as_bool) == Some(true);
    let (light, why) =
        if conflicts || matches!(status, "conflict" | "need_rebase" | "broken_status") {
            (Light::Red, "it has conflicts or must be rebased")
        } else if draft {
            (Light::Orange, "it is a draft")
        } else if status == "mergeable" {
            (Light::Green, "")
        } else {
            (Light::Orange, "it cannot be merged yet")
        };
    let number = mr.get("iid").and_then(Value::as_u64).unwrap_or(0);
    let why: Vec<&str> = Some(why).filter(|w| !w.is_empty()).into_iter().collect();
    Ok(Some(ServiceChip {
        text: format!("MR !{number}"),
        light,
        url: Some(text(mr, "web_url").to_owned()).filter(|u| !u.is_empty()),
        hint: hint(&format!("Merge request !{number}"), light, &why),
    }))
}

/// The chip of GitLab's last pipeline of a branch (`pipelines?ref=…`);
/// `None` when it has none.
pub fn gitlab_pipeline(json: &str) -> Result<Option<ServiceChip>, String> {
    let list: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Some(pipeline) = list.as_array().and_then(|l| l.first()) else {
        return Ok(None);
    };
    let status = text(pipeline, "status");
    let light = match status {
        "success" | "skipped" => Light::Green,
        "failed" => Light::Red,
        _ => Light::Orange,
    };
    Ok(Some(ServiceChip {
        text: "CI".to_owned(),
        light,
        url: Some(text(pipeline, "web_url").to_owned()).filter(|u| !u.is_empty()),
        hint: format!(
            "Last pipeline: {}. Click to open it.",
            status.replace('_', " ")
        ),
    }))
}

/// A GitLab project path for its REST API: every byte that is not a
/// letter, a digit or `-._~` written as `%XX`, the slashes too.
pub fn encoded(path: &str) -> String {
    let mut out = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_github_pr_is_green_orange_or_red_by_its_checks_and_review() {
        let pr = |rest: &str| {
            format!(
                r#"{{"number":12,"url":"https://github.com/o/r/pull/12","state":"OPEN",{rest}}}"#
            )
        };
        let light = |rest: &str| github_pr(&pr(rest)).unwrap().unwrap().light;
        let passed = r#""statusCheckRollup":[{"status":"COMPLETED","conclusion":"SUCCESS"}]"#;
        let chip = github_pr(&pr(passed)).unwrap().unwrap();
        assert_eq!(chip.text, "PR #12");
        assert_eq!(chip.light, Light::Green);
        assert_eq!(chip.url.as_deref(), Some("https://github.com/o/r/pull/12"));
        assert_eq!(
            light(r#""statusCheckRollup":[{"status":"IN_PROGRESS","conclusion":""}]"#),
            Light::Orange
        );
        assert_eq!(
            light(
                r#""statusCheckRollup":[{"status":"COMPLETED","conclusion":"FAILURE"},{"state":"PENDING"}]"#
            ),
            Light::Red
        );
        assert_eq!(
            light(r#""statusCheckRollup":[{"state":"ERROR"}]"#),
            Light::Red
        );
        assert_eq!(
            light(r#""reviewDecision":"REVIEW_REQUIRED""#),
            Light::Orange
        );
        assert_eq!(light(r#""reviewDecision":"CHANGES_REQUESTED""#), Light::Red);
        assert_eq!(light(r#""mergeStateStatus":"DIRTY""#), Light::Red);
        assert_eq!(light(r#""isDraft":true"#), Light::Orange);
        let hint = github_pr(&pr(r#""isDraft":true,"reviewDecision":"REVIEW_REQUIRED""#))
            .unwrap()
            .unwrap()
            .hint;
        assert_eq!(
            hint,
            "Pull request #12: a review is required, it is a draft. Click to open it."
        );
        // A merged one is no chip; output that is no JSON is an error.
        assert_eq!(github_pr(r#"{"state":"MERGED","number":3}"#), Ok(None));
        assert!(github_pr("no pull requests found").is_err());
    }

    #[test]
    fn a_github_run_tells_its_conclusion() {
        let run = |status: &str, conclusion: &str| {
            format!(
                r#"[{{"status":"{status}","conclusion":"{conclusion}","url":"https://github.com/o/r/actions/runs/1","workflowName":"CI","headSha":"a"}}]"#
            )
        };
        let chip = github_run(&run("completed", "success")).unwrap().unwrap();
        assert_eq!((chip.text.as_str(), chip.light), ("CI", Light::Green));
        assert_eq!(chip.hint, "Last runs: CI success. Click to open the worst.");
        let light = |s, c| github_run(&run(s, c)).unwrap().unwrap().light;
        assert_eq!(light("in_progress", ""), Light::Orange);
        assert_eq!(light("completed", "failure"), Light::Red);
        assert_eq!(light("completed", "timed_out"), Light::Red);
        assert_eq!(light("completed", "cancelled"), Light::Orange);
        assert_eq!(github_run("[]"), Ok(None));
        // Every workflow of the newest commit counts; older commits do not.
        let several = r#"[
            {"status":"completed","conclusion":"success","url":"q","workflowName":"CodeQL","headSha":"b"},
            {"status":"completed","conclusion":"failure","url":"c","workflowName":"CI","headSha":"b"},
            {"status":"completed","conclusion":"failure","url":"old","workflowName":"CI","headSha":"a"}
        ]"#;
        let chip = github_run(several).unwrap().unwrap();
        assert_eq!(chip.light, Light::Red);
        assert_eq!(chip.url.as_deref(), Some("c"));
        assert_eq!(
            chip.hint,
            "Last runs: CodeQL success, CI failure. Click to open the worst."
        );
    }

    #[test]
    fn a_gitlab_mr_and_pipeline_get_their_lights() {
        let mr = |rest: &str| {
            format!(r#"[{{"iid":7,"web_url":"https://gitlab.com/g/p/-/merge_requests/7",{rest}}}]"#)
        };
        let light = |rest: &str| gitlab_mr(&mr(rest)).unwrap().unwrap().light;
        let chip = gitlab_mr(&mr(r#""detailed_merge_status":"mergeable""#))
            .unwrap()
            .unwrap();
        assert_eq!((chip.text.as_str(), chip.light), ("MR !7", Light::Green));
        assert_eq!(
            light(r#""detailed_merge_status":"ci_still_running""#),
            Light::Orange
        );
        assert_eq!(light(r#""detailed_merge_status":"conflict""#), Light::Red);
        assert_eq!(
            light(r#""has_conflicts":true,"detailed_merge_status":"mergeable""#),
            Light::Red
        );
        assert_eq!(
            light(r#""draft":true,"detailed_merge_status":"mergeable""#),
            Light::Orange
        );
        assert_eq!(gitlab_mr("[]"), Ok(None));

        let pipeline = |status: &str| {
            format!(r#"[{{"status":"{status}","web_url":"https://gitlab.com/g/p/-/pipelines/9"}}]"#)
        };
        let light = |s| gitlab_pipeline(&pipeline(s)).unwrap().unwrap().light;
        assert_eq!(light("success"), Light::Green);
        assert_eq!(light("running"), Light::Orange);
        assert_eq!(light("failed"), Light::Red);
        assert_eq!(gitlab_pipeline("[]"), Ok(None));
    }

    #[test]
    fn missing_tools_and_failed_reads_are_grey_and_say_why() {
        let missing = tool_chip(Forge::GitHub, false);
        assert_eq!(
            (missing.text.as_str(), missing.light),
            ("gh missing", Light::Grey)
        );
        assert_eq!(missing.url, None);
        let out = tool_chip(Forge::GitLab, true);
        assert_eq!(out.text, "glab not logged in");
        assert!(out.hint.contains("glab auth login"), "{}", out.hint);
        let failed = failed_chip("PR", "\nHTTP 502: Bad gateway\nmore");
        assert_eq!(failed.text, "PR ?");
        assert_eq!(failed.hint, "Cannot read the PR: HTTP 502: Bad gateway");
        assert_eq!(
            encoded("group/sub-1/my project"),
            "group%2Fsub-1%2Fmy%20project"
        );
    }
}
