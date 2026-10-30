use super::manager::pass;
use super::*;

fn second_castle(w: &mut GameWorld) -> EntityId {
    w.insert_building(
        PlayerId(0),
        BuildingKind::Castle,
        aoe_core::TileRect::from_xywh(150, 90, 4, 4),
        true,
    )
    .unwrap()
}
fn two_vassals() -> (GameWorld, EntityId, EntityId, EntityId, EntityId) {
    let (mut w, a_castle) = setup();
    let b_castle = second_castle(&mut w);
    let a = trained(&mut w, a_castle);
    w.issue_garrison(PlayerId(0), a, a_castle).unwrap();
    advance(&mut w, 2);
    let b = trained(&mut w, b_castle);
    w.issue_garrison(PlayerId(0), b, b_castle).unwrap();
    advance(&mut w, 2);
    (w, a_castle, b_castle, a, b)
}
fn at(w: &mut GameWorld, x: i32, y: i32) -> EntityId {
    w.spawn_villager(
        PlayerId(0),
        WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap(),
    )
    .unwrap()
}

#[test]
fn overlapping_domains_split_at_equal_distance_and_nothing_has_two_masters() {
    let (mut w, _, _, a, b) = two_vassals();
    // Castle centres are 92 and 152: the bisector is x = 122, inside both circles.
    let near_a: Vec<_> = (0..6).map(|i| at(&mut w, 100 + i * 3, 110)).collect();
    let near_b: Vec<_> = (0..6).map(|i| at(&mut w, 128 + i * 3, 110)).collect();
    for _ in 0..4 {
        pass(&mut w);
    }
    assert!(near_a.iter().all(|id| w.vassal_controller(*id) == Some(a)));
    assert!(near_b.iter().all(|id| w.vassal_controller(*id) == Some(b)));
    let homes = &w.economy.vassal_homes;
    assert!(homes[&a].workers.is_disjoint(&homes[&b].workers));
    for unit in w.units().filter(|u| u.kind == UnitKind::Villager) {
        let owners = [a, b]
            .into_iter()
            .filter(|v| homes[v].owns(unit.position))
            .count();
        assert!(owners <= 1, "{:?} has {owners} owners", unit.position);
    }
}

#[test]
fn an_exact_tie_goes_to_the_lower_id_and_a_lone_castle_keeps_its_whole_circle() {
    let (w, _, _, a, b) = two_vassals();
    let tie = WorldPosition::new(
        122 * aoe_core::FIXED_SUBUNITS_PER_TILE,
        92 * aoe_core::FIXED_SUBUNITS_PER_TILE,
    );
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    assert!(w.economy.vassal_homes[&low].owns(tie));
    assert!(!w.economy.vassal_homes[&high].owns(tie));
    let (mut solo, _) = setup();
    let castle = solo
        .player_buildings_of(PlayerId(0), BuildingKind::Castle)
        .next()
        .unwrap()
        .id;
    let id = trained(&mut solo, castle);
    solo.issue_garrison(PlayerId(0), id, castle).unwrap();
    advance(&mut solo, 2);
    let far = WorldPosition::from_tile_center(TileCoord::new(180, 92)).unwrap();
    assert!(solo.economy.vassal_homes[&id].owns(far));
}

#[test]
fn a_deactivated_neighbour_releases_its_half_to_the_other_castle() {
    let (mut w, _, b_castle, a, b) = two_vassals();
    let v = at(&mut w, 140, 110);
    pass(&mut w);
    pass(&mut w);
    assert_eq!(w.vassal_controller(v), Some(b));
    w.issue_ungarrison(PlayerId(0), b_castle, Some(b)).unwrap();
    assert_eq!(w.vassal_controller(v), None);
    pass(&mut w);
    pass(&mut w);
    assert_eq!(
        w.vassal_controller(v),
        Some(a),
        "inside A's radius, now A's"
    );
}

#[test]
fn a_managed_worker_that_never_moves_is_retasked_and_counts_as_idle() {
    let (mut w, _, vassal) = super::manager::active();
    pass(&mut w);
    let id = w
        .units()
        .find(|u| w.vassal_controller(u.id) == Some(vassal))
        .unwrap()
        .id;
    // The reported fault: a "Moving" task with no order, planner or progress.
    // Drive only the Vassal pass, so nothing else normalises the stuck state.
    let stuck = |w: &mut GameWorld| {
        w.economy.gatherers.remove(&id);
        let index = w.lookup[&id];
        w.units[index].state.task = UnitTask::Moving;
        w.units[index].state.moving = false;
        w.units[index].state.planning = false;
        w.units[index].order = None;
    };
    stuck(&mut w);
    let period = crate::VASSAL_RULES.period_ticks;
    w.tick.0 = (w.tick.0 / period + 1) * period - period;
    let passes = crate::VASSAL_STALL_SECONDS * u64::from(w.config().tick_hz) / period;
    for _ in 0..passes - 1 {
        w.tick.0 += period;
        w.advance_vassals(&mut Vec::new());
        assert_eq!(
            w.unit(id).unwrap().task,
            UnitTask::Moving,
            "not yet stalled"
        );
    }
    for _ in 0..3 {
        w.tick.0 += period;
        w.advance_vassals(&mut Vec::new());
    }
    let unit = w.unit(id).unwrap();
    assert!(
        unit.task != UnitTask::Moving || w.movement_order(id).is_some(),
        "a Moving worker must have somewhere to go"
    );
    assert_eq!(w.vassal_controller(id), Some(vassal), "still managed");
}

#[test]
fn the_territory_demo_shows_adoption_then_a_split_when_the_second_vassal_shelters() {
    let (mut w, _) = GameWorld::vassal_territory_demo(aoe_core::Seed(7)).unwrap();
    let vassals: Vec<_> = w.economy.vassal_homes.keys().copied().collect();
    let (first, second) = (vassals[0], vassals[1]);
    let home = w.vassal_castle(first).unwrap();
    w.issue_garrison(PlayerId(0), first, home).unwrap();
    advance(&mut w, 80);
    let farmers = |w: &GameWorld, v| {
        w.units()
            .filter(|u| w.gathering_kind(u.id).is_some() && w.vassal_controller(u.id) == Some(v))
            .count()
    };
    assert_eq!(
        farmers(&w, first),
        40,
        "one Vassal adopts every busy farmer"
    );
    let castle = w.vassal_castle(second).unwrap();
    w.issue_garrison(PlayerId(0), second, castle).unwrap();
    advance(&mut w, 100);
    let (a, b) = (farmers(&w, first), farmers(&w, second));
    assert!(
        a + b <= 40 && a >= 8 && b >= 8,
        "split at the border: {a}/{b}"
    );
    for u in w.units().filter(|u| w.gathering_kind(u.id).is_some()) {
        let owner = if u.position.x < 127 * aoe_core::FIXED_SUBUNITS_PER_TILE {
            first
        } else {
            second
        };
        assert!(
            w.vassal_controller(u.id).is_none_or(|v| v == owner),
            "{:?} is on the wrong side",
            u.position
        );
    }
}
