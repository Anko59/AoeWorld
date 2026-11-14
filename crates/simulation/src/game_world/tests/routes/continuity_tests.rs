//! A local traffic detour must not invalidate the retained route's join.
use super::super::far_orders::{center, wide_world};
use super::*;

#[test]
fn traffic_at_a_partial_route_end_keeps_the_distant_order_alive() {
    let mut w = wide_world();
    let destination = center(400, 30);
    let id = w.spawn_villager(PlayerId(0), center(40, 30)).unwrap();
    w.issue_move(id, destination).unwrap();
    let index = w.lookup[&id];
    assert!(w.units[index].planner.is_some());
    let end = *w.units[index].route.back().unwrap();
    assert_ne!(end, destination);
    // Arrive at the last known leg beside a parked large body. A local
    // refinement can only land outside the old continuation's source tile.
    let from = WorldPosition::new(end.x - 800, end.y);
    let old = w.units[index].state.position;
    w.units[index].state.position = from;
    w.units[index].state.previous_position = from;
    let state = w.units[index].state;
    w.track_unit_step(id, old.tile_floor(), &state);
    w.move_bucket(index, aoe_core::ChunkCoord::from_position(from));
    w.units[index].route.clear();
    let mut order = w.units[index].order.unwrap();
    order.origin = from;
    order.waypoint = end;
    order.segment_length = 800;
    order.travelled = 0;
    w.units[index].order = Some(order);
    w.spawn_kind(PlayerId(0), UnitKind::BatteringRam, end)
        .unwrap();
    for (dx, dy) in [(-512, -512), (-512, 512), (512, -512), (512, 512)] {
        w.spawn_villager(PlayerId(0), WorldPosition::new(end.x + dx, end.y + dy))
            .unwrap();
    }
    assert!(w.refine_unit_route(index, &mut order));
    let join = w.units[index]
        .route
        .back()
        .copied()
        .unwrap_or(order.waypoint);
    assert_ne!(
        join.tile_floor(),
        end.tile_floor(),
        "exercise a changed continuation join"
    );
    w.units[index].order = Some(order);
    for _ in 0..12_000 {
        w.advance();
        assert_eq!(
            w.movement_failure(id),
            None,
            "{}",
            w.formation_member_status(id)
        );
        if w.movement_order(id).is_none() {
            break;
        }
    }
    assert_eq!(w.unit(id).unwrap().position, destination);
    assert_eq!(w.route_accounting_error(), None);
}
