//! `make carry-review`: on a pull request `synchronize` event, posts
//! `harness/review` = success on the new head when its own patch and the
//! current review policy equal what the marker recorded ([`super::marker`]) and
//! otherwise posts nothing but one comment saying why. The same rule as the
//! local re-check carry (`recheck/carry.rs`), moved to where dev is merged in.
//! A head whose own patch differs never gets a success.
use super::{
    BASE, Result, api, comments,
    marker::{Marker, latest, patch_fingerprint},
    post_comment, require_main_session, short,
};
use crate::{
    review::Report,
    ship::{git, github::gh},
};
use std::path::Path;

/// What to do with a new head.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    /// Post success, naming the approved head it is carried from.
    Carry { from: String },
    /// Post nothing; say why.
    Refuse(String),
}

/// Whether the approval in `marker` covers `head`, a commit of a pull request
/// into `base`. Both must already be commits of this repository.
pub(super) fn decide(
    root: &Path,
    marker: Option<&Marker>,
    head: &str,
    base: &str,
) -> Result<Decision> {
    let Some(marker) = marker else {
        return Ok(Decision::Refuse(
            "no review marker from a maintainer on this pull request (`make ship` and `make review-pr` write it)"
                .into(),
        ));
    };
    if patch_fingerprint(root, base, head)? != marker.patch {
        return Ok(Decision::Refuse(format!(
            "patch changed: the own change of {} differs from the approved one of {}",
            short(head),
            short(&marker.head)
        )));
    }
    let policy = Report::policy_fingerprint_at(root, base, &marker.tier)?;
    if policy != marker.policy {
        return Ok(Decision::Refuse(format!(
            "policy changed: the review policy on {BASE} differs from the one {} was approved under",
            short(&marker.head)
        )));
    }
    Ok(Decision::Carry {
        from: marker.head.clone(),
    })
}

pub(in crate::ship) fn execute(root: &Path) -> Result<()> {
    require_main_session("carry-review")?;
    let number: u64 = std::env::var("CARRY_PR")
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or("set CARRY_PR to the number of the pull request")?;
    carry(root, number)
}

pub(super) fn carry(root: &Path, number: u64) -> Result<()> {
    let repository = git::origin_repository(root)?;
    let pull = api(root, &[&format!("repos/{repository}/pulls/{number}")])?;
    let text = |pointer: &str| pull.pointer(pointer).and_then(|v| v.as_str()).unwrap_or("");
    if text("/state") != "open" || text("/base/ref") != BASE {
        println!("carry-review: #{number} is not an open pull request into {BASE}; nothing to do");
        return Ok(());
    }
    if pull["draft"].as_bool() == Some(true) {
        println!("carry-review: #{number} is a draft; review status waits for approval");
        return Ok(());
    }
    let head = text("/head/sha").to_owned();
    // Data only: the objects are read by Git, no code of the pull request runs.
    if super::fetch(root, &format!("refs/pull/{number}/head"))? != head {
        println!(
            "carry-review: #{number} moved past {}; its next event decides",
            short(&head)
        );
        return Ok(());
    }
    let base = super::fetch(root, &format!("refs/heads/{BASE}"))?;
    let marker = latest(&comments(root, &repository, number)?);
    match decide(root, marker.as_ref(), &head, &base)? {
        Decision::Carry { from } => {
            let description = format!("carried from {}: patch and policy unchanged", short(&from));
            gh(
                root,
                &[
                    "api",
                    "--method",
                    "POST",
                    &format!("repos/{repository}/statuses/{head}"),
                    "-f",
                    "state=success",
                    "-f",
                    "context=harness/review",
                    "-f",
                    &format!("description={description}"),
                ],
            )?;
            post_comment(
                root,
                &repository,
                number,
                &format!(
                    "`harness/review` was carried to {}: {description}",
                    short(&head)
                ),
            )?;
            println!("carry-review: #{number} {}: {description}", short(&head));
        }
        Decision::Refuse(why) => {
            post_comment(
                root,
                &repository,
                number,
                &format!(
                    "`harness/review` was not carried to {}: {why}. Ship again for a review of the change.",
                    short(&head)
                ),
            )?;
            println!("carry-review: #{number} {}: {why}", short(&head));
        }
    }
    Ok(())
}
