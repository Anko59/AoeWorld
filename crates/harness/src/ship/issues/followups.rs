//! A passing review's leftovers become issues (docs/issues.md): one issue per
//! confirmed or disputed critical or major finding. The minor and nit ones
//! are one section per pull request in the standing issue `Review follow-ups`
//! (opened once, found by its fingerprint marker), and the same list is posted
//! as a pull request comment. Refuted findings are not leftovers. Every field
//! comes from reviewer output, so it is treated as untrusted text.
use super::{
    AGENT_LABEL, BODY_LIMIT, MARKER, TITLE_LIMIT, Target, duplicate_index, fingerprint,
    marker_line, validate_text,
};
use crate::review::{
    Report,
    protocol::{Finding, Severity, Status},
};
use std::{collections::HashSet, path::Path};

const FOLLOW_UP_LABEL: &str = "review-follow-up";
pub(super) const STANDING_TITLE: &str = "Review follow-ups";
const FIELD_LIMIT: usize = 8_000;
const LINE_LIMIT: usize = 300;
/// What one pull request's section of the standing issue may hold.
const SECTION_LIMIT: usize = 8_000;
/// The sections kept in the standing issue; older ones fold into one line.
const MAX_SECTIONS: usize = 30;
const SECTION_HEADING: &str = "## PR #";
const FOLDED: &str = "> Folded ";
const INTRO: &str = "Minor and nit findings of passing reviews, one section per pull request (docs/issues.md#review-follow-ups). `make ship` adds each section; tick findings off as they are fixed.";

/// One issue to file: deduplicated by its title, commented with `note` when
/// it is already open.
#[derive(Debug)]
pub(super) struct Planned {
    pub(super) title: String,
    pub(super) labels: Vec<String>,
    pub(super) body: String,
    pub(super) note: String,
}

/// What a passing review leaves: an issue per serious finding, and the
/// minor and nit section of the pull request.
#[derive(Debug)]
pub(super) struct Plan {
    pub(super) issues: Vec<Planned>,
    pub(super) section: Option<Section>,
}

/// The minor and nit findings of one pull request, as its section.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Section {
    pub(super) pr: u64,
    pub(super) url: String,
    pub(super) head: String,
    /// `- [ ] …` lines, the last one maybe counting what did not fit.
    pub(super) items: Vec<String>,
}

impl Section {
    fn render(&self) -> String {
        let mut text = format!(
            "{SECTION_HEADING}{} — {}\n\nThe passing review at {} left these minor and nit findings:\n",
            self.pr,
            inline(&self.url),
            self.head
        );
        for item in &self.items {
            text.push_str(item);
            text.push('\n');
        }
        text.trim_end().to_owned()
    }
}

/// File the leftovers of the passing `report` of pull request `url`. Returns
/// what could not be filed: the review already passed, so the ship goes on.
pub(in crate::ship) fn file(root: &Path, url: &str, report: &Report) -> Vec<String> {
    let Some(pr) = pr_number(url) else {
        return vec![format!("no pull request number in {url}")];
    };
    let plan = plan(pr, url, report);
    // An empty current list still clears this PR's earlier section.
    let section = plan.section.unwrap_or_else(|| Section {
        pr,
        url: url.to_owned(),
        head: report.head[..12.min(report.head.len())].to_owned(),
        items: Vec::new(),
    });
    let target = match Target::load(root) {
        Ok(target) => target,
        Err(error) => return vec![format!("could not list open issues: {error}")],
    };
    let mut problems = Vec::new();
    for item in plan.issues {
        match target.file(root, &item.title, &item.labels, &item.body, &item.note) {
            Ok(outcome) => eprintln!("ship: review follow-up {outcome}"),
            Err(error) => problems.push(format!("{}: {error}", item.title)),
        }
    }
    // The comment is posted even when the standing issue cannot be updated.
    let recorded = match record(&target, root, &section) {
        Ok(recorded) => {
            if let Some(url) = &recorded {
                eprintln!("ship: review follow-ups recorded in {url}");
            }
            recorded
        }
        Err(error) => {
            problems.push(format!("{STANDING_TITLE}: {error}"));
            None
        }
    };
    if let Err(error) = comment(&target, root, &section, recorded.as_deref()) {
        problems.push(format!("comment on #{}: {error}", section.pr));
    }
    problems
}

pub(super) fn pr_number(url: &str) -> Option<u64> {
    let (rest, number) = url.trim_end_matches('/').rsplit_once('/')?;
    let number = number.parse().ok().filter(|n| *n > 0)?;
    rest.ends_with("/pull").then_some(number)
}

pub(super) fn plan(pr: u64, url: &str, report: &Report) -> Plan {
    let head = &report.head[..12.min(report.head.len())];
    let note = format!("Reported again by the passing review of #{pr} at {head}.");
    let leftovers = report
        .findings
        .iter()
        .filter(|finding| matches!(finding.status, Status::Confirmed | Status::Disputed));
    let (serious, small): (Vec<&Finding>, Vec<&Finding>) = leftovers.partition(|finding| {
        matches!(
            finding.reported.severity,
            Severity::Critical | Severity::Major
        )
    });
    let mut seen = HashSet::new();
    let mut issues = Vec::new();
    for finding in serious {
        let title = clip(
            &format!(
                "Review finding at {}: {}",
                location(finding),
                inline(&finding.reported.claim)
            ),
            TITLE_LIMIT,
        );
        if !seen.insert(fingerprint(&title)) {
            continue;
        }
        let priority = match finding.reported.severity {
            Severity::Critical => "priority:critical",
            _ => "priority:high",
        };
        let body = fit(&finding_body(pr, head, finding), &title);
        issues.push(Planned {
            title,
            labels: labels(priority),
            body,
            note: note.clone(),
        });
    }
    let section = (!small.is_empty()).then(|| Section {
        pr,
        url: url.to_owned(),
        head: head.to_owned(),
        items: items(pr, &small),
    });
    Plan { issues, section }
}

fn labels(priority: &str) -> Vec<String> {
    [AGENT_LABEL, FOLLOW_UP_LABEL, priority]
        .map(str::to_owned)
        .to_vec()
}

fn severity(finding: &Finding) -> &'static str {
    match finding.reported.severity {
        Severity::Critical => "critical",
        Severity::Major => "major",
        Severity::Minor => "minor",
        Severity::Nit => "nit",
    }
}

fn location(finding: &Finding) -> String {
    let file = clip(&inline(&finding.reported.file), 160);
    match finding.reported.line {
        Some(line) => format!("{file}:{line}"),
        None => file,
    }
}

fn finding_body(pr: u64, head: &str, finding: &Finding) -> String {
    let status = match finding.status {
        Status::Confirmed => "confirmed",
        _ => "disputed",
    };
    let mut body = format!(
        "The passing review of #{pr} at {head} left this {status} {} finding.\n\n- Location: {}\n- Category: {}\n- Claim: {}\n",
        severity(finding),
        location(finding),
        clip(&inline(&finding.reported.category), LINE_LIMIT),
        clip(&inline(&finding.reported.claim), FIELD_LIMIT),
    );
    for (heading, text) in [
        ("Trigger", &finding.reported.trigger),
        ("Expected vs actual", &finding.reported.expected_vs_actual),
        ("Evidence", &finding.reported.evidence),
    ] {
        if !text.trim().is_empty() {
            body.push_str(&format!(
                "\n**{heading}**\n\n{}\n",
                clip(&untrusted(text), FIELD_LIMIT)
            ));
        }
    }
    body.push_str("\nFiled by `make ship` (docs/issues.md).\n");
    body
}

/// The checklist lines of a section, within `SECTION_LIMIT`; the rest is
/// counted in one line that points at the review.
fn items(pr: u64, findings: &[&Finding]) -> Vec<String> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    let mut size = 0;
    let mut omitted = 0;
    for finding in findings {
        let line = format!(
            "- [ ] {} — {} ({})",
            location(finding),
            clip(&inline(&finding.reported.claim), LINE_LIMIT),
            severity(finding)
        );
        if !seen.insert(line.clone()) {
            continue;
        }
        let length = line.chars().count() + 1;
        if size + length > SECTION_LIMIT - 200 {
            omitted += 1;
        } else {
            size += length;
            items.push(line);
        }
    }
    if omitted > 0 {
        items.push(format!(
            "- {omitted} more finding(s) are in the review on #{pr}."
        ));
    }
    items
}

/// Put `section` into the standing issue, which is opened when it is
/// missing and there are findings. An empty section removes this PR's list.
/// Returns the issue's URL when one exists.
fn record(target: &Target, root: &Path, section: &Section) -> super::Result<Option<String>> {
    match duplicate_index(&target.issues, STANDING_TITLE) {
        Some(index) => {
            let issue = &target.issues[index];
            let number = issue.number.to_string();
            for _ in 0..3 {
                let current: Snapshot = serde_json::from_str(&target.gh(
                    root,
                    &["issue", "view", &number, "--json", "body,updatedAt"],
                    None,
                )?)?;
                let body = standing_body(&current.body, section);
                validate_text("ISSUE_BODY", &body, BODY_LIMIT)?;
                // GitHub has no compare-and-set for issue edits. Keep this
                // check immediately before the write to narrow the race.
                let updated = target.gh(
                    root,
                    &[
                        "issue",
                        "view",
                        &number,
                        "--json",
                        "updatedAt",
                        "--jq",
                        ".updatedAt",
                    ],
                    None,
                )?;
                if updated != current.updated_at {
                    continue;
                }
                target.gh(
                    root,
                    &["issue", "edit", &number, "--body-file", "-"],
                    Some(body.as_bytes()),
                )?;
                return Ok(Some(issue.url.clone()));
            }
            Err("standing issue was busy after 3 attempts; leaving it untouched".into())
        }
        None if section.items.is_empty() => Ok(None),
        None => {
            let body = standing_body("", section);
            let outcome = target.create(root, STANDING_TITLE, &labels("priority:low"), &body)?;
            Ok(Some(
                outcome
                    .strip_prefix("opened ")
                    .unwrap_or(outcome.as_str())
                    .to_owned(),
            ))
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    body: String,
    updated_at: String,
}

mod standing;

/// Splice this PR's section into the fresh body, then fold the oldest sections
/// past the count or size bound. All retained sections stay byte-identical.
pub(super) fn standing_body(current: &str, section: &Section) -> String {
    standing::update(current, section)
}

/// The pull request comment with the same list, pointing at the standing issue.
fn comment(
    target: &Target,
    root: &Path,
    section: &Section,
    recorded: Option<&str>,
) -> super::Result<()> {
    if section.items.is_empty() {
        let body = format!(
            "No open minor or nit findings in the passing review of PR #{} at {}.\n",
            section.pr, section.head
        );
        target.gh(
            root,
            &["pr", "comment", &section.pr.to_string(), "--body-file", "-"],
            Some(body.as_bytes()),
        )?;
        return Ok(());
    }
    let mut body = format!(
        "Minor and nit findings of the passing review at {}",
        section.head
    );
    if let Some(url) = recorded {
        body.push_str(&format!(", also recorded in {url}"));
    }
    body.push_str(":\n\n");
    for item in &section.items {
        body.push_str(item);
        body.push('\n');
    }
    target.gh(
        root,
        &["pr", "comment", &section.pr.to_string(), "--body-file", "-"],
        Some(body.as_bytes()),
    )?;
    Ok(())
}

/// What the body may hold so the marked body stays within the limit.
pub(super) fn body_budget(title: &str) -> usize {
    BODY_LIMIT - marker_line(title).chars().count()
}

pub(super) fn fit(body: &str, title: &str) -> String {
    clip(body, body_budget(title))
}

/// At most `limit` characters, ending in `…` when cut.
pub(super) fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut clipped: String = text.chars().take(limit.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

/// Reviewer text without control, bidirectional or zero-width characters or
/// `<`, so it can never carry an HTML comment (and with it the reserved
/// fingerprint marker) nor display differently from what it says.
pub(super) fn untrusted(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\t' => c.to_string(),
            '<' => "&lt;".into(),
            '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2069}'
            | '\u{FEFF}' => " ".into(),
            c if c.is_control() => " ".into(),
            c => c.to_string(),
        })
        .collect()
}

/// [`untrusted`] on one line, with whitespace collapsed.
pub(super) fn inline(text: &str) -> String {
    untrusted(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
#[path = "tests/followups.rs"]
mod tests;
