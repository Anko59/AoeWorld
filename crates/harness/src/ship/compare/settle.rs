//! When a captured view is settled, decided from the page's samples of
//! `#scene` (`data-chunks-pending`, `data-chunks-failed`, `data-frame`), as
//! agreed in issue #360: nothing pending, and the frame unchanged for a window
//! of polling. The window is 1.5 s, or four times the median frame time when
//! software frames are slower, capped at 4 s. A frame time is an interval
//! between two frame changes no longer than 1 s: a longer gap is idle time
//! (waiting for chunks, or a late redraw), not the time a frame takes. A quiet
//! view that was already drawn settles when the window closes; a frame or
//! pending change restarts it. A chunk that failed and loaded on retry is not a
//! failure. The page counts a failed chunk as pending too, so a view fails only
//! when every chunk still missing has failed (`failed == pending`) for 12 s,
//! two of the client's 5 s retry cycles. `capture.mjs` polls with the same
//! rules to know when to stop; this decision is the one the run reports.
use serde::Deserialize;

/// The shortest the frame must stay unchanged, with nothing pending, to settle.
pub(crate) const QUIET_MS: i64 = 1500;
/// The longest the window may be.
pub(crate) const QUIET_MAX_MS: i64 = 4000;
/// The window is this many median frame times.
pub(crate) const FRAME_INTERVALS: i64 = 4;
/// The longest gap between two frame changes that counts as frame time; a
/// longer gap is idle.
pub(crate) const IDLE_MS: i64 = 1000;
/// The window's median is taken over at most this many recent frame times.
pub(crate) const FRAME_SAMPLES: usize = 5;
/// How long every chunk still missing must have failed to fail the view: two
/// of the client's 5 s retry cycles.
pub(crate) const FAILED_MS: i64 = 12_000;

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
    /// Every chunk still missing has failed for `FAILED_MS`; the count is the
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

/// A sample where every chunk still missing has failed.
fn failing(sample: &Sample) -> bool {
    sample.failed > 0 && sample.failed == sample.pending
}

/// How long the frame must stay unchanged to settle: `QUIET_MS`, or
/// `FRAME_INTERVALS` times the median of the last `FRAME_SAMPLES` frame times
/// (intervals between two frame changes of at most `IDLE_MS`), capped at
/// `QUIET_MAX_MS`.
pub(crate) fn window_ms(history: &[Sample]) -> i64 {
    let mut frame_times = Vec::new();
    let mut previous_change = None;
    for pair in history.windows(2) {
        if pair[0].frame == pair[1].frame {
            continue;
        }
        if let Some(previous) = previous_change {
            let gap = pair[1].at - previous;
            if gap <= IDLE_MS {
                frame_times.push(gap);
            }
        }
        previous_change = Some(pair[1].at);
    }
    let mut recent = frame_times[frame_times.len().saturating_sub(FRAME_SAMPLES)..].to_vec();
    if recent.is_empty() {
        return QUIET_MS;
    }
    recent.sort_unstable();
    let middle = recent.len() / 2;
    let median = if recent.len() % 2 == 1 {
        recent[middle]
    } else {
        (recent[middle - 1] + recent[middle]) / 2
    };
    (FRAME_INTERVALS * median).clamp(QUIET_MS, QUIET_MAX_MS)
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
    // The failing run opens at the first sample since a sample that was not
    // failing: every chunk still missing has been failing since then.
    if failing(last) {
        let run_start = history
            .iter()
            .rposition(|s| !failing(s))
            .map_or(0, |index| index + 1);
        if last.at - history[run_start].at >= FAILED_MS {
            return Verdict::Failed {
                failed: last.failed,
            };
        }
    }
    if last.pending != 0 || last.failed != 0 || last.frame <= 0 {
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
    Verdict::Settled {
        frame: last.frame,
        quiet_ms,
        window_ms,
    }
}
