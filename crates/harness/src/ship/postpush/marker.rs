//! What `make ship` or `make review-pr` approved, kept on the pull request. It is a comment with a
//! hidden marker line, updated in place by the next ship, not the status
//! description: a status description holds 140 characters and the two
//! fingerprints alone are 128. Only a marker written by someone with write
//! access counts ([`super::Comment::trusted`]).
use super::{Comment, Result, comments, post_comment, pull_number, short};
use crate::{
    review::Report,
    ship::{git, github::gh},
};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::path::Path;

pub(super) const TAG: &str = "<!-- aoe-harness:review-marker -->";

/// The approved head, its own-patch identity, the review policy fingerprint
/// and the tier and grade the review passed with.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub(super) struct Marker {
    pub(super) head: String,
    pub(super) patch: String,
    pub(super) policy: String,
    pub(super) tier: String,
    pub(super) grade: u8,
}

/// SHA-256 of the own patch of `head` against `base`: the identity a review
/// covers, recomputed from Git.
pub(super) fn patch_fingerprint(root: &Path, base: &str, head: &str) -> Result<String> {
    let (_, identity) = git::change_identity_from(root, base, head)?;
    Ok(sha2::Sha256::digest(&identity)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

impl Marker {
    /// The marker of a passing report, patch identity recomputed from Git.
    pub(super) fn of(root: &Path, report: &Report) -> Result<Self> {
        let policy = report
            .policy_fingerprint
            .clone()
            .ok_or("the review stored no policy fingerprint")?;
        Ok(Self {
            head: report.head.clone(),
            patch: patch_fingerprint(root, &report.base, &report.head)?,
            policy,
            tier: report.tier.clone(),
            grade: report.grade,
        })
    }

    pub(super) fn render(&self) -> String {
        format!(
            "{TAG}\n<!-- {} -->\nReview passed on `{}` ({}/10, {} tier). `make ship` and `make review-pr` keep this \
             comment current; when `dev` moves and the pull request is updated (dev merged in), \
             `aoe-harness carry-review` carries `harness/review` to the new head only if the \
             patch and the review policy are unchanged (docs/post-push.md).",
            serde_json::to_string(self).unwrap_or_default(),
            short(&self.head),
            self.grade,
            self.tier
        )
    }

    pub(super) fn parse(body: &str) -> Option<Self> {
        let mut lines = body.lines();
        (lines.next()? == TAG).then_some(())?;
        let json = lines.next()?.strip_prefix("<!-- ")?.strip_suffix(" -->")?;
        serde_json::from_str(json).ok()
    }
}

/// The marker a trusted comment holds, the newest one.
pub(super) fn latest(comments: &[Comment]) -> Option<Marker> {
    comments
        .iter()
        .filter(|comment| comment.trusted())
        .filter_map(|comment| Marker::parse(&comment.body))
        .next_back()
}

/// Writes the marker of `report` on the pull request at `url`: edits the
/// newest trusted marker comment, else adds one.
pub(crate) fn record(root: &Path, url: &str, report: &Report) -> Result<()> {
    let repository = git::origin_repository(root)?;
    let number = pull_number(url)?;
    let body = Marker::of(root, report)?.render();
    let existing = comments(root, &repository, number)?
        .into_iter()
        .rfind(|comment| comment.trusted() && comment.body.starts_with(TAG));
    if let Some(comment) = existing {
        let path = format!("repos/{repository}/issues/comments/{}", comment.id);
        let edited = gh(
            root,
            &[
                "api",
                "--method",
                "PATCH",
                &path,
                "-f",
                &format!("body={body}"),
            ],
        );
        if edited.is_ok() {
            return Ok(());
        }
    }
    post_comment(root, &repository, number, &body)
}
