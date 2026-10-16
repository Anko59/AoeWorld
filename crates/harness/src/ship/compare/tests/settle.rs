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
fn a_permanently_failing_chunk_fails_the_view_at_twelve_seconds_with_the_count() {
    // The client counts a failed chunk as pending too: pending 2, failed 2 from
    // 250 ms on, the frame quiet. The run starts at 250 ms, so it fails at
    // 12250 ms, not at the 90 s view timeout and not as "not settled".
    let mut history = vec![sample(0, 0, 0, 41)];
    history.extend((1..=49).map(|i| sample(i * 250, 2, 2, 41)));
    assert_eq!(
        decide(&history[..49]),
        Verdict::Unsettled {
            pending: 2,
            failed: 2
        }
    );
    assert_eq!(decide(&history), Verdict::Failed { failed: 2 });
}

#[test]
fn a_failing_run_below_the_pending_count_does_not_start_the_clock() {
    // Three still loading, one failed: the failed count is not the pending one.
    let history: Vec<Sample> = (0..=60).map(|i| sample(i * 250, 3, 1, 41)).collect();
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 3,
            failed: 1
        }
    );
}

#[test]
fn a_drop_back_in_the_failed_count_restarts_the_twelve_seconds() {
    // Failing for 11.5 s, one retry loads (pending 2, failed 1) at 11750 ms,
    // then the same two keep failing from 12000 ms: the clock starts again.
    let mut history = vec![sample(0, 0, 0, 41)];
    history.extend((1..=46).map(|i| sample(i * 250, 2, 2, 41)));
    history.push(sample(11750, 2, 1, 41));
    history.extend((48..=96).map(|i| sample(i * 250, 2, 2, 41)));
    assert_eq!(
        decide(&history[..history.len() - 1]),
        Verdict::Unsettled {
            pending: 2,
            failed: 2
        }
    );
    assert_eq!(decide(&history), Verdict::Failed { failed: 2 });
}
#[test]
fn failed_chunks_inside_the_twelve_second_clock_are_not_yet_a_failure() {
    let history = [
        sample(0, 0, 0, 41),
        sample(250, 3, 3, 41),
        sample(500, 3, 3, 41),
    ];
    assert_eq!(
        decide(&history),
        Verdict::Unsettled {
            pending: 3,
            failed: 3
        }
    );
}
#[test]
fn a_chunk_that_failed_and_loaded_on_retry_does_not_fail_the_view() {
    // One chunk pending and failed from 250 ms to 1000 ms, then loaded on the
    // retry: the window opens at 1250 ms, and settles at 2750 ms.
    let mut history = vec![sample(0, 0, 0, 3)];
    history.extend((1..=4).map(|i| sample(i * 250, 1, 1, 3)));
    history.extend((5..=11).map(|i| sample(i * 250, 0, 0, 3)));
    assert_eq!(
        decide(&history[..history.len() - 1]),
        Verdict::Unsettled {
            pending: 0,
            failed: 0
        }
    );
    assert_eq!(
        decide(&history),
        Verdict::Settled {
            frame: 3,
            quiet_ms: 1500,
            window_ms: 1500
        }
    );
    assert_eq!(recovered_failures(&history), 1);
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

/// Samples every 50 ms, with the frame changing at each of `changes`.
fn frame_changes(changes: &[i64], until: i64) -> Vec<Sample> {
    (0..=until / 50)
        .map(|i| {
            let at = i * 50;
            sample(
                at,
                0,
                0,
                changes.iter().filter(|&&c| c <= at).count() as i64,
            )
        })
        .collect()
}

#[test]
fn a_slow_software_frame_sets_the_window_to_four_times_its_median() {
    // Frame times 500, 600, 700, 650 and 550 ms: the median is 600 ms.
    let history = frame_changes(&[500, 1000, 1600, 2300, 2950, 3500], 3500);
    assert_eq!(window_ms(&history), 2400);
}

#[test]
fn only_the_last_five_frame_times_set_the_median() {
    // Two 900 ms frame times come before 500, 600, 700, 650 and 550 ms: the
    // median of the last five is 600 ms, so 2400 ms, not the 650 ms of all seven.
    let history = frame_changes(&[1000, 1900, 2800, 3300, 3900, 4600, 5250, 5800], 5800);
    assert_eq!(window_ms(&history), 2400);
}

#[test]
fn a_late_redraw_after_an_idle_gap_is_not_a_frame_time() {
    // The frame changes at 1 s and then again at 7 s: the 6 s gap is idle, so
    // the window stays at its 1.5 s floor, not 4 s.
    let history = frame_changes(&[1000, 7000], 7000);
    assert_eq!(window_ms(&history), 1500);
}

#[test]
fn a_retry_redraw_after_a_long_gap_leaves_the_window_at_its_floor() {
    // Frames at 2 s and 7 s (a failed chunk retried after 5 s): not a frame time.
    let history = frame_changes(&[2000, 7000], 7000);
    assert_eq!(window_ms(&history), 1500);
}

#[test]
fn the_window_never_exceeds_four_seconds() {
    // Frames a full idle threshold apart: four times 1000 ms is the cap.
    let history = frame_changes(&[1000, 2000, 3000, 4000, 5000, 6000], 6000);
    assert_eq!(window_ms(&history), 4000);
}

#[test]
fn missing_or_unpresented_frames_are_not_settled_even_with_zero_pending() {
    for frame in [-1, 0] {
        let history = [sample(0, 0, 0, frame), sample(5000, 0, 0, frame)];
        assert!(
            matches!(decide(&history), Verdict::Unsettled { .. }),
            "{frame}"
        );
    }
}
