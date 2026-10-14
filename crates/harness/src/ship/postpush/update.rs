//! `make update-prs`: on every push to `dev`, rebases the open pull requests
//! into `dev` that have auto-merge armed and are behind it, so that CI tests
//! the combination GitHub will merge. Linear history requires a rebase
//! (`updatePullRequestBranch`, REBASE). A rebase that conflicts leaves the pull
//! request untouched and adds one comment naming the files. At most
//! `UPDATE_MAX_PRS` pull requests are handled per run, the oldest first.
use super::{BASE, Result, api, comments, post_comment, require_main_session, short};
use crate::ship::{git, github::gh};
use std::path::Path;

const DEFAULT_MAX: usize = 5;
const CONFLICT_TAG: &str = "<!-- aoe-harness:update-conflict ";
const FAILURE_TAG: &str = "<!-- aoe-harness:update-failure ";

const LIST: &str = "query ListWaiting($owner: String!, $name: String!) { \
    repository(owner: $owner, name: $name) { \
    pullRequests(states: OPEN, baseRefName: \"dev\", first: 100, \
    orderBy: {field: CREATED_AT, direction: ASC}) { \
    nodes { id number headRefOid isCrossRepository autoMergeRequest { enabledAt } } } } }";

const REBASE: &str = "mutation UpdateBranch($id: ID!, $oid: GitObjectID!) { \
    updatePullRequestBranch(input: {pullRequestId: $id, expectedHeadOid: $oid, \
    updateMethod: REBASE}) { pullRequest { number } } }";

struct Waiting {
    id: String,
    number: u64,
    head: String,
}

pub(in crate::ship) fn execute(root: &Path) -> Result<()> {
    require_main_session("update-prs")?;
    // A rebase pushed with the workflow's own token starts no CI run, so the
    // workflow passes a personal token; without one, the job does nothing.
    if std::env::var("GITHUB_ACTIONS").is_ok_and(|v| v == "true")
        && std::env::var("GH_TOKEN").unwrap_or_default().is_empty()
    {
        println!(
            "update-prs: skipped, the PR_UPDATE_TOKEN secret is not set (a rebase pushed with \
             the workflow token would not start CI)"
        );
        return Ok(());
    }
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
            Ok(_) => println!(
                "update-prs: #{} rebased onto {BASE} (was {})",
                pr.number,
                short(&pr.head)
            ),
            Err(error) => match super::replay::conflicting_files(root, pr.number, &pr.head) {
                Ok(files) if !files.is_empty() => conflict(root, &repository, &pr, &files)?,
                diagnosis => {
                    let detail = match diagnosis {
                        Ok(_) => "local commit replay succeeded; no conflicting files".to_owned(),
                        Err(why) => format!("conflict diagnosis unavailable: {why}"),
                    };
                    failure(root, &repository, &pr, &format!("{error}; {detail}"))?;
                    problems.push(format!("#{}: {error}", pr.number));
                }
            },
        }
    }
    if problems.is_empty() {
        return Ok(());
    }
    Err(format!("update-prs: {}", problems.join("; ")).into())
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
            "This pull request could not be rebased onto `{BASE}` ({} files conflict: {list}). \
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
            "This pull request could not be rebased onto `{BASE}` for another reason: {why}. \
         It is left as it is; retry the update after resolving this failure."
        ),
    )?;
    println!("update-prs: #{} rebase failed: {why}", pr.number);
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
