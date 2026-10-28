//! Group movement regressions; measurements use the ordinary movement phase.
use super::far_orders::{center, wide_world};
use super::*;

fn army(w: &mut GameWorld, count: usize) -> Vec<EntityId> {
    (0..count)
        .map(|n| {
            w.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                center(40 + (n % 10) as i32, 40 + (n / 10) as i32),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn formations_measure_individual_army_baseline() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 200);
    let before = w.route_scheduler_stats().metrics.orders;
    for &id in &ids {
        w.issue_move(id, center(180, 60)).unwrap();
    }
    let mut movement = std::time::Duration::ZERO;
    for _ in 0..200 {
        let start = std::time::Instant::now();
        w.advance_with_probe(&mut |_| true, &mut |phase| {
            if phase == TickPhase::Movement {
                movement += start.elapsed();
            }
        });
    }
    eprintln!(
        "army=200 individual route_orders={} movement_200_ticks_us={}",
        w.route_scheduler_stats().metrics.orders - before,
        movement.as_micros()
    );
    assert!(ids.iter().any(|&id| w.unit(id).unwrap().moving));
}

mod behavior;
mod block;
mod chokepoint;

#[test]
fn formations_measure_shared_army_route() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 200);
    let before = w.route_scheduler_stats().metrics.orders;
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        aoe_core::Formation::Line,
        center(180, 60),
        false,
        false,
    )
    .unwrap();
    let mut movement = std::time::Duration::ZERO;
    for _ in 0..200 {
        let start = std::time::Instant::now();
        w.advance_with_probe(&mut |_| true, &mut |phase| {
            if phase == TickPhase::Movement {
                movement += start.elapsed();
            }
        });
    }
    eprintln!(
        "army=200 shared route_orders={} movement_200_ticks_us={}",
        w.route_scheduler_stats().metrics.orders - before,
        movement.as_micros()
    );
    assert_eq!(w.route_scheduler_stats().metrics.orders - before, 1);
}

mod pressure;

mod measurement;
