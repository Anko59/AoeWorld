use crate::ship::compare::settle::{Sample, Verdict, decide, recovered_failures, window_ms};

fn sample(at: i64, pending: i64, failed: i64, frame: i64) -> Sample {
    Sample {
        at,
        pending,
        failed,
        frame,
    }
}

#[test]
fn a_quiet_view_that_was_already_drawn_settles_after_one_and_a_half_seconds() {
    // Polled every 250 ms with the frame at 3 from the first poll on.
    let history: Vec<Sample> = (0..=6).map(|i| sample(i * 250, 0, 0, 3)).collect();
    assert_eq!(
        decide(&history),
        Verdict::Settled {
            frame: 3,
            quiet_ms: 1500,
            window_ms: 1500
        }
    );
}

#[test]
fn just_under_one_and_a_half_seconds_of_quiet_is_not_yet_settled() {
    let history = [sample(0, 0, 0, 3), sample(1250, 0, 0, 3)];
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 0,
            failed: 0
        }
    );
}

#[test]
fn a_view_still_loading_does_not_settle_while_chunks_are_pending() {
    // Pending 4 for 10 s with the frame unchanged: still loading.
    let history: Vec<Sample> = (0..=40).map(|i| sample(i * 250, 4, 0, 7)).collect();
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 4,
            failed: 0
        }
    );
}

#[test]
fn a_render_inside_the_window_restarts_it() {
    // Quiet at frame 5 for 1 s, then frame 6 lands: the window restarts there.
    let mut history = vec![
        sample(0, 0, 0, 5),
        sample(500, 0, 0, 5),
        sample(1000, 0, 0, 5),
    ];
    history.push(sample(1250, 0, 0, 6));
    history.push(sample(1500, 0, 0, 6));
    history.push(sample(1750, 0, 0, 6));
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 0,
            failed: 0
        }
    );
    history.push(sample(2750, 0, 0, 6));
    assert_eq!(
        decide(&history),
        Verdict::Settled {
            frame: 6,
            quiet_ms: 1500,
            window_ms: 1500
        }
    );
}

#[test]
fn pending_chunks_restart_the_window_even_when_the_frame_holds() {
    let mut history = vec![
        sample(0, 0, 0, 9),
        sample(1000, 0, 0, 9),
        sample(1500, 2, 0, 9),
    ];
    history.push(sample(1750, 0, 0, 9));
    history.push(sample(2500, 0, 0, 9));
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 0,
            failed: 0
        }
    );
    history.push(sample(3250, 0, 0, 9));
    assert_eq!(
        decide(&history),
        Verdict::Settled {
            frame: 9,
            quiet_ms: 1500,
            window_ms: 1500
        }
    );
}

#[test]
fn failed_chunks_still_failed_when_the_window_closes_fail_the_view_with_the_count() {
    // Failed 3 from 250 ms on, the frame quiet: the window closes at 1500 ms
    // with the chunks still failed.
    let history: Vec<Sample> = (0..=7)
        .map(|i| sample(i * 250, 0, if i == 0 { 0 } else { 3 }, 41))
        .collect();
    assert_eq!(decide(&history), Verdict::Failed { failed: 3 });
}

#[test]
fn failed_chunks_inside_the_open_window_are_not_yet_a_failure() {
    let history = [
        sample(0, 0, 0, 41),
        sample(250, 0, 3, 41),
        sample(500, 0, 3, 41),
    ];
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 0,
            failed: 3
        }
    );
}

#[test]
fn a_chunk_that_failed_and_loaded_on_retry_does_not_fail_the_view() {
    // The issue #360 history: 0, 0, 2, 0, 0 with the frame quiet throughout.
    let failed = [0, 0, 2, 0, 0, 0, 0];
    let history: Vec<Sample> = failed
        .iter()
        .enumerate()
        .map(|(i, &f)| sample(i as i64 * 250, 0, f, 3))
        .collect();
    assert_eq!(
        decide(&history),
        Verdict::Settled {
            frame: 3,
            quiet_ms: 1500,
            window_ms: 1500
        }
    );
    assert_eq!(recovered_failures(&history), 2);
}

#[test]
fn the_recovered_failures_are_the_drops_between_polls() {
    // Rises do not count: the drops 3 -> 1 and 1 -> 0 recovered 2 + 1 chunks.
    let failed = [0, 2, 3, 1, 0];
    let history: Vec<Sample> = failed
        .iter()
        .enumerate()
        .map(|(i, &f)| sample(i as i64 * 250, 0, f, 3))
        .collect();
    assert_eq!(recovered_failures(&history), 3);
    assert_eq!(recovered_failures(&[sample(0, -1, -1, -1)]), 0);
}

#[test]
fn a_slow_frame_stretches_the_window_to_four_frame_intervals() {
    // The frame changes at 500 ms and 1000 ms, then holds: the last interval is
    // 500 ms, so the window is 2000 ms, not 1500 ms.
    let frames = [1, 1, 2, 2, 3, 3, 3, 3, 3, 3, 3];
    let history: Vec<Sample> = frames
        .iter()
        .enumerate()
        .map(|(i, &f)| sample(i as i64 * 250, 0, 0, f))
        .collect();
    assert_eq!(window_ms(&history), 2000);
    // Quiet for 1500 ms at 2500 ms: enough for the 1.5 s floor, not for 2 s.
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 0,
            failed: 0
        }
    );
    let mut later = history.clone();
    later.push(sample(2750, 0, 0, 3));
    assert!(matches!(decide(&later), Verdict::Unsettled { .. }));
    later.push(sample(3000, 0, 0, 3));
    assert_eq!(
        decide(&later),
        Verdict::Settled {
            frame: 3,
            quiet_ms: 2000,
            window_ms: 2000
        }
    );
}

#[test]
fn a_fast_frame_keeps_the_window_at_one_and_a_half_seconds() {
    // Frames 50 ms apart: four intervals are 200 ms, far under the floor.
    let history: Vec<Sample> = (0..40).map(|i| sample(i * 50, 0, 0, i)).collect();
    assert_eq!(window_ms(&history), 1500);
    assert_eq!(window_ms(&[sample(0, 0, 0, 1)]), 1500);
}

#[test]
fn no_samples_and_unknown_attributes_are_never_settled() {
    assert_eq!(decide(&[]), Verdict::NoSamples);
    // A missing attribute reads as -1 in the page.
    assert_eq!(
        decide(&[sample(0, -1, -1, -1), sample(2000, -1, -1, -1)]),
        Verdict::Unsettled {
            pending: -1,
            failed: -1
        }
    );
}
