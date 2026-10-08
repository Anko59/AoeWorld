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

#[derive(Clone, Debug)]
struct Issue {
    number: u64,
    title: String,
    fingerprint_marker: Option<String>,
    labels: Vec<Label>,
    created_at: String,
    url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueResponse {
    number: u64,
    title: String,
    body: Option<String>,
    #[serde(default)]
    labels: Vec<Label>,
    #[serde(default, rename = "created_at")]
    created_at: String,
    #[serde(default, rename = "html_url")]
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

fn fingerprint_marker(body: &str) -> Option<String> {
    let marker = body.rsplit_once(MARKER)?.1.split_once(" -->")?.0;
    (marker.len() == 16 && marker.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| marker.to_owned())
}

fn contains_marker_prefix(value: &str) -> bool {
    let normalized = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    normalized.contains("<!--aoe-fingerprint:")
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
    ) || label.strip_prefix("area:").is_some_and(area_label)
}

fn area_label(area: &str) -> bool {
    !area.is_empty()
        && area
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_lowercase()))
}

fn labels_from(value: &str) -> Result<Vec<String>> {
    let mut labels = vec![AGENT_LABEL.to_owned()];
    let mut seen = HashSet::from([AGENT_LABEL.to_owned()]);
    for label in value.split(',').map(str::trim) {
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
    let mut child = Command::new("gh")
        .current_dir(root)
        .args(["api", "--hostname", host, "--paginate", &endpoint])
        .env_remove("GH_HOST")
        .env_remove("GH_REPO")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("gh stdout was not piped")?;
    let parsed = parse_issue_pages(stdout);
    if parsed.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let issues = parsed?;
    if !status.success() {
        return Err(
            format!("gh api --hostname {host} --paginate {endpoint} failed with {status}").into(),
        );
    }
    Ok(issues)
}

fn parse_issue_pages<R: Read>(reader: R) -> Result<Vec<Issue>> {
    let mut issues = Vec::new();
    for page in serde_json::Deserializer::from_reader(reader).into_iter::<Vec<IssueResponse>>() {
        for response in page? {
            let fingerprint_marker = response.body.as_deref().and_then(fingerprint_marker);
            if response.pull_request.is_some() {
                continue;
            }
            if issues.len() == ISSUE_CAP {
                return Err(format!(
                    "origin has more than {ISSUE_CAP} open issues; refusing incomplete issue selection"
                )
                .into());
            }
            issues.push(Issue {
                number: response.number,
                title: response.title,
                fingerprint_marker,
                labels: response.labels,
                created_at: response.created_at,
                url: response.url,
            });
        }
    }
    Ok(issues)
}

fn duplicate_index(issues: &[Issue], title: &str) -> Option<usize> {
    let digest = fingerprint(title);
    issues.iter().position(|issue| {
        fingerprint(&issue.title) == digest
            && issue.fingerprint_marker.as_deref() == Some(digest.as_str())
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
    let mut bytes = Vec::new();
    input
        .take(((BODY_LIMIT + TITLE_LIMIT + 1_024) * 4 + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > (BODY_LIMIT + TITLE_LIMIT + 1_024) * 4 {
        return Err("issue input exceeds the maximum UTF-8 byte size".into());
    }
    let input = String::from_utf8(bytes).map_err(|_| "issue input is not valid UTF-8")?;
    let (title, labels, body) = parse_issue_input(&input)?;
    validate_text("ISSUE_TITLE", &title, TITLE_LIMIT)?;
    if contains_marker_prefix(&title) || contains_marker_prefix(&body) {
        return Err("issue title and body must not contain the reserved fingerprint marker".into());
    }
    let body = marked_body(&body, &title)?;
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

fn parse_issue_input(input: &str) -> Result<(String, Vec<String>, String)> {
    let (header, body) = input
        .split_once("\n\n")
        .ok_or("issue input must have a blank line between its header and body")?;
    let mut lines = header.lines();
    let title = lines.next().ok_or("issue title is required")?.to_owned();
    if title.is_empty() {
        return Err("issue title is required".into());
    }
    let labels = match lines.next() {
        Some(line) if line.starts_with("labels: ") => labels_from(&line[8..])?,
        Some(_) => return Err("the second header line must be `labels: a, b`".into()),
        None => labels_from("")?,
    };
    if lines.next().is_some() {
        return Err("issue input has too many header lines".into());
    }
    Ok((title, labels, body.to_owned()))
}

#[cfg(test)]
#[path = "issues/tests/mod.rs"]
mod tests;
