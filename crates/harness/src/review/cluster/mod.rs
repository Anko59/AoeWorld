//! One finding per defect. Reviewers with different angles routinely report
//! the same defect in other words; left alone, a report lists it four or five
//! times, and each copy is graded, filed and counted. After every round the
//! findings that describe the same defect are folded into the earliest one:
//! same file, lines within `LINE_SPAN` of each other, and similar claims. The
//! cluster counts at the highest severity of its members, later votes apply to
//! it, and every member stays on it, as reported. Pure text comparison: no
//! model is asked.
use super::protocol::{Finding, Member, Reported};
use std::collections::BTreeSet;

/// Findings further apart than this many lines are different defects.
const LINE_SPAN: u32 = 3;
/// Claims sharing this share of the shorter claim's words are the same defect...
const SAME_WORDS: f64 = 0.5;
/// ...or this share, when they also name the same identifier (a variable,
/// function or issue number) that is rarely a coincidence.
const SAME_WORDS_AND_NAME: f64 = 0.3;

const COMMON: &[&str] = &[
    "the", "and", "for", "not", "that", "this", "with", "from", "but", "its", "when", "which",
    "does", "than", "then", "into", "can", "any", "has", "have", "will", "would", "also", "only",
    "still", "even", "each", "both", "are", "was", "were", "never", "all", "one",
];

/// Fold the findings that describe one defect into the earliest of them.
pub(crate) fn merge(findings: &mut Vec<Finding>) {
    let mut i = 0;
    while i < findings.len() {
        let mut j = i + 1;
        while j < findings.len() {
            if same_defect(&findings[i], &findings[j]) {
                let other = findings.remove(j);
                absorb(&mut findings[i], other);
                // The cluster grew: an earlier skipped finding may now match.
                j = i + 1;
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

fn absorb(cluster: &mut Finding, other: Finding) {
    cluster.reported.severity = cluster.reported.severity.min(other.reported.severity);
    cluster.members.push(Member {
        id: other.id,
        reporter: other.reporter,
        round: other.round,
        reported: other.reported,
    });
    cluster.members.extend(other.members);
    cluster.votes.extend(other.votes);
}

/// Every text of the cluster: the finding itself and its members, as reported.
fn texts(finding: &Finding) -> impl Iterator<Item = &Reported> {
    std::iter::once(&finding.reported).chain(finding.members.iter().map(|m| &m.reported))
}

fn same_defect(a: &Finding, b: &Finding) -> bool {
    texts(a).any(|x| texts(b).any(|y| similar(x, y)))
}

fn similar(a: &Reported, b: &Reported) -> bool {
    if a.file != b.file {
        return false;
    }
    match (a.line, b.line) {
        (Some(x), Some(y)) if x.abs_diff(y) <= LINE_SPAN => {}
        _ => return false,
    }
    let (words_a, words_b) = (words(&a.claim), words(&b.claim));
    let shorter = words_a.len().min(words_b.len());
    if shorter == 0 {
        return false;
    }
    let shared = words_a.intersection(&words_b).count() as f64 / shorter as f64;
    let same_name = !names(&a.claim).is_disjoint(&names(&b.claim));
    shared >= SAME_WORDS || (same_name && shared >= SAME_WORDS_AND_NAME)
}

fn words(text: &str) -> BTreeSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3 && !COMMON.contains(w))
        .map(str::to_owned)
        .collect()
}

/// Identifier-like words: `AOE_E2E_WORKERS`, `v7`, anything with a digit or
/// an underscore.
fn names(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 4 && w.chars().any(|c| c == '_' || c.is_ascii_digit()))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests;
