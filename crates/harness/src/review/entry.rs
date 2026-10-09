use super::{Config, Live, Plan, Report, Result, Tier, config::Model, review_with_branch, trusted};
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
    let judge = std::env::var(trusted::JUDGE_REV).ok();
    let policy = Policy::pin(root, judge.as_deref(), tier, runtime, plan)?;
    let live = Live {
        runtime,
        root,
        model: policy.model.clone(),
    };
    review_with_branch(root, branch, tier, &policy, task, plan, &live)
}

/// The one policy commit a review runs under, pinned before anything reads
/// it: its config, the model chosen from it, its prompts (read while pinned)
/// and its fingerprint all come from `commit`. Dropping it unpins.
pub(super) struct Policy {
    _pin: trusted::Pin,
    pub(super) commit: String,
    pub(super) config: Config,
    pub(super) runtime: Runtime,
    pub(super) model: Model,
}

impl Policy {
    pub(super) fn pin(
        root: &Path,
        judge: Option<&str>,
        tier: Tier,
        runtime: Runtime,
        plan: &Plan,
    ) -> Result<Self> {
        let (pin, commit) = trusted::pin(root, judge)?;
        let config = Config::load(root)?;
        let model = config.model_for(tier, runtime, matches!(plan, Plan::Closing { .. }));
        Ok(Self {
            _pin: pin,
            commit,
            config,
            runtime,
            model,
        })
    }
}
