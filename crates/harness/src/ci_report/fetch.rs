//! What the command reads: the CI bot's performance comments (cached under
//! `.cache/ci-report/`, so a second run lists only what changed since the
//! first), the `dev` log and the history of the baseline files.
use super::actions::Gh;
use super::comments::MARKER;
use super::series::{Commit, parse_commits};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};

type Result<T> = std::result::Result<T, String>;
/// A `git` call: its arguments in, its standard output out.
pub(super) type Git<'a> = &'a dyn Fn(&[&str]) -> Result<String>;

const BOT: &str = "github-actions[bot]";
const COMMENTS: &str = "comments.jsonl";
const CURSOR: &str = "comments.cursor";

/// One version of one performance comment. CI rewrites a pull request's
/// comment in place on every run, so a comment has a version per run seen.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct CommentVersion {
    pub(crate) id: u64,
    pub(crate) pr: u64,
    pub(crate) updated: String,
    pub(crate) body: String,
}

/// One line of the listing: every comment, the body only for marked ones.
#[derive(Deserialize)]
struct Listed {
    id: u64,
    issue_url: String,
    updated_at: String,
    bot: bool,
    body: Option<String>,
}

fn jq() -> String {
    format!(
        r#".[] | {{id, issue_url, updated_at, bot: (.user.login == "{BOT}"), body: (if ((.body // "") | startswith("{MARKER}")) then .body else null end)}}"#
    )
}

/// The cached versions and the newest edit seen; a damaged cache is no cache.
fn read_cache(dir: &Path) -> (Vec<CommentVersion>, Option<String>) {
    let versions: Option<Vec<CommentVersion>> = fs::read_to_string(dir.join(COMMENTS))
        .ok()
        .and_then(|text| {
            text.lines()
                .map(|line| serde_json::from_str(line).ok())
                .collect()
        });
    let cursor = fs::read_to_string(dir.join(CURSOR))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty());
    match (versions, cursor) {
        (Some(versions), Some(cursor)) => (versions, Some(cursor)),
        _ => (Vec::new(), None),
    }
}

/// Every version of the bot's performance comments on the repository's
/// issues and pull requests, oldest first. Listing the repository's issue
/// comments since the last run's newest edit is cheap; the first run lists all.
pub(super) fn comments(cache_dir: &Path, repo: &str, gh: Gh) -> Result<Vec<CommentVersion>> {
    let (mut versions, mut cursor) = read_cache(cache_dir);
    let mut path = format!("repos/{repo}/issues/comments?per_page=100");
    if let Some(since) = &cursor {
        path.push_str(&format!("&since={since}"));
    }
    let listing = gh(&["api", "--paginate", &path, "--jq", &jq()])?;
    let mut known: BTreeSet<(u64, String)> = versions
        .iter()
        .map(|version| (version.id, version.updated.clone()))
        .collect();
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let listed: Listed =
            serde_json::from_str(line).map_err(|error| format!("comments: {error}"))?;
        if cursor.as_ref().is_none_or(|seen| listed.updated_at > *seen) {
            cursor = Some(listed.updated_at.clone());
        }
        let pr = listed
            .issue_url
            .rsplit('/')
            .next()
            .and_then(|n| n.parse().ok());
        if let (true, Some(body), Some(pr)) = (listed.bot, listed.body, pr)
            && known.insert((listed.id, listed.updated_at.clone()))
        {
            versions.push(CommentVersion {
                id: listed.id,
                pr,
                updated: listed.updated_at,
                body,
            });
        }
    }
    versions.sort_by(|a, b| a.updated.cmp(&b.updated).then(a.id.cmp(&b.id)));
    let lines: Vec<String> = versions
        .iter()
        .filter_map(|version| serde_json::to_string(version).ok())
        .collect();
    fs::create_dir_all(cache_dir)
        .and_then(|()| fs::write(cache_dir.join(COMMENTS), lines.join("\n") + "\n"))
        .and_then(|()| fs::write(cache_dir.join(CURSOR), cursor.unwrap_or_default()))
        .map_err(|error| format!("{}: {error}", cache_dir.display()))?;
    Ok(versions)
}

/// The commits of `origin/dev`, oldest first.
pub(super) fn dev_commits(git: Git) -> Result<Vec<Commit>> {
    parse_commits(&git(&["log", "origin/dev", "--format=%H%x09%ct%x09%s"])?)
}

/// Each commit of `origin/dev` that changed `path` with the file's text at
/// it, oldest first, as (index in `commits`, text).
pub(super) fn file_history(
    git: Git,
    commits: &[Commit],
    path: &str,
) -> Result<Vec<(usize, String)>> {
    let listed = git(&["log", "origin/dev", "--format=%H", "--", path])?;
    let mut history = Vec::new();
    for sha in listed.lines().filter(|line| !line.is_empty()) {
        let Some(position) = commits.iter().position(|commit| commit.sha == sha) else {
            continue;
        };
        history.push((position, git(&["show", &format!("{sha}:{path}")])?));
    }
    history.sort_by_key(|(position, _)| *position);
    Ok(history)
}
