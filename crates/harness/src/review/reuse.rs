//! Reuse of a passing review for the same change (identical file blobs)
//! after a rebase, including across bases: a review of a stacked branch
//! against its parent answers for the same branch rebased onto dev after the
//! parent merged, when Git's raw identity diff is byte-identical.
use super::{Report, base, report::directory};
use crate::ship::git;
use std::{fs, path::Path};

impl Report {
    /// Copy an original passing review onto `head` when Git confirms the same
    /// change (identical file blobs) and the reviewers were given the same
    /// `task`. The change of `head` is taken against the current
    /// origin/<base>; the source's against origin/dev for a review of dev, or
    /// against the parent commit it recorded for a stacked review (the parent
    /// branch may be gone once merged). The stored change fingerprint is
    /// never consulted, and a reuse report can never become a source. By
    /// design only a review recorded for this same branch name is eligible,
    /// even when another branch holds an identical change: another branch's
    /// review (closing or full) never answers this branch's findings. One
    /// policy commit is pinned first: the floor, the eligible tiers, the merge
    /// grade and the policy fingerprint all come from it, and it must be
    /// origin/dev itself: a judge built from an older origin/dev never reuses.
    /// Any complete report for this head at an eligible tier against the
    /// current base blocks reuse, including a failing one.
    pub(crate) fn reuse_for_change(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
        task: &str,
    ) -> Result<Option<Self>, String> {
        let judge = std::env::var(super::trusted::JUDGE_REV).ok();
        Self::reuse_for_change_judged(root, head, branch, tiers, task, judge.as_deref())
    }

    /// `reuse_for_change` for the judge built from `judge` (origin/dev when
    /// `None`).
    pub(crate) fn reuse_for_change_judged(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
        task: &str,
        judge: Option<&str>,
    ) -> Result<Option<Self>, String> {
        let (_pin, policy) = super::trusted::pin(root, judge)?;
        let dev = git::git(root, &["rev-parse", "refs/remotes/origin/dev"])?;
        if policy != dev {
            return Ok(None);
        }
        let target = base::current();
        let target_commit = git::git(
            root,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("refs/remotes/origin/{target}^{{commit}}"),
            ],
        )?;
        let floor = super::floor(root).map_err(|e| e.to_string())?;
        let tiers: Vec<&str> = tiers
            .iter()
            .copied()
            .filter(|name| {
                <super::Tier as clap::ValueEnum>::value_variants()
                    .iter()
                    .any(|tier| tier.name() == *name && *tier >= floor)
            })
            .collect();
        if tiers.is_empty() || Self::has_complete(root, head, &tiers)? {
            return Ok(None);
        }
        let config = super::config::Config::load(root)?;
        let wanted_task = Self::task_fingerprint(&Self::effective_task(root, head, task)?);
        let (merge_base, wanted_identity) = git::change_identity(root, &target, head)?;
        let wanted_fingerprint = git::change_fingerprint(root, &target, head)?.1;
        let Ok(entries) = fs::read_dir(directory(root)?) else {
            return Ok(None);
        };
        let mut candidates: Vec<Self> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|report| {
                // The same commit against the same base was answered above.
                (report.head != head || !report.against_current_base())
                    && !branch.is_empty()
                    && report.branch == branch
                    && full_id(&report.head)
                    && report.reused_from.is_none()
                    && tiers.contains(&report.tier.as_str())
                    && report.task_fingerprint.as_deref() == Some(wanted_task.as_str())
                    && report.complete()
                    && report.passes()
                    && (report.closing || report.grade >= config.merge_grade)
            })
            .collect();
        candidates.sort_by_key(|report| std::cmp::Reverse(report.finished));
        for mut source in candidates {
            let Ok(current_policy) = Self::policy_fingerprint_at(root, &policy, &source.tier)
            else {
                continue;
            };
            if source.policy_fingerprint.as_deref() != Some(current_policy.as_str()) {
                continue;
            }
            let Ok(identity) = source_identity(root, &source) else {
                continue;
            };
            if identity != wanted_identity {
                continue;
            }
            let source_head = std::mem::replace(&mut source.head, head.to_owned());
            source.branch = branch.to_owned();
            source.base = target_commit;
            source.base_branch = Some(target);
            source.merge_base = merge_base;
            source.floor = floor.name().to_owned();
            source.merge_grade = config.merge_grade;
            source.change_fingerprint = Some(wanted_fingerprint);
            source.reused_from = Some(source_head);
            source.finished = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs());
            source.store(root)?;
            return Ok(Some(source));
        }
        Ok(None)
    }
}

fn full_id(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The source's raw identity diff: against the current origin/dev for a
/// review of dev (as before stacking existed), otherwise against the parent
/// commit the stacked review recorded, whose merge base Git recomputes.
fn source_identity(root: &Path, source: &Report) -> Result<Vec<u8>, String> {
    match base::of(source.base_branch.as_deref()) {
        base::DEV => git::change_identity(root, base::DEV, &source.head).map(|(_, raw)| raw),
        _ if full_id(&source.base) => {
            git::change_identity_from(root, &source.base, &source.head).map(|(_, raw)| raw)
        }
        other => Err(format!("stacked review on {other} records no base commit")),
    }
}
