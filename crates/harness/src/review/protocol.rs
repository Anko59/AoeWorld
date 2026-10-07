//! The review protocol, without any model: finding and verdict schemas, the
//! status of each finding from the other reviewers' votes, when to stop, and
//! the caps confirmed findings put on the grade. Everything here is computed
//! in Rust; models only report findings, vote and write the grade text.
use serde::{Deserialize, Serialize};

pub(crate) const BEGIN: &str = "AOE-REVIEW-BEGIN";
pub(crate) const END: &str = "AOE-REVIEW-END";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Severity {
    Critical,
    Major,
    Minor,
    Nit,
}

/// One finding as a reviewer reports it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Reported {
    pub(crate) file: String,
    #[serde(default)]
    pub(crate) line: Option<u32>,
    pub(crate) severity: Severity,
    #[serde(default)]
    pub(crate) category: String,
    pub(crate) claim: String,
    #[serde(default)]
    pub(crate) trigger: String,
    #[serde(default)]
    pub(crate) expected_vs_actual: String,
    #[serde(default)]
    pub(crate) evidence: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Vote {
    Upheld,
    Partial,
    Refuted,
    Unverifiable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) id: String,
    pub(crate) verdict: Vote,
    #[serde(default)]
    pub(crate) evidence: String,
}

/// What a reviewer returns between the markers. Unknown keys are refused, so
/// a misspelt `verdicts` is a failed session rather than an empty vote.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Answer {
    #[serde(default)]
    pub(crate) findings: Vec<Reported>,
    #[serde(default)]
    pub(crate) verdicts: Vec<Verdict>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Grade {
    pub(crate) grade: u8,
    pub(crate) summary: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Status {
    Confirmed,
    Disputed,
    Refuted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Cast {
    pub(crate) reviewer: usize,
    pub(crate) round: u32,
    pub(crate) vote: Vote,
    pub(crate) evidence: String,
}

/// A finding with its reporter, votes and computed status.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Finding {
    pub(crate) id: String,
    pub(crate) reporter: usize,
    pub(crate) round: u32,
    #[serde(flatten)]
    pub(crate) reported: Reported,
    pub(crate) votes: Vec<Cast>,
    pub(crate) status: Status,
}

impl Finding {
    pub(crate) fn test_integrity(&self) -> bool {
        let category = self.reported.category.to_ascii_lowercase();
        category.contains("test") && category.contains("integrity")
    }
}

/// The JSON a reviewer printed between the markers, from any runtime's
/// output: plain text, or JSON lines whose string fields carry the message.
pub(crate) fn extract(output: &str) -> Option<String> {
    let mut texts = vec![output.to_owned()];
    for line in output.lines() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            collect_strings(&value, &mut texts);
        }
    }
    texts.iter().rev().find_map(|text| between_markers(text))
}

/// The body between a BEGIN and an END marker that parses as a JSON object,
/// preferring the last answer. A reviewer may quote the markers inside its
/// JSON strings (when reviewing this very code), so every pair is tried; the
/// first END after the last BEGIN is the fallback.
fn between_markers(text: &str) -> Option<String> {
    let clean = |body: &str| {
        body.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_owned()
    };
    let begins: Vec<usize> = text
        .match_indices(BEGIN)
        .map(|(i, _)| i + BEGIN.len())
        .collect();
    for &start in begins.iter().rev() {
        let ends: Vec<usize> = text[start..]
            .match_indices(END)
            .map(|(i, _)| start + i)
            .collect();
        for &end in ends.iter().rev() {
            let body = clean(&text[start..end]);
            if serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&body).is_ok() {
                return Some(body);
            }
        }
    }
    let start = *begins.last()?;
    let end = start + text[start..].find(END)?;
    Some(clean(&text[start..end]))
}

fn collect_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) if text.contains(BEGIN) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect_strings(v, out)),
        serde_json::Value::Object(map) => map.values().for_each(|v| collect_strings(v, out)),
        _ => {}
    }
}

/// Each other reviewer's latest vote on the finding.
fn latest(finding: &Finding) -> Vec<&Cast> {
    let mut latest: Vec<&Cast> = Vec::new();
    for cast in &finding.votes {
        if cast.reviewer == finding.reporter {
            continue;
        }
        match latest.iter_mut().find(|c| c.reviewer == cast.reviewer) {
            Some(slot) if slot.round <= cast.round => *slot = cast,
            Some(_) => {}
            None => latest.push(cast),
        }
    }
    latest
}

fn count(votes: &[&Cast], wanted: Vote) -> usize {
    votes.iter().filter(|c| c.vote == wanted).count()
}

/// With one reviewer nothing can be cross-checked: every finding stands.
/// Otherwise each other reviewer's latest vote counts once; `partial` upholds,
/// `unverifiable` abstains.
pub(crate) fn status(finding: &Finding, reviewers: usize) -> Status {
    if reviewers <= 1 {
        return Status::Confirmed;
    }
    let votes = latest(finding);
    let upheld = count(&votes, Vote::Upheld) + count(&votes, Vote::Partial);
    let refuted = count(&votes, Vote::Refuted);
    if upheld > refuted {
        Status::Confirmed
    } else if refuted > upheld {
        Status::Refuted
    } else {
        Status::Disputed
    }
}

/// `partial` means "real, but a different severity or scope": a finding that
/// full `upheld` votes alone do not confirm counts one severity lower.
fn downgraded(finding: &Finding) -> bool {
    let votes = latest(finding);
    !votes.is_empty() && count(&votes, Vote::Upheld) <= count(&votes, Vote::Refuted)
}

/// The severity a confirmed finding counts at, after the `partial` downgrade.
/// A disputed finding counts one severity lower too: a single dissenting (or
/// prompt-injected) reviewer cannot make a critical finding vanish.
fn effective(finding: &Finding) -> Severity {
    let lower = finding.status == Status::Disputed || downgraded(finding);
    match (finding.reported.severity, lower) {
        (severity, false) => severity,
        (Severity::Critical, true) => Severity::Major,
        (Severity::Major, true) => Severity::Minor,
        (_, true) => Severity::Nit,
    }
}

/// The highest grade the confirmed and disputed findings allow: critical caps at 4, major
/// at 7. A test-integrity finding (weakened or gamed tests) caps at 4 from
/// major up; a minor one, such as a small coverage gap, counts by severity.
pub(crate) fn cap(findings: &[Finding]) -> u8 {
    let standing = findings.iter().filter(|f| f.status != Status::Refuted);
    let mut cap = 10;
    for finding in standing {
        let limit = match effective(finding) {
            Severity::Critical => 4,
            Severity::Major if finding.test_integrity() => 4,
            Severity::Major => 7,
            Severity::Minor | Severity::Nit => 10,
        };
        cap = cap.min(limit);
    }
    cap
}

/// Converged: the round changed no status and added no finding.
pub(crate) fn converged(before: &[(String, Status)], after: &[Finding]) -> bool {
    before.len() == after.len()
        && after.iter().all(|f| {
            before
                .iter()
                .any(|(id, status)| id == &f.id && *status == f.status)
        })
}
