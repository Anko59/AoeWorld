//! When a captured view is settled, decided from the page's samples of
//! `#scene` (`data-chunks-pending`, `data-chunks-failed`, `data-frame`), as
//! agreed in issue #360: nothing pending and nothing failed, and the frame
//! unchanged for 1.5 s of polling. A quiet view that was already drawn settles
//! after 1.5 s; a frame or pending change restarts the window. `capture.mjs`
//! polls with the same rule to know when to stop; this decision is the one the
//! run reports.
use serde::Deserialize;

/// How long the frame must stay unchanged, with nothing pending, to settle.
pub(crate) const QUIET_MS: i64 = 1500;

/// One poll of the page. `at` is the poll's time in epoch milliseconds. A
/// missing attribute is -1 (never settled).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Sample {
    pub(crate) at: i64,
    pub(crate) pending: i64,
    pub(crate) failed: i64,
    pub(crate) frame: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// Nothing pending or failed, and the frame stayed at `frame` for `quiet_ms`.
    Settled {
        frame: i64,
        quiet_ms: i64,
    },
    /// Some sample counted failed chunks; the count is the latest one.
    Failed {
        failed: i64,
    },
    /// Still pending, or the frame moved within the last 1.5 s.
    Unsettled {
        pending: i64,
        failed: i64,
    },
    NoSamples,
}

/// The verdict over a view's history, oldest sample first.
pub(crate) fn decide(history: &[Sample]) -> Verdict {
    let Some(last) = history.last() else {
        return Verdict::NoSamples;
    };
    if let Some(sample) = history.iter().rev().find(|s| s.failed > 0) {
        return Verdict::Failed {
            failed: sample.failed,
        };
    }
    if last.pending != 0 {
        return Verdict::Unsettled {
            pending: last.pending,
            failed: last.failed,
        };
    }
    // The window opens at the first sample since the frame last changed or
    // something was pending: the frame has been `last.frame` since then.
    let window_start = history
        .iter()
        .rposition(|s| s.frame != last.frame || s.pending != 0)
        .map_or(0, |index| index + 1);
    let quiet_ms = last.at - history[window_start].at;
    if quiet_ms >= QUIET_MS {
        Verdict::Settled {
            frame: last.frame,
            quiet_ms,
        }
    } else {
        Verdict::Unsettled {
            pending: last.pending,
            failed: last.failed,
        }
    }
}
