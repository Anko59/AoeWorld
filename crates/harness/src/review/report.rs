//! The review report: stored as JSON next to the ship evidence
//! (`<git common dir>/aoe-ship/reviews/<sha>-<tier>.json`, where agents cannot
//! write) and rendered as Markdown for the pull request.
use super::protocol::{Finding, Status};
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
    /// reaches the merge grade.
    pub(crate) fn passes(&self) -> bool {
        self.failures.is_empty() && self.grade >= self.merge_grade
    }

    fn count(&self, status: Status) -> usize {
        self.findings.iter().filter(|f| f.status == status).count()
    }

    /// Every review is kept: a new attempt never replaces an earlier one.
    pub(crate) fn store(&self, root: &Path) -> Result<PathBuf, String> {
        let directory = directory(root)?;
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        for attempt in 1.. {
            let path = directory.join(format!("{}-{}-{attempt}.json", self.head, self.tier));
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
        match self.grade {
            9..=10 => "🟢",
            8 => "🟡",
            _ => "🔴",
        }
    }

    pub(crate) fn headline(&self) -> String {
        format!(
            "{} **Review: {}/10** ({} tier · {} reviewer{} · {} round{} · {} {}): {} confirmed · {} disputed · {} refuted",
            self.badge(),
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
