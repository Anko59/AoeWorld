use super::{
    extent::{arrival_tolerance, slot_extent_tiles},
    orders::{for_each_until_stop, paced},
};
use aoe_sdk::UnitKind;
use serde_json::json;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
        .block_on(future)
}

fn probe() -> serde_json::Value {
    json!({"extent_tiles": 6.0, "speeds_tiles_per_tick": {
        "Militia": 0.0016, "BatteringRam": 0.0012, "Mangonel": 0.0010}})
}

#[test]
fn march_siege_groups_get_a_wider_arrival_radius() {
    let foot = arrival_tolerance(&probe(), &[UnitKind::Militia; 40]);
    let mut mixed = vec![UnitKind::Militia; 20];
    mixed.extend([UnitKind::BatteringRam; 20]);
    let siege = arrival_tolerance(&probe(), &mixed);
    assert!(siege > foot + 1.0, "siege {siege} foot {foot}");
    // Ram diameter 922: gap = 2 * 922 + 2 + 16 subunits against 1040.
    let expect = 6.0 * (2.0 * 922.0 + 2.0 + 16.0) / 1040.0;
    assert!((slot_extent_tiles(&probe(), &mixed) - expect).abs() < 1e-9);
}

#[test]
fn march_scatter_stops_when_asked() {
    let stop = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(60);
    let sent = block_on(paced(
        0..1_000,
        Duration::from_millis(1),
        &stop,
        deadline,
        |i| {
            if i == 3 {
                stop.store(true, Ordering::Relaxed);
            }
            true
        },
    ));
    assert_eq!(sent, 4);
}

#[test]
fn march_scatter_ends_at_the_requested_march_time() {
    let stop = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_millis(50);
    let started = Instant::now();
    let sent = block_on(paced(
        0..100_000,
        Duration::from_millis(2),
        &stop,
        deadline,
        |_| true,
    ));
    assert!(sent > 0 && sent < 100_000, "sent {sent}");
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn march_planning_stops_when_asked() {
    let stop = AtomicBool::new(false);
    let mut planned = Vec::new();
    for_each_until_stop(&mut [0, 1, 2, 3], &stop, |turn, _| {
        planned.push(turn);
        if turn == 1 {
            stop.store(true, Ordering::Relaxed);
        }
    });
    assert_eq!(planned, [0, 1]);
}
