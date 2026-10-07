//! When reviews do not converge (docs/review.md). Agents work without a
//! person, so the policy always ends on its own: up to three full reviews;
//! then, if the findings are converging, up to two closing reviews that
//! re-verify every blocking finding and audit only the latest fixes; then a
//! final fix verified by tests and gates. A branch whose findings are not
//! converging is split into smaller pull requests. Never another full review
//! hoping for a better grade.
use super::{
    Report,
    protocol::{Finding, Status, cap},
};

/// Full reviews a branch may fail before the closing reviews.
pub(crate) const BUDGET: usize = 3;
/// Closing reviews a converging branch may fail before its final fix.
pub(crate) const CLOSING_BUDGET: usize = 2;
/// Incomplete reviews (some session never answered) in a row before
/// `make ship` stops and asks to retry later.
pub(crate) const UNANSWERED: usize = 3;
/// How long "reviewers unavailable" lasts before `make ship` tries again.
pub(crate) const UNAVAILABLE_SECONDS: u64 = 3600;
/// Marks findings carried from earlier reviews: every reviewer votes on them.
pub(crate) const CARRIED: usize = usize::MAX;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Plan {
    Full,
    /// Verify `prior` at HEAD and audit `since..HEAD` (the fixes).
    Closing {
        since: String,
        prior: Vec<Finding>,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum Next {
    Review(Plan),
    /// Commit the fixes first: a reviewed commit is never reviewed again.
    FixFirst(String),
    /// The findings are not converging: split the change into smaller PRs.
    Split(String),
    /// The review budget is spent on a converging branch: the last closing
    /// review's findings, all in the last fixes, are fixed with tests and
    /// ship without another model review.
    FinalFix(Box<Report>),
    /// Reviewers keep failing to answer (outage, quota): retry later. An
    /// incomplete review checked nothing, so it never counts toward the budget.
    Unavailable(String),
}

fn blocking(report: &Report) -> Vec<&Finding> {
    report.blocking()
}

/// Critical or a major test weakening: the findings that cap a grade at 4.
fn severe(report: &Report) -> bool {
    blocking(report)
        .iter()
        .any(|f| cap(std::slice::from_ref(*f)) <= 4)
}

/// Findings carried into a closing review that are still there: fixes that failed.
fn still_open(report: &Report) -> usize {
    blocking(report)
        .iter()
        .filter(|f| f.reporter == CARRIED)
        .count()
}

/// What `make ship` does next on a branch, from its stored reviews (oldest
/// first) and the commit about to ship.
pub(crate) fn next(history: &[Report], head: &str, now: u64) -> Next {
    // Reviews before the branch's last passing one are settled.
    let start = history
        .iter()
        .rposition(Report::passes)
        .map_or(0, |i| i + 1);
    let tail = &history[start..];
    // Recent incomplete reviews only: an outage passes, and then reviews run again.
    let unanswered = tail
        .iter()
        .rev()
        .take_while(|r| !r.complete() && now.saturating_sub(r.finished) < UNAVAILABLE_SECONDS)
        .count();
    if unanswered >= UNANSWERED {
        return Next::Unavailable(format!(
            "the last {unanswered} reviews were incomplete (sessions gave no answer); the reviewers are unavailable, so ship again in an hour"
        ));
    }
    // An incomplete review checked nothing: it neither counts nor blocks a re-run.
    let failing: Vec<&Report> = tail.iter().filter(|r| r.complete()).collect();
    // Reviewing a commit again cannot change it: only the grade's noise.
    if let Some(failed) = failing.iter().find(|r| r.head == head) {
        return Next::FixFirst(format!(
            "{} already failed a review; fix and commit the confirmed findings, then ship again",
            short(&failed.head)
        ));
    }
    let full: Vec<&Report> = failing.iter().copied().filter(|r| !r.closing).collect();
    let closing: Vec<&Report> = failing.iter().copied().filter(|r| r.closing).collect();
    if full.len() < BUDGET {
        return Next::Review(Plan::Full);
    }
    let (previous, last) = (full[full.len() - 2], full[full.len() - 1]);
    if severe(last) || blocking(last).len() > blocking(previous).len() {
        return Next::Split(format!(
            "blocking findings went from {} to {}{} over the last two full reviews",
            blocking(previous).len(),
            blocking(last).len(),
            if severe(last) {
                ", including a critical one or a major test weakening"
            } else {
                ""
            }
        ));
    }
    if let Some(bad) = closing.iter().find(|r| severe(r) || still_open(r) > 0) {
        return Next::Split(format!(
            "the closing review of {} found {} earlier finding(s) still open{}",
            short(&bad.head),
            still_open(bad),
            if severe(bad) {
                " or a critical one"
            } else {
                ""
            }
        ));
    }
    if closing.len() >= CLOSING_BUDGET {
        return Next::FinalFix(Box::new(closing[closing.len() - 1].clone()));
    }
    let mut prior: Vec<Finding> = Vec::new();
    for finding in failing.iter().flat_map(|r| blocking(r)) {
        let same = |f: &Finding| {
            f.reported.file == finding.reported.file && f.reported.claim == finding.reported.claim
        };
        if !prior.iter().any(same) {
            let mut carried = finding.clone();
            carried.id = format!("P{}", prior.len() + 1);
            carried.reporter = CARRIED;
            carried.round = 0;
            carried.votes.clear();
            carried.status = Status::Disputed;
            prior.push(carried);
        }
    }
    Next::Review(Plan::Closing {
        since: failing[failing.len() - 1].head.clone(),
        prior,
    })
}

/// The record of a final fix: the last closing review, carried to the fixing
/// commit. It passes on tests and gates; no model reviewed the fix itself.
pub(crate) fn final_fix(reviewed: &Report, head: &str, now: u64) -> Report {
    let mut report = reviewed.clone();
    report.head = head.to_owned();
    // It comes after the review it closes, in the branch's history.
    report.started = now;
    report.finished = now;
    report.closing = false;
    report.final_fix = true;
    report.failures.clear();
    report.summary = format!(
        "Final fix after {BUDGET} full and {CLOSING_BUDGET} closing reviews: the {} finding(s) the last closing review ({}) confirmed in the previous fixes are fixed here, verified by tests and gates; no model reviewed this commit. A post-merge review issue follows.",
        reviewed.blocking().len(),
        short(&reviewed.head)
    );
    report
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}
