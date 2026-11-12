//! Timer work must preserve every intermediate observation, not just completion.
use super::*;

fn scenario(sleep: bool) -> (GameWorld, EntityId, [EntityId; 2]) {
    let (mut w, _, _) = village();
    w.set_work_sleep(sleep);
    set_stock(&mut w, 10_000, 10_000, 10_000, 10_000);
    let workers = [villager_at(&mut w, 99, 100), villager_at(&mut w, 102, 100)];
    let house = w
        .issue_build(
            PlayerId(0),
            &workers[..1],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    (w, house, workers)
}

#[test]
fn construction_timers_preserve_join_leave_damage_cancel_and_observations() {
    let (mut on, house, workers) = scenario(true);
    let (mut off, _, _) = scenario(false);
    for tick in 0..650 {
        for w in [&mut on, &mut off] {
            match tick {
                80 => w
                    .issue_construct(PlayerId(0), &workers[1..], house)
                    .unwrap(),
                160 => w.issue_stop(PlayerId(0), workers[0]).unwrap(),
                220 => {
                    // A test mutation supplies the same wake as combat damage.
                    w.wake_construction(house);
                    w.economy.buildings.get_mut(&house).unwrap().hp -= 20;
                }
                280 => w
                    .issue_construct(PlayerId(0), &workers[..1], house)
                    .unwrap(),
                350 => w.issue_cancel_construction(PlayerId(0), house).unwrap(),
                _ => {}
            }
            w.advance();
        }
        assert_eq!(
            on.building(house),
            off.building(house),
            "building at {tick}"
        );
        assert_eq!(
            on.known_buildings(PlayerId(0)),
            off.known_buildings(PlayerId(0)),
            "memory at {tick}"
        );
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "hash at {tick}"
        );
        if tick % 37 == 0 {
            let a = on.snapshot([0; 32]).unwrap();
            let b = off.snapshot([0; 32]).unwrap();
            assert_eq!(a.buildings, b.buildings, "saved progress at {tick}");
        }
    }
    assert!(on.economy.tick_work.builder_slept > 100);
    assert_eq!(off.economy.tick_work.builder_slept, 0);
}

#[test]
fn construction_timers_complete_on_the_same_tick() {
    let (mut on, house, _) = scenario(true);
    let (mut off, _, _) = scenario(false);
    for tick in 0..550 {
        on.advance();
        off.advance();
        assert_eq!(
            on.building(house),
            off.building(house),
            "building at {tick}"
        );
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "hash at {tick}"
        );
    }
    assert!(on.building(house).unwrap().is_complete());
    assert!(on.economy.tick_work.builder_slept > 400);
}

#[test]
#[should_panic(expected = "missed wake: builder")]
fn construction_detector_rejects_unannounced_damage() {
    let (mut w, house, _) = scenario(true);
    for _ in 0..20 {
        w.advance();
    }
    w.economy.buildings.get_mut(&house).unwrap().hp -= 1;
    w.advance();
}

#[test]
fn repair_timers_preserve_resource_bills_and_release() {
    let (mut on, house, workers) = scenario(true);
    let (mut off, _, _) = scenario(false);
    for _ in 0..550 {
        on.advance();
        off.advance();
    }
    let before = on.construction_work();
    for w in [&mut on, &mut off] {
        w.economy.buildings.get_mut(&house).unwrap().hp -= 100;
        set_stock(w, 0, 1, 0, 0);
        w.issue_construct(PlayerId(0), &workers, house).unwrap();
    }
    for tick in 0..220 {
        for w in [&mut on, &mut off] {
            if tick == 80 {
                set_stock(w, 10_000, 10_000, 10_000, 10_000);
            }
            w.advance();
        }
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "repair at {tick}"
        );
    }
    assert_eq!(
        on.building(house).unwrap().hp,
        on.building(house).unwrap().max_hp
    );
    assert!(on.construction_work().slept - before.slept > 100);
    assert!(on.construction_work().visits - before.visits < 20);
}

#[test]
fn walking_builders_wait_for_movement_events() {
    let setup = |sleep| {
        let (mut w, _, _) = village();
        w.set_work_sleep(sleep);
        fixture::survey(&mut w, PlayerId(0), at(100, 100), 8);
        let worker = villager_at(&mut w, 20, 20);
        w.issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
        w
    };
    let (mut on, mut off) = (setup(true), setup(false));
    for tick in 0..80 {
        on.advance();
        off.advance();
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "walk at {tick}"
        );
    }
    assert!(on.construction_work().slept > 70);
    assert!(on.construction_work().visits < 5);
    assert_eq!(off.construction_work().visits, 80);
}

#[test]
fn completed_farm_retry_timers_preserve_hashes_until_space_opens() {
    let setup = |sleep| {
        let (mut w, _, _, farm) = super::super::farming::mill_and_farm();
        w.set_work_sleep(sleep);
        let rect = w.building(farm).unwrap().footprint;
        let blockers = crate::game_world::movement_step::slots::perimeter_positions(
            TileRect::from_xywh(rect.min.x + 1, rect.min.y + 1, 1, 1),
            UnitKind::Villager,
        )
        .into_iter()
        .map(|at| w.spawn_villager(PlayerId(1), at).unwrap())
        .collect::<Vec<_>>();
        (w, blockers)
    };
    let (mut on, blockers) = setup(true);
    let (mut off, _) = setup(false);
    for tick in 0..1000 {
        for w in [&mut on, &mut off] {
            if tick == 750 {
                for &id in &blockers {
                    w.remove_unit(id);
                }
            }
            w.advance();
        }
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "farm retry at {tick}"
        );
    }
    assert!(on.construction_work().visits < off.construction_work().visits / 2);
}
