//! Attribution for diagnostics only. No obsolete report is accepted as a review.
use crate::ship::git;
use std::{collections::BTreeSet, path::Path};

/// Branch commits, including its pre-rebase reflog tips, excluding the selected
/// base's ancestry: a shared dev commit does not identify this branch.
pub(super) fn lineage(root: &Path, branch: &str, head: &str, base: &str) -> BTreeSet<String> {
    let reflog = if branch.is_empty() {
        String::new()
    } else {
        git::git(
            root,
            &[
                "reflog",
                "show",
                "--format=%H",
                &format!("refs/heads/{branch}"),
            ],
        )
        .unwrap_or_default()
    };
    let mut args = vec!["rev-list", head];
    args.extend(reflog.lines().filter(|sha| super::full_id(sha)));
    args.extend(["--not", base, "--"]);
    let mut lineage: BTreeSet<String> = git::git(root, &args)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect();
    lineage.insert(head.to_owned());
    lineage
}

pub(super) fn belongs(
    path: &Path,
    bytes: Option<&[u8]>,
    branch: &str,
    lineage: &BTreeSet<String>,
) -> bool {
    // Reading only a JSON header attributes an unreadable report; it never
    // restores its superseded schema or makes it eligible for reuse.
    if let Some(header) = bytes.and_then(|b| serde_json::from_slice::<serde_json::Value>(b).ok()) {
        if !branch.is_empty() && header.get("branch").and_then(|v| v.as_str()) == Some(branch) {
            return true;
        }
        if header
            .get("head")
            .and_then(|v| v.as_str())
            .is_some_and(|sha| lineage.contains(sha))
        {
            return true;
        }
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split_once('-'))
        .is_some_and(|(sha, _)| super::full_id(sha) && lineage.contains(sha))
}
