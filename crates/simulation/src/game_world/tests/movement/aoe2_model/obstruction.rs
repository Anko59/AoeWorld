use super::*;

#[test]
fn a_distant_rejoin_in_the_seeded_flat_village_keeps_progressing() {
    let (original, pages, water) = economy::fixture::flat_fixture(false);
    let request = aoe_map::MapRequest {
        requested_side_meters: 1000,
        compression: aoe_map::Ratio {
            numerator: 1,
            denominator: 1,
        },
        seed: 3,
        ..aoe_map::MapRequest::default()
    };
    let package = aoe_map::MapPackage::with_prepared_environment(
        1,
        request,
        Vec::new(),
        Default::default(),
        Default::default(),
        original.environment,
    )
    .unwrap();
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    w.prepare_navigation(&|| false).unwrap();
    let id = w
        .initialize_village(PlayerId(0), TileCoord::new(402, 360))
        .unwrap();
    let goal = at(415, 363);
    w.issue_move(id, goal).unwrap();
    let i = w.lookup[&id];
    let mut trace = Vec::new();
    for tick in 0..1500 {
        w.advance();
        if tick % 64 == 0 {
            trace.push((
                tick,
                w.units[i].state.position,
                w.units[i].order,
                w.units[i].route.clone(),
            ));
        }
    }
    let mut order = w.units[i].order;
    w.local_refinement_left = 4;
    let refined = order.as_mut().is_some_and(|o| w.refine_unit_route(i, o));
    let solid = (355..367)
        .flat_map(|y| (397..412).map(move |x| TileCoord::new(x, y)))
        .filter(|&t| !w.tile_passable(t))
        .collect::<Vec<_>>();
    assert_eq!(
        w.unit(id).unwrap().position,
        goal,
        "refined={refined}, draft={order:?}, route={:?}, solid={solid:?}, trace={trace:#?}",
        w.units[i].route
    );
}

#[test]
fn a_distant_rejoin_goes_around_the_town_center_and_worker_row() {
    let mut w = world();
    w.economy.blocked_tiles.extend([
        TileCoord::new(40, 41),
        TileCoord::new(41, 41),
        TileCoord::new(40, 42),
        TileCoord::new(41, 42),
    ]);
    for x in 40..43 {
        w.spawn_villager(PlayerId(0), at(x, 40)).unwrap();
    }
    let from = WorldPosition::new(40 * 1024 + 69, 40 * 1024 + 671);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Scout, from).unwrap();
    let goal = at(52, 43);
    w.issue_move_waypoints(id, &[at(43, 41), goal]).unwrap();
    let i = w.lookup[&id];
    let mut order = w.units[i].order.unwrap();
    assert!(w.refine_unit_route(i, &mut order), "local rejoin failed");
    w.units[i].order = Some(order);
    for _ in 0..1000 {
        w.advance();
    }
    assert_eq!(
        w.unit(id).unwrap().position,
        goal,
        "order={:?}, route={:?}, obstruction={:?}",
        w.units[i].order,
        w.units[i].route,
        w.units[i].obstruction
    );
}

#[test]
fn a_static_corner_sweep_rejects_a_chord_with_two_clear_endpoints() {
    let mut w = world();
    w.economy.blocked_tiles.insert(TileCoord::new(39, 40));
    let corner = WorldPosition::new(40 * 1024, 40 * 1024);
    let from = WorldPosition::new(corner.x + 20, corner.y - 287);
    let to = WorldPosition::new(corner.x + 256, corner.y);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Scout, from).unwrap();
    let i = w.lookup[&id];
    assert!(w.body_position_clear(UnitKind::Scout, from));
    assert!(w.body_position_clear(UnitKind::Scout, to));
    assert!(!w.local_unit_step_clear(i, from, to));
    w.issue_task_move(id, to).unwrap();
    for _ in 0..300 {
        w.advance();
    }
    assert_eq!(w.unit(id).unwrap().position, to);
}

#[test]
fn the_starting_scout_passes_the_idle_villagers_and_town_center() {
    let (mut w, id) = GameWorld::default_with_village(Seed(1));
    let rect = w
        .buildings()
        .find(|b| b.kind == BuildingKind::TownCenter)
        .unwrap()
        .footprint;
    let goal = at(
        (rect.min.x + rect.max.x) / 2 + 20,
        (rect.min.y + rect.max.y) / 2,
    );
    w.issue_move(id, goal).unwrap();
    let destination = w.movement_order(id).unwrap().destination;
    for _ in 0..1000 {
        w.advance();
    }
    let i = w.lookup[&id];
    assert_eq!(
        w.unit(id).unwrap().position,
        destination,
        "order={:?}, route={:?}, obstruction={:?}",
        w.units[i].order,
        w.units[i].route.iter().take(4).collect::<Vec<_>>(),
        w.units[i].obstruction
    );
}

#[test]
fn a_local_detour_bypasses_an_occupied_intermediate_point() {
    let mut w = world();
    let id = w.spawn_villager(PlayerId(0), at(40, 40)).unwrap();
    let blocked = at(41, 40);
    w.spawn_villager(PlayerId(0), blocked).unwrap();
    let goal = at(50, 40);
    w.issue_move_waypoints(id, &[blocked, goal]).unwrap();
    let i = w.lookup[&id];
    let mut order = w.units[i].order.unwrap();
    assert!(w.refine_unit_route(i, &mut order));
    assert_ne!(order.waypoint, blocked);
    assert_eq!(w.units[i].route.back(), Some(&goal));
    w.units[i].order = Some(order);
    for _ in 0..400 {
        w.advance();
    }
    assert_eq!(w.unit(id).unwrap().position, goal);
}

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
