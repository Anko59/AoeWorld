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
    let row = &w.order_demand()[OrderIssuer::PythonAi as usize];
    assert_eq!(row.orders, 2);
    assert_eq!(row.near_reissues, 1);
    assert_eq!(row.reassigned_10s, 1);
    assert_eq!(row.superseded, 1);
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
        w.order_demand()[OrderIssuer::Steward as usize].superseded,
        0
    );
}
