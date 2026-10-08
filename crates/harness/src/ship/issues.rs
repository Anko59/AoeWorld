//! `make issue` records out-of-scope work; `make next` selects the next task.
use super::git;
use serde::Deserialize;
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const AGENT_LABEL: &str = "agent-found";
const MARKER: &str = "<!-- aoe-fingerprint: ";
const TITLE_LIMIT: usize = 256;
const BODY_LIMIT: usize = 60_000;
const ISSUE_CAP: usize = 10_000;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Issue {
    number: u64,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "pull_request")]
    pull_request: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct Label {
    name: String,
}

impl Issue {
    fn has_label(&self, wanted: &str) -> bool {
        self.labels.iter().any(|label| label.name == wanted)
    }
}

fn fingerprint(title: &str) -> String {
    let normalized = title
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    blake3::hash(normalized.as_bytes()).to_hex()[..16].to_owned()
}

fn validate_text(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.chars().count() > limit {
        return Err(format!("{field} exceeds {limit} characters").into());
    }
    if value
        .chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(format!("{field} contains a forbidden control character").into());
    }
    Ok(())
}

fn allowed_label(label: &str) -> bool {
    matches!(
        label,
        "priority:critical" | "priority:high" | "priority:medium" | "priority:low" | "blocked"
    ) || label
        .strip_prefix("area:")
        .is_some_and(|area| !area.trim().is_empty() && !area.chars().any(char::is_control))
}

fn labels_from(value: Option<String>) -> Result<Vec<String>> {
    let mut labels = vec![AGENT_LABEL.to_owned()];
    let mut seen = HashSet::from([AGENT_LABEL.to_owned()]);
    for label in value.unwrap_or_default().split(',').map(str::trim) {
        if label.is_empty() {
            continue;
        }
        if !allowed_label(label) {
            return Err(format!("label `{label}` is not allowed").into());
        }
        if seen.insert(label.to_owned()) {
            labels.push(label.to_owned());
        }
    }
    Ok(labels)
}

fn marked_body(body: &str, title: &str) -> Result<String> {
    validate_text("ISSUE_BODY", body, BODY_LIMIT)?;
    let marked = format!(
        "{}\n\n{MARKER}{} -->\n",
        body.trim_end(),
        fingerprint(title)
    );
    validate_text("ISSUE_BODY", &marked, BODY_LIMIT)?;
    Ok(marked)
}

fn gh_call(root: &Path, _host: &str, args: &[&str], input: Option<&[u8]>) -> Result<String> {
    let mut command = Command::new("gh");
    command
        .current_dir(root)
        .args(args)
        .env_remove("GH_HOST")
        .env_remove("GH_REPO");
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("gh stdin was not piped")?
            .write_all(input)?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "gh {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn open_issues(root: &Path, host: &str, repository: &str) -> Result<Vec<Issue>> {
    let endpoint = format!("repos/{repository}/issues?state=open&per_page=100");
    let json = gh_call(
        root,
        host,
        &[
            "api",
            "--hostname",
            host,
            "--paginate",
            "--slurp",
            &endpoint,
        ],
        None,
    )?;
    parse_issue_pages(&json)
}

fn parse_issue_pages(json: &str) -> Result<Vec<Issue>> {
    let pages: Vec<Vec<Issue>> = serde_json::from_str(json)?;
    let mut issues = Vec::new();
    for issue in pages
        .into_iter()
        .flatten()
        .filter(|issue| issue.pull_request.is_none())
    {
        if issues.len() == ISSUE_CAP {
            return Err(format!(
                "origin has more than {ISSUE_CAP} open issues; refusing incomplete issue selection"
            )
            .into());
        }
        issues.push(issue);
    }
    Ok(issues)
}

fn duplicate_index(issues: &[Issue], title: &str) -> Option<usize> {
    let digest = fingerprint(title);
    let marker = format!("{MARKER}{digest} -->");
    issues.iter().position(|issue| {
        fingerprint(&issue.title) == digest
            && issue.body.contains(&marker)
            && issue.has_label(AGENT_LABEL)
    })
}

fn rank(issues: &[Issue]) -> Vec<&Issue> {
    let priority = |issue: &Issue| {
        ["priority:critical", "priority:high", "priority:medium"]
            .iter()
            .position(|wanted| issue.has_label(wanted))
            .unwrap_or(if issue.has_label("priority:low") {
                4
            } else {
                3
            })
    };
    let mut ready: Vec<_> = issues
        .iter()
        .filter(|issue| !issue.has_label("blocked"))
        .collect();
    ready.sort_by(|a, b| {
        (priority(a), &a.created_at, a.number).cmp(&(priority(b), &b.created_at, b.number))
    });
    ready
}

fn repository(root: &Path) -> Result<(String, String)> {
    let (host, path) = git::origin_host_repository(root)?;
    Ok((host.clone(), format!("{host}/{path}")))
}

pub(super) fn issue(root: &Path) -> Result<()> {
    issue_with_input(root, std::io::stdin().lock())
}

fn issue_with_input<R: Read>(root: &Path, input: R) -> Result<()> {
    if !crate::agents::issue_commands_allowed() {
        return Err(
            "make issue is available only to the main session, testers and implementers".into(),
        );
    }
    let title = std::env::var("ISSUE_TITLE").map_err(|_| "set ISSUE_TITLE")?;
    validate_text("ISSUE_TITLE", &title, TITLE_LIMIT)?;
    let mut bytes = Vec::new();
    input
        .take((BODY_LIMIT * 4 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > BODY_LIMIT * 4 {
        return Err("ISSUE_BODY exceeds the maximum UTF-8 byte size".into());
    }
    let body = String::from_utf8(bytes).map_err(|_| "ISSUE_BODY is not valid UTF-8")?;
    let body = marked_body(&body, &title)?;
    let labels = labels_from(std::env::var("ISSUE_LABELS").ok())?;
    let (host, repo) = repository(root)?;
    let issues = open_issues(
        root,
        &host,
        repo.split_once('/').map(|(_, r)| r).unwrap_or(&repo),
    )?;
    if let Some(index) = duplicate_index(&issues, &title) {
        let number = issues[index].number.to_string();
        gh_call(
            root,
            &host,
            &[
                "issue",
                "comment",
                &number,
                "--repo",
                &repo,
                "--body",
                "Duplicate request filed; keeping this issue as the shared task.",
            ],
            None,
        )?;
        println!("issue: already open, commented on {}", issues[index].url);
        return Ok(());
    }
    for label in &labels {
        gh_call(
            root,
            &host,
            &[
                "label", "create", label, "--repo", &repo, "--force", "--color", "BFD4F2",
            ],
            None,
        )?;
    }
    let mut args = vec![
        "issue",
        "create",
        "--repo",
        &repo,
        "--title",
        &title,
        "--body-file",
        "-",
    ];
    for label in &labels {
        args.extend(["--label", label]);
    }
    let url = gh_call(root, &host, &args, Some(body.as_bytes()))?;
    println!("issue: opened {url}");
    Ok(())
}

pub(super) fn next(root: &Path) -> Result<()> {
    if !crate::agents::issue_commands_allowed() {
        return Err(
            "make next is available only to the main session, testers and implementers".into(),
        );
    }
    let (host, repo) = repository(root)?;
    let issues = open_issues(
        root,
        &host,
        repo.split_once('/').map(|(_, r)| r).unwrap_or(&repo),
    )?;
    match rank(&issues).first() {
        Some(issue) => println!("#{} {}\n{}", issue.number, issue.title, issue.url),
        None => println!("no open issue is ready"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "issues/tests/mod.rs"]
mod tests;
