//! The review report: stored as JSON next to the ship evidence
//! (`<git common dir>/aoe-ship/reviews/<sha>-<tier>.json`, where agents cannot
//! write) and rendered as Markdown for the pull request.
use super::protocol::{Finding, Status, blocking};
use crate::ship::git;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use std::{fs, path::Path, path::PathBuf, process::Command};

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
}

pub(crate) fn directory(root: &Path) -> Result<PathBuf, String> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/reviews"))
}

impl Report {
    /// Fingerprint the current trusted policy and prompts for one tier and
    /// runtime. JSON object keys are sorted by serde_json before hashing.
    pub(crate) fn policy_fingerprint(
        root: &Path,
        tier: &str,
        runtime: &str,
    ) -> Result<String, String> {
        let text = super::trusted(root, "gates/review.json")?;
        let config: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| format!("gates/review.json: {error}"))?;
        let tier_policy = config["tiers"][tier].clone();
        let models = config["models"][runtime].clone();
        if tier_policy.is_null() || models.is_null() {
            return Err(format!(
                "gates/review.json: missing {tier}/{runtime} policy"
            ));
        }
        let personas = tier_policy["personas"]
            .as_array()
            .ok_or_else(|| format!("gates/review.json: tier {tier} has no personas"))?;
        let canonical = serde_json::to_vec(&serde_json::json!({
            "tier": tier_policy,
            "runtime": runtime,
            "models": models,
        }))
        .map_err(|error| error.to_string())?;
        let mut hash = sha2::Sha256::new();
        hash.update((canonical.len() as u64).to_be_bytes());
        hash.update(canonical);
        for relative in std::iter::once("gates/review/preamble.md".to_owned())
            .chain(personas.iter().map(|persona| {
                format!(
                    "gates/review/personas/{}.md",
                    persona.as_str().unwrap_or_default()
                )
            }))
            .chain(std::iter::once("gates/review/grader.md".to_owned()))
        {
            let bytes = trusted_bytes(root, &relative)?;
            hash.update((relative.len() as u64).to_be_bytes());
            hash.update(relative.as_bytes());
            hash.update((bytes.len() as u64).to_be_bytes());
            hash.update(bytes);
        }
        Ok(hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
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

    /// The passing review of `head`, if one is stored (any of `tiers`).
    pub(crate) fn load_passing(
        root: &Path,
        head: &str,
        tiers: &[&str],
    ) -> Result<Option<Self>, String> {
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
            .find(|r| r.head == head && tiers.contains(&r.tier.as_str()) && r.passes()))
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
    /// change (identical file blobs) against the current origin/dev. Stored
    /// fingerprints are never consulted, and a reuse report can never become
    /// a source. Any complete report for this head at an eligible tier blocks
    /// reuse, including a failing report.
    pub(crate) fn reuse_for_change(
        root: &Path,
        head: &str,
        branch: &str,
        tiers: &[&str],
    ) -> Result<Option<Self>, String> {
        if Self::has_complete(root, head, tiers)? {
            return Ok(None);
        }
        let config = super::config::Config::load(root)?;
        let Some(floor) = tiers.first() else {
            return Ok(None);
        };
        let (merge_base, wanted_identity) = git::change_identity(root, "dev", head)?;
        let wanted_fingerprint = git::change_fingerprint(root, "dev", head)?.1;
        let base = git::git(root, &["rev-parse", "refs/remotes/origin/dev"])?;
        let Ok(entries) = fs::read_dir(directory(root)?) else {
            return Ok(None);
        };
        let mut candidates: Vec<Self> = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| fs::read(entry.path()).ok())
            .filter_map(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .filter(|report| {
                report.head != head
                    && report.head.len() == 40
                    && report.head.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && report.reused_from.is_none()
                    && tiers.contains(&report.tier.as_str())
                    && report.complete()
                    && report.passes()
                    && (report.closing || report.grade >= config.merge_grade)
            })
            .collect();
        candidates.sort_by_key(|report| std::cmp::Reverse(report.finished));
        for mut source in candidates {
            let Ok(current_policy) = Self::policy_fingerprint(root, &source.tier, &source.runtime)
            else {
                continue;
            };
            let configured_personas =
                tier_from_name(&source.tier).and_then(|tier| config.tiers.get(&tier));
            if source.policy_fingerprint.as_deref() != Some(current_policy.as_str())
                || configured_personas.is_none_or(|tier| tier.personas != source.personas)
            {
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
            source.floor = (*floor).to_owned();
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

fn trusted_bytes(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("refs/remotes/origin/dev:{relative}")])
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        fs::read(root.join(relative)).map_err(|error| format!("{relative}: {error}"))
    }
}

fn tier_from_name(name: &str) -> Option<super::config::Tier> {
    use super::config::Tier;
    [Tier::Low, Tier::Medium, Tier::High, Tier::Xhigh, Tier::Max]
        .into_iter()
        .find(|tier| tier.name() == name)
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
