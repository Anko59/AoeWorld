//! The one `review:` line `make ship` and `make review` print before any
//! reviewer starts (docs/review.md): which review was chosen and why.
use crate::review::Plan;

/// The line for `plan`; `why` is the first reason a stored review could not
/// answer for this branch tip, and is only used for a full review.
pub(super) fn line(plan: &Plan, why: &str) -> String {
    match plan {
        Plan::Full => format!("review: full review — {why}"),
        Plan::Closing { since, prior } => format!(
            "review: closing review of the fixes since {} against {} carried finding(s)",
            &since[..12.min(since.len())],
            prior.len()
        ),
        Plan::Increment(increment) => format!("review: {}", increment.sentence()),
    }
}
