//! `make update-prs`: on every push to `dev`, updates the open pull requests
//! into `dev` that have auto-merge armed and are behind it, so that CI tests
//! the combination GitHub will merge. The update merges `dev` into the pull
//! request (`updatePullRequestBranch`, MERGE): a REBASE would rewrite the
//! author's signed commits as unsigned ones, and `dev` requires signed commits
//! on the head branch even for a squash merge; the squash keeps `dev` linear.
//! An update that conflicts leaves the pull
//! request untouched and adds one comment naming the files. At most
//! `UPDATE_MAX_PRS` pull requests are handled per run, the oldest first.
use super::{BASE, Result, api, carry, comments, post_comment, require_main_session, short};
use crate::ship::{git, github::gh};
use std::{
    path::Path,
    time::{Duration, Instant},
};

const DEFAULT_MAX: usize = 5;
pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(5);
pub(super) const HEAD_TIMEOUT: Duration = Duration::from_secs(120);
const CONFLICT_TAG: &str = "<!-- aoe-harness:update-conflict ";
const FAILURE_TAG: &str = "<!-- aoe-harness:update-failure ";

const LIST: &str = "query ListWaiting($owner: String!, $name: String!) { \
    repository(owner: $owner, name: $name) { \
    pullRequests(states: OPEN, baseRefName: \"dev\", first: 100, \
    orderBy: {field: CREATED_AT, direction: ASC}) { \
    nodes { id number headRefOid headRefName isCrossRepository autoMergeRequest { enabledAt } } } } }";

const HEAD: &str = "query UpdatedHead($owner: String!, $name: String!, $number: Int!) { \
    repository(owner: $owner, name: $name) { pullRequest(number: $number) { headRefOid } } }";

const REBASE: &str = "mutation UpdateBranch($id: ID!, $oid: GitObjectID!) { \
    updatePullRequestBranch(input: {pullRequestId: $id, expectedHeadOid: $oid, \
    updateMethod: MERGE}) { pullRequest { number } } }";

struct Waiting {
    id: String,
    number: u64,
    head: String,
    branch: String,
}

pub(in crate::ship) fn execute(root: &Path) -> Result<()> {
    require_main_session("update-prs")?;
    let max = std::env::var("UPDATE_MAX_PRS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_MAX);
    update(root, max)
}

fn waiting(root: &Path, repository: &str) -> Result<Vec<Waiting>> {
    let (owner, name) = repository
        .split_once('/')
        .ok_or("origin has no owner/name")?;
    let listing = api(
        root,
        &[
            "graphql",
            "-f",
            &format!("query={LIST}"),
            "-F",
            &format!("owner={owner}"),
            "-F",
            &format!("name={name}"),
        ],
    )?;
    let nodes = listing
        .pointer("/data/repository/pullRequests/nodes")
        .and_then(|nodes| nodes.as_array())
        .ok_or("the pull request listing has no nodes")?;
    let text = |node: &serde_json::Value, key: &str| {
        node.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned()
    };
    let mut found: Vec<Waiting> = nodes
        .iter()
        .filter(|node| !node["autoMergeRequest"].is_null())
        .filter(|node| node["isCrossRepository"] != true)
        .filter_map(|node| {
            Some(Waiting {
                id: text(node, "id"),
                number: node["number"].as_u64()?,
                head: text(node, "headRefOid"),
                branch: text(node, "headRefName"),
            })
        })
        .collect();
    found.sort_by_key(|pr| pr.number);
    Ok(found)
}

fn behind(root: &Path, repository: &str, pr: &Waiting) -> Result<bool> {
    let compare = api(
        root,
        &[&format!("repos/{repository}/compare/{BASE}...{}", pr.head)],
    )?;
    Ok(compare["behind_by"].as_u64().unwrap_or(0) > 0)
}

pub(super) fn update(root: &Path, max: usize) -> Result<()> {
    update_within(root, max, HEAD_TIMEOUT, POLL_INTERVAL)
}

/// [`update`] with the bound on how long an updated head may take to appear.
pub(super) fn update_within(
    root: &Path,
    max: usize,
    timeout: Duration,
    interval: Duration,
) -> Result<()> {
    let repository = git::origin_repository(root)?;
    let mut handled = 0;
    let mut problems = Vec::new();
    for pr in waiting(root, &repository)? {
        if handled == max {
            println!(
                "update-prs: stopped at {max} pull request(s); the next push to {BASE} continues"
            );
            break;
        }
        if !behind(root, &repository, &pr)? {
            continue;
        }
        handled += 1;
        let rebased = gh(
            root,
            &[
                "api",
                "graphql",
                "-f",
                &format!("query={REBASE}"),
                "-f",
                &format!("id={}", pr.id),
                "-f",
                &format!("oid={}", pr.head),
            ],
        );
        match rebased {
            Ok(_) => match finish_rebase(root, &repository, &pr, timeout, interval) {
                Ok(()) => println!(
                    "update-prs: #{} updated (dev merged in) (was {}), review checked and CI dispatched",
                    pr.number,
                    short(&pr.head)
                ),
                Err(error) => {
                    record_failure(root, &repository, &pr, &error.to_string(), &mut problems);
                }
            },
            Err(error) => match super::replay::conflicting_files(root, pr.number, &pr.head) {
                Ok(files) if !files.is_empty() => conflict(root, &repository, &pr, &files)?,
                diagnosis => {
                    let detail = match diagnosis {
                        Ok(_) => "local commit replay succeeded; no conflicting files".to_owned(),
                        Err(why) => format!("conflict diagnosis unavailable: {why}"),
                    };
                    record_failure(
                        root,
                        &repository,
                        &pr,
                        &format!("{error}; {detail}"),
                        &mut problems,
                    );
                }
            },
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!("update-prs: {}", problems.join("; ")).into())
}

fn record_failure(
    root: &Path,
    repository: &str,
    pr: &Waiting,
    why: &str,
    problems: &mut Vec<String>,
) {
    problems.push(format!("#{}: {why}", pr.number));
    if let Err(error) = failure(root, repository, pr, why) {
        problems.push(format!(
            "#{}: failure comment could not be posted: {error}",
            pr.number
        ));
    }
}

fn moved_head(
    root: &Path,
    repository: &str,
    pr: &Waiting,
    timeout: Duration,
    interval: Duration,
) -> Result<String> {
    let (owner, name) = repository
        .split_once('/')
        .ok_or("origin has no owner/name")?;
    let start = Instant::now();
    loop {
        let pull = api(
            root,
            &[
                "graphql",
                "-f",
                &format!("query={HEAD}"),
                "-F",
                &format!("owner={owner}"),
                "-F",
                &format!("name={name}"),
                "-F",
                &format!("number={}", pr.number),
            ],
        )?;
        let head = pull
            .pointer("/data/repository/pullRequest/headRefOid")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or("the updated pull request has no headRefOid")?;
        if head != pr.head {
            return Ok(head.to_owned());
        }
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(format!(
                "head did not move from {} within {} seconds after the dev merge was accepted",
                short(&pr.head),
                timeout.as_secs()
            )
            .into());
        }
        std::thread::sleep(interval.min(remaining));
    }
}

fn finish_rebase(
    root: &Path,
    repository: &str,
    pr: &Waiting,
    timeout: Duration,
    interval: Duration,
) -> Result<()> {
    moved_head(root, repository, pr, timeout, interval)?;
    carry::carry(root, pr.number).map_err(|error| format!("review carry failed: {error}"))?;
    if pr.branch.is_empty() {
        return Err("the pull request has no headRefName for CI dispatch".into());
    }
    gh(
        root,
        &[
            "api",
            "--method",
            "POST",
            &format!("repos/{repository}/actions/workflows/ci.yml/dispatches"),
            "-f",
            &format!("ref={}", pr.branch),
        ],
    )
    .map_err(|error| format!("CI dispatch failed: {error}"))?;
    println!("update-prs: #{} CI dispatched on {}", pr.number, pr.branch);
    Ok(())
}

/// One comment per head, so a conflict that stays does not repeat on every push.
fn conflict(root: &Path, repository: &str, pr: &Waiting, files: &[String]) -> Result<()> {
    let list = files
        .iter()
        .map(|f| format!("`{f}`"))
        .collect::<Vec<_>>()
        .join(", ");
    refusal(
        root,
        repository,
        pr,
        CONFLICT_TAG,
        &format!(
            "This pull request could not be updated (dev merged in) from `{BASE}` ({} files conflict: {list}). \
             It is left as it is: rebase it, resolve the conflict, and ship again.",
            files.len()
        ),
    )?;
    println!("update-prs: #{} conflicts with {BASE} in {list}", pr.number);
    Ok(())
}

fn failure(root: &Path, repository: &str, pr: &Waiting, why: &str) -> Result<()> {
    refusal(
        root,
        repository,
        pr,
        FAILURE_TAG,
        &format!(
            "Updating this pull request onto `{BASE}` failed: {why}. \
         The dev merge may already have landed; resolve this failure and retry the failed step."
        ),
    )?;
    println!("update-prs: #{} update failed: {why}", pr.number);
    Ok(())
}

/// One comment per head and kind: a transient failure cannot hide a conflict.
fn refusal(root: &Path, repository: &str, pr: &Waiting, prefix: &str, message: &str) -> Result<()> {
    let tag = format!("{prefix}{} -->", pr.head);
    if comments(root, repository, pr.number)?
        .iter()
        .any(|comment| comment.body.starts_with(&tag))
    {
        return Ok(());
    }
    post_comment(root, repository, pr.number, &format!("{tag}\n{message}"))
}
