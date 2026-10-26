use super::*;

#[test]
fn town_center_arches_accept_and_cross_center_destinations() {
    for mapped in [false, true] {
        let mut world = if mapped {
            super::super::far_orders::wide_world()
        } else {
            super::super::world()
        };
        let tc = world
            .place_town_center(PlayerId(0), TileCoord::new(64, 64))
            .unwrap();
        // Unit 109 and decorative annexes 619/620 do not obstruct. Annex
        // 618 alone blocks the NW 2x2; its offset is NE=1, SE=-1.
        for y in 64..68 {
            for x in 64..68 {
                assert_eq!(
                    world.tile_passable(TileCoord::new(x, y)),
                    !(x < 66 && y < 66)
                );
            }
        }
        let scout = world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(62, 66)).unwrap(),
            )
            .unwrap();
        for tile in [TileCoord::new(66, 66), TileCoord::new(69, 66)] {
            let target = WorldPosition::from_tile_center(tile).unwrap();
            assert_eq!(
                world.issue_move(scout, target),
                Ok(true),
                "mapped={mapped}, tile={tile:?}"
            );
            for _ in 0..200 {
                world.advance();
            }
            assert_eq!(world.unit(scout).unwrap().position, target);
            assert_eq!(world.movement_failure(scout), None);
        }
        let builder = world
            .spawn_villager(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(62, 68)).unwrap(),
            )
            .unwrap();
        world
            .economy
            .players
            .get_mut(&PlayerId(0))
            .unwrap()
            .stockpile
            .wood = 1000;
        assert_eq!(
            world.issue_build(
                PlayerId(0),
                &[builder],
                BuildingKind::House,
                TileCoord::new(66, 66),
                false
            ),
            Err(EconomyError::InvalidFootprint),
            "open arches still reserve the placement footprint"
        );
        world.remove_building(tc);
        for y in 64..68 {
            for x in 64..68 {
                assert!(world.tile_passable(TileCoord::new(x, y)));
            }
        }
    }
}

#[test]
fn a_town_center_foundation_opens_its_arches_only_on_completion() {
    let mut world = super::super::far_orders::wide_world();
    let footprint = aoe_core::TileRect::from_xywh(64, 64, 4, 4);
    let tc = world
        .insert_building(PlayerId(0), BuildingKind::TownCenter, footprint, false)
        .unwrap();
    for y in 64..68 {
        for x in 64..68 {
            assert!(!world.tile_passable(TileCoord::new(x, y)));
        }
    }
    let builder = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(63, 66)).unwrap(),
        )
        .unwrap();
    world.economy.buildings.get_mut(&tc).unwrap().work_total = 3;
    world.issue_construct(PlayerId(0), &[builder], tc).unwrap();
    world.advance();
    assert!(world.building(tc).unwrap().is_complete());
    for y in 64..68 {
        for x in 64..68 {
            assert_eq!(
                world.tile_passable(TileCoord::new(x, y)),
                !(x < 66 && y < 66)
            );
        }
    }
}

fn distant_order_reaches(target: TileCoord, initial_budget: u32) {
    let (mut world, scout) = GameWorld::default_with_village(Seed(1));
    let footprint = world.buildings().next().unwrap().footprint;
    let solid = aoe_core::TileRect::from_xywh(footprint.min.x, footprint.min.y, 2, 2);
    assert_eq!(
        world.unit(scout).unwrap().position.tile_floor(),
        TileCoord::new(8192, 8192)
    );
    world.planning_budget = initial_budget;
    world.fast_route_budget = initial_budget;
    let destination = WorldPosition::from_tile_center(target).unwrap();
    assert_eq!(world.issue_move(scout, destination), Ok(true));
    let index = world.lookup[&scout];
    assert!(
        world.units[index].planner.is_some(),
        "keep the incremental planner"
    );
    if initial_budget == 1 {
        assert!(world.unit(scout).unwrap().planning);
    }
    for _ in 0..12_000 {
        world.advance();
        let unit = world.unit(scout).unwrap();
        assert!(!solid.contains(unit.position.tile_floor()));
        assert_eq!(world.movement_failure(scout), None);
        if !unit.moving && !unit.planning {
            assert_eq!(unit.position, destination);
            return;
        }
    }
    panic!("distant village order did not arrive within bounded ticks: {target:?}");
}

#[test]
fn village_scout_reaches_review_destination_7892_8042() {
    distant_order_reaches(TileCoord::new(7892, 8042), crate::MAX_ROUTE_WORK_PER_TICK);
}

#[test]
fn village_scout_reaches_review_destination_7680_7936() {
    distant_order_reaches(TileCoord::new(7680, 7936), crate::MAX_ROUTE_WORK_PER_TICK);
}

#[test]
fn village_scout_reaches_opposite_quadrant_8492_8342() {
    distant_order_reaches(TileCoord::new(8492, 8342), crate::MAX_ROUTE_WORK_PER_TICK);
}

#[test]
fn village_scout_reaches_opposite_quadrant_8704_8448() {
    distant_order_reaches(TileCoord::new(8704, 8448), crate::MAX_ROUTE_WORK_PER_TICK);
}

#[test]
fn village_order_keeps_its_frontier_when_the_tick_budget_runs_out() {
    distant_order_reaches(TileCoord::new(7892, 8042), 1);
}

#[test]
fn uniform_incremental_route_detours_around_the_center_without_corner_cutting() {
    let (mut world, scout, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    let solid = aoe_core::TileRect::from_xywh(footprint.min.x, footprint.min.y, 2, 2);
    let start = TileCoord::new(footprint.min.x - 2, footprint.min.y + 1);
    world
        .issue_move(scout, WorldPosition::from_tile_center(start).unwrap())
        .unwrap();
    advance(&mut world, 200);
    assert_eq!(world.unit(scout).unwrap().position.tile_floor(), start);
    let target = TileCoord::new(footprint.max.x + 2, start.y);
    world.planning_budget = 1;
    world
        .issue_move(scout, WorldPosition::from_tile_center(target).unwrap())
        .unwrap();
    let mut previous = start;
    let mut detoured = false;
    for _ in 0..1000 {
        world.advance();
        let tile = world.unit(scout).unwrap().position.tile_floor();
        assert!(!solid.contains(tile));
        if previous.x != tile.x && previous.y != tile.y {
            assert!(!solid.contains(TileCoord::new(previous.x, tile.y)));
            assert!(!solid.contains(TileCoord::new(tile.x, previous.y)));
        }
        detoured |= tile.y < solid.min.y || tile.y >= solid.max.y;
        previous = tile;
    }
    assert!(detoured);
    assert_eq!(previous, target);
}

#[test]
fn gathering_and_returning_continue_through_the_shared_incremental_planner() {
    let (mut world, node, id) = resource_world(ResourceKind::Wood);
    world.initialize_village(PlayerId(0), node.tile).unwrap();
    let before = world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    world.planning_budget = 1;
    world.issue_gather(PlayerId(0), id, node.id).unwrap();
    let mut returning = false;
    for _ in 0..6000 {
        world.advance();
        let unit = world.unit(id).unwrap();
        assert!(unit.carried.is_none_or(|load| load.amount <= 10));
        returning |= unit.task == UnitTask::Returning;
        if world.player_economy(PlayerId(0)).unwrap().stockpile.wood == before + 10 {
            assert!(returning);
            assert_eq!(world.gather_target(id), Some(node.id));
            assert_eq!(
                world.resource_remaining(node.id),
                Some(node.initial_amount - 10)
            );
            return;
        }
    }
    panic!("villager did not return and deposit within bounded ticks");
}
