//! A bulk load counts each owner's population space once, at its end, and
//! looks at no training queue: a count per building reads every house again,
//! the square of a town's size.
use super::*;
use crate::{Age, BuildingKind, EconomyError, LateGameSeat};
use aoe_core::TileRect;

const OWNERS: [PlayerId; 2] = [PlayerId(0), PlayerId(1)];

/// House `n` of `owner`'s town, finished or a foundation.
fn place(world: &mut GameWorld, owner: PlayerId, n: i32, finished: bool) {
    let rect = TileRect::from_xywh(
        4 + 3 * (n % 32),
        4 + 3 * (n / 32) + 100 * i32::from(owner.0),
        2,
        2,
    );
    world
        .insert_building(owner, BuildingKind::House, rect, finished)
        .unwrap();
}

fn space(world: &GameWorld, owner: PlayerId) -> u32 {
    world.player_economy(owner).unwrap().population_cap
}

/// Two towns of `houses` finished houses and one foundation each.
fn towns(houses: i32) -> GameWorld {
    let mut saved = world();
    for owner in OWNERS {
        for n in 0..=houses {
            place(&mut saved, owner, n, n < houses);
        }
    }
    saved
}

#[test]
fn a_restore_counts_each_owners_housing_once() {
    let reads = |houses: i32| {
        let (restored, report) = roundtrip(&towns(houses), world());
        assert_eq!(report.buildings_lost, 0, "{report:?}");
        restored.economy.tick_work.housing_reads
    };
    // Each owner's houses and its foundation, read once.
    assert_eq!(reads(16), 2 * 17);
    assert_eq!(reads(64), 2 * 65);
}

/// A restore writes the saved economy of a listed player over the count, so
/// only an owner the save does not list shows what was counted.
#[test]
fn an_owner_the_save_does_not_list_still_gets_its_housing_counted() {
    let mut snapshot = towns(7).snapshot([0; 32]).unwrap();
    snapshot.players.clear();
    let mut restored = world();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    for owner in OWNERS {
        assert_eq!(space(&restored, owner), 35, "a foundation houses nobody");
    }
}

/// Buildings placed after a bulk load are counted at once again.
#[test]
fn a_house_finished_after_a_restore_adds_its_space_at_once() {
    let (mut restored, _) = roundtrip(&towns(3), world());
    place(&mut restored, OWNERS[0], 10, true);
    assert_eq!(space(&restored, OWNERS[0]), 20);
}

#[test]
fn a_seeded_seat_counts_its_housing_once() {
    let (mut seeded, scout) = GameWorld::default_with_village(aoe_core::Seed(7));
    let center = seeded.unit(scout).unwrap().position.tile_floor();
    let before = seeded.economy.tick_work.housing_reads;
    let seat = LateGameSeat {
        age: Age::Castle,
        buildings: vec![
            (BuildingKind::House, 24),
            (BuildingKind::Farm, 6),
            (BuildingKind::Castle, 1),
        ],
        wall_tiles: 8,
        radius: 60,
        ..LateGameSeat::default()
    };
    let placed = seeded.seed_late_game(OWNERS[0], center, &seat).unwrap();
    assert_eq!(placed.buildings, 31);
    // The village's Town Center, the houses and the Castle, read once.
    assert_eq!(seeded.economy.tick_work.housing_reads - before, 1 + 24 + 1);
    assert_eq!(space(&seeded, OWNERS[0]), 5 + 24 * 5 + 20);
}

/// A bridge on a single tile is refused, after the houses were placed.
#[test]
fn a_seat_that_fails_half_way_counts_what_it_placed_once() {
    let (mut seeded, scout) = GameWorld::default_with_village(aoe_core::Seed(7));
    let center = seeded.unit(scout).unwrap().position.tile_floor();
    let before = seeded.economy.tick_work.housing_reads;
    let seat = LateGameSeat {
        buildings: vec![(BuildingKind::House, 3), (BuildingKind::Bridge, 1)],
        radius: 40,
        ..LateGameSeat::default()
    };
    assert_eq!(
        seeded.seed_late_game(OWNERS[0], center, &seat),
        Err(EconomyError::InvalidFootprint)
    );
    assert_eq!(seeded.economy.tick_work.housing_reads - before, 1 + 3);
    assert_eq!(space(&seeded, OWNERS[0]), 5 + 3 * 5);
}

#[test]
fn a_seat_that_places_nothing_is_still_counted() {
    let (mut seeded, scout) = GameWorld::default_with_village(aoe_core::Seed(7));
    let center = seeded.unit(scout).unwrap().position.tile_floor();
    seeded
        .economy
        .players
        .get_mut(&OWNERS[0])
        .unwrap()
        .population_cap = 99;
    let seat = LateGameSeat::default();
    seeded.seed_late_game(OWNERS[0], center, &seat).unwrap();
    assert_eq!(space(&seeded, OWNERS[0]), 5);
}

#[test]
fn a_load_inside_a_load_leaves_its_owners_to_the_outer_one() {
    let mut world = world();
    world.count_housing_once(|world| {
        place(world, OWNERS[0], 0, true);
        world.count_housing_once(|world| place(world, OWNERS[1], 0, true));
        assert_eq!(world.economy.tick_work.housing_reads, 0);
        place(world, OWNERS[1], 1, true);
    });
    assert_eq!(world.economy.tick_work.housing_reads, 1 + 2);
    assert_eq!(space(&world, OWNERS[0]), 5);
    assert_eq!(space(&world, OWNERS[1]), 10);
}

#[test]
fn a_load_that_unwinds_leaves_no_count_waiting() {
    let mut world = world();
    let load = std::panic::AssertUnwindSafe(|| {
        world.count_housing_once(|world| {
            place(world, OWNERS[0], 0, true);
            assert!(world.tick().0 > 0, "the load stops half way");
        });
    });
    assert!(std::panic::catch_unwind(load).is_err());
    assert_eq!(space(&world, OWNERS[0]), 5);
    place(&mut world, OWNERS[0], 1, true);
    assert_eq!(space(&world, OWNERS[0]), 10);
}

#[test]
fn restoring_units_looks_at_no_training_queue() {
    let (mut saved, start) = flat();
    saved.initialize_village(PlayerId(0), start).unwrap();
    let center = saved.buildings().next().unwrap().id;
    saved
        .issue_train(PlayerId(0), center, crate::UnitKind::Villager)
        .unwrap();
    let (mut restored, report) = roundtrip(&saved, flat().0);
    assert!(report.units >= 4, "{report:?}");
    assert_eq!(restored.building(center).unwrap().training_queue.len(), 1);
    assert_eq!(restored.economy.tick_work.training_reads, 0);
    // The queue is awake: it trains on from where it was saved.
    for _ in 0..600 {
        restored.advance();
    }
    assert_eq!(restored.building(center).unwrap().training_queue.len(), 0);
}
