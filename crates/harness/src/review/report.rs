//! The review report: stored as JSON next to the ship evidence
//! (`<git common dir>/aoe-ship/reviews/<sha>-<tier>.json`, where agents cannot
//! write) and rendered as Markdown for the pull request.
use super::protocol::{Finding, Status, blocking};
use crate::ship::git;
use serde::{Deserialize, Serialize};
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
}

pub(crate) fn directory(root: &Path) -> Result<PathBuf, String> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/reviews"))
}

impl Report {
    /// A review passes only when every session answered and the capped grade
    /// reaches the merge grade; a closing review when no confirmed finding
    /// blocks and every carried finding was shown fixed.
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
