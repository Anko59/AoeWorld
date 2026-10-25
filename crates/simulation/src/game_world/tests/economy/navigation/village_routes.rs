use super::*;

fn distant_order_reaches(target: TileCoord, initial_budget: u32) {
    let (mut world, scout) = GameWorld::default_with_village(Seed(1));
    let footprint = world.buildings().next().unwrap().footprint;
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
        assert!(!footprint.contains(unit.position.tile_floor()));
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
        assert!(!footprint.contains(tile));
        if previous.x != tile.x && previous.y != tile.y {
            assert!(!footprint.contains(TileCoord::new(previous.x, tile.y)));
            assert!(!footprint.contains(TileCoord::new(tile.x, previous.y)));
        }
        detoured |= tile.y < footprint.min.y || tile.y >= footprint.max.y;
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
