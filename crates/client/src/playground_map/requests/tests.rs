use super::*;

fn demand(start: i32, count: usize) -> Vec<Coordinate> {
    (0..count)
        .map(|offset| (start + offset as i32, 0))
        .collect()
}

#[test]
fn full_old_camera_window_is_replaced_without_waiting_for_old_completions() {
    let mut window = RequestWindow::default();
    let old = window.reconcile(&demand(0, 64), |_| true, || ());
    assert_eq!(old.started.len(), MAX_REQUESTED_CHUNKS);
    let new = window.reconcile(&demand(100, 64), |_| true, || ());
    assert_eq!(new.cancelled.len(), MAX_REQUESTED_CHUNKS);
    assert_eq!(new.started.len(), MAX_REQUESTED_CHUNKS);
    assert_eq!(window.pending.len(), MAX_REQUESTED_CHUNKS);
    for request in old.started {
        assert!(!window.complete(request.coordinate, request.id));
    }
    for request in new.started {
        assert!(window.complete(request.coordinate, request.id));
    }
    assert!(window.pending.is_empty());
}

#[test]
fn same_coordinate_replacement_survives_retired_success_or_error_and_reconnect() {
    let mut window = RequestWindow::default();
    let old = window
        .reconcile(&[(1, 2)], |_| true, || ())
        .started
        .remove(0);
    window.reconcile(&[], |_| true, || ());
    let new = window
        .reconcile(&[(1, 2)], |_| true, || ())
        .started
        .remove(0);
    assert!(!window.complete(old.coordinate, old.id));
    assert_eq!(window.pending.len(), 1);
    assert!(window.complete(new.coordinate, new.id));
    let before_clear = window
        .reconcile(&[(1, 2)], |_| true, || ())
        .started
        .remove(0);
    assert_eq!(window.clear().len(), 1);
    let after_clear = window
        .reconcile(&[(1, 2)], |_| true, || ())
        .started
        .remove(0);
    assert!(!window.complete(before_clear.coordinate, before_clear.id));
    assert!(window.complete(after_clear.coordinate, after_clear.id));
}

#[test]
fn unchanged_or_overlapping_demand_keeps_live_handles_and_does_not_duplicate() {
    let mut window = RequestWindow::default();
    window.reconcile(&demand(0, 64), |_| true, || ());
    let same = window.reconcile(&demand(0, 64), |_| true, || panic!("no new handle"));
    assert!(same.cancelled.is_empty() && same.started.is_empty());
    let shifted = window.reconcile(&demand(1, 64), |_| true, || ());
    assert_eq!(shifted.cancelled.len(), 1);
    assert_eq!(shifted.started.len(), 1);
    assert_eq!(shifted.started[0].coordinate, (64, 0));
    let duplicate = window.reconcile(&[(64, 0); 100], |_| true, || panic!("already pending"));
    assert!(duplicate.started.is_empty());
    assert_eq!(window.pending.len(), 1);
}

#[test]
fn unsupported_abort_retains_original_cap_until_completed() {
    let mut window = RequestWindow::default();
    let old = window.reconcile(&demand(0, 64), |_| false, || ());
    let shifted = window.reconcile(&demand(100, 64), |_| false, || ());
    assert!(shifted.cancelled.is_empty() && shifted.started.is_empty());
    assert!(window.complete(old.started[0].coordinate, old.started[0].id));
    let next = window.reconcile(&demand(100, 64), |_| false, || ());
    assert_eq!(next.started.len(), 1);
    assert_eq!(window.pending.len(), MAX_REQUESTED_CHUNKS);
}

#[test]
fn oversized_unique_demand_never_exceeds_the_existing_request_budget() {
    let mut window = RequestWindow::default();
    let update = window.reconcile(&demand(-100, 1024), |_| true, || ());
    assert_eq!(update.started.len(), 64);
    assert_eq!(window.pending.len(), 64);
}
