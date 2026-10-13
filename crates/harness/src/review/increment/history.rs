//! The cumulative rule: small follow-ups add up (docs/review.md). Lines count
//! however they came to be approved, so a large change cannot get past a full
//! review in slices, even when a slice failed and its fixes passed.
use super::super::Report;

/// How a stored report counts towards `since_full`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Step {
    /// A passing full review, of the whole diff or of an increment.
    Full,
    /// A passing closing-style increment review of this many lines.
    Small(usize),
    /// A failed closing-style increment review of this many lines: approved
    /// only if a closing review of its fixes passes.
    FailedSmall(usize),
    /// A failed full review of an increment, settled the same way.
    FailedFull,
    /// A passing closing review of fixes: it approves what failed before it.
    Fixes,
    /// Anything else: changes nothing.
    Other,
}

pub(in crate::review) fn slice(report: &Report) -> Step {
    // A reused copy describes the approval it was copied from.
    if !report.complete() || report.reused_from.is_some() {
        return Step::Other;
    }
    match (&report.increment, report.passes()) {
        (Some(increment), true) if increment.closing => Step::Small(increment.lines),
        (Some(increment), false) if increment.closing => Step::FailedSmall(increment.lines),
        (Some(_), false) => Step::FailedFull,
        (None, true) if report.closing => Step::Fixes,
        (_, true) => Step::Full,
        (None, false) => Step::Other,
    }
}

/// What a failed increment review leaves to be approved by its fixes.
#[derive(Clone, Copy)]
enum Pending {
    Nothing,
    Lines(usize),
    Full,
}

/// Lines approved by closing-style increment reviews since the last full
/// review, walking the history forward: a full review resets the total, a
/// passing closing-style review adds its lines, a failed one is pending (the
/// latest wins) until a closing review of fixes passes and settles it: pending
/// lines are added, a pending full review resets the total.
pub(crate) fn since_full(history: impl Iterator<Item = Step>) -> usize {
    let (mut total, mut pending) = (0, Pending::Nothing);
    for step in history {
        match step {
            Step::Full => (total, pending) = (0, Pending::Nothing),
            Step::Small(lines) => (total, pending) = (total + lines, Pending::Nothing),
            Step::FailedSmall(lines) => pending = Pending::Lines(lines),
            Step::FailedFull => pending = Pending::Full,
            Step::Fixes => {
                match pending {
                    Pending::Nothing => {}
                    Pending::Lines(lines) => total += lines,
                    Pending::Full => total = 0,
                }
                pending = Pending::Nothing;
            }
            Step::Other => {}
        }
    }
    total
}
