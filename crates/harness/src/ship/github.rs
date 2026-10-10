//! `make ship`'s GitHub side: `gh` checks, the pull request (found by head
//! branch, on origin's repository), the description, and the
//! showcase video probe.
use super::{Evidence, Options, Result, describe, git};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

/// The oldest `gh` with `pr edit --attach` (2.100) for PR videos; older ones are refused.
pub(super) const GH_MINIMUM: (u32, u32) = (2, 100);

/// A video GitHub can attach, whose file name is safe to pass to
/// `gh --attach` and to show in Markdown (no `#`, spaces, brackets or newlines).
pub(super) fn attachable_video(path: &Path) -> bool {
    let safe_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        });
    safe_name
        && matches!(
            path.extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("mp4" | "mov" | "webm")
        )
}

pub(super) fn gh_version(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(2)?;
    let mut parts = version.split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// What the pull request checks found before any gate ran.
#[derive(Default)]
pub(super) struct Preflight {
    /// The validated `SHIP_BODY` text.
    pub(super) body: Option<String>,
    /// The open pull request a stacked base belongs to.
    pub(super) parent: Option<super::stack::Parent>,
    /// The base the branch's open pull request targets now, if any.
    pub(super) current_base: Option<String>,
}

/// Before any gate runs: a usable `gh`, a valid base, and a title and
/// description when the branch has no open pull request yet, so nothing is
/// pushed half-way.
pub(super) fn preflight_pull_request(root: &Path, options: &Options) -> Result<Preflight> {
    if let Some(video) = &options.video
        && !attachable_video(video)
    {
        return Err(format!(
            "SHIP_VIDEO {} must be a .mp4, .mov or .webm file named with letters, digits, `.`, `_` or `-` only",
            video.display()
        )
        .into());
    }
    let version = gh(root, &["--version"])?;
    match gh_version(&version) {
        Some(found) if found >= GH_MINIMUM => {}
        _ => {
            return Err(format!(
                "gh {}.{} or newer is required (found: {})",
                GH_MINIMUM.0,
                GH_MINIMUM.1,
                version.lines().next().unwrap_or_default()
            )
            .into());
        }
    }
    let branch = git::branch(root)?;
    let repository = git::origin_repository(root)?;
    let parent = if options.base == crate::review::base::DEV {
        None
    } else {
        Some(super::stack::parent(root, &repository, &options.base)?)
    };
    let existing = open_pull_request(root, &repository, &branch)?;
    let open = existing.is_some();
    if !open && (options.title.is_none() || options.body_file.is_none()) {
        return Err("a new pull request needs SHIP_TITLE and SHIP_BODY (a description file); nothing was run".into());
    }
    let template_text = if let Some(body) = &options.body_file {
        if !body.is_file() {
            return Err(format!("SHIP_BODY {} is not a file", body.display()).into());
        }
        let text = fs::read_to_string(body)?;
        let template = describe::Template::parse(&text)?;
        let probe = match &options.video {
            Some(video) => Some(probe_video(root, video)?),
            None => None,
        };
        template.check(changed_lines(root, &options.base)?, probe)?;
        Some(text)
    } else {
        None
    };
    Ok(Preflight {
        body: template_text,
        parent,
        current_base: existing.map(|pr| pr.base),
    })
}

/// A branch's open pull request: its URL and the branch it targets.
#[derive(serde::Deserialize)]
struct OpenPr {
    url: String,
    #[serde(rename = "baseRefName")]
    base: String,
}

/// The open pull request whose head is `branch` (never a PR number that
/// happens to equal a numeric branch name).
fn open_pull_request(root: &Path, repository: &str, branch: &str) -> Result<Option<OpenPr>> {
    let found = gh(
        root,
        &[
            "pr",
            "list",
            "--repo",
            repository,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "url,baseRefName",
            "--jq",
            ".[0] // empty",
        ],
    )?;
    if found.is_empty() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&found)?))
}

/// `gh pr create` into `base` (dev, or a stacked parent branch).
pub(super) fn create_args(
    repository: &str,
    base: &str,
    branch: &str,
    title: &str,
    body: &str,
) -> Vec<String> {
    [
        "pr",
        "create",
        "--repo",
        repository,
        "--base",
        base,
        "--head",
        branch,
        "--title",
        title,
        "--body-file",
        body,
    ]
    .map(str::to_owned)
    .to_vec()
}

/// `gh pr edit`, moving the pull request to `retarget` when given; `None`
/// when there is nothing to change.
pub(super) fn edit_args(
    url: &str,
    repository: &str,
    title: Option<&str>,
    body: Option<&str>,
    retarget: Option<&str>,
) -> Vec<String> {
    let mut args = vec!["pr", "edit", url, "--repo", repository];
    for (flag, value) in [
        ("--title", title),
        ("--body-file", body),
        ("--base", retarget),
    ] {
        if let Some(value) = value {
            args.extend([flag, value]);
        }
    }
    args.into_iter().map(str::to_owned).collect()
}

pub(super) fn pull_request(root: &Path, evidence: &Evidence, options: &Options) -> Result<String> {
    let branch = evidence.branch.as_str();
    let repository = git::origin_repository(root)?;
    let body = options.body_file.as_ref().map(|p| p.display().to_string());
    let existing = open_pull_request(root, &repository, branch)?;
    let url = match existing {
        Some(pr) => {
            let retarget = super::stack::retarget(Some(&pr.base), &options.base);
            let args = edit_args(
                &pr.url,
                &repository,
                options.title.as_deref(),
                body.as_deref(),
                retarget,
            );
            if args.len() > 5 {
                gh(root, &args.iter().map(String::as_str).collect::<Vec<_>>())?;
            }
            pr.url
        }
        None => {
            let (Some(title), Some(body)) = (options.title.as_deref(), body.as_deref()) else {
                return Err(
                    "a new pull request needs SHIP_TITLE and SHIP_BODY (a description file)".into(),
                );
            };
            let args = create_args(&repository, &options.base, branch, title, body);
            gh(root, &args.iter().map(String::as_str).collect::<Vec<_>>())?
        }
    };
    println!("{url}");
    Ok(url)
}

/// Run one publication call. GitHub refuses to arm auto-merge on a pull
/// request that is already mergeable ("clean status"); that merge call is then
/// made directly, still pinned to the reviewed commit by --match-head-commit.
pub(crate) fn publish_call(root: &Path, call: &[String]) -> Result<()> {
    let args: Vec<&str> = call.iter().map(String::as_str).collect();
    match gh(root, &args) {
        Ok(_) => Ok(()),
        Err(error) => match direct_merge(call, &error.to_string()) {
            Some(direct) => {
                let direct: Vec<&str> = direct.iter().map(String::as_str).collect();
                gh(root, &direct).map(|_| ())
            }
            None => Err(error),
        },
    }
}

/// The same merge without `--auto`, when GitHub refused auto-merge because the
/// pull request is already clean and the call is pinned to a commit.
pub(crate) fn direct_merge(call: &[String], error: &str) -> Option<Vec<String>> {
    let merge = call.first().map(String::as_str) == Some("pr")
        && call.get(1).map(String::as_str) == Some("merge")
        && call.iter().any(|a| a == "--auto")
        && call.iter().any(|a| a == "--match-head-commit");
    (merge && error.contains("clean status"))
        .then(|| call.iter().filter(|a| *a != "--auto").cloned().collect())
}

pub(crate) fn gh(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("gh").current_dir(root).args(args).output()?;
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

/// Lines added plus removed against the merge base with origin/<base>; a
/// binary file (`-` in numstat) counts as larger than any `low` PR.
pub(super) fn changed_lines(root: &Path, base: &str) -> Result<u64> {
    let (_, merge_base) = git::base(root, base, true)?;
    let numstat = git::git(root, &["diff", "--numstat", &merge_base, "HEAD"])?;
    Ok(numstat
        .lines()
        .map(|l| {
            l.split('\t')
                .take(2)
                .map(|n| {
                    n.parse::<u64>()
                        .unwrap_or(super::describe::LOW_MAX_CHANGED_LINES + 1)
                })
                .sum::<u64>()
        })
        .sum())
}

/// Duration and sound of a video, read by ffmpeg in the pinned browser image.
fn probe_video(root: &Path, video: &Path) -> Result<(u32, bool)> {
    if !attachable_video(video) {
        return Err(format!(
            "SHIP_VIDEO {} must be a .mp4, .mov or .webm file named with letters, digits, `.`, `_` or `-` only",
            video.display()
        )
        .into());
    }
    let video =
        fs::canonicalize(video).map_err(|e| format!("SHIP_VIDEO {}: {e}", video.display()))?;
    let output = Command::new("make")
        .current_dir(root)
        .args(["--no-print-directory", "video-probe"])
        .env("SHIP_VIDEO", &video)
        .output()?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    describe::parse_probe(&text)
        .ok_or_else(|| format!("{} is not a readable video", video.display()).into())
}

/// The two-part description: the agent's template and the review.
pub(super) fn description(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
    report: &crate::review::Report,
    body: &str,
) -> Result<PathBuf> {
    let template = describe::Template::parse(body)?;
    let video = options
        .video
        .as_ref()
        .and_then(|v| v.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    let text = describe::render(
        &template,
        video.as_deref(),
        report,
        evidence,
        &describe::footer(options.runtime),
        &super::metrics::table(root, &evidence.base_branch)?,
        &super::test_first::summary(root, &evidence.merge_base, &evidence.head),
    );
    let text = match &options.parent {
        Some(parent) => format!("{}\n{text}", super::stack::note(parent)),
        None => text,
    };
    // A private directory in the git common dir, never a guessable /tmp path.
    let path = super::evidence::directory(root)?
        .with_file_name("bodies")
        .join(format!("{}.md", report.head));
    fs::create_dir_all(path.parent().ok_or("no body directory")?)?;
    fs::write(&path, text)?;
    Ok(path)
}
