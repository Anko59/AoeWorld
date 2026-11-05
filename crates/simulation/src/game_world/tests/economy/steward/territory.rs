use super::manager::pass;
use super::*;

fn second_town_center(w: &mut GameWorld) -> EntityId {
    w.insert_building(
        PlayerId(0),
        BuildingKind::TownCenter,
        aoe_core::TileRect::from_xywh(150, 90, 4, 4),
        true,
    )
    .unwrap()
}
fn two_stewards() -> (GameWorld, EntityId, EntityId, EntityId, EntityId) {
    let (mut w, a_town_center) = setup();
    let b_town_center = second_town_center(&mut w);
    let a = trained(&mut w, a_town_center);
    w.issue_garrison(PlayerId(0), a, a_town_center).unwrap();
    advance(&mut w, 2);
    let b = trained(&mut w, b_town_center);
    w.issue_garrison(PlayerId(0), b, b_town_center).unwrap();
    advance(&mut w, 2);
    (w, a_town_center, b_town_center, a, b)
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
    let (mut w, _, _, a, b) = two_stewards();
    // Town Center centres are 92 and 152: the bisector is x = 122, inside both circles.
    let near_a: Vec<_> = (0..6).map(|i| at(&mut w, 100 + i * 3, 110)).collect();
    let near_b: Vec<_> = (0..6).map(|i| at(&mut w, 128 + i * 3, 110)).collect();
    for _ in 0..4 {
        pass(&mut w);
    }
    assert!(near_a.iter().all(|id| w.steward_controller(*id) == Some(a)));
    assert!(near_b.iter().all(|id| w.steward_controller(*id) == Some(b)));
    let homes = &w.economy.steward_homes;
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
fn crossing_workers_keep_their_foreign_farm_or_build_job_until_they_reach_its_domain() {
    for building in [false, true] {
        let (mut w, _, _, a, b) = two_stewards();
        let kind = if building {
            BuildingKind::House
        } else {
            BuildingKind::Farm
        };
        let site = w
            .insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(110, 108, kind.rules().side, kind.rules().depth),
                !building,
            )
            .unwrap();
        let worker = at(&mut w, 130, 110);
        if building {
            w.issue_construct(PlayerId(0), &[worker], site).unwrap();
        } else {
            w.issue_farm(PlayerId(0), worker, site).unwrap();
        }
        pass(&mut w);
        assert!(w.economy.steward_homes[&b].owns(w.unit(worker).unwrap().position));
        assert_eq!(
            w.steward_controller(worker),
            None,
            "{b:?} must not take a worker serving a site in {a:?}'s domain"
        );
        // Check every step of the ordinary trip, then adoption at actual
        // arrival. A six-second window alone does not guarantee entry.
        for _ in 0..1_200 {
            if w.economy.steward_homes[&a].owns(w.unit(worker).unwrap().position) {
                break;
            }
            assert_eq!(w.steward_controller(worker), None);
            if building {
                assert_eq!(w.economy.builders[&worker].building, site);
            } else {
                assert!(
                    w.building(site)
                        .unwrap()
                        .footprint
                        .contains(w.gather_tile(worker).unwrap())
                );
            }
            w.advance();
        }
        assert!(w.economy.steward_homes[&a].owns(w.unit(worker).unwrap().position));
        pass(&mut w);
        assert_eq!(w.steward_controller(worker), Some(a));
    }
}

#[test]
fn an_exact_tie_goes_to_the_lower_id_and_a_lone_town_center_keeps_its_whole_circle() {
    let (w, _, _, a, b) = two_stewards();
    let tie = WorldPosition::new(
        122 * aoe_core::FIXED_SUBUNITS_PER_TILE,
        92 * aoe_core::FIXED_SUBUNITS_PER_TILE,
    );
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    assert!(w.economy.steward_homes[&low].owns(tie));
    assert!(!w.economy.steward_homes[&high].owns(tie));
    let (mut solo, town_center) = setup();
    let id = trained(&mut solo, town_center);
    solo.issue_garrison(PlayerId(0), id, town_center).unwrap();
    advance(&mut solo, 2);
    let far = WorldPosition::from_tile_center(TileCoord::new(180, 92)).unwrap();
    assert!(solo.economy.steward_homes[&id].owns(far));
}

#[test]
fn a_deactivated_neighbour_releases_its_half_to_the_other_town_center() {
    let (mut w, _, b_town_center, a, b) = two_stewards();
    let v = at(&mut w, 140, 110);
    pass(&mut w);
    pass(&mut w);
    assert_eq!(w.steward_controller(v), Some(b));
    w.issue_ungarrison(PlayerId(0), b_town_center, Some(b))
        .unwrap();
    assert_eq!(w.steward_controller(v), None);
    pass(&mut w);
    pass(&mut w);
    assert_eq!(
        w.steward_controller(v),
        Some(a),
        "inside A's radius, now A's"
    );
}

#[test]
fn a_managed_worker_that_never_moves_is_retasked_and_counts_as_idle() {
    let (mut w, _, steward) = super::manager::active();
    pass(&mut w);
    let id = w
        .units()
        .find(|u| w.steward_controller(u.id) == Some(steward))
        .unwrap()
        .id;
    // The reported fault: a "Moving" task with no order, planner or progress.
    // Drive only the Steward pass, so nothing else normalises the stuck state.
    let stuck = |w: &mut GameWorld| {
        w.economy.gatherers.remove(&id);
        let index = w.lookup[&id];
        w.units[index].state.task = UnitTask::Moving;
        w.units[index].state.moving = false;
        w.units[index].state.planning = false;
        w.units[index].order = None;
    };
    stuck(&mut w);
    let period = crate::STEWARD_RULES.period_ticks;
    w.tick.0 = (w.tick.0 / period + 1) * period - period;
    let passes = crate::STEWARD_STALL_SECONDS * u64::from(w.config().tick_hz) / period;
    for _ in 0..passes - 1 {
        w.tick.0 += period;
        w.advance_stewards(&mut Vec::new());
        assert_eq!(
            w.unit(id).unwrap().task,
            UnitTask::Moving,
            "not yet stalled"
        );
    }
    for _ in 0..3 {
        w.tick.0 += period;
        w.advance_stewards(&mut Vec::new());
    }
    let unit = w.unit(id).unwrap();
    assert!(
        unit.task != UnitTask::Moving || w.movement_order(id).is_some(),
        "a Moving worker must have somewhere to go"
    );
    assert_eq!(w.steward_controller(id), Some(steward), "still managed");
}

/// Drives only the Steward passes for longer than the stall limit.
fn stall_passes(w: &mut GameWorld) {
    let period = crate::STEWARD_RULES.period_ticks;
    let passes = 2 * crate::STEWARD_STALL_SECONDS * u64::from(w.config().tick_hz) / period;
    w.tick.0 = w.tick.0 / period * period;
    for _ in 0..passes {
        w.tick.0 += period;
        w.advance_stewards(&mut Vec::new());
    }
}

/// Stops the watchdog has issued for this villager.
fn strikes(w: &GameWorld, steward: EntityId, id: EntityId) -> u8 {
    w.economy.steward_homes[&steward]
        .stalls
        .get(&id)
        .map_or(0, |s| s.strikes)
}

fn freeze(w: &mut GameWorld, id: EntityId, planning: bool, order: bool) {
    let index = w.lookup[&id];
    w.units[index].state.task = UnitTask::Moving;
    w.units[index].state.moving = false;
    w.units[index].state.planning = planning;
    if !order {
        w.units[index].order = None;
    }
}

#[test]
fn a_stationary_villager_under_a_player_order_is_not_stopped_but_a_frozen_stray_is() {
    // (managed first, then overridden by the player), planning, with an order.
    for (managed, planning, order) in [
        (true, true, false),
        (true, false, true),
        (false, false, true),
    ] {
        let (mut w, _, steward) = super::manager::active();
        pass(&mut w);
        let id = w
            .units()
            .find(|u| u.kind == UnitKind::Villager && w.steward_controller(u.id) == Some(steward))
            .unwrap()
            .id;
        let far = WorldPosition::from_tile_center(TileCoord::new(150, 100)).unwrap();
        w.economy.gatherers.remove(&id);
        if !managed {
            w.forget_steward_worker(id);
        }
        w.issue_move(id, far).unwrap();
        freeze(&mut w, id, planning, order);
        stall_passes(&mut w);
        assert_eq!(
            strikes(&w, steward, id),
            0,
            "player order stopped (managed={managed} planning={planning} order={order})"
        );
    }
    let (mut w, _, steward) = super::manager::active();
    pass(&mut w);
    let id = w
        .units()
        .find(|u| u.kind == UnitKind::Villager && w.steward_controller(u.id) == Some(steward))
        .unwrap()
        .id;
    w.economy.gatherers.remove(&id);
    w.forget_steward_worker(id);
    freeze(&mut w, id, false, false);
    stall_passes(&mut w);
    assert!(
        strikes(&w, steward, id) >= 1,
        "frozen stray recovered {:?} {:?} {:?}",
        w.unit(id)
            .map(|u| (u.task, u.moving, u.planning, u.position)),
        w.economy.steward_homes[&steward].stalls.get(&id),
        w.steward_controller(id)
    );
}

#[test]
fn territory_showcase_keeps_forty_farmers_alive_before_activation() {
    let (mut w, _) = GameWorld::steward_territory_demo(aoe_core::Seed(7)).unwrap();
    let farmers: Vec<_> = w
        .units()
        .filter(|u| w.gathering_kind(u.id) == Some(aoe_map::ResourceKind::Food))
        .map(|u| u.id)
        .collect();
    assert_eq!(farmers.len(), 40);
    let ticks = w.config().tick_hz * 180;
    advance(&mut w, ticks);
    assert!(
        farmers
            .iter()
            .all(|id| w.unit(*id).is_some_and(|u| u.hp > 0)),
        "the capture must isolate management instead of losing farmers to hostile actors"
    );
}

#[test]
fn the_territory_demo_shows_adoption_then_a_split_when_the_second_steward_shelters() {
    let (mut w, _) = GameWorld::steward_territory_demo(aoe_core::Seed(7)).unwrap();
    let stewards: Vec<_> = w.economy.steward_homes.keys().copied().collect();
    let (first, second) = (stewards[0], stewards[1]);
    let town_centers: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .filter(|b| b.footprint.min.y == 100)
        .map(|b| b.id)
        .collect();
    let home = town_centers[0];
    w.issue_garrison(PlayerId(0), first, home).unwrap();
    advance(&mut w, 80);
    let farmers = |w: &GameWorld, v| {
        w.units()
            .filter(|u| w.gathering_kind(u.id).is_some() && w.steward_controller(u.id) == Some(v))
            .count()
    };
    assert_eq!(
        farmers(&w, first),
        40,
        "one Steward adopts every busy farmer"
    );
    let town_center = town_centers[1];
    w.issue_garrison(PlayerId(0), second, town_center).unwrap();
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
            w.steward_controller(u.id).is_none_or(|v| v == owner),
            "{:?} is on the wrong side",
            u.position
        );
    }
}
