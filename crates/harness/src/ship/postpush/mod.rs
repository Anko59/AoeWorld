//! What happens to a pull request after `make ship` pushed it (docs/post-push.md).
//! The shared review publisher records what was approved ([`marker`]); while the pull request
//! waits for CI, a workflow merges dev into it when `dev` moves ([`update`]) and carries
//! `harness/review` to the updated head when the patch and the review policy
//! are unchanged ([`carry`]). Nothing here merges or approves a pull request.
use super::{Result, git, github::gh};
use serde::Deserialize;
use std::path::Path;

pub(super) mod carry;
pub(super) mod marker;
mod replay;
pub(super) mod reship;
pub(super) mod update;

#[cfg(test)]
mod tests;

/// The branch these commands serve: pull requests into `dev` only.
const BASE: &str = "dev";

/// One comment of a pull request, as the issues API lists it.
#[derive(Deserialize)]
pub(super) struct Comment {
    pub(super) id: u64,
    #[serde(default)]
    pub(super) body: String,
    #[serde(default)]
    pub(super) author_association: String,
}

impl Comment {
    /// Written by someone with write access: anyone else can comment on a
    /// pull request, so a marker of theirs is never believed.
    pub(super) fn trusted(&self) -> bool {
        matches!(
            self.author_association.as_str(),
            "OWNER" | "MEMBER" | "COLLABORATOR"
        )
    }
}

pub(super) fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// `gh api` with `args`, its output read as JSON.
pub(super) fn api(root: &Path, args: &[&str]) -> Result<serde_json::Value> {
    let (host, _) = git::origin_host_repository(root)?;
    let mut full = vec!["api", "--hostname", &host];
    full.extend(args);
    Ok(serde_json::from_str(&gh(root, &full)?)?)
}

/// Every comment of pull request `number`, oldest first.
pub(super) fn comments(root: &Path, repository: &str, number: u64) -> Result<Vec<Comment>> {
    let path = format!("repos/{repository}/issues/{number}/comments");
    let pages = api(root, &["--paginate", "--slurp", &path])?;
    let mut all = Vec::new();
    // `--slurp` wraps the pages in one array; a single page arrives as is.
    for page in pages.as_array().into_iter().flatten() {
        match page.as_array() {
            Some(items) => all.extend(items.iter().cloned()),
            None => all.push(page.clone()),
        }
    }
    all.into_iter()
        .map(|item| serde_json::from_value(item).map_err(Into::into))
        .collect()
}

pub(super) fn post_comment(root: &Path, repository: &str, number: u64, body: &str) -> Result<()> {
    let path = format!("repos/{repository}/issues/{number}/comments");
    gh(
        root,
        &[
            "api",
            "--method",
            "POST",
            &path,
            "-f",
            &format!("body={body}"),
        ],
    )?;
    Ok(())
}

/// The pull request number at the end of `https://github.com/o/r/pull/N`.
pub(super) fn pull_number(url: &str) -> Result<u64> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|last| last.parse().ok())
        .ok_or_else(|| format!("no pull request number in {url}").into())
}

/// Fetches one ref of origin and returns the commit it points at.
pub(super) fn fetch(root: &Path, reference: &str) -> Result<String> {
    git::git(
        root,
        &["fetch", "--quiet", "--no-tags", "origin", reference],
    )?;
    Ok(git::git(root, &["rev-parse", "FETCH_HEAD^{commit}"])?)
}

/// CI only: refuse to run for a role that is not the main session.
pub(super) fn require_main_session(command: &str) -> Result<()> {
    if crate::agents::main_session() {
        return Ok(());
    }
    Err(format!("make {command} is available only to the main session and CI").into())
}
