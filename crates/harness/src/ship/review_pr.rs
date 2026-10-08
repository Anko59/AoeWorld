//! Review same-repository pull requests that were not opened by `make ship`.
use super::{gh, git, review_gate};
use crate::{
    agents::Runtime,
    review::{
        self, Tier,
        closing::{self, Next},
    },
};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug, Deserialize)]
struct PrMetadata {
    #[serde(rename = "headRefOid")]
    head_oid: String,
    #[serde(rename = "headRefName")]
    head_name: String,
    #[serde(rename = "baseRefName")]
    base_name: String,
    #[serde(rename = "isCrossRepository")]
    cross_repository: bool,
    #[allow(dead_code)]
    author: serde_json::Value,
}

pub(super) fn execute(root: &Path) -> Result<()> {
    let number =
        std::env::var("REVIEW_PR").map_err(|_| "set REVIEW_PR to a pull request number")?;
    let number = number.trim();
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) || number == "0" {
        return Err("REVIEW_PR must be a positive pull request number".into());
    }
    let repository = git::origin_repository(root)?;
    let metadata = read_metadata(root, number, &repository)?;
    validate_metadata(&metadata)?;
    git::git(root, &["check-ref-format", "--branch", &metadata.head_name])?;
    git::git(root, &["fetch", "origin", &metadata.head_name])?;

    let worktree = Worktree::create(root, &metadata.head_oid)?;
    let task = gh(
        root,
        &[
            "pr",
            "view",
            number,
            "--repo",
            &repository,
            "--json",
            "body",
        ],
    )?;
    let body: serde_json::Value = serde_json::from_str(&task)?;
    let task = body
        .get("body")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let history = review::history(&worktree.path, &metadata.head_name)?;
    let plan = match closing::next(&history, &metadata.head_oid, unix_now()) {
        Next::Review(plan) => plan,
        Next::FixFirst(why) | Next::Split(why) | Next::Unavailable(why) => return Err(why.into()),
    };
    let tier = required_tier(
        review::floor(&worktree.path)?,
        dependency_major_bump(&worktree.path, "origin/dev", &metadata.head_oid)?,
    );
    let runtime = runtime_from_env()?;
    let report = review::review_for_branch(
        &worktree.path,
        &metadata.head_name,
        tier,
        runtime,
        task,
        &plan,
    )?;
    println!("{}", report.markdown());
    if !report.passes() {
        return Err(if report.complete() {
            format!(
                "review grade {}/10 is below {}",
                report.grade, report.merge_grade
            )
        } else {
            format!(
                "review incomplete: {} session(s) gave no usable answer",
                report.failures.len()
            )
        }
        .into());
    }
    let url = format!("https://github.com/{repository}/pull/{number}");
    for call in publication_calls(&repository, &url, &report)? {
        gh(root, &call.iter().map(String::as_str).collect::<Vec<_>>())?;
    }
    Ok(())
}

fn runtime_from_env() -> Result<Runtime> {
    match std::env::var("REVIEW_RUNTIME")
        .ok()
        .filter(|value| !value.is_empty())
    {
        Some(name) => clap::ValueEnum::from_str(&name, true)
            .map_err(|_| format!("REVIEW_RUNTIME={name}: use claude, codex, dsh or pi").into()),
        None => Ok(Runtime::Claude),
    }
}

fn read_metadata(root: &Path, number: &str, repository: &str) -> Result<PrMetadata> {
    let text = gh(
        root,
        &[
            "pr",
            "view",
            number,
            "--repo",
            repository,
            "--json",
            "headRefOid,headRefName,baseRefName,author,isCrossRepository",
        ],
    )?;
    Ok(serde_json::from_str(&text)?)
}

fn validate_metadata(metadata: &PrMetadata) -> Result<()> {
    if metadata.cross_repository {
        return Err("review-pr refuses pull requests from forks".into());
    }
    if metadata.base_name != "dev" {
        return Err(format!(
            "review-pr only reviews pull requests based on dev (found {})",
            metadata.base_name
        )
        .into());
    }
    if metadata.head_oid.len() != 40
        || !metadata
            .head_oid
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || metadata.head_name.is_empty()
    {
        return Err("pull request metadata has an invalid head commit or empty branch".into());
    }
    Ok(())
}

fn required_tier(floor: Tier, major_bump: bool) -> Tier {
    if major_bump && floor < Tier::Medium {
        Tier::Medium
    } else {
        floor
    }
}

struct Worktree {
    root: PathBuf,
    path: PathBuf,
}

impl Worktree {
    fn create(root: &Path, oid: &str) -> Result<Self> {
        let path =
            std::env::temp_dir().join(format!("aoeworld-review-pr-{}-{oid}", std::process::id()));
        if path.exists() {
            return Err(format!(
                "temporary review worktree already exists: {}",
                path.display()
            )
            .into());
        }
        git::git(
            root,
            &["worktree", "add", "--detach", &path.to_string_lossy(), oid],
        )?;
        Ok(Self {
            root: root.to_owned(),
            path,
        })
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .status();
    }
}

/// Status and merge calls come from ship's shared publisher; this adds the PR comment.
fn publication_calls(
    repository: &str,
    url: &str,
    report: &review::Report,
) -> Result<Vec<Vec<String>>> {
    let mut calls = review_gate::publish_calls(repository, url, report)?;
    calls.push(vec![
        "pr".into(),
        "comment".into(),
        url.into(),
        "--repo".into(),
        repository.into(),
        "--body".into(),
        report.markdown(),
    ]);
    Ok(calls)
}

fn dependency_major_bump(root: &Path, base: &str, head: &str) -> Result<bool> {
    for path in ["Cargo.toml", "Cargo.lock", "package.json"] {
        let old = git::git(root, &["show", &format!("{base}:{path}")]).unwrap_or_default();
        let new = git::git(root, &["show", &format!("{head}:{path}")]).unwrap_or_default();
        if old == new {
            continue;
        }
        let (before, after) = match path {
            "package.json" => (json_dependencies(&old), json_dependencies(&new)),
            "Cargo.toml" => (cargo_dependencies(&old), cargo_dependencies(&new)),
            _ => (lock_dependencies(&old), lock_dependencies(&new)),
        };
        if has_major_bump(&before, &after) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn major(version: &str) -> Option<u64> {
    let start = version.find(|c: char| c.is_ascii_digit())?;
    version[start..].split('.').next()?.parse().ok()
}

fn has_major_bump(before: &BTreeMap<String, String>, after: &BTreeMap<String, String>) -> bool {
    before.iter().any(|(name, old_version)| {
        after.get(name).is_some_and(|new_version| {
            major(old_version)
                .zip(major(new_version))
                .is_some_and(|(old, new)| new > old)
        })
    })
}

fn json_dependencies(text: &str) -> BTreeMap<String, String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(entries) = value.get(section).and_then(serde_json::Value::as_object) {
            for (name, value) in entries {
                if let Some(version) = value.as_str() {
                    result.insert(name.clone(), version.to_owned());
                }
            }
        }
    }
    result
}

fn cargo_dependencies(text: &str) -> BTreeMap<String, String> {
    let mut active = false;
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            active = [
                "[dependencies]",
                "[dev-dependencies]",
                "[build-dependencies]",
                "[workspace.dependencies]",
            ]
            .contains(&trimmed);
            continue;
        }
        if !active {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let version = if value.starts_with('{') {
            value.split("version").nth(1).and_then(|part| {
                part.split_once('=')
                    .map(|(_, v)| v.trim().trim_matches([' ', '"', '\'', ',', '}']))
            })
        } else {
            Some(value.trim_matches(['"', '\'']))
        };
        if let Some(version) = version.filter(|value| major(value).is_some()) {
            result.insert(name.trim().to_owned(), version.to_owned());
        }
    }
    result
}

fn lock_dependencies(text: &str) -> BTreeMap<String, String> {
    let mut name = None;
    let mut result = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            name = None;
        } else if let Some(value) = line.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_owned());
        } else if let (Some(name), Some(value)) = (name.as_ref(), line.strip_prefix("version = ")) {
            result.insert(name.clone(), value.trim_matches('"').to_owned());
        }
    }
    result
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests;
