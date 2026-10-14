//! When a captured view is settled, decided from the page's samples of
//! `#scene` (`data-chunks-pending`, `data-chunks-failed`, `data-frame`), as
//! agreed in issue #360: nothing pending, and the frame unchanged for a window
//! of polling. The window is at least 1.5 s, and four times the last interval
//! between frame changes when a software frame is slower than that. A quiet
//! view that was already drawn settles when the window closes; a frame or
//! pending change restarts it. Chunks that failed and then loaded on retry are
//! not failures: a view fails only if chunks still count as failed when the
//! window closes. `capture.mjs` polls with the same rule to know when to stop;
//! this decision is the one the run reports.
use serde::Deserialize;

/// The shortest the frame must stay unchanged, with nothing pending, to settle.
pub(crate) const QUIET_MS: i64 = 1500;
/// The window is at least this many measured intervals between frame changes.
pub(crate) const FRAME_INTERVALS: i64 = 4;

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
    /// Nothing pending, and the frame stayed at `frame` for `quiet_ms`, which
    /// is at least the `window_ms` the rule asks for.
    Settled {
        frame: i64,
        quiet_ms: i64,
        window_ms: i64,
    },
    /// Chunks still count as failed when the window closes; the count is the
    /// latest one.
    Failed {
        failed: i64,
    },
    /// Still pending, or the frame moved within the window.
    Unsettled {
        pending: i64,
        failed: i64,
    },
    NoSamples,
}

/// How long the frame must stay unchanged to settle: `QUIET_MS`, or
/// `FRAME_INTERVALS` times the last interval between two frame changes in the
/// history when that is longer.
pub(crate) fn window_ms(history: &[Sample]) -> i64 {
    let changes: Vec<i64> = history
        .windows(2)
        .filter(|pair| pair[0].frame != pair[1].frame)
        .map(|pair| pair[1].at)
        .collect();
    let interval = changes
        .windows(2)
        .last()
        .map_or(0, |pair| pair[1] - pair[0]);
    QUIET_MS.max(FRAME_INTERVALS * interval)
}

/// The chunk failures that loaded again: the drops in `data-chunks-failed`
/// between polls. A failed chunk that left the view without loading counts too.
pub(crate) fn recovered_failures(history: &[Sample]) -> u64 {
    history
        .windows(2)
        .filter(|pair| pair[0].failed >= 0 && pair[1].failed >= 0)
        .map(|pair| (pair[0].failed - pair[1].failed).max(0))
        .sum::<i64>()
        .try_into()
        .unwrap_or(0)
}

/// The verdict over a view's history, oldest sample first.
pub(crate) fn decide(history: &[Sample]) -> Verdict {
    let Some(last) = history.last() else {
        return Verdict::NoSamples;
    };
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
    let window_ms = window_ms(history);
    if quiet_ms < window_ms {
        return Verdict::Unsettled {
            pending: last.pending,
            failed: last.failed,
        };
    }
    if last.failed > 0 {
        return Verdict::Failed {
            failed: last.failed,
        };
    }
    Verdict::Settled {
        frame: last.frame,
        quiet_ms,
        window_ms,
    }
}
