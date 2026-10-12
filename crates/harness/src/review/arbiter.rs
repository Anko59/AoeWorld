//! The arbiter (docs/review.md): at tier high and above, one fresh session on
//! the runtime's strong model rules `upheld` or `refuted` on every major or
//! critical finding the cross-examination left disputed. Its ruling is
//! recorded as a vote of the reserved reviewer `ARBITER` and sets the status.
//! No usable answer leaves a finding disputed, as before.
use super::{
    prompt::{Subject, cut, field_limit, fit, read},
    protocol::{ARBITER, BEGIN, Cast, END, Finding, Severity, Status, Vote},
    session::{Ask, Strong, ask_checked},
};
use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Ruled {
    Upheld,
    Refuted,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ruling {
    id: String,
    ruling: Ruled,
    #[serde(default)]
    evidence: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rulings {
    rulings: Vec<Ruling>,
}

pub(super) fn readable(text: &str) -> Result<(), String> {
    serde_json::from_str::<Rulings>(text)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The findings the arbiter rules on: disputed and major or critical.
fn pending(findings: &[Finding]) -> Vec<&Finding> {
    findings
        .iter()
        .filter(|f| f.status == Status::Disputed && f.reported.severity <= Severity::Major)
        .collect()
}

fn prompt(root: &Path, subject: &Subject, pending: &[&Finding]) -> Result<String, String> {
    let limit = field_limit(pending.len() * 8);
    let mut listing = String::new();
    for finding in pending {
        let r = &finding.reported;
        let votes: String = finding
            .votes
            .iter()
            .map(|v| {
                format!(
                    "  - a reviewer voted `{:?}`: {}\n",
                    v.vote,
                    cut(&v.evidence, limit)
                )
            })
            .collect();
        listing.push_str(&format!(
            "### {} ({:?})\n- where: {}{}\n- claim: {}\n- trigger: {}\n- expected vs actual: {}\n- evidence: {}\n- votes:\n{}\n",
            finding.id,
            r.severity,
            cut(&r.file, limit),
            r.line.map(|l| format!(":{l}")).unwrap_or_default(),
            cut(&r.claim, limit),
            cut(&r.trigger, limit),
            cut(&r.expected_vs_actual, limit),
            cut(&r.evidence, limit),
            if votes.is_empty() { "  - none\n" } else { &votes }
        ));
    }
    let preamble = read(root, "gates/review/preamble.md")?;
    let arbiter = read(root, "gates/review/arbiter.md")?;
    fit(subject, |text| {
        format!(
            "{preamble}\n\n{arbiter}\n\n{text}\n## Disputed findings\n\n{listing}## Your answer\n\nEnd your reply with exactly one JSON object between the markers, with one ruling per finding:\n\n{BEGIN}\n{{\"rulings\": [{{\"id\": \"<finding id>\", \"ruling\": \"<upheld|refuted>\", \"evidence\": \"<file:line and why>\"}}, <one per finding>]}}\n{END}\n"
        )
    })
}

/// Set the status of each finding the arbiter ruled on. Findings it skipped,
/// or that were not disputed, are left as they are.
fn apply(findings: &mut [Finding], rulings: Vec<Ruling>, round: u32) {
    for ruling in rulings {
        let Some(finding) = findings
            .iter_mut()
            .find(|f| f.id == ruling.id && f.status == Status::Disputed)
            .filter(|f| f.reported.severity <= Severity::Major)
        else {
            continue;
        };
        let (vote, status) = match ruling.ruling {
            Ruled::Upheld => (Vote::Upheld, Status::Confirmed),
            Ruled::Refuted => (Vote::Refuted, Status::Refuted),
        };
        finding.votes.push(Cast {
            reviewer: ARBITER,
            round,
            vote,
            evidence: ruling.evidence,
        });
        finding.status = status;
    }
}

/// Ask the arbiter about every still-disputed major or critical finding. A
/// session that gives no usable answer is not a failed review: the findings
/// stay disputed. `round` numbers its rulings after the last vote.
pub(super) fn rule(
    root: &Path,
    subject: &Subject,
    findings: &mut [Finding],
    round: u32,
    ask: &dyn Ask,
) -> Result<(), String> {
    let pending = pending(findings);
    if pending.is_empty() {
        return Ok(());
    }
    let prompt = prompt(root, subject, &pending)?;
    let answer = ask_checked(&Strong(ask), vec![prompt], readable)
        .pop()
        .and_then(Result::ok)
        .and_then(|text| serde_json::from_str::<Rulings>(&text).ok());
    if let Some(answer) = answer {
        apply(findings, answer.rulings, round);
    }
    Ok(())
}
