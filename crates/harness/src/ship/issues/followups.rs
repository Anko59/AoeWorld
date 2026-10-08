//! A passing review's leftovers become issues (docs/issues.md): one issue per
//! confirmed or disputed critical or major finding, and one checklist per pull
//! request for the minor and nit ones. Refuted findings are not leftovers.
//! Every field comes from reviewer output, so it is treated as untrusted text.
use super::{AGENT_LABEL, BODY_LIMIT, TITLE_LIMIT, Target, fingerprint, marker_line};
use crate::review::{
    Report,
    protocol::{Finding, Severity, Status},
};
use std::{collections::HashSet, path::Path};

const FOLLOW_UP_LABEL: &str = "review-follow-up";
const FIELD_LIMIT: usize = 8_000;
const LINE_LIMIT: usize = 300;

/// One issue to file: deduplicated by its title, commented with `note` when
/// it is already open.
#[derive(Debug)]
pub(super) struct Planned {
    pub(super) title: String,
    pub(super) labels: Vec<String>,
    pub(super) body: String,
    pub(super) note: String,
}

/// File the leftovers of the passing `report` of pull request `url`. Returns
/// what could not be filed: the review already passed, so the ship goes on.
pub(in crate::ship) fn file(root: &Path, url: &str, report: &Report) -> Vec<String> {
    let Some(pr) = pr_number(url) else {
        return vec![format!("no pull request number in {url}")];
    };
    let planned = plan(pr, report);
    if planned.is_empty() {
        return Vec::new();
    }
    let target = match Target::load(root) {
        Ok(target) => target,
        Err(error) => return vec![format!("could not list open issues: {error}")],
    };
    let mut problems = Vec::new();
    for item in planned {
        match target.file(root, &item.title, &item.labels, &item.body, &item.note) {
            Ok(outcome) => eprintln!("ship: review follow-up {outcome}"),
            Err(error) => problems.push(format!("{}: {error}", item.title)),
        }
    }
    problems
}

pub(super) fn pr_number(url: &str) -> Option<u64> {
    let (rest, number) = url.trim_end_matches('/').rsplit_once('/')?;
    let number = number.parse().ok().filter(|n| *n > 0)?;
    rest.ends_with("/pull").then_some(number)
}

pub(super) fn plan(pr: u64, report: &Report) -> Vec<Planned> {
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
    let mut planned = Vec::new();
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
        planned.push(Planned {
            title,
            labels: labels(priority),
            body,
            note: note.clone(),
        });
    }
    if !small.is_empty() {
        let title = format!("Review follow-ups for #{pr}");
        let body = checklist(pr, head, &small, &title);
        // A later review may leave new lines: the comment carries the list.
        let note = format!("{note}\n\n{body}");
        planned.push(Planned {
            title,
            labels: labels("priority:low"),
            body,
            note,
        });
    }
    planned
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

fn checklist(pr: u64, head: &str, findings: &[&Finding], title: &str) -> String {
    let mut body = format!(
        "Minor and nit findings the passing review of #{pr} at {head} left. Tick each one off when it is fixed.\n\n"
    );
    let reserve = 80;
    let budget = body_budget(title).saturating_sub(reserve);
    let mut seen = HashSet::new();
    let mut omitted = 0;
    for finding in findings {
        let line = format!(
            "- [ ] {} — {} ({})\n",
            location(finding),
            clip(&inline(&finding.reported.claim), LINE_LIMIT),
            severity(finding)
        );
        if !seen.insert(line.clone()) {
            continue;
        }
        if body.chars().count() + line.chars().count() > budget {
            omitted += 1;
        } else {
            body.push_str(&line);
        }
    }
    if omitted > 0 {
        body.push_str(&format!(
            "- {omitted} more finding(s) are in the review on #{pr}.\n"
        ));
    }
    body
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
