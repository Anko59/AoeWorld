//! Route guides (`route_scheduler/fields.rs`, #978 slice 6e).
use super::super::far_orders::{center as at, wide_world as world};
use super::*;
use crate::game_world::route_scheduler::FIELD_CHUNK;
use crate::{BuildingKind, UnitKind};
use std::collections::BTreeMap;

/// The destination chunk (5, 1) and the box a guide to it covers.
fn boxes() -> (TileRect, TileRect) {
    (
        TileRect::from_xywh(5 * FIELD_CHUNK, FIELD_CHUNK, FIELD_CHUNK, FIELD_CHUNK),
        TileRect::from_xywh(4 * FIELD_CHUNK, 0, 3 * FIELD_CHUNK, 3 * FIELD_CHUNK),
    )
}

#[test]
fn a_guide_descent_goes_round_buildings_on_steps_a_search_could_take() {
    let mut w = world();
    // A row of houses across the straight line to the destination chunk.
    for y in (34..62).step_by(2) {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(150, y, 2, 2),
            true,
        )
        .unwrap();
    }
    w.prepare_navigation(&|| false).unwrap();
    let (goal, area) = boxes();
    let guide = w.route_guide(PlayerId(0), goal, area).unwrap();
    let from = TileCoord::new(132, 48);
    let tiles = w.guide_descent(&guide, PlayerId(0), from, goal).unwrap();
    assert!(goal.contains(*tiles.last().unwrap()));
    let clear = w.route_edges(PlayerId(0)).unwrap();
    for pair in tiles.windows(2) {
        assert!(!w.economy.blocked_tiles.contains(&pair[1]), "{pair:?}");
        assert!(clear(
            pair[0],
            (pair[1].x - pair[0].x, pair[1].y - pair[0].y)
        ));
    }
    assert!(
        tiles.iter().any(|t| t.y < 34 || t.y >= 62),
        "the descent goes round the row"
    );
}

#[test]
fn the_second_wait_for_a_chunk_walks_a_guide_and_an_attacker_keeps_its_search() {
    let mut w = world();
    w.prepare_navigation(&|| false).unwrap();
    let first = w
        .spawn_kind(PlayerId(0), UnitKind::Scout, at(132, 40))
        .unwrap();
    let second = w
        .spawn_kind(PlayerId(0), UnitKind::Scout, at(134, 44))
        .unwrap();
    let attacker = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(136, 48))
        .unwrap();
    let target = w
        .spawn_kind(PlayerId(1), UnitKind::Villager, at(175, 50))
        .unwrap();
    w.planning_budget = 1;
    w.fast_route_budget = 0;
    w.issue_move(first, at(170, 45)).unwrap();
    w.issue_move(second, at(180, 40)).unwrap();
    w.issue_attack(PlayerId(0), attacker, target).unwrap();
    assert!(w.combat.attackers.contains_key(&attacker));
    // Refused a search slot: waiting with no planner, as on R1.
    for id in [first, second] {
        w.clear_planner(w.lookup[&id]);
    }
    let mut waiting = BTreeMap::from([(PlayerId(0), vec![first, second, attacker])]);
    let (mut budget, mut changed) = (u32::MAX, Vec::new());
    w.serve_from_fields(&mut waiting, &mut budget, &mut changed);
    // The first wait only marks the chunk; the second is routed by a guide
    // and searches its last leg; the attacker keeps waiting for its search.
    assert_eq!(waiting[&PlayerId(0)], [first, attacker]);
    assert_eq!(changed, [second]);
    let index = w.lookup[&second];
    assert!(!w.units[index].route.is_empty());
    assert!(w.units[index].planner.is_some());
}
