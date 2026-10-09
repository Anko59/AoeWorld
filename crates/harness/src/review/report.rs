//! The review report: stored as JSON next to the ship evidence
//! (`<git common dir>/aoe-ship/reviews/<sha>-<tier>.json`, where agents cannot
//! write) and rendered as Markdown for the pull request.
use super::protocol::{Finding, Status, blocking};
use crate::ship::git;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::{fs, path::Path, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    pub(crate) version: u16,
    pub(crate) head: String,
    pub(crate) branch: String,
    pub(crate) base: String,
    pub(crate) merge_base: String,
    pub(crate) tier: String,
    pub(crate) floor: String,
    pub(crate) runtime: String,
    pub(crate) model: String,
    pub(crate) effort: String,
    pub(crate) personas: Vec<String>,
    pub(crate) rounds: u32,
    pub(crate) findings: Vec<Finding>,
    /// What the grader wrote, before the confirmed findings capped it.
    pub(crate) written_grade: u8,
    pub(crate) grade: u8,
    pub(crate) summary: String,
    /// Reviewer or grader sessions that gave no usable answer.
    pub(crate) failures: Vec<String>,
    pub(crate) merge_grade: u8,
    pub(crate) started: u64,
    pub(crate) finished: u64,
    /// A closing review (docs/review.md): it passes on "no blocking finding
    /// left", and its grade is reported, not gated.
    #[serde(default)]
    pub(crate) closing: bool,
    /// SHA-256 of the raw Git file identity diff relative to origin/dev.
    /// Reuse always recomputes this value from Git; it is informational only.
    #[serde(default)]
    #[serde(alias = "patch_id")]
    pub(crate) change_fingerprint: Option<String>,
    /// SHA-256 of the trusted reviewer policy used for this review.
    #[serde(default)]
    pub(crate) policy_fingerprint: Option<String>,
    /// Original commit whose review this report reuses, if any.
    #[serde(default)]
    pub(crate) reused_from: Option<String>,
    /// SHA-256 of the task the reviewers were given (the `SHIP_BODY` text, or
    /// the commit log when it is empty): a review of one description never
    /// answers for another.
    #[serde(default)]
    pub(crate) task_fingerprint: Option<String>,
}

fn hex(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn directory(root: &Path) -> Result<PathBuf, String> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/reviews"))
}

impl Report {
    /// Fingerprint the task text exactly as the reviewers receive it.
    pub(crate) fn task_fingerprint(task: &str) -> String {
        hex(sha2::Sha256::digest(task.as_bytes()))
    }

    /// The task the reviewers of `head` receive: `task`, or the branch's
    /// commit messages since origin/dev when it is empty.
    pub(crate) fn effective_task(root: &Path, head: &str, task: &str) -> Result<String, String> {
        if !task.trim().is_empty() {
            return Ok(task.to_owned());
        }
        let merge_base = git::git(root, &["merge-base", "refs/remotes/origin/dev", head])?;
        git::git(
            root,
            &["log", "--format=%B", &format!("{merge_base}..{head}")],
        )
    }

    /// Fingerprint the review engine tracked by `origin/dev` for one tier.
    #[cfg(test)]
    pub(crate) fn policy_fingerprint(root: &Path, tier: &str) -> Result<String, String> {
        let commit = git::git(
            root,
            &["rev-parse", "--verify", "refs/remotes/origin/dev^{commit}"],
        )?;
        Self::policy_fingerprint_at(root, &commit, tier)
    }

    /// Fingerprint the review engine at one origin/dev commit for one tier.
    pub(crate) fn policy_fingerprint_at(
        root: &Path,
        commit: &str,
        tier: &str,
    ) -> Result<String, String> {
        let mut hash = sha2::Sha256::new();
        for path in [
            "gates/review.json",
            "gates/registry.json",
            "gates/review",
            "crates/harness/src/review",
        ] {
            let object = git::git(root, &["rev-parse", &format!("{commit}:{path}")])?;
            hash.update((object.len() as u64).to_be_bytes());
            hash.update(object.as_bytes());
        }
        hash.update((tier.len() as u64).to_be_bytes());
        hash.update(tier.as_bytes());
        Ok(hex(hash.finalize()))
    }

    /// A review passes only when every session answered and the capped grade
    /// reaches the merge grade; a closing review when no finding blocks and no
    /// carried finding is left undecided (one confirmed only as minor passes).
    pub(crate) fn passes(&self) -> bool {
        self.failures.is_empty()
            && if self.closing {
                // No finding may still block, and no carried finding may be
                // left undecided: a carried finding confirmed only as minor
                // (partial votes) no longer blocks, as in a full review.
                !self.findings.iter().any(|f| {
                    blocking(f)
                        || (f.reporter == super::closing::CARRIED && f.status == Status::Disputed)
                })
            } else {
                self.grade >= self.merge_grade
            }
    }

    /// Every session answered: the review counts toward the branch's budget.
    pub(crate) fn complete(&self) -> bool {
        self.failures.is_empty()
    }

    /// The confirmed findings that block a merge.
    pub(crate) fn blocking(&self) -> Vec<&Finding> {
        self.findings.iter().filter(|f| blocking(f)).collect()
    }

    fn count(&self, status: Status) -> usize {
        self.findings.iter().filter(|f| f.status == status).count()
    }

    /// Every review is kept: a new attempt never replaces an earlier one.
    pub(crate) fn store(&self, root: &Path) -> Result<PathBuf, String> {
        let directory = directory(root)?;
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let kind = if self.closing { "-closing" } else { "" };
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        for attempt in 1.. {
            let path = directory.join(format!("{}-{}{kind}-{attempt}.json", self.head, self.tier));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    std::io::Write::write_all(&mut file, &bytes).map_err(|e| e.to_string())?;
                    return Ok(path);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(format!("{}: {e}", path.display())),
            }
        }
        unreachable!("attempt numbers are unbounded")
    }

    /// The passing review of `head`, if one is stored (any of `tiers`) for the
    /// task the reviewers would receive now: a report without a task
    /// fingerprint, or of another description, is not reused.
    pub(crate) fn load_passing(
        root: &Path,
        head: &str,
        tiers: &[&str],
        task: &str,
    ) -> Result<Option<Self>, String> {
        let wanted = Self::task_fingerprint(&Self::effective_task(root, head, task)?);
        let Ok(entries) = fs::read_dir(directory(root)?) else {
            return Ok(None);
        };
        Ok(entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{head}-"))
            })
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .find(|r| {
                r.head == head
                    && tiers.contains(&r.tier.as_str())
                    && r.task_fingerprint.as_deref() == Some(wanted.as_str())
                    && r.passes()
            }))
    }

    /// Whether a complete report already exists for this head at an eligible
    /// tier, regardless of whether it passed.
    pub(crate) fn has_complete(root: &Path, head: &str, tiers: &[&str]) -> Result<bool, String> {
        let Ok(entries) = fs::read_dir(directory(root)?) else {
            return Ok(false);
        };
        Ok(entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{head}-"))
            })
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .any(|report| {
                report.head == head && tiers.contains(&report.tier.as_str()) && report.complete()
            }))
    }

    /// Copy an original passing review onto `head` when Git confirms the same
    /// change (identical file blobs) against the current origin/dev and the
    /// reviewers were given the same `task`. The stored change fingerprint is
    /// never consulted, and a reuse report can never become a source. By
    /// design only a review recorded for this same branch name is eligible,
    /// even when another branch holds an identical change: another branch's
    /// review (closing or full) never answers this branch's findings. One
    /// policy commit is pinned first: the floor, the eligible tiers, the merge
    /// grade and the policy fingerprint all come from it, and it must be
    /// origin/dev itself: a judge built from an older origin/dev never reuses.
    /// Any complete report for this head at an eligible tier blocks reuse,
    /// including a failing one.
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
        let base = git::git(root, &["rev-parse", "refs/remotes/origin/dev"])?;
        if policy != base {
            return Ok(None);
        }
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
        let (merge_base, wanted_identity) = git::change_identity(root, "dev", head)?;
        let wanted_fingerprint = git::change_fingerprint(root, "dev", head)?.1;
        let Ok(entries) = fs::read_dir(directory(root)?) else {
            return Ok(None);
        };
        let mut candidates: Vec<Self> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|report| {
                report.head != head
                    && !branch.is_empty()
                    && report.branch == branch
                    && report.head.len() == 40
                    && report.head.bytes().all(|byte| byte.is_ascii_hexdigit())
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
            let source_head = source.head.clone();
            let Ok((_, identity)) = git::change_identity(root, "dev", &source.head) else {
                continue;
            };
            if identity != wanted_identity {
                continue;
            }
            source.head = head.to_owned();
            source.branch = branch.to_owned();
            source.base = base.clone();
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

    /// The emoji badge for the grade.
    pub(crate) fn badge(&self) -> &'static str {
        if self.closing {
            return if self.passes() { "🟡" } else { "🔴" };
        }
        match self.grade {
            9..=10 => "🟢",
            8 => "🟡",
            _ => "🔴",
        }
    }

    pub(crate) fn headline(&self) -> String {
        format!(
            "{} **{}: {}/10** ({} tier · {} reviewer{} · {} round{} · {} {}): {} confirmed · {} disputed · {} refuted",
            self.badge(),
            if self.closing {
                "Closing review"
            } else {
                "Review"
            },
            self.grade,
            self.tier,
            self.personas.len(),
            if self.personas.len() == 1 { "" } else { "s" },
            self.rounds,
            if self.rounds == 1 { "" } else { "s" },
            self.model,
            self.effort,
            self.count(Status::Confirmed),
            self.count(Status::Disputed),
            self.count(Status::Refuted),
        )
    }

    pub(crate) fn markdown(&self) -> String {
        let mut out = format!(
            "{}\n\n> {}\n\n",
            self.headline(),
            self.summary.trim().replace('\n', "\n> ")
        );
        if let Some(source) = &self.reused_from {
            out.push_str(&format!(
                "Review of `{}` reused (same change, identical file blobs); CI re-runs every gate on this commit.\n\n",
                &source[..12.min(source.len())]
            ));
        }
        if self.closing {
            out.push_str(&format!(
                "Closing review after three failed reviews: it {} on \"no blocking finding left\"; the grade is reported, not gated (docs/review.md).\n\n",
                if self.passes() { "passed" } else { "failed" }
            ));
        }
        if self.written_grade != self.grade {
            out.push_str(&format!(
                "The grader wrote {}/10; confirmed and disputed findings cap it at {}/10.\n\n",
                self.written_grade, self.grade
            ));
        }
        for status in [Status::Confirmed, Status::Disputed, Status::Refuted] {
            let findings: Vec<&Finding> = self
                .findings
                .iter()
                .filter(|f| f.status == status)
                .collect();
            if findings.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "<details><summary>{:?} findings ({})</summary>\n\n",
                status,
                findings.len()
            ));
            for f in findings {
                let r = &f.reported;
                out.push_str(&format!(
                    "- **{}** `{:?}` `{}{}` — {}\n  - trigger: {}\n  - expected vs actual: {}\n  - votes: {}\n",
                    f.id,
                    r.severity,
                    r.file,
                    r.line.map(|l| format!(":{l}")).unwrap_or_default(),
                    r.claim,
                    r.trigger,
                    r.expected_vs_actual,
                    f.votes.iter().map(|v| format!("{:?}", v.vote)).collect::<Vec<_>>().join(", ")
                ));
            }
            out.push_str("\n</details>\n\n");
        }
        if !self.failures.is_empty() {
            out.push_str(&format!(
                "⚠️ Incomplete review: {}\n",
                self.failures.join("; ")
            ));
        }
        out
    }
}

/// Every stored review of `branch`, oldest first.
pub(crate) fn history(root: &Path, branch: &str) -> Result<Vec<Report>, String> {
    let Ok(entries) = fs::read_dir(directory(root)?) else {
        return Ok(vec![]);
    };
    let mut reports: Vec<Report> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<Report>(&bytes).ok())
        .filter(|report| report.branch == branch)
        .collect();
    reports.sort_by_key(|r| (r.finished, r.started));
    Ok(reports)
}
