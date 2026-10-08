use super::{Config, Live, Plan, Report, Result, Tier, review_with_branch};
use crate::{agents::Runtime, ship::git};
use std::path::Path;

pub(crate) fn review(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
) -> Result<Report> {
    let branch = git::branch(root).unwrap_or_default();
    review_for_branch(root, &branch, tier, runtime, task, plan)
}

/// Run the normal review engine while associating a detached review worktree
/// with its source branch for review-budget history.
pub(crate) fn review_for_branch(
    root: &Path,
    branch: &str,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
) -> Result<Report> {
    let config = Config::load(root)?;
    let model = config.model_for(tier, runtime, matches!(plan, Plan::Closing { .. }));
    let live = Live {
        runtime,
        root,
        model,
    };
    review_with_branch(root, branch, tier, runtime, task, plan, &live)
}
