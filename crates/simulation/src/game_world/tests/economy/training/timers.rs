//! Completion timers preserve queue observations and every external wake.
use super::super::buildings::set_stock;
use super::*;

fn setup(sleep: bool) -> (GameWorld, EntityId, EntityId) {
    let (mut w, _, center) = village();
    w.set_work_sleep(sleep);
    set_stock(&mut w, 10_000, 10_000, 10_000, 10_000);
    let house = w
        .insert_building(
            PlayerId(0),
            BuildingKind::House,
            TileRect::from_xywh(20, 20, 2, 2),
            true,
        )
        .unwrap();
    (w, center, house)
}

fn same(on: &GameWorld, off: &GameWorld, tick: usize) {
    assert_eq!(
        on.canonical_hash_hex(),
        off.canonical_hash_hex(),
        "queue hash at {tick}"
    );
    assert_eq!(
        on.known_buildings(PlayerId(0)),
        off.known_buildings(PlayerId(0)),
        "queue memory at {tick}"
    );
    if tick.is_multiple_of(41) {
        assert_eq!(
            on.snapshot([0; 32]).unwrap().buildings,
            off.snapshot([0; 32]).unwrap().buildings,
            "saved queues at {tick}"
        );
    }
}

#[test]
fn training_timers_project_progress_through_queue_append_and_cancellation() {
    let (mut on, center, _) = setup(true);
    let (mut off, _, _) = setup(false);
    let mut visits = [0; 2];
    for tick in 0..900 {
        for (n, w) in [&mut on, &mut off].into_iter().enumerate() {
            match tick {
                0 => {
                    w.issue_train(PlayerId(0), center, UnitKind::Villager)
                        .unwrap();
                    w.issue_train(PlayerId(0), center, UnitKind::Villager)
                        .unwrap();
                }
                120 => w.issue_cancel_training(PlayerId(0), center).unwrap(),
                260 => w
                    .issue_train(PlayerId(0), center, UnitKind::Villager)
                    .unwrap(),
                _ => {}
            }
            w.advance();
            visits[n] += w.economy.tick_work.training_buildings;
        }
        same(&on, &off, tick);
        assert_eq!(
            on.building(center),
            off.building(center),
            "training at {tick}"
        );
    }
    assert!(visits[0] < 16, "a running queue must sleep: {visits:?}");
    assert_eq!(visits[1], 900);
}

#[test]
fn training_timers_wake_on_housing_population_and_building_loss() {
    let (mut on, center, house) = setup(true);
    let (mut off, _, _) = setup(false);
    let mut extra = None;
    let mut visits = [0; 2];
    for tick in 0..1100 {
        for (n, w) in [&mut on, &mut off].into_iter().enumerate() {
            match tick {
                0 => {
                    w.issue_train(PlayerId(0), center, UnitKind::Villager)
                        .unwrap();
                    w.issue_train(PlayerId(0), center, UnitKind::Villager)
                        .unwrap();
                }
                90 => {
                    extra = Some(
                        w.spawn_villager(
                            PlayerId(0),
                            WorldPosition::from_tile_center(TileCoord::new(30, 30)).unwrap(),
                        )
                        .unwrap(),
                    )
                }
                100 => w.remove_building(house),
                220 => {
                    let id = extra.unwrap();
                    w.kill_unit(w.lookup[&id]);
                    w.remove_unit(id);
                }
                430 => w.issue_cancel_training(PlayerId(0), center).unwrap(),
                600 => {
                    w.insert_building(
                        PlayerId(0),
                        BuildingKind::House,
                        TileRect::from_xywh(20, 20, 2, 2),
                        true,
                    )
                    .unwrap();
                }
                700 => w.remove_building(center),
                _ => {}
            }
            w.advance();
            visits[n] += w.economy.tick_work.training_buildings;
        }
        same(&on, &off, tick);
        if (100..220).contains(&tick) {
            assert_eq!(
                on.building(center).unwrap().training_queue[0].progress_ticks,
                100
            );
        }
    }
    assert!(
        visits[0] < 24,
        "population-blocked queues must sleep: {visits:?}"
    );
    assert!(on.building(center).is_none());
}

#[test]
fn research_timers_preserve_cancellation_restart_and_completion() {
    let (mut on, center, _) = setup(true);
    let (mut off, _, _) = setup(false);
    let mut visits = [0; 2];
    for tick in 0..1000 {
        for (n, w) in [&mut on, &mut off].into_iter().enumerate() {
            match tick {
                0 | 150 => w
                    .issue_research(PlayerId(0), center, crate::Tech::Loom)
                    .unwrap(),
                100 => w.issue_cancel_research(PlayerId(0), center).unwrap(),
                700 => {
                    w.complete_research(PlayerId(0), crate::Tech::FeudalAge, &mut Vec::new());
                    w.issue_research(PlayerId(0), center, crate::Tech::TownWatch)
                        .unwrap();
                }
                _ => {}
            }
            w.advance();
            visits[n] += w.economy.tick_work.research_buildings;
        }
        same(&on, &off, tick);
        assert_eq!(
            on.building(center),
            off.building(center),
            "research at {tick}"
        );
    }
    assert!(on.has_researched(PlayerId(0), crate::Tech::Loom));
    assert!(visits[0] < 16, "research must sleep: {visits:?}");
    assert!(visits[1] > 800);
}

#[test]
fn a_completed_queue_waits_for_an_exit_movement_event() {
    let setup_blocked = |sleep| {
        let (mut w, center, _) = setup(sleep);
        let rect = w.building(center).unwrap().footprint;
        let mut blockers = Vec::new();
        for tile in crate::game_world::economy::placement::perimeter(rect) {
            if !w.units().any(|u| u.position.tile_floor() == tile) {
                blockers.push(
                    w.spawn_unit(PlayerId(1), WorldPosition::from_tile_center(tile).unwrap())
                        .unwrap(),
                );
            }
        }
        for unit in &mut w.units {
            unit.state.stance = aoe_core::CombatStance::NoAttack;
        }
        w.issue_train(PlayerId(0), center, UnitKind::Villager)
            .unwrap();
        (w, center, blockers)
    };
    let (mut on, center, blockers) = setup_blocked(true);
    let (mut off, _, _) = setup_blocked(false);
    let mut visits = [0; 2];
    for tick in 0..800 {
        for (n, w) in [&mut on, &mut off].into_iter().enumerate() {
            if tick == 650 {
                w.remove_unit(blockers[0]);
            }
            w.advance();
            visits[n] += w.economy.tick_work.training_buildings;
        }
        same(&on, &off, tick);
        if tick == 600 {
            assert_eq!(
                on.building(center).unwrap().training_queue[0].progress_ticks,
                500
            );
        }
    }
    assert!(on.building(center).unwrap().training_queue.is_empty());
    assert!(visits[0] < 16, "blocked spawn must sleep: {visits:?}");
}

#[test]
fn a_higher_id_completion_preserves_the_lower_ids_credit_before_population_fills() {
    let setup_two = |sleep| {
        let (mut w, first, _) = setup(sleep);
        let second = w
            .insert_building(
                PlayerId(0),
                BuildingKind::TownCenter,
                TileRect::from_xywh(100, 130, 4, 4),
                true,
            )
            .unwrap();
        let workers = (0..10)
            .map(|n| {
                w.spawn_villager(
                    PlayerId(0),
                    WorldPosition::from_tile_center(TileCoord::new(30 + n, 35)).unwrap(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        w.issue_train(PlayerId(0), second, UnitKind::Villager)
            .unwrap();
        (w, first, workers[0])
    };
    let (mut on, first, worker) = setup_two(true);
    let (mut off, _, _) = setup_two(false);
    for tick in 0..850 {
        for w in [&mut on, &mut off] {
            if tick == 100 {
                w.issue_train(PlayerId(0), first, UnitKind::Villager)
                    .unwrap();
            }
            if tick == 650 {
                w.kill_unit(w.lookup[&worker]);
            }
            w.advance();
        }
        same(&on, &off, tick);
        if tick == 500 {
            assert_eq!(
                on.building(first).unwrap().training_queue[0].progress_ticks,
                400
            );
        }
    }
    assert!(on.building(first).unwrap().training_queue.is_empty());
}

#[test]
#[should_panic(expected = "missed wake: queue")]
fn queue_detector_rejects_an_unannounced_progress_edit() {
    let (mut w, center, _) = setup(true);
    w.issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut w, 10);
    w.economy.buildings.get_mut(&center).unwrap().training_queue[0].progress_ticks += 1;
    w.advance();
}
