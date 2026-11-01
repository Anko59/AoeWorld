use super::*;
use std::collections::{BTreeMap, BTreeSet};
mod repair;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

/// Empty land with wood already paid for, no TC producing extra residents,
/// and enough housing that every worker can concentrate on farming.
fn farmland(count: i32) -> (GameWorld, EntityId, Vec<EntityId>) {
    let mut w = world();
    w.set_fog_disabled(true);
    let footprint = aoe_core::TileRect::from_xywh(90, 90, 4, 4);
    let castle = w
        .insert_building(PlayerId(0), BuildingKind::Castle, footprint, true)
        .unwrap();
    w.insert_building(
        PlayerId(0),
        BuildingKind::Mill,
        aoe_core::TileRect::from_xywh(105, 105, 2, 2),
        true,
    )
    .unwrap();
    for n in 0..10 {
        w.insert_building(
            PlayerId(0),
            BuildingKind::House,
            aoe_core::TileRect::from_xywh(10 + n * 3, 10, 2, 2),
            true,
        )
        .unwrap();
    }
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 10_000,
        wood: 50_000,
        gold: 10_000,
        stone: 10_000,
    };
    let vassal = w
        .spawn_kind(PlayerId(0), UnitKind::Vassal, at(89, 90))
        .unwrap();
    w.bind_vassal(vassal, castle, footprint);
    let workers = (0..count)
        .map(|n| {
            w.spawn_villager(PlayerId(0), at(115 + (n % 8) * 2, 115 + (n / 8) * 2))
                .unwrap()
        })
        .collect();
    w.issue_garrison(PlayerId(0), vassal, castle).unwrap();
    w.advance_garrisons(&mut Vec::new());
    assert!(w.unit(vassal).is_none());
    (w, vassal, workers)
}

fn farm(w: &mut GameWorld, x: i32, y: i32, complete: bool) -> EntityId {
    w.insert_building(
        PlayerId(0),
        BuildingKind::Farm,
        aoe_core::TileRect::from_xywh(x, y, 3, 3),
        complete,
    )
    .unwrap()
}

fn worker_farm(w: &GameWorld, worker: EntityId) -> Option<EntityId> {
    let tile = w.gather_tile(worker)?;
    w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .find(|b| b.footprint.contains(tile))
        .map(|b| b.id)
}

fn farming_diagnostics(w: &GameWorld, workers: &[EntityId]) -> String {
    use crate::game_world::economy::WorkTarget;
    let unassigned: Vec<_> = workers
        .iter()
        .filter(|id| worker_farm(w, **id).is_none())
        .map(|id| {
            let u = w.unit(*id).unwrap();
            (
                *id,
                u.task,
                u.position,
                w.gather_tile(*id),
                w.gathering_kind(*id),
                w.economy.builders.get(id).map(|job| {
                    (
                        job.building,
                        w.building(job.building).map(|b| {
                            (
                                b.footprint,
                                b.state,
                                b.work_done,
                                b.work_total,
                                b.food_remaining,
                            )
                        }),
                    )
                }),
            )
        })
        .collect();
    let mills: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Mill)
        .filter(|b| b.is_complete())
        .map(|b| (b.id, b.footprint.min))
        .collect();
    let farms = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .count();
    let orphaned: Vec<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .filter(|b| !workers.iter().any(|id| worker_farm(w, *id) == Some(b.id)))
        .map(|b| {
            let builders: Vec<_> = w
                .economy
                .builders
                .iter()
                .filter(|(_, job)| job.building == b.id)
                .map(|(id, _)| *id)
                .collect();
            (
                b.id,
                b.footprint,
                b.state,
                b.food_remaining,
                w.economy.gatherers.count_target(WorkTarget::Farm(b.id)),
                builders,
            )
        })
        .collect();
    format!(
        "unassigned (id,task,position,gather tile,kind,builder target): {unassigned:?}; farms: {farms}; completed Mills: {mills:?}; Farms without assigned farmer (id,footprint,state,food,reservations,builders): {orphaned:?}"
    )
}

fn assert_farm_reservations(w: &GameWorld, workers: &[EntityId]) {
    use crate::game_world::economy::WorkTarget;
    for farm in w.player_buildings_of(PlayerId(0), BuildingKind::Farm) {
        let assigned = workers
            .iter()
            .filter(|id| worker_farm(w, **id) == Some(farm.id))
            .count();
        assert_eq!(
            w.economy.gatherers.count_target(WorkTarget::Farm(farm.id)),
            assigned,
            "farm {:?} live reservation count must match its farmers",
            farm.id
        );
    }
}

#[test]
fn farming_showcase_starts_with_twenty_four_workers_and_empty_farmland() {
    let (mut w, primary) = GameWorld::vassal_farming_demo(Seed(7)).unwrap();
    assert_eq!(w.config().tick_hz, 1);
    assert_eq!(w.unit(primary).unwrap().kind, UnitKind::Scout);
    assert_eq!(
        w.units().filter(|u| u.kind == UnitKind::Villager).count(),
        24
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
            .count(),
        0,
        "the capture must not add newly trained villagers"
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
            .count(),
        0,
        "the manager must visibly create every farm"
    );
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Mill)
            .any(|b| b.is_complete())
    );
    let p = w.player_economy(PlayerId(0)).unwrap();
    assert!(p.population_cap >= p.population_used + 3);
    assert!(
        p.stockpile.wood
            >= 24 * BuildingKind::Farm.rules().cost.wood + 3 * BuildingKind::Mill.rules().cost.wood
    );
    let vassal = w.units().find(|u| u.kind == UnitKind::Vassal).unwrap().id;
    advance(&mut w, 2);
    assert!(
        w.unit(vassal).is_none(),
        "the Vassal enters its adjacent Castle"
    );
    assert!(w.vassal_status(vassal).is_some(), "the domain is active");
}

#[test]
fn farmland_only_domain_puts_every_worker_on_a_distinct_attended_farm() {
    let (mut w, vassal, workers) = farmland(40);
    let mut settled = false;
    let mut observed = BTreeMap::new();
    for tick in 0..20_000 {
        w.advance();
        for worker in &workers {
            let u = w.unit(*worker).unwrap();
            if let Some(farm) = worker_farm(&w, *worker)
                && u.task == UnitTask::Farming
                && w.building(farm)
                    .unwrap()
                    .footprint
                    .contains(u.position.tile_floor())
            {
                observed.insert(*worker, farm);
            }
        }
        if tick > 2_000
            && workers.iter().all(|id| {
                w.vassal_controller(*id) == Some(vassal)
                    && worker_farm(&w, *id).is_some_and(|farm| observed.get(id) == Some(&farm))
                    && w.unit(*id).unwrap().task != UnitTask::Idle
            })
            && w.player_economy(PlayerId(0)).unwrap().stockpile.food > 10_000
            && w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
                .all(|b| b.is_complete())
        {
            settled = true;
            break;
        }
    }
    let assigned: BTreeSet<_> = workers
        .iter()
        .filter_map(|id| worker_farm(&w, *id))
        .collect();
    assert!(
        settled,
        "only {}/{} workers have distinct farm jobs; idle: {}; productive observations: {observed:?}; food stock: {}; {}",
        assigned.len(),
        workers.len(),
        workers
            .iter()
            .filter(|id| w.unit(**id).unwrap().task == UnitTask::Idle)
            .count(),
        w.player_economy(PlayerId(0)).unwrap().stockpile.food,
        farming_diagnostics(&w, &workers)
    );
    assert_eq!(
        assigned.len(),
        workers.len(),
        "{}",
        farming_diagnostics(&w, &workers)
    );
    let farms: BTreeSet<_> = w
        .player_buildings_of(PlayerId(0), BuildingKind::Farm)
        .map(|b| b.id)
        .collect();
    assert_eq!(farms, assigned, "no farm may be left unattended");
    let plots: Vec<_> = assigned.iter().map(|id| w.building(*id).unwrap()).collect();
    let adjacent_pairs = plots
        .iter()
        .enumerate()
        .map(|(i, a)| {
            plots[i + 1..]
                .iter()
                .filter(|b| {
                    let (a, b) = (a.footprint, b.footprint);
                    ((a.max.x == b.min.x || b.max.x == a.min.x)
                        && a.min.y < b.max.y
                        && b.min.y < a.max.y)
                        || ((a.max.y == b.min.y || b.max.y == a.min.y)
                            && a.min.x < b.max.x
                            && b.min.x < a.max.x)
                })
                .count()
        })
        .sum::<usize>();
    assert!(
        adjacent_pairs >= workers.len() / 2,
        "farms must tile together: only {adjacent_pairs} pairs share an edge"
    );
    for id in assigned {
        let b = w.building(id).unwrap();
        let center = TileCoord::new(b.footprint.min.x + 1, b.footprint.min.y + 1);
        let dropoff = w
            .nearest_dropoff(PlayerId(0), center, ResourceKind::Food)
            .unwrap();
        let rect = w.building(dropoff).unwrap().footprint;
        let dx = center.x - center.x.clamp(rect.min.x - 1, rect.max.x);
        let dy = center.y - center.y.clamp(rect.min.y - 1, rect.max.y);
        assert!(
            dx * dx + dy * dy <= 36,
            "farm {id:?} is too far from food drop-off"
        );
    }
    assert!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Mill)
            .filter(|b| b.is_complete())
            .count()
            >= 2,
        "a full cluster must expand with another nearby Mill"
    );
}

#[test]
fn two_idle_farmers_reclaim_two_free_farms_in_one_pass_without_new_foundations() {
    let (mut w, vassal, workers) = farmland(2);
    // Keep food first for both assignments, isolating live farm reservations
    // from the separate resource-search starvation covered by the large test.
    w.economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile
        .food = 0;
    let a = farm(&mut w, 108, 105, true);
    let b = farm(&mut w, 108, 108, true);
    w.issue_farm(PlayerId(0), workers[0], a).unwrap();
    w.issue_farm(PlayerId(0), workers[1], b).unwrap();
    for worker in &workers {
        w.issue_stop(PlayerId(0), *worker).unwrap();
    }
    super::manager::pass(&mut w);
    let assigned: BTreeSet<_> = workers
        .iter()
        .filter_map(|id| worker_farm(&w, *id))
        .collect();
    assert_eq!(assigned, BTreeSet::from([a, b]));
    assert!(
        workers
            .iter()
            .all(|id| w.vassal_controller(*id) == Some(vassal))
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
            .count(),
        2
    );
}

#[test]
fn an_attended_farm_foundation_does_not_take_a_worker_from_an_abandoned_farm() {
    let (mut w, _, workers) = farmland(2);
    let unfinished = farm(&mut w, 108, 105, false);
    let abandoned = farm(&mut w, 108, 108, true);
    w.issue_construct(PlayerId(0), &[workers[0]], unfinished)
        .unwrap();
    super::manager::pass(&mut w);
    assert_eq!(worker_farm(&w, workers[1]), Some(abandoned));
    assert_eq!(
        w.economy
            .builders
            .values()
            .filter(|b| b.building == unfinished)
            .count(),
        1,
        "one farm needs one future farmer"
    );
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
            .count(),
        2
    );
}

#[test]
fn managed_farmer_reseeds_the_same_plot_without_leaving_an_extra_farm() {
    let (mut w, vassal, workers) = farmland(1);
    let plot = farm(&mut w, 108, 105, true);
    w.issue_farm(PlayerId(0), workers[0], plot).unwrap();
    advance(&mut w, 600);
    assert_eq!(w.vassal_controller(workers[0]), Some(vassal));
    let wood = w.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    w.economy.buildings.get_mut(&plot).unwrap().food_remaining = Some(1);
    let mut rebuilding = false;
    for _ in 0..3_000 {
        w.advance();
        let building = w.building(plot).expect("prepaid plot survives reseeding");
        rebuilding |= !building.is_complete();
        if rebuilding && building.is_complete() && worker_farm(&w, workers[0]) == Some(plot) {
            assert!(building.food_remaining.unwrap() > 1);
            assert_eq!(w.vassal_controller(workers[0]), Some(vassal));
            assert_eq!(
                w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
                    .count(),
                1
            );
            // Reseeding consumes a prepaid entry; replacement payment occurs
            // on the next bounded manager pass rather than at completion.
            super::manager::pass(&mut w);
            assert!(w.player_economy(PlayerId(0)).unwrap().stockpile.wood < wood);
            return;
        }
    }
    panic!("manager must refill reseed queue and keep its farmer on the same plot");
}

#[test]
fn farmland_expansion_and_abandoned_farm_reclaims_replay_deterministically() {
    let (mut a, vassal, workers) = farmland(12);
    let (mut b, replay_vassal, replay_workers) = farmland(12);
    assert_eq!(vassal, replay_vassal);
    assert_eq!(workers, replay_workers);
    for tick in 0..4_000 {
        if tick == 2_000 {
            for w in [&mut a, &mut b] {
                for worker in &workers[..2] {
                    assert!(worker_farm(w, *worker).is_some());
                    w.issue_stop(PlayerId(0), *worker).unwrap();
                    assert_eq!(w.vassal_controller(*worker), None);
                }
            }
            assert_eq!(a.canonical_hash(), b.canonical_hash());
        }
        a.advance();
        b.advance();
        if (tick + 1) % 20 == 0 {
            assert_eq!(a.canonical_hash(), b.canonical_hash(), "tick {tick}");
            assert_eq!(a.vassal_work(), b.vassal_work());
            assert_farm_reservations(&a, &workers);
            assert_farm_reservations(&b, &replay_workers);
        }
    }
    assert_eq!(a.canonical_hash(), b.canonical_hash());
    let assigned: BTreeSet<_> = workers
        .iter()
        .filter_map(|id| worker_farm(&a, *id))
        .collect();
    assert_eq!(
        assigned.len(),
        workers.len(),
        "{}",
        farming_diagnostics(&a, &workers)
    );
    assert!(
        workers
            .iter()
            .all(|id| a.vassal_controller(*id) == Some(vassal))
    );
    assert_eq!(
        a.player_buildings_of(PlayerId(0), BuildingKind::Farm)
            .count(),
        12
    );
    assert!(
        a.player_buildings_of(PlayerId(0), BuildingKind::Mill)
            .filter(|b| b.is_complete())
            .count()
            >= 2,
        "replay must include expansion beyond the first Mill"
    );
}

#[test]
fn an_adopted_farmer_gets_a_nearby_dropoff_without_abandoning_its_plot() {
    let (mut w, vassal, workers) = farmland(1);
    let plot = farm(&mut w, 135, 105, true);
    w.issue_farm(PlayerId(0), workers[0], plot).unwrap();
    advance(&mut w, 2_000);
    assert_eq!(w.vassal_controller(workers[0]), Some(vassal));
    assert_eq!(worker_farm(&w, workers[0]), Some(plot));
    assert_eq!(
        w.player_buildings_of(PlayerId(0), BuildingKind::Farm)
            .count(),
        1,
        "keep the already paid-for plot"
    );
    let rect = w.building(plot).unwrap().footprint;
    let center = TileCoord::new(rect.min.x + 1, rect.min.y + 1);
    let dropoff = w
        .nearest_dropoff(PlayerId(0), center, ResourceKind::Food)
        .unwrap();
    let rect = w.building(dropoff).unwrap().footprint;
    let dx = center.x - center.x.clamp(rect.min.x - 1, rect.max.x);
    let dy = center.y - center.y.clamp(rect.min.y - 1, rect.max.y);
    assert!(
        dx * dx + dy * dy <= 36,
        "the adopted farmer must not keep hauling to a distant Mill"
    );
}
