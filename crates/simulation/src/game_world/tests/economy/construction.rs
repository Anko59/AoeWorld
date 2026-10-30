//! Construction speed, builders and palisade walls.
use super::{buildings::*, *};

mod approach;
mod continuation;
mod eject;

#[test]
fn six_builders_reserve_distinct_perimeter_tiles_before_and_after_arrival() {
    use std::collections::BTreeSet;
    let (mut world, _, _) = village();
    let builders = (0..6)
        .map(|i| villager_at(&mut world, 97, 98 + i))
        .collect::<Vec<_>>();
    let house = world
        .issue_build(
            PlayerId(0),
            &builders,
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let destinations = builders
        .iter()
        .map(|id| {
            world.movement_order(*id).map_or_else(
                || world.unit(*id).unwrap().position.tile_floor(),
                |o| o.target_tile,
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        destinations.len(),
        builders.len(),
        "reserve before the next tick"
    );
    for _ in 0..220 {
        world.advance();
        let working = builders
            .iter()
            .filter_map(|id| {
                let u = world.unit(*id).unwrap();
                (u.task == UnitTask::Building).then_some(u.position.tile_floor())
            })
            .collect::<Vec<_>>();
        assert_eq!(working.iter().collect::<BTreeSet<_>>().len(), working.len());
        if working.len() == builders.len() {
            return;
        }
        assert!(!world.building(house).unwrap().is_complete());
    }
    panic!("all six builders must reach their own perimeter tile");
}

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

#[test]
fn a_house_takes_twenty_five_seconds_with_one_builder() {
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
    advance(&mut world, 499);
    assert!(!world.building(id).unwrap().is_complete());
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Building);
    assert_eq!(world.building(id).unwrap().progress_permille(), 998);
    world.advance();
    let house = world.building(id).unwrap();
    assert!(house.is_complete());
    assert_eq!((house.progress_permille(), house.hp), (1000, 550));
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Idle);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        10
    );
}

#[test]
fn right_click_repairs_a_damaged_own_building_and_releases_the_builder() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
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
    world.economy.buildings.get_mut(&house).unwrap().hp -= 20;
    let mut replay = village().0;
    let replay_builder = villager_at(&mut replay, 99, 100);
    let replay_house = replay
        .issue_build(
            PlayerId(0),
            &[replay_builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    finish(&mut replay, replay_house, 700);
    replay.economy.buildings.get_mut(&replay_house).unwrap().hp -= 20;
    world
        .issue_construct(PlayerId(0), &[builder], house)
        .unwrap();
    replay
        .issue_construct(PlayerId(0), &[replay_builder], replay_house)
        .unwrap();
    for _ in 0..40 {
        world.advance();
        replay.advance();
        assert_eq!(world.canonical_hash(), replay.canonical_hash());
        assert_eq!(
            world.remembered_building(PlayerId(0), house).unwrap().hp,
            world.building(house).unwrap().hp,
            "repair HP reaches the subscribed building view"
        );
    }
    let state = world.building(house).unwrap();
    assert_eq!(state.hp, state.max_hp);
    assert_eq!(state.progress_permille(), 1000);
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Idle);
}

#[test]
fn three_builders_take_base_times_three_over_five() {
    // base * 3 / (n + 2) = 25 * 3 / 5 = 15 s = 300 ticks.
    let (mut world, _, _) = village();
    let builders = [
        villager_at(&mut world, 99, 100),
        villager_at(&mut world, 99, 101),
        villager_at(&mut world, 100, 99),
    ];
    let id = world
        .issue_build(
            PlayerId(0),
            &builders,
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(finish(&mut world, id, 600), 300);
    for builder in builders {
        assert_eq!(world.unit(builder).unwrap().task, UnitTask::Idle);
    }
}

#[test]
fn builders_walk_to_the_foundation_before_adding_progress() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 90, 100);
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 0);
    let id = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Outpost,
            at(100, 100),
            false,
        )
        .unwrap();
    advance(&mut world, 20);
    assert_eq!(world.building(id).unwrap().progress_permille(), 0);
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Moving);
    // Ten tiles at 0.8 tiles/s, then 300 ticks of work.
    let ticks = finish(&mut world, id, 1000) + 20;
    assert!((300 + 200..=300 + 260).contains(&ticks), "{ticks}");
    let tile = world.unit(builder).unwrap().position.tile_floor();
    assert!(tile.x.abs_diff(100) <= 1 && tile.y.abs_diff(100) <= 1);
}

#[test]
fn a_move_order_abandons_construction_and_the_foundation_stays() {
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
    advance(&mut world, 100);
    let progress = world.building(id).unwrap().progress_permille();
    world
        .issue_move(
            builder,
            WorldPosition::from_tile_center(at(90, 90)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 100);
    assert_eq!(world.building(id).unwrap().progress_permille(), progress);
    // A second villager resumes the stranded foundation; nothing is charged.
    let wood = stock(&world).1;
    let other = villager_at(&mut world, 99, 101);
    world.issue_construct(PlayerId(0), &[other], id).unwrap();
    assert_eq!(stock(&world).1, wood, "resuming costs nothing");
    advance(&mut world, 60);
    assert!(world.building(id).unwrap().progress_permille() > progress);
    finish(&mut world, id, 700);
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_cap,
        10
    );
}

#[test]
fn construction_adds_hit_points_without_healing_combat_damage() {
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
    advance(&mut world, 250);
    let before = world.building(id).unwrap().hp;
    world.economy.buildings.get_mut(&id).unwrap().hp = before - 100;
    world.advance();
    let after = world.building(id).unwrap().hp;
    assert!(after < before, "damage stays: {after} vs {before}");
    finish(&mut world, id, 700);
    let house = world.building(id).unwrap();
    assert!(house.is_complete());
    // A foundation starts at 1 HP, so full work ends at max - 100 + 1.
    assert_eq!(house.hp, house.max_hp - 99, "completion does not heal");
}

#[test]
fn construct_orders_reject_complete_foreign_and_missing_buildings() {
    let (mut world, _, center) = village();
    let builder = villager_at(&mut world, 99, 100);
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let other = villager_at(&mut world, 99, 101);
    let stranger = world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::from_tile_center(at(30, 30)).unwrap(),
        )
        .unwrap();
    let before = world.canonical_hash();
    for (player, builders, target, expected) in [
        (
            PlayerId(0),
            vec![other],
            center,
            EconomyError::AlreadyComplete,
        ),
        (
            PlayerId(0),
            vec![other],
            EntityId(9999),
            EconomyError::UnknownEntity,
        ),
        (PlayerId(1), vec![stranger], house, EconomyError::NotOwner),
        (PlayerId(0), vec![stranger], house, EconomyError::NotOwner),
        (PlayerId(0), vec![], house, EconomyError::OrderTooLarge),
    ] {
        assert_eq!(
            world.issue_construct(player, &builders, target),
            Err(expected)
        );
        assert_eq!(world.canonical_hash(), before);
    }
    finish(&mut world, house, 700);
    assert_eq!(
        world.issue_construct(PlayerId(0), &[other], house),
        Err(EconomyError::AlreadyComplete)
    );
}

fn palisade(
    world: &mut GameWorld,
    builders: &[EntityId],
    from: TileCoord,
    to: TileCoord,
) -> Result<Vec<EntityId>, EconomyError> {
    world.issue_build_wall(
        PlayerId(0),
        builders,
        BuildingKind::PalisadeWall,
        from,
        to,
        false,
    )
}

fn wall_ring(world: &mut GameWorld, builders: &[EntityId]) -> Vec<EntityId> {
    let mut ids = Vec::new();
    for (from, to) in [
        (at(100, 100), at(104, 100)),
        (at(104, 101), at(104, 104)),
        (at(103, 104), at(100, 104)),
        (at(100, 103), at(100, 101)),
    ] {
        ids.extend(palisade(world, builders, from, to).unwrap());
    }
    ids
}

#[test]
fn palisade_segments_cost_two_wood_each_and_take_six_seconds_per_tile() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 99);
    fixture::survey(&mut world, PlayerId(0), at(102, 100), 2);
    let ids = palisade(&mut world, &[builder], at(100, 100), at(104, 100)).unwrap();
    assert_eq!(ids.len(), 5);
    assert_eq!(stock(&world).1, 190);
    for id in &ids {
        let wall = world.building(*id).unwrap();
        assert_eq!((wall.max_hp, wall.footprint.width()), (250, 1));
    }
    // One builder: 5 segments * 120 ticks plus short walks between them.
    let mut ticks = 0;
    while !ids
        .iter()
        .all(|id| world.building(*id).unwrap().is_complete())
    {
        world.advance();
        ticks += 1;
        assert!(ticks < 900, "the builder should continue along the line");
    }
    assert!(ticks >= 600, "{ticks}");
    assert_eq!(world.unit(builder).unwrap().task, UnitTask::Idle);
    // Too long, or any segment invalid: nothing is placed or charged.
    let before = world.canonical_hash();
    assert_eq!(
        palisade(&mut world, &[builder], at(0, 10), at(100, 10)),
        Err(EconomyError::OrderTooLarge)
    );
    assert_eq!(
        palisade(&mut world, &[builder], at(102, 98), at(102, 102)),
        Err(EconomyError::InvalidFootprint),
        "the line crosses an existing segment"
    );
    assert_eq!(world.canonical_hash(), before);
}

#[test]
fn a_completed_palisade_ring_blocks_pathing_in_both_directions() {
    let (mut world, _, _) = village();
    let inside = villager_at(&mut world, 102, 102);
    let builders = [
        villager_at(&mut world, 97, 97),
        villager_at(&mut world, 107, 107),
        villager_at(&mut world, 97, 107),
    ];
    let ids = wall_ring(&mut world, &builders);
    assert_eq!(ids.len(), 16);
    let mut ticks = 0;
    while !ids
        .iter()
        .all(|id| world.building(*id).unwrap().is_complete())
    {
        world.advance();
        ticks += 1;
        if ticks >= 3000 {
            let left: Vec<_> = ids
                .iter()
                .filter(|id| !world.building(**id).unwrap().is_complete())
                .map(|id| {
                    let b = world.building(*id).unwrap();
                    (b.footprint.min, b.progress_permille())
                })
                .collect();
            let crew: Vec<_> = builders
                .iter()
                .map(|id| {
                    let u = world.unit(*id).unwrap();
                    (u.position.tile_floor(), u.task)
                })
                .collect();
            panic!("ring unfinished after {ticks} ticks: {left:?}; builders {crew:?}");
        }
    }
    for id in &ids {
        let wall = world.building(*id).unwrap();
        assert!(!world.tile_passable(wall.footprint.min));
    }
    let outside = WorldPosition::from_tile_center(at(110, 102)).unwrap();
    assert_eq!(
        world.issue_move(inside, outside),
        Err(GameWorldError::Unreachable),
        "no route out of a closed ring"
    );
    // From outside the order may be accepted (the planner only learns later
    // that the goal is sealed), but the villager never gets in.
    let _ = world.issue_move(
        builders[0],
        WorldPosition::from_tile_center(at(102, 102)).unwrap(),
    );
    for _ in 0..1500 {
        world.advance();
        let tile = world.unit(builders[0]).unwrap().position.tile_floor();
        assert!(
            !(101..=103).contains(&tile.x) || !(101..=103).contains(&tile.y),
            "entered the sealed ring at {tile:?}"
        );
    }
    let around = WorldPosition::from_tile_center(at(110, 100)).unwrap();
    assert!(world.issue_move(builders[0], around).is_ok());
}

#[test]
fn a_wall_makes_the_route_longer_but_not_impossible() {
    fn travel(with_wall: bool) -> u32 {
        let (mut world, _, _) = village();
        let walker = villager_at(&mut world, 98, 100);
        if with_wall {
            let builder = villager_at(&mut world, 99, 94);
            fixture::survey(&mut world, PlayerId(0), at(100, 100), 5);
            let ids = palisade(&mut world, &[builder], at(100, 95), at(100, 105)).unwrap();
            let mut guard = 0;
            while !ids
                .iter()
                .all(|id| world.building(*id).unwrap().is_complete())
            {
                world.advance();
                guard += 1;
                assert!(guard < 4000);
            }
        }
        let goal = at(102, 100);
        world
            .issue_move(walker, WorldPosition::from_tile_center(goal).unwrap())
            .unwrap();
        let mut ticks = 0;
        while world.unit(walker).unwrap().position.tile_floor() != goal || ticks == 0 {
            world.advance();
            ticks += 1;
            assert!(ticks < 2000);
            let tile = world.unit(walker).unwrap().position.tile_floor();
            assert!(
                !(with_wall && tile.x == 100 && (95..=105).contains(&tile.y)),
                "the walker crossed the palisade at {tile:?}"
            );
        }
        ticks
    }
    assert!(travel(true) > travel(false) + 40);
}

#[test]
fn coordinates_at_the_integer_limit_are_rejected_without_panicking() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    let before = world.canonical_hash();
    for (from, to) in [
        (at(i32::MAX, 10), at(i32::MAX, 12)),
        (at(10, i32::MAX), at(12, i32::MAX)),
        (at(i32::MAX - 1, i32::MAX), at(i32::MAX, i32::MAX)),
        (at(i32::MIN, i32::MIN), at(i32::MIN, i32::MIN + 2)),
    ] {
        assert_eq!(
            palisade(&mut world, &[builder], from, to),
            Err(EconomyError::InvalidFootprint),
            "{from:?} to {to:?}"
        );
    }
    for origin in [at(i32::MAX, i32::MAX), at(i32::MAX, 5), at(i32::MIN, 5)] {
        assert_eq!(
            world.issue_build(PlayerId(0), &[builder], BuildingKind::House, origin, false),
            Err(EconomyError::InvalidFootprint)
        );
    }
    assert_eq!(world.canonical_hash(), before);
}
