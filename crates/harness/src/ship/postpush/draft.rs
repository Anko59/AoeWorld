//! Publish the gated head as a draft before starting its review.
use crate::ship::{Evidence, Options, Result, git, github, recheck};
use std::{fs, path::Path};

pub(in crate::ship) fn start(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
    body: &str,
    started: &str,
    tracking: Option<&str>,
) -> Result<()> {
    options.lock.reacquire(root, "make ship (draft push)")?;
    crate::timeline::stage(root, "draft-push", serde_json::json!({}), || {
        let subject = git::subject(root)?;
        if subject.head != evidence.head
            || subject.tree != evidence.tree
            || subject.branch != evidence.branch
        {
            return Err("ship: checkout changed after the gates; refusing draft push".into());
        }
        let repository = git::origin_repository(root)?;
        let existing = github::open_pull_request(root, &repository, &evidence.branch)?;
        if let Some(pr) = &existing {
            // One read: whether it is a draft and whether auto-merge is armed. A review-fix
            // reship finds a draft with no auto-merge, which GitHub refuses to disable.
            let state = github::gh(
                root,
                &[
                    "pr",
                    "view",
                    &pr.url,
                    "--repo",
                    &repository,
                    "--json",
                    "isDraft,autoMergeRequest",
                ],
            )?;
            let state = PrState::parse(&state)?;
            if state.armed() {
                let args = github::disable_auto_args(&pr.url, &repository);
                match github::gh(root, &args.iter().map(String::as_str).collect::<Vec<_>>()) {
                    Err(error) if !github::auto_merge_not_enabled(&error.to_string()) => {
                        return Err(error);
                    }
                    _ => {}
                }
            }
            if !state.is_draft {
                github::gh(
                    root,
                    &["pr", "ready", &pr.url, "--repo", &repository, "--undo"],
                )?;
            }
        }
        // Publish the exact bytes describe-check accepted, even if the input file changes.
        let body_file = super::super::evidence::directory(root)?
            .with_file_name("bodies")
            .join(format!("{}-draft.md", evidence.head));
        fs::create_dir_all(body_file.parent().ok_or("no body directory")?)?;
        fs::write(&body_file, body)?;
        push(root, evidence, options, started, tracking, None)?;
        let options = Options {
            body_file: Some(body_file),
            ..options.clone()
        };
        github::upsert(root, evidence, &options, existing, true)?;
        Ok(())
    })?;
    if options.lock.release() {
        eprintln!("ship: draft pushed; lock released for the review (CI runs alongside it)");
    }
    Ok(())
}

/// The same lease rules at both pushes. After the draft push, `started` and
/// `tracking` are the head that push wrote, including a start-of-ship rebase.
pub(in crate::ship) fn push(
    root: &Path,
    evidence: &Evidence,
    options: &Options,
    started: &str,
    tracking: Option<&str>,
    lease: Option<String>,
) -> Result<()> {
    let lease = if evidence.head != started && lease.is_none() {
        recheck::lease(root, options, &git::subject(root)?, started)?
    } else {
        lease
    };
    let expected = lease
        .as_deref()
        .or(options.force.then_some(tracking.unwrap_or("")));
    let refreshed = match expected {
        Some(tip) => git::lease::refresh(root, &evidence.branch, tip, started),
        None => git::lease::refresh_default(
            root,
            &evidence.branch,
            started,
            &evidence.head,
            tracking.is_some(),
        ),
    }
    .map_err(|error| format!("git push failed: {error}"))?;
    git::publish::push(root, evidence, Some(&refreshed))
}

/// `gh pr view --json isDraft,autoMergeRequest`, parsed here rather than with `--jq` so the
/// exact shape GitHub returns is what the tests feed in.
#[derive(serde::Deserialize, Debug, PartialEq, Eq)]
pub(in crate::ship) struct PrState {
    #[serde(rename = "isDraft")]
    pub(in crate::ship) is_draft: bool,
    #[serde(rename = "autoMergeRequest")]
    auto_merge_request: Option<serde_json::Value>,
}

impl PrState {
    pub(in crate::ship) fn parse(json: &str) -> crate::ship::Result<Self> {
        serde_json::from_str(json).map_err(|error| {
            format!("ship: unreadable pull request state ({error}): {json}").into()
        })
    }

    /// Whether GitHub has an auto-merge armed for this pull request.
    pub(in crate::ship) fn armed(&self) -> bool {
        self.auto_merge_request.is_some()
    }
}
