use super::{Commands, closing, floor, history, now};
use super::{Config, Live, Plan, Report, Result, Tier, config::Model, review_with_branch, trusted};
use crate::{agents::Runtime, ship::git};
use std::{
    fs,
    path::{Path, PathBuf},
};

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
    review_judged(root, branch, judge.as_deref(), tier, runtime, task, plan)
}

/// The review of a throwaway repository of its own (`make review-calibrate`):
/// the judge's commit lives in the real checkout, so its policy is that
/// repository's origin/dev.
pub(super) fn review_isolated(
    root: &Path,
    tier: Tier,
    runtime: Runtime,
    task: &str,
) -> Result<Report> {
    let branch = git::branch(root).unwrap_or_default();
    review_judged(root, &branch, None, tier, runtime, task, &Plan::Full)
}

fn review_judged(
    root: &Path,
    branch: &str,
    judge: Option<&str>,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    plan: &Plan,
) -> Result<Report> {
    let policy = Policy::pin(root, judge, tier, runtime, plan)?;
    let live = Live {
        runtime,
        root,
        model: policy.model.clone(),
        strong: policy.config.arbiter_model(runtime),
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

pub(crate) fn execute(command: Commands) -> Result<()> {
    let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    match command {
        Commands::Review {
            tier,
            runtime,
            task,
        } => {
            let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
            let requested_base = env("SHIP_BASE").unwrap_or_else(|| super::base::DEV.into());
            crate::ship::stack::refuse_base(&requested_base, &git::branch(&root)?)?;
            let _scope = if requested_base != super::base::DEV
                && crate::ship::stack::local_commit(&root, &requested_base).is_ok()
            {
                super::base::scope_local(&requested_base)
            } else {
                super::base::scope(&requested_base)
            };
            let tier = match (tier, env("REVIEW_TIER")) {
                (Some(tier), _) => tier,
                (None, Some(name)) => clap::ValueEnum::from_str(&name, true).map_err(|_| {
                    format!("REVIEW_TIER={name}: use low, medium, high, xhigh or max")
                })?,
                (None, None) => floor(&root)?,
            };
            let runtime = match (runtime, env("REVIEW_RUNTIME")) {
                (Some(runtime), _) => runtime,
                (None, Some(name)) => clap::ValueEnum::from_str(&name, true)
                    .map_err(|_| format!("REVIEW_RUNTIME={name}: use claude, codex, dsh or pi"))?,
                (None, None) => Runtime::Claude,
            };
            let task = task.or_else(|| env("REVIEW_TASK").map(PathBuf::from));
            let task = match task {
                Some(path) => fs::read_to_string(path)?,
                None => String::new(),
            };
            if requested_base != super::base::DEV {
                let report = review_on_base(&root, &requested_base, tier, runtime, &task, &review)?;
                println!("{}", report.markdown());
                return Ok(());
            }
            // `make review` spends the same budget as `make ship`.
            let head = git::git(&root, &["rev-parse", "HEAD"])?;
            let plan = match closing::next(&history(&root, &git::branch(&root)?)?, &head, now()) {
                closing::Next::Review(plan) => plan,
                closing::Next::FixFirst(why)
                | closing::Next::Split(why)
                | closing::Next::Unavailable(why) => return Err(why.into()),
            };
            let report = review(&root, tier, runtime, &task, &plan)?;
            println!("{}", report.markdown());
            if report.passes() {
                Ok(())
            } else {
                Err(if report.failures.is_empty() {
                    format!(
                        "review grade {}/10 is below {}",
                        report.grade, report.merge_grade
                    )
                } else {
                    format!(
                        "review incomplete: {} session(s) gave no usable answer",
                        report.failures.len()
                    )
                }
                .into())
            }
        }
        Commands::ReviewCalibrate => Ok(super::calibrate::from_env(&root)?),
        Commands::ReviewFloor => {
            println!("{}", floor(&root)?.name());
            Ok(())
        }
    }
}

/// Review-only entry for a stacked base. Local parents do not need a PR;
/// gates always run before a stored approval is consulted, and nothing pushes.
pub(crate) fn review_on_base(
    root: &Path,
    base: &str,
    tier: Tier,
    runtime: Runtime,
    task: &str,
    reviewer: &crate::ship::review_gate::Reviewer<'_>,
) -> Result<Report> {
    use crate::ship::{Options, evidence::Verdict, judge, review_gate, stack};
    stack::refuse_base(base, &git::branch(root)?)?;
    let _selected = if base != super::base::DEV && stack::local_commit(root, base).is_ok() {
        super::base::scope_local(base)
    } else {
        super::base::scope(base)
    };
    let (commit, _) = git::base(root, base, true)?;
    let _anchor = super::base::scope_at(base, &commit);
    let options = Options {
        base: base.into(),
        no_fetch: true,
        tier: Some(tier),
        runtime,
        ..Options::default()
    };
    let evidence = judge(root, &options)?;
    if evidence.verdict != Verdict::Pass {
        return Err("review: gates did not pass; no review or publication".into());
    }
    // REVIEW_TASK is already read: keep its exact bytes without a temporary file.
    let report = review_gate::require_task(root, &evidence, &options, task, reviewer)?;
    Ok(report)
}
