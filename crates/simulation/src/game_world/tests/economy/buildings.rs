//! The building rules table, placement, refunds, housing, drop-off, barracks
//! and the outpost.
use super::*;

mod finalization;
use crate::{BUILDING_RULES, ConstructionState, MAX_POPULATION, trainable_units};

pub(super) fn set_stock(world: &mut GameWorld, food: u32, wood: u32, gold: u32, stone: u32) {
    let stock = &mut world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .stockpile;
    (stock.food, stock.wood, stock.gold, stock.stone) = (food, wood, gold, stone);
}
pub(super) fn stock(world: &GameWorld) -> (u32, u32, u32, u32) {
    let s = world.player_economy(PlayerId(0)).unwrap().stockpile;
    (s.food, s.wood, s.gold, s.stone)
}
pub(super) fn villager_at(world: &mut GameWorld, x: i32, y: i32) -> EntityId {
    let at = WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    world.spawn_villager(PlayerId(0), at).unwrap()
}
/// Advance until `id` is complete, returning the ticks it took.
pub(super) fn finish(world: &mut GameWorld, id: EntityId, limit: u32) -> u32 {
    for tick in 1..=limit {
        world.advance();
        if world.building(id).is_some_and(|b| b.is_complete()) {
            return tick;
        }
    }
    panic!("building {id:?} not complete after {limit} ticks");
}
fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

#[test]
fn the_rules_table_holds_the_dark_age_values_in_kind_order() {
    use BuildingKind::*;
    for (index, rules) in BUILDING_RULES.iter().enumerate() {
        assert_eq!(
            rules.kind as usize, index,
            "{:?} is out of order",
            rules.kind
        );
    }
    // (kind, wood, stone, seconds, hp, side)
    for (kind, wood, stone, seconds, hp, side) in [
        (House, 25, 0, 25, 550, 2),
        (LumberCamp, 100, 0, 35, 600, 2),
        (MiningCamp, 100, 0, 35, 600, 2),
        (Mill, 100, 0, 35, 600, 2),
        (Farm, 60, 0, 15, 480, 3),
        (Barracks, 175, 0, 50, 1200, 3),
        (PalisadeWall, 2, 0, 6, 250, 1),
        (Outpost, 25, 5, 15, 500, 1),
    ] {
        let rules = kind.rules();
        let cost = rules.cost;
        assert_eq!(
            (cost.food, cost.wood, cost.gold, cost.stone),
            (0, wood, 0, stone),
            "{kind:?}"
        );
        assert_eq!(
            (rules.build_seconds, rules.max_hp, rules.side),
            (seconds, hp, side),
            "{kind:?}"
        );
        assert!(rules.buildable);
    }
    assert!(TownCenter.rules().buildable);
    assert_eq!(Farm.rules().requires, Some(Mill));
    assert_eq!(Farm.rules().food, 175);
    assert_eq!(Outpost.rules().los, 10);
    assert!(!Farm.rules().blocks);
}

#[test]
fn each_camp_accepts_only_its_resources_and_the_center_everything() {
    use BuildingKind::*;
    use ResourceKind::*;
    for (kind, accepted) in [
        (TownCenter, vec![Food, Wood, Gold, Stone]),
        (LumberCamp, vec![Wood]),
        (MiningCamp, vec![Gold, Stone]),
        (Mill, vec![Food]),
        (House, vec![]),
        (Farm, vec![]),
        (Barracks, vec![]),
        (PalisadeWall, vec![]),
        (Outpost, vec![]),
    ] {
        for resource in [Food, Wood, Gold, Stone] {
            assert_eq!(kind.accepts(resource), accepted.contains(&resource));
        }
    }
}

#[test]
fn foundation_is_charged_on_placement_and_starts_at_zero_progress() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(stock(&world), (200, 175, 100, 200));
    let house = world.building(id).unwrap();
    assert_eq!(house.state, ConstructionState::UnderConstruction);
    assert_eq!(house.progress_permille(), 0);
    assert_eq!((house.footprint.width(), house.footprint.height()), (2, 2));
    assert_eq!((house.hp, house.max_hp), (1, 550));
    assert!(!world.tile_passable(at(100, 100)) && !world.tile_passable(at(101, 101)));
    // The housing is not counted before completion.
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().population_cap, 5);
}

#[test]
fn invalid_placements_and_orders_are_refused_atomically() {
    let (mut world, scout, center) = village();
    let villager = villager_at(&mut world, 99, 100);
    let tc = world.building(center).unwrap().footprint;
    world
        .issue_build(
            PlayerId(0),
            &[villager],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let stranger = world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::from_tile_center(at(30, 30)).unwrap(),
        )
        .unwrap();
    let before = world.canonical_hash();
    let cases: Vec<(
        PlayerId,
        Vec<EntityId>,
        BuildingKind,
        TileCoord,
        EconomyError,
    )> = vec![
        // Outside the map, over the Town Center, over another foundation.
        (
            PlayerId(0),
            vec![villager],
            BuildingKind::House,
            at(255, 255),
            EconomyError::InvalidFootprint,
        ),
        (
            PlayerId(0),
            vec![villager],
            BuildingKind::House,
            tc.min,
            EconomyError::InvalidFootprint,
        ),
        (
            PlayerId(0),
            vec![villager],
            BuildingKind::House,
            at(101, 101),
            EconomyError::InvalidFootprint,
        ),
        // Units on the footprint never refuse it; see construction/eject.rs.
        (
            PlayerId(0),
            vec![villager],
            BuildingKind::TownCenter,
            at(150, 150),
            EconomyError::RequiresCastleAge,
        ),
        (
            PlayerId(0),
            vec![villager],
            BuildingKind::Farm,
            at(150, 150),
            EconomyError::RequirementMissing,
        ),
        (
            PlayerId(0),
            vec![scout],
            BuildingKind::House,
            at(150, 150),
            EconomyError::WrongUnitKind,
        ),
        (
            PlayerId(0),
            vec![stranger],
            BuildingKind::House,
            at(150, 150),
            EconomyError::NotOwner,
        ),
        (
            PlayerId(0),
            vec![EntityId(9999)],
            BuildingKind::House,
            at(150, 150),
            EconomyError::UnknownEntity,
        ),
        (
            PlayerId(0),
            vec![],
            BuildingKind::House,
            at(150, 150),
            EconomyError::OrderTooLarge,
        ),
    ];
    for (player, builders, kind, origin, expected) in cases {
        assert_eq!(
            world.issue_build(player, &builders, kind, origin, false),
            Err(expected),
            "{kind:?} at {origin:?}"
        );
        assert_eq!(world.canonical_hash(), before);
    }
    set_stock(&mut world, 0, 24, 0, 0);
    let poor = world.canonical_hash();
    assert_eq!(
        world.issue_build(
            PlayerId(0),
            &[villager],
            BuildingKind::House,
            at(150, 150),
            false
        ),
        Err(EconomyError::InsufficientResources)
    );
    set_stock(&mut world, 0, 100, 0, 4);
    assert_eq!(
        world.issue_build(
            PlayerId(0),
            &[villager],
            BuildingKind::Outpost,
            at(150, 150),
            false
        ),
        Err(EconomyError::InsufficientResources),
        "the outpost needs 5 stone"
    );
    set_stock(&mut world, 0, 24, 0, 0);
    assert_eq!(world.canonical_hash(), poor);
}

#[test]
fn cancelling_before_progress_refunds_everything_and_frees_the_site() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 60, 100);
    fixture::survey(&mut world, PlayerId(0), at(101, 101), 2);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Barracks,
            at(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(stock(&world).1, 25);
    assert_eq!(
        world.issue_cancel_construction(PlayerId(1), id),
        Err(EconomyError::NotOwner)
    );
    world.issue_cancel_construction(PlayerId(0), id).unwrap();
    assert_eq!(stock(&world), (200, 200, 100, 200));
    assert!(world.building(id).is_none());
    assert!(world.tile_passable(at(101, 101)));
    advance(&mut world, 1);
    assert_ne!(world.unit(builder).unwrap().task, UnitTask::Building);
    assert_eq!(
        world.issue_cancel_construction(PlayerId(0), id),
        Err(EconomyError::UnknownEntity),
        "a second cancel cannot refund twice"
    );
    world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Barracks,
            at(100, 100),
            false,
        )
        .unwrap();
}

#[test]
fn cancelling_after_progress_refunds_only_the_undone_share() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::LumberCamp,
            at(100, 100),
            false,
        )
        .unwrap();
    // One builder needs 35 s = 700 ticks; stop at 350 ticks (half done).
    advance(&mut world, 350);
    let permille = world.building(id).unwrap().progress_permille();
    assert_eq!(permille, 500);
    world.issue_cancel_construction(PlayerId(0), id).unwrap();
    assert_eq!(stock(&world).1, 100 + 50);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    finish(&mut world, id, 600);
    assert_eq!(
        world.issue_cancel_construction(PlayerId(0), id),
        Err(EconomyError::AlreadyComplete)
    );
}

#[test]
fn houses_raise_the_population_cap_by_five_up_to_one_thousand() {
    let (mut world, _, center) = village();
    set_stock(&mut world, 10000, 10000, 0, 0);
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    let builder = villager_at(&mut world, 99, 100);
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    finish(&mut world, house, 700);
    let player = world.player_economy(PlayerId(0)).unwrap();
    assert_eq!(player.population_cap, 10);
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    // 205 houses would give 1030; the current ceiling is 1000.
    let mut houses = vec![house];
    for index in 0..204 {
        let (x, y) = (110 + (index % 11) * 3, 110 + (index / 11) * 3);
        let worker = villager_at(&mut world, x - 1, y);
        let id = world
            .issue_build(PlayerId(0), &[worker], BuildingKind::House, at(x, y), false)
            .unwrap();
        world.economy.buildings.get_mut(&id).unwrap().work_done =
            world.building(id).unwrap().work_total - 3;
        houses.push(id);
    }
    advance(&mut world, 2);
    assert!(
        houses
            .iter()
            .all(|id| world.building(*id).unwrap().is_complete())
    );
    let player = world.player_economy(PlayerId(0)).unwrap();
    assert_eq!(player.population_cap, MAX_POPULATION);
    // Losing a completed house (not possible yet) would use the same recount;
    // a cancelled foundation never changed the cap.
    let extra = villager_at(&mut world, 200, 200);
    let spare = world
        .issue_build(
            PlayerId(0),
            &[extra],
            BuildingKind::House,
            at(201, 200),
            false,
        )
        .unwrap();
    world.issue_cancel_construction(PlayerId(0), spare).unwrap();
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        MAX_POPULATION
    );
}

#[test]
fn the_nearest_dropoff_depends_on_the_resource_kind() {
    let (mut world, _, center) = village();
    set_stock(&mut world, 0, 1000, 0, 0);
    let mut camps = Vec::new();
    for (kind, x) in [
        (BuildingKind::LumberCamp, 100),
        (BuildingKind::MiningCamp, 110),
        (BuildingKind::Mill, 120),
    ] {
        let worker = villager_at(&mut world, x - 1, 100);
        let id = world
            .issue_build(PlayerId(0), &[worker], kind, at(x, 100), false)
            .unwrap();
        if camps.is_empty() {
            // Under construction a camp is not a drop-off yet.
            assert_eq!(
                world.nearest_dropoff(PlayerId(0), at(x - 2, 100), ResourceKind::Wood),
                Some(center)
            );
        }
        finish(&mut world, id, 800);
        camps.push(id);
    }
    let [lumber, mining, mill] = camps[..] else {
        unreachable!()
    };
    let near = |x, resource| world.nearest_dropoff(PlayerId(0), at(x, 101), resource);
    assert_eq!(near(100, ResourceKind::Wood), Some(lumber));
    assert_eq!(
        near(100, ResourceKind::Gold),
        Some(mining),
        "nearer than the center"
    );
    assert_eq!(
        world.nearest_dropoff(PlayerId(0), at(70, 70), ResourceKind::Wood),
        Some(center)
    );
    assert_eq!(near(100, ResourceKind::Food), Some(mill));
    assert_eq!(near(110, ResourceKind::Gold), Some(mining));
    assert_eq!(near(110, ResourceKind::Stone), Some(mining));
    assert_eq!(near(110, ResourceKind::Wood), Some(lumber));
    assert_eq!(near(120, ResourceKind::Food), Some(mill));
    assert_eq!(near(120, ResourceKind::Wood), Some(lumber));
    assert_eq!(
        world.nearest_dropoff(PlayerId(1), at(100, 101), ResourceKind::Wood),
        None,
        "another player's buildings never count"
    );
}
