use super::*;

#[test]
fn a_local_detour_rejoins_a_long_segment_without_losing_its_endpoint() {
    let mut w = world();
    let id = w.spawn_villager(PlayerId(0), at(40, 40)).unwrap();
    w.spawn_villager(PlayerId(0), at(41, 40)).unwrap();
    let goal = at(50, 40);
    w.issue_move_waypoints(id, &[goal]).unwrap();
    let i = w.lookup[&id];
    let mut order = w.units[i].order.unwrap();
    assert!(w.refine_unit_route(i, &mut order));
    assert_eq!(w.units[i].route.back(), Some(&goal));
    assert!(distance(order.waypoint, w.unit(id).unwrap().position) <= 1536 * 1536);
    w.units[i].order = Some(order);
    for _ in 0..400 {
        w.advance();
    }
    assert_eq!(w.unit(id).unwrap().position, goal);
}

#[test]
fn spawn_overlap_repair_cannot_push_a_body_into_a_separated_neighbour() {
    let mut w = world();
    let origin = at(40, 40);
    let a = w.spawn_villager(PlayerId(0), origin).unwrap();
    w.spawn_villager(PlayerId(0), WorldPosition::new(origin.x + 100, origin.y))
        .unwrap();
    let c = w
        .spawn_villager(PlayerId(0), WorldPosition::new(origin.x - 420, origin.y))
        .unwrap();
    let index = w.lookup[&a];
    w.advance_spacing_indices(&mut Vec::new(), [index]);
    assert!(
        distance(w.unit(a).unwrap().position, w.unit(c).unwrap().position) >= 410 * 410,
        "repair may rearrange an existing overlap, never introduce a new one"
    );
    for _ in 0..200 {
        w.advance();
    }
    let ids = w.units().map(|u| u.id).collect::<Vec<_>>();
    for (n, &a) in ids.iter().enumerate() {
        for &b in &ids[n + 1..] {
            assert!(
                distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position) >= 410 * 410
            );
        }
    }
}

#[test]
fn land_and_water_followers_do_not_share_an_obstruction_window() {
    let mut w = world();
    let origin = at(30, 30);
    let a = w.spawn_villager(PlayerId(0), origin).unwrap();
    let b = w
        .spawn_villager(PlayerId(0), WorldPosition::new(origin.x + 400, origin.y))
        .unwrap();
    for id in [a, b] {
        w.issue_task_move(id, at(50, 30)).unwrap();
    }
    assert!(w.following_unit(w.lookup[&a]));
    // Probe domain filtering independently of terrain and naval path admission.
    let i = w.lookup[&b];
    let body = w.units[i].state;
    w.untrack_unit_cell(b, crate::game_world::wildlife::cell(body.position));
    w.units[i].state.kind = UnitKind::FishingShip;
    let body = w.units[i].state;
    w.track_unit_cell(&body);
    assert!(!w.following_unit(w.lookup[&a]));
}
