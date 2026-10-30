use super::*;
use crate::VASSAL_RULES;

fn active() -> (GameWorld, EntityId, EntityId) {
    let (mut w, castle) = setup();
    let id = trained(&mut w, castle);
    w.issue_garrison(PlayerId(0), id, castle).unwrap();
    advance(&mut w, 2);
    // Census/reseed tests isolate the economy: a wolf killing a worker should
    // correctly trigger a replacement, which is covered separately from a
    // shelter operation preserving the number of living residents.
    let wolves: Vec<_> = w
        .animals()
        .filter(|a| a.kind == crate::AnimalKind::Wolf)
        .map(|a| a.id)
        .collect();
    for wolf in wolves {
        w.strike_animal(wolf, id, u16::MAX);
    }
    (w, castle, id)
}

fn pass(w: &mut GameWorld) {
    advance(w, VASSAL_RULES.period_ticks as u32);
}

#[test]
fn economy_waits_for_home_garrison_and_releases_control_on_ejection() {
    let (mut w, castle) = setup();
    let id = trained(&mut w, castle);
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    pass(&mut w);
    assert!(workers.iter().all(|id| w.vassal_controller(*id).is_none()));
    assert!(w.buildings().all(|b| b.training_queue.is_empty()));
    w.issue_garrison(PlayerId(0), id, castle).unwrap();
    pass(&mut w);
    assert!(
        workers
            .iter()
            .any(|worker| w.vassal_controller(*worker) == Some(id))
    );
    w.issue_ungarrison(PlayerId(0), castle, Some(id)).unwrap();
    assert!(workers.iter().all(|id| w.vassal_controller(*id).is_none()));
    pass(&mut w);
    assert_eq!(w.vassal_work().domains, 0);
}

#[test]
fn domain_is_a_circle_and_never_touches_military_foreign_or_outside_workers() {
    let (mut w, _, _) = active();
    let mut untouched = Vec::new();
    for (player, kind, tile) in [
        (PlayerId(0), UnitKind::Villager, TileCoord::new(220, 92)),
        (PlayerId(0), UnitKind::Villager, TileCoord::new(170, 170)),
        (PlayerId(1), UnitKind::Villager, TileCoord::new(90, 115)),
        (PlayerId(0), UnitKind::Scout, TileCoord::new(115, 90)),
    ] {
        let id = w
            .spawn_kind(player, kind, WorldPosition::from_tile_center(tile).unwrap())
            .unwrap();
        untouched.push(id);
    }
    pass(&mut w);
    for id in untouched {
        assert_eq!(w.vassal_controller(id), None);
        assert_eq!(w.gathering_kind(id), None);
        assert!(!w.economy.builders.contains_key(&id));
        assert!(w.movement_order(id).is_none());
    }
}

#[test]
fn a_manual_order_takes_a_worker_out_until_it_is_idle_again() {
    let (mut w, _, vassal) = active();
    pass(&mut w);
    let id = w
        .units()
        .find(|u| w.vassal_controller(u.id) == Some(vassal))
        .unwrap()
        .id;
    let destination = WorldPosition::from_tile_center(TileCoord::new(130, 69)).unwrap();
    w.issue_move(id, destination).unwrap();
    assert_eq!(w.vassal_controller(id), None);
    for _ in 0..4 {
        pass(&mut w);
        assert_eq!(w.vassal_controller(id), None);
        assert_eq!(w.movement_order(id).unwrap().destination, destination);
    }
    w.issue_stop(PlayerId(0), id).unwrap();
    pass(&mut w);
    assert_eq!(w.vassal_controller(id), Some(vassal));
}

#[test]
fn training_counts_existing_workers_all_tc_queues_and_sheltered_villagers() {
    let (mut w, _, _) = active();
    for i in 0..200 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(3 + (i % 70) * 3, 180 + (i / 70) * 3, 2, 2),
            true,
        )
        .unwrap();
    }
    let second = w
        .insert_building(
            PlayerId(0),
            BuildingKind::TownCenter,
            aoe_core::TileRect::from_xywh(110, 70, 4, 4),
            true,
        )
        .unwrap();
    for i in 0..95 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(115 + (i % 10) * 2, 105 + (i / 10) * 2))
                .unwrap(),
        )
        .unwrap();
    }
    let tc = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .id;
    w.issue_train(PlayerId(0), tc, UnitKind::Villager).unwrap();
    pass(&mut w);
    let queued = |w: &GameWorld| {
        w.buildings()
            .flat_map(|b| &b.training_queue)
            .filter(|e| e.kind == UnitKind::Villager)
            .count()
    };
    assert_eq!(
        queued(&w),
        2,
        "98 residents plus two reserved births, across both TCs"
    );
    assert_eq!(w.building(second).unwrap().training_queue.len(), 1);
    advance(&mut w, 600);
    assert_eq!(
        w.units().filter(|u| u.kind == UnitKind::Villager).count(),
        100
    );
    assert_eq!(queued(&w), 0);
    // A manually sheltered resident still belongs to the economic census.
    let worker = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    w.issue_garrison(PlayerId(0), worker, tc).unwrap();
    advance(&mut w, 1_000);
    assert!(
        w.building(tc)
            .unwrap()
            .garrison
            .iter()
            .any(|g| g.unit.id == worker)
    );
    assert_eq!(queued(&w), 0);
    let lost = w
        .units()
        .find(|u| u.kind == UnitKind::Villager && u.hp > 0)
        .unwrap()
        .id;
    w.kill_unit(w.lookup[&lost]);
    pass(&mut w);
    assert_eq!(queued(&w), 1, "a death frees exactly one economic slot");
}

#[test]
fn manager_builds_a_mill_and_farms_and_prepays_normal_reseeds() {
    let (mut w, _, vassal) = active();
    // No map resource nodes: the food fallback must use ordinary construction.
    advance(&mut w, 2_500);
    let mill = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .find(|b| b.is_complete())
        .unwrap();
    assert!(mill.farm_reseeds > 0);
    let farm = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .find(|b| b.is_complete())
        .unwrap()
        .id;
    let plot = w.building(farm).unwrap().footprint;
    let farmer = w
        .units()
        .find(|u| u.task == UnitTask::Farming && plot.contains(u.position.tile_floor()))
        .unwrap()
        .id;
    assert_eq!(w.vassal_controller(farmer), Some(vassal));
    w.economy.buildings.get_mut(&farm).unwrap().food_remaining = Some(0);
    advance(&mut w, 2_000);
    assert!(
        w.building(farm).unwrap().food_remaining.unwrap() > 0,
        "normal reseed rebuilt the same plot"
    );
}

#[test]
fn deterministic_manager_replays_orders_control_and_budget_counts() {
    let (mut a, _, _) = active();
    let (mut b, _, _) = active();
    for tick in 0..300 {
        if tick == 100 {
            for w in [&mut a, &mut b] {
                let id = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
                w.issue_move(
                    id,
                    WorldPosition::from_tile_center(TileCoord::new(120, 70)).unwrap(),
                )
                .unwrap();
            }
        }
        a.advance();
        b.advance();
        assert_eq!(a.canonical_hash(), b.canonical_hash(), "tick {tick}");
        assert_eq!(a.vassal_work(), b.vassal_work());
    }
}

#[test]
fn hundred_thousand_population_manager_work_stays_inside_its_world_budget() {
    let mut w = GameWorld::new(WorldConfig::new(4096, 4096, Seed(7)).unwrap()).unwrap();
    w.set_fog_disabled(true);
    w.set_limits(crate::WorldLimits::for_match(100, 1_000));
    for seat in 0..100_u16 {
        let p = PlayerId(seat);
        let (x, y) = (
            i32::from(seat % 10) * 400 + 100,
            i32::from(seat / 10) * 400 + 100,
        );
        let footprint = aoe_core::TileRect::from_xywh(x, y, 4, 4);
        let castle = w
            .insert_building(p, BuildingKind::Castle, footprint, true)
            .unwrap();
        let economy = w.economy.players.get_mut(&p).unwrap();
        economy.age = Age::Castle;
        economy.stockpile = Stockpile {
            food: 10_000,
            wood: 10_000,
            gold: 10_000,
            stone: 10_000,
        };
        for raw in 0..999 {
            w.spawn_villager(
                p,
                WorldPosition::from_tile_center(TileCoord::new(
                    x - 35 + raw % 30,
                    y - 35 + raw / 30,
                ))
                .unwrap(),
            )
            .unwrap();
        }
        let id = w
            .spawn_kind(
                p,
                UnitKind::Vassal,
                WorldPosition::from_tile_center(TileCoord::new(x - 1, y)).unwrap(),
            )
            .unwrap();
        w.bind_vassal(id, castle, footprint);
        w.issue_garrison(p, id, castle).unwrap();
    }
    w.advance_garrisons(&mut Vec::new());
    assert_eq!(
        w.unit_count() + w.buildings().map(|b| b.garrison.len()).sum::<usize>(),
        100_000
    );
    w.tick = aoe_core::Tick(VASSAL_RULES.period_ticks);
    let started = std::time::Instant::now();
    w.advance_vassals(&mut Vec::new());
    let cost = w.vassal_work();
    eprintln!(
        "100 players, 100000 population, 100 Vassals: manager {:?}, {:?}",
        cost,
        started.elapsed()
    );
    assert_eq!(cost.domains, VASSAL_RULES.domains_per_pass);
    assert!(
        cost.unit_visits <= VASSAL_RULES.domains_per_pass * VASSAL_RULES.unit_visits_per_domain
    );
    assert!(
        cost.building_visits
            <= VASSAL_RULES.domains_per_pass * VASSAL_RULES.building_visits_per_domain
    );
    assert!(cost.orders <= VASSAL_RULES.domains_per_pass * VASSAL_RULES.orders_per_domain);
    assert!(
        cost.resource_searches
            <= VASSAL_RULES.domains_per_pass * VASSAL_RULES.resource_searches_per_domain
    );
    assert!(
        cost.site_checks <= VASSAL_RULES.domains_per_pass * VASSAL_RULES.site_checks_per_domain
    );
    w.tick.0 += 1;
    w.advance_vassals(&mut Vec::new());
    assert_eq!(w.vassal_work().domains, 0, "not an every-tick AI");
}

#[test]
fn local_placement_keeps_farms_arches_and_live_units_reserved() {
    let (mut w, _) = setup();
    let at = TileCoord::new(140, 140);
    let farm = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Farm,
            aoe_core::TileRect::from_xywh(at.x, at.y, 3, 3),
            true,
        )
        .unwrap();
    assert_eq!(
        w.validate_building_placement(BuildingKind::House, at),
        Err(EconomyError::InvalidFootprint)
    );
    w.remove_building(farm);
    let worker = w
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(at).unwrap())
        .unwrap();
    // Units step aside for ordered foundations; only the AI's own site
    // search (`vassal_build_near`) keeps its people's ground reserved.
    assert!(
        w.validate_building_placement(BuildingKind::House, at)
            .is_ok()
    );
    w.remove_unit(worker);
    assert!(
        w.validate_building_placement(BuildingKind::House, at)
            .is_ok()
    );
    let tc = w
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .next()
        .unwrap()
        .footprint;
    let arch = TileCoord::new(tc.max.x - 1, tc.max.y - 1);
    assert!(w.tile_passable(arch));
    assert_eq!(
        w.validate_building_placement(BuildingKind::House, arch),
        Err(EconomyError::InvalidFootprint)
    );
}

#[test]
fn farm_foundation_stays_near_a_food_dropoff_when_worker_is_far() {
    let (mut w, _, _) = active();
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    for worker in workers {
        w.remove_unit(worker);
    }
    w.insert_building(
        PlayerId(0),
        BuildingKind::Mill,
        aoe_core::TileRect::from_xywh(105, 105, 2, 2),
        true,
    )
    .unwrap();
    w.spawn_villager(
        PlayerId(0),
        WorldPosition::from_tile_center(TileCoord::new(130, 105)).unwrap(),
    )
    .unwrap();
    pass(&mut w);
    let farm = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .next()
        .unwrap();
    let mill = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .next()
        .unwrap();
    let point = farm.footprint.min;
    let dx = point.x
        - point
            .x
            .clamp(mill.footprint.min.x - 1, mill.footprint.max.x);
    let dy = point.y
        - point
            .y
            .clamp(mill.footprint.min.y - 1, mill.footprint.max.y);
    let distance = dx * dx + dy * dy;
    assert!(
        distance <= 16,
        "farm squared distance from mill ring: {distance}"
    );
}

#[test]
fn nearest_unfinished_building_wins_over_lower_id() {
    let (mut w, _, _) = active();
    let workers: Vec<_> = w
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    for worker in workers {
        w.remove_unit(worker);
    }
    let far = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(120, 120, 2, 2),
            false,
        )
        .unwrap();
    let near = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(100, 100, 2, 2),
            false,
        )
        .unwrap();
    let worker = w
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(97, 100)).unwrap(),
        )
        .unwrap();
    pass(&mut w);
    assert_eq!(
        w.economy.builders.get(&worker).unwrap().building,
        near,
        "near is 2 tiles away; far is over 20 tiles away (id {})",
        far.0
    );
}
