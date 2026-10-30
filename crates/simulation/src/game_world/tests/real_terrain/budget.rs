use super::*;

#[test]
fn occupied_waypoint_waits_from_actual_body_without_banking_progress() {
    let mut w = super::super::far_orders::wide_world();
    w.set_fog_disabled(true);
    let waypoint = center(31, 30);
    let from = WorldPosition::new(waypoint.x - 411, waypoint.y);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Archer, from).unwrap();
    w.spawn_kind(PlayerId(0), UnitKind::Archer, waypoint)
        .unwrap();
    w.issue_move(id, center(45, 30)).unwrap();
    let index = w.lookup[&id];
    w.clear_planner(index);
    w.units[index].route.clear();
    w.units[index].order = Some(MovementOrder {
        origin: from,
        waypoint,
        destination: center(45, 30),
        target_tile: TileCoord::new(45, 30),
        segment_length: 411,
        travelled: 0,
        speed_carry: 0,
    });
    // Other movers can consume the four refinements. This actor must retain
    // a coherent body/order pair while awaiting the fair route scheduler.
    w.local_refinement_left = 0;
    w.advance_movement();
    let order = w.movement_order(id).unwrap();
    assert!(w.unit(id).unwrap().planning);
    assert_eq!(order.origin, w.unit(id).unwrap().position);
    assert_eq!(order.waypoint, order.origin);
    assert_eq!(order.travelled, 0);
    let before = w.unit(id).unwrap().position;
    w.refill_route_budgets();
    w.advance_movement();
    let after = w.unit(id).unwrap().position;
    let (speed, denominator) = w.movement_speed(UnitKind::Archer, PlayerId(0));
    let moved =
        (i64::from(after.x - before.x).pow(2) + i64::from(after.y - before.y).pow(2)).isqrt();
    eprintln!(
        "occupied waypoint: second tick moved={moved}, speed limit={}",
        speed.div_ceil(denominator)
    );
    assert!(moved > 0, "the deferred mover must resume");
    assert!(moved <= speed.div_ceil(denominator) as i64 + 2);
    let goals = [(id, center(45, 30))];
    let ideal = reference_ticks(&w, &goals);
    let result = measure(&mut w, &goals, 500, ideal);
    eprintln!("real-terrain occupied-waypoint-recovery {result:?}");
    result.assert_works(1);
}
