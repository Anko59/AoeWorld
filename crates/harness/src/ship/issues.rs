//! `make issue` records out-of-scope work; `make next` selects the next task.
use super::{gh, git};
use serde::Deserialize;
use std::{collections::HashSet, path::Path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const AGENT_LABEL: &str = "agent-found";
const MARKER: &str = "<!-- aoe-fingerprint: ";
const TITLE_LIMIT: usize = 256;
const BODY_LIMIT: usize = 60_000;

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
    let marked = format!(
        "{}\n\n{MARKER}{} -->\n",
        body.trim_end(),
        fingerprint(title)
    );
    validate_text("ISSUE_BODY", &marked, BODY_LIMIT)?;
    Ok(marked)
}

fn open_issues(root: &Path, repository: &str) -> Result<Vec<Issue>> {
    let json = gh(
        root,
        &[
            "issue",
            "list",
            "--repo",
            repository,
            "--state",
            "open",
            "--limit",
            "500",
            "--json",
            "number,title,body,labels,createdAt,url",
        ],
    )?;
    Ok(serde_json::from_str(&json)?)
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

fn repository(root: &Path) -> Result<String> {
    Ok(git::origin_repository(root)?)
}

pub(super) fn issue(root: &Path) -> Result<()> {
    let title = std::env::var("ISSUE_TITLE").map_err(|_| "set ISSUE_TITLE")?;
    let body = std::env::var("ISSUE_BODY").map_err(|_| "set ISSUE_BODY to the body text")?;
    validate_text("ISSUE_TITLE", &title, TITLE_LIMIT)?;
    let body = marked_body(&body, &title)?;
    let labels = labels_from(std::env::var("ISSUE_LABELS").ok())?;
    let repo = repository(root)?;
    let issues = open_issues(root, &repo)?;
    if let Some(index) = duplicate_index(&issues, &title) {
        let number = issues[index].number.to_string();
        gh(
            root,
            &[
                "issue",
                "comment",
                &number,
                "--repo",
                &repo,
                "--body",
                "Duplicate request filed; keeping this issue as the shared task.",
            ],
        )?;
        println!("issue: already open, commented on {}", issues[index].url);
        return Ok(());
    }
    for label in &labels {
        gh(
            root,
            &[
                "label", "create", label, "--repo", &repo, "--force", "--color", "BFD4F2",
            ],
        )?;
    }
    let mut args = vec![
        "issue", "create", "--repo", &repo, "--title", &title, "--body", &body,
    ];
    for label in &labels {
        args.extend(["--label", label]);
    }
    let url = gh(root, &args)?;
    println!("issue: opened {url}");
    Ok(())
}

pub(super) fn next(root: &Path) -> Result<()> {
    let repo = repository(root)?;
    let issues = open_issues(root, &repo)?;
    match rank(&issues).first() {
        Some(issue) => println!("#{} {}\n{}", issue.number, issue.title, issue.url),
        None => println!("no open issue is ready"),
    }
    Ok(())
}

#[cfg(test)]
#[path = "issues/tests/mod.rs"]
mod tests;
