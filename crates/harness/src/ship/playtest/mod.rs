//! Advisory gameplay evidence for the exact gated revision.
use serde::Deserialize;
use std::{fs, path::Path, process::Command, time::Duration};

const GAMEPLAY: &[&str] = &[
    "crates/simulation/",
    "crates/server/",
    "crates/client/",
    "crates/sdk/",
    "sdk/",
    "ai/",
    "crates/protocol/",
];
// Changing this switch makes a missing or failed gameplay report block shipping.
const BLOCKING: bool = false;

#[derive(Deserialize)]
pub(super) struct Report {
    revision: String,
    pass: bool,
    failures: Vec<String>,
    ticks: Ticks,
    session_drops: u64,
    scenarios: Vec<Scenario>,
    recording: Option<String>,
}
#[derive(Deserialize)]
struct Ticks {
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}
#[derive(Deserialize)]
struct Scenario {
    name: String,
    pass: bool,
    detail: String,
}

pub(super) fn gameplay(path: &str) -> bool {
    GAMEPLAY.iter().any(|prefix| path.starts_with(prefix))
}
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace(['\r', '\n'], " ")
}
impl Report {
    pub(super) fn matches(&self, head: &str) -> bool {
        self.revision == head
    }
    fn parse(json: &str, head: &str) -> Option<Self> {
        let report: Self = serde_json::from_str(json).ok()?;
        (report.revision == head).then_some(report)
    }
    pub(super) fn summary(&self) -> String {
        format!(
            "playtest: {}/{} scenarios pass · tick p95 {} ms (advisory)",
            self.scenarios.iter().filter(|s| s.pass).count(),
            self.scenarios.len(),
            self.ticks.p95_ms
        )
    }
    pub(super) fn section(&self) -> String {
        let mut out = format!(
            "\n## Playtest (advisory)\n\n{}\n\n| Scenario | Pass | Detail |\n|---|---|---|\n",
            if self.pass { "PASS" } else { "FAIL" }
        );
        for s in &self.scenarios {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                cell(&s.name),
                if s.pass { "PASS" } else { "FAIL" },
                cell(&s.detail)
            ));
        }
        out.push_str(&format!(
            "\nTick p50 {} ms, p95 {} ms, max {} ms; session drops: {}.\n",
            self.ticks.p50_ms, self.ticks.p95_ms, self.ticks.max_ms, self.session_drops
        ));
        for line in self.failures.iter().flat_map(|f| f.lines()).take(10) {
            out.push_str(&format!("- {}\n", line));
        }
        if let Some(path) = &self.recording {
            out.push_str(&format!("\nRecording: {}\n", cell(path)));
        }
        out
    }
}

fn target_available(root: &Path, make: &str) -> bool {
    Command::new(make)
        .args(["-rRqp"])
        .current_dir(root)
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with("playtest:"))
        })
}

pub(super) fn announce(report: Option<&Report>, head: &str) {
    if let Some(report) = report.filter(|r| r.matches(head)) {
        eprintln!("{}", report.summary());
    }
}

pub(super) fn append(
    path: &Path,
    report: Option<&Report>,
    applicable: bool,
    head: &str,
) -> std::io::Result<()> {
    use std::io::Write;
    if applicable {
        let section = report.filter(|r| r.matches(head)).map_or_else(
            || "\n## Playtest (advisory)\n\nReport unavailable for this revision.\n".to_owned(),
            Report::section,
        );
        fs::OpenOptions::new()
            .append(true)
            .open(path)?
            .write_all(section.as_bytes())?;
    }
    Ok(())
}

pub(super) fn run(
    root: &Path,
    base: &str,
    head: &str,
    slots: &crate::slots::SlotEnv,
) -> Option<Report> {
    let changed = match super::git::changed_between(root, base, head) {
        Ok(paths) => paths.iter().any(|path| gameplay(path)),
        Err(error) => {
            eprintln!("playtest: could not select changed paths ({error})");
            return None;
        }
    };
    if !changed {
        return None;
    }
    let path = root.join("reports/playtest").join(format!("{head}.json"));
    if !path.exists() {
        let available = target_available(root, "make");
        if !available {
            eprintln!("playtest: not available on this revision");
            return None;
        }
        match crate::slots::guard::hold_forced(root, "playtest", slots) {
            Ok(slot) => {
                let env = slot.env();
                let env: Vec<_> = env
                    .iter()
                    .map(|(key, value)| (*key, value.as_str()))
                    .collect();
                if let Err(error) = crate::process::run_in(
                    root,
                    "make",
                    &["playtest"],
                    &env,
                    Duration::from_secs(30 * 60),
                ) {
                    eprintln!("playtest: {error} (advisory)");
                }
            }
            Err(error) => eprintln!("playtest: {error} (advisory)"),
        }
    }
    let report = fs::read_to_string(path)
        .ok()
        .and_then(|json| Report::parse(&json, head));
    if report.is_none() {
        eprintln!("playtest: report missing or invalid for this revision (advisory)");
    }
    report
}

pub(super) fn draft_advisory(
    root: &Path,
    evidence: &super::Evidence,
    body: Option<&str>,
    report: Option<&Report>,
    applicable: bool,
) {
    if let Some(body) = body.filter(|_| applicable)
        && let Err(error) = update_draft(root, evidence, body, report)
    {
        eprintln!("playtest: could not update draft ({error})");
    }
}

// A failed review leaves the early PR as a draft; keep its advisory evidence visible too.
pub(super) fn update_draft(
    root: &Path,
    evidence: &super::Evidence,
    body: &str,
    report: Option<&Report>,
) -> super::Result<()> {
    let repository = super::git::origin_repository(root)?;
    let Some(pr) = super::github::open_pull_request(root, &repository, &evidence.branch)? else {
        return Ok(());
    };
    let path = super::evidence::directory(root)?
        .with_file_name("bodies")
        .join(format!("{}-draft.md", evidence.head));
    fs::write(&path, body)?;
    append(&path, report, true, &evidence.head)?;
    super::github::gh(
        root,
        &[
            "pr",
            "edit",
            &pr.url,
            "--repo",
            &repository,
            "--body-file",
            &path.to_string_lossy(),
        ],
    )?;
    Ok(())
}

pub(super) fn enforce(report: Option<&Report>, applicable: bool) -> super::Result<()> {
    if BLOCKING && applicable && !report.is_some_and(|r| r.pass) {
        return Err("ship: playtest failed or unavailable".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
