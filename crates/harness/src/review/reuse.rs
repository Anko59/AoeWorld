//! Reuse of a passing review for the same patch (exact changed bytes)
//! after a rebase or a description edit, including across bases: a review of a stacked branch
//! against its parent answers for the same branch rebased onto dev after the
//! parent moves or merges, when changed hunks, paths and modes still match.
use super::{Report, base, report::directory};
use crate::ship::git;
use std::{fs, path::Path};

/// Why a stored review does or does not answer for the current branch tip.
pub(crate) enum Reuse {
    Reused(Box<Report>),
    /// The first failing condition, as a sentence for the ship log.
    Declined(String),
}

/// What a reused report records when the description text changed after the
/// approval of the identical patch.
const DESCRIPTION_CHANGED: &str = "description text changed after approval; code unchanged";

thread_local! {
    static ANNOUNCED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Remember the line `make ship` printed for the review about to run, so the
/// report that review stores carries it.
pub(crate) fn announce(line: &str) {
    ANNOUNCED.with(|slot| *slot.borrow_mut() = Some(line.to_owned()));
}

/// The announced line, once.
pub(crate) fn take_announced() -> Option<String> {
    ANNOUNCED.with(|slot| slot.borrow_mut().take())
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// "9/10 of 76994762ab12", or "closing review of 76994762ab12".
fn label(report: &Report) -> String {
    if report.closing {
        format!("closing review of {}", short(&report.head))
    } else {
        format!("{}/10 of {}", report.grade, short(&report.head))
    }
}

impl Report {
    /// Copy an original passing review onto `head` when Git confirms the same
    /// patch. The reviewers' task text need not match: the approval is of the
    /// code, so an identical patch under an unchanged policy stays approved
    /// when only the description changed (the report then records the new task
    /// fingerprint and a note). The change of `head` is taken against the
    /// current selected base; the source's against its recorded base commit
    /// (the parent branch may have moved or be gone once merged). The stored
    /// change fingerprint is never consulted, and a reuse report can never
    /// become a source. By design only a review recorded for this same branch
    /// name is eligible, even when another branch holds an identical change:
    /// another branch's review (closing or full) never answers this branch's
    /// findings. One policy commit is pinned first: the floor, the eligible
    /// tiers, the merge grade and the policy fingerprint all come from it, and
    /// it must be origin/dev itself: a judge built from an older origin/dev
    /// never reuses. Any complete report for this head at an eligible tier
    /// against the current base is the only source for that head: a passing one
    /// stands whatever the description, a failing one blocks reuse.
    pub(crate) fn reuse_decision(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
        task: &str,
    ) -> Result<Reuse, String> {
        let judge = std::env::var(super::trusted::JUDGE_REV).ok();
        Self::reuse_decision_judged(root, head, branch, tiers, task, judge.as_deref())
    }

    /// `reuse_decision` as an option, for tests.
    #[cfg(test)]
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
    #[cfg(test)]
    pub(crate) fn reuse_for_change_judged(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
        task: &str,
        judge: Option<&str>,
    ) -> Result<Option<Self>, String> {
        Ok(
            match Self::reuse_decision_judged(root, head, branch, tiers, task, judge)? {
                Reuse::Reused(report) => Some(*report),
                Reuse::Declined(_) => None,
            },
        )
    }

    /// `reuse_decision` for the judge built from `judge` (origin/dev when
    /// `None`).
    pub(crate) fn reuse_decision_judged(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
        task: &str,
        judge: Option<&str>,
    ) -> Result<Reuse, String> {
        let declined = |why: &str| Ok(Reuse::Declined(why.to_owned()));
        let (_pin, policy) = super::trusted::pin(root, judge)?;
        let dev = git::git(root, &["rev-parse", "refs/remotes/origin/dev"])?;
        if policy != dev {
            return declined("the reviewer policy is pinned to an older dev than origin/dev");
        }
        let target = base::current();
        let (target_commit, _) = git::base(root, &target, false)?;
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
        if tiers.is_empty() {
            return declined(&format!(
                "no eligible tier is at or above the floor ({})",
                floor.name()
            ));
        }
        let config = super::config::Config::load(root)?;
        let wanted_task = Self::task_fingerprint(&Self::effective_task(root, head, task)?);
        let (merge_base, wanted_identity) = git::change_identity(root, &target, head)?;
        let wanted_fingerprint = git::change_fingerprint(root, &target, head)?.1;
        let mut unreadable = 0;
        let mut candidates: Vec<Self> = Self::complete_for(root, head, &tiers)?;
        if candidates.is_empty() {
            for entry in fs::read_dir(directory(root)?)
                .into_iter()
                .flatten()
                .flatten()
            {
                let Some(report) = fs::read(entry.path())
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
                else {
                    unreadable += 1;
                    continue;
                };
                if report.head != head
                    && !branch.is_empty()
                    && report.branch == branch
                    && full_id(&report.head)
                    && report.reused_from.is_none()
                    && report.complete()
                    && report.passes()
                    && (report.closing || report.grade >= config.merge_grade)
                {
                    candidates.push(report);
                }
            }
        } else {
            // This exact commit was reviewed before: its passing review stands
            // whatever the description (also after an earlier reuse); one that
            // failed means a review.
            // Only this branch's own review, and one that records the task
            // its reviewers saw (a report without it is never reused).
            candidates.retain(|report| {
                !branch.is_empty()
                    && report.branch == branch
                    && report.task_fingerprint.is_some()
                    && report.passes()
                    && (report.closing || report.grade >= config.merge_grade)
            });
            if candidates.is_empty() {
                return declined("this exact commit's review did not pass");
            }
        }
        candidates.sort_by_key(|report| std::cmp::Reverse(report.finished));
        let mut first = None;
        for mut source in candidates {
            if let Err(why) = source.answers(root, &policy, &tiers, &wanted_identity) {
                first.get_or_insert(why);
                continue;
            }
            let task_changed = source.task_fingerprint.as_deref() != Some(wanted_task.as_str());
            let source_head = source
                .reused_from
                .clone()
                .unwrap_or_else(|| std::mem::replace(&mut source.head, head.to_owned()));
            source.head = head.to_owned();
            source.branch = branch.to_owned();
            source.base = target_commit.clone();
            source.base_branch = target.clone();
            source.merge_base = merge_base.clone();
            source.floor = floor.name().to_owned();
            source.merge_grade = config.merge_grade;
            source.change_fingerprint = Some(wanted_fingerprint.clone());
            source.task_fingerprint = Some(wanted_task.clone());
            source.note = task_changed.then(|| DESCRIPTION_CHANGED.to_owned());
            source.decision = Some(format!(
                "review: reusing {} (patch and policy unchanged{})",
                label(&Self {
                    head: source_head.clone(),
                    ..source.clone()
                }),
                if task_changed {
                    "; description text changed"
                } else {
                    ""
                }
            ));
            source.reused_from = Some(source_head);
            source.finished = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| duration.as_secs());
            source.store(root)?;
            return Ok(Reuse::Reused(Box::new(source)));
        }
        Ok(Reuse::Declined(first.unwrap_or_else(|| {
            if unreadable > 0 {
                format!("{unreadable} stored review report(s) are unreadable and no readable passing review of this branch exists")
            } else {
                "no passing review of this branch".to_owned()
            }
        })))
    }

    /// Whether this passing review answers for the patch `wanted`, or the
    /// first condition that fails: the patch, the tier, the review policy.
    /// The task text is deliberately not one of them: it is only compared
    /// after the patch is known to be identical.
    fn answers(
        &self,
        root: &Path,
        policy: &str,
        tiers: &[&str],
        wanted: &[u8],
    ) -> Result<(), String> {
        let identity = source_identity(root, self)
            .map_err(|_| format!("the base commit of the {} is missing", label(self)))?;
        if identity != wanted {
            return Err(format!("the patch differs from the {}", label(self)));
        }
        if !tiers.contains(&self.tier.as_str()) {
            return Err(format!(
                "the {} is a {} review and this ship needs at least {}",
                label(self),
                self.tier,
                tiers.first().copied().unwrap_or("the floor")
            ));
        }
        let current = Self::policy_fingerprint_at(root, policy, &self.tier)?;
        if self.policy_fingerprint.as_deref() != Some(current.as_str()) {
            return Err(format!(
                "the review policy on dev changed since the {}",
                label(self)
            ));
        }
        Ok(())
    }
}

fn full_id(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Always use the recorded base, including for dev: a moved base must not
/// change the source's original patch. Missing objects decline reuse.
pub(super) fn source_identity(root: &Path, source: &Report) -> Result<Vec<u8>, String> {
    if !full_id(&source.base) {
        return Err("review records no base commit".into());
    }
    git::change_identity_from(root, &source.base, &source.head).map(|(_, patch)| patch)
}
