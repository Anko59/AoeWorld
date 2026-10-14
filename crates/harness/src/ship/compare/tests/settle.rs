use crate::ship::compare::settle::{Sample, Verdict, decide};

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
            quiet_ms: 1500
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
            quiet_ms: 1500
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
            quiet_ms: 1500
        }
    );
}

#[test]
fn failed_chunks_fail_the_view_with_the_count() {
    let history = [
        sample(0, 0, 0, 41),
        sample(250, 0, 3, 41),
        sample(500, 0, 3, 41),
    ];
    assert_eq!(decide(&history), Verdict::Failed { failed: 3 });
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
