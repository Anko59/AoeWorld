use super::*;
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig};

fn at(x: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, 20)).unwrap()
}

#[test]
fn movement_demand_counts_issuer_repeats_and_supersession_without_changing_hash() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(20)).unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(100)))
        .unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(101)))
        .unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(150)))
        .unwrap();
    let row = &w.order_demand()[OrderIssuer::PythonAi as usize];
    assert_eq!(row.orders, 3);
    assert_eq!(row.near_reissues, 1);
    assert_eq!(row.reassigned_10s, 1);
    assert_eq!(row.superseded, 1);
    assert_eq!(row.superseded_10s, 1);
    assert_eq!(row.route_requests, 2);
    let hash = w.canonical_hash();
    w.order_diagnostics = Default::default();
    assert_eq!(w.canonical_hash(), hash, "diagnostics are not game state");
}

#[test]
fn nested_issuer_restores_the_outer_source() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.with_order_issuer(OrderIssuer::Vassal, |w| {
        w.with_order_issuer(OrderIssuer::General, |_| ());
        assert_eq!(w.order_diagnostics.issuer, OrderIssuer::Vassal);
    });
    assert_eq!(w.order_diagnostics.issuer, OrderIssuer::Automation);
}

#[test]
fn source_decisions_include_orders_that_do_not_start_a_trip() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    w.with_order_issuer(OrderIssuer::Player, |w| w.record_order_decisions(2));
    assert_eq!(w.order_demand()[OrderIssuer::Player as usize].decisions, 2);
    assert_eq!(w.order_demand()[OrderIssuer::Player as usize].orders, 0);
}

#[test]
fn stopping_a_measured_trip_supersedes_its_original_issuer() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(20)).unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(100)))
        .unwrap();
    w.with_order_issuer(OrderIssuer::Steward, |w| w.issue_stop(PlayerId(0), id))
        .unwrap();
    assert_eq!(
        w.order_demand()[OrderIssuer::PythonAi as usize].superseded,
        1
    );
    assert_eq!(
        w.order_demand()[OrderIssuer::PythonAi as usize].superseded_10s,
        1
    );
    assert_eq!(
        w.order_demand()[OrderIssuer::Steward as usize].superseded,
        0
    );
}

#[test]
fn redundant_order_sources_distinguish_callers_without_changing_flat_counts_or_hash() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(20)).unwrap();
    w.issue_task_move(id, at(100)).unwrap();
    w.issue_task_move(id, at(100)).unwrap();
    w.issue_task_move(id, at(101)).unwrap();
    let sources = w.order_redundancy_sources();
    assert_eq!(
        sources.len(),
        2,
        "the two redundant request sites stay distinct"
    );
    assert!(sources.keys().all(|source| source.contains(file!())));
    assert_eq!(sources.values().map(|v| v.near_reissues).sum::<u64>(), 2);
    assert_eq!(sources.values().map(|v| v.superseded_10s).sum::<u64>(), 1);
    let flat = w.order_demand()[OrderIssuer::Automation as usize];
    assert_eq!(flat.orders, 3);
    assert_eq!(flat.near_reissues, 2);
    assert_eq!(flat.superseded_10s, 1);
    let hash = w.canonical_hash();
    w.order_diagnostics = Default::default();
    assert_eq!(w.canonical_hash(), hash);
    assert!(w.order_redundancy_sources().is_empty());
}

#[test]
fn retained_and_accepted_requests_are_counted_apart_and_route_work_follows_the_issuer() {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(20)).unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(100)))
        .unwrap();
    w.with_order_issuer(OrderIssuer::PythonAi, |w| w.issue_move(id, at(100)))
        .unwrap();
    let row = &w.order_demand()[OrderIssuer::PythonAi as usize];
    assert_eq!((row.orders, row.accepted, row.retained), (2, 1, 1));
    assert_eq!(w.order_mover_census()[OrderIssuer::PythonAi as usize], 1);
    let issue_work: u64 = w.order_route_work().iter().map(|r| r[0]).sum();
    let python_issue = w.order_route_work()[OrderIssuer::PythonAi as usize][0];
    assert_eq!(issue_work, python_issue, "issue-time work is the caller's");
}
