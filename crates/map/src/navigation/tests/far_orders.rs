//! A far order must not wait for a whole-route search: the planner emits a
//! first proven leg from one tick of work and plans the rest as it is used.
use super::*;

/// One simulation tick of planner work (`MAX_ROUTE_WORK_PER_TICK`).
const TICK_WORK: u32 = 16_384;
const WIDTH: i32 = 4_096;
const ROW: i32 = 10;

fn first_poll(terrain: &MapChunkGenerator, origin: TileCoord, destination: TileCoord) -> Path {
    let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    match planner.poll(terrain, &ResourceOverlay::default(), TICK_WORK, &|| false) {
        RoutePlannerPoll::Path(path) => path,
        other => panic!("a far order did not start from one tick of work: {other:?}"),
    }
}

fn assert_walkable_steps(terrain: &MapChunkGenerator, tiles: &[TileCoord]) {
    assert!(tiles.windows(2).all(|pair| {
        (pair[0].x - pair[1].x).abs() <= 1
            && (pair[0].y - pair[1].y).abs() <= 1
            && terrain
                .edge_between(pair[0], pair[1])
                .eq(&EdgePassability::Passable)
    }));
}

#[test]
fn far_route_emits_its_first_leg_from_one_tick_of_work() {
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    let origin = TileCoord::new(1, ROW);
    let path = first_poll(&terrain, origin, TileCoord::new(WIDTH - 2, ROW));
    assert_eq!(path.tiles.first(), Some(&origin));
    assert!(path.tiles.len() > 1);
    assert!(path.tiles.iter().all(|tile| tile.y == ROW));
    assert_walkable_steps(&terrain, &path.tiles);
}

#[test]
fn far_north_south_route_emits_its_first_leg_from_one_tick_of_work() {
    // The octile estimate used to drop the straight remainder whenever the
    // route was longer in y than in x, leaving these orders an uninformed
    // flood from the origin.
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    for (origin, destination) in [
        (TileCoord::new(ROW, 1), TileCoord::new(ROW, WIDTH - 2)),
        (TileCoord::new(ROW, WIDTH - 2), TileCoord::new(ROW, 1)),
        (TileCoord::new(300, 1), TileCoord::new(1, WIDTH - 2)),
    ] {
        let path = first_poll(&terrain, origin, destination);
        assert_eq!(path.tiles.first(), Some(&origin));
        assert!(path.tiles.len() > 1);
        assert_walkable_steps(&terrain, &path.tiles);
        let last = path.tiles.last().expect("leg end");
        assert!(super::heuristic(*last, destination) < super::heuristic(origin, destination));
    }
    assert_eq!(
        super::heuristic(TileCoord::new(3, 0), TileCoord::new(0, 10)),
        3 * u64::from(DIAGONAL_COST) + 7 * u64::from(ORTHOGONAL_COST)
    );
    assert_eq!(
        super::heuristic(TileCoord::new(0, 0), TileCoord::new(0, 10)),
        super::heuristic(TileCoord::new(0, 0), TileCoord::new(10, 0))
    );
}

#[test]
fn far_diagonal_route_emits_its_first_leg_from_one_tick_of_work() {
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    let origin = TileCoord::new(1, 1);
    let path = first_poll(&terrain, origin, TileCoord::new(WIDTH - 2, WIDTH - 700));
    assert_eq!(path.tiles.first(), Some(&origin));
    assert!(path.tiles.len() > 1);
    assert_walkable_steps(&terrain, &path.tiles);
}

#[test]
fn far_route_keeps_only_leg_sized_search_state() {
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    let origin = TileCoord::new(1, ROW);
    let destination = TileCoord::new(WIDTH - 2, ROW);
    let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let tiles = collect_route(&mut planner, &terrain, TICK_WORK);
    assert_eq!(tiles.last(), Some(&destination));
    // The whole-route search retained every visited tile of the 4,094-tile
    // row; a leg-wise search retains only the current leg.
    assert!(
        planner.peak_retained_entries() <= 2_048,
        "retained {} entries",
        planner.peak_retained_entries()
    );
}

#[test]
fn far_route_is_the_same_for_every_budget_partition() {
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    let origin = TileCoord::new(1, 1);
    let destination = TileCoord::new(700, 300);
    let mut by_tick = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let expected = collect_route(&mut by_tick, &terrain, TICK_WORK);
    assert_eq!(expected.first(), Some(&origin));
    assert_eq!(expected.last(), Some(&destination));
    assert_walkable_steps(&terrain, &expected);

    let overlay = ResourceOverlay::default();
    let mut starved = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let mut tiles = vec![origin];
    for _ in 0..1_000_000 {
        match starved.poll(&terrain, &overlay, 7, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                assert_eq!(path.tiles.first(), tiles.last());
                tiles.extend(path.tiles.into_iter().skip(1));
                if starved.is_terminal() {
                    break;
                }
            }
            other => panic!("starved far route failed: {other:?}"),
        }
    }
    assert_eq!(tiles, expected);
    assert_eq!(starved.work(), by_tick.work());
}

#[test]
fn walled_far_route_starts_at_once_and_still_finds_the_distant_gateway() {
    // 64 water samples over 256 tiles: the wall is four tiles thick and its
    // only gateway lies about 80 tiles north of the straight line, farther
    // than one tick of search reaches.
    const WALLED_WIDTH: i32 = 256;
    let terrain = flat_terrain_with_water_wall(WALLED_WIDTH, 32, 30);
    let origin = TileCoord::new(8, 200);
    let destination = TileCoord::new(250, 200);
    let overlay = ResourceOverlay::default();
    assert!(super::walkable(&terrain, origin) && super::walkable(&terrain, destination));
    assert!(
        (origin.x..destination.x).any(|x| !super::walkable(&terrain, TileCoord::new(x, origin.y))),
        "fixture wall must block the straight line"
    );

    let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let mut tiles = vec![origin];
    let mut polls_before_first_leg = None;
    for poll in 1..=4_096_u32 {
        match planner.poll(&terrain, &overlay, TICK_WORK, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                polls_before_first_leg.get_or_insert(poll);
                assert_eq!(path.tiles.first(), tiles.last());
                tiles.extend(path.tiles.into_iter().skip(1));
                if planner.is_terminal() {
                    break;
                }
            }
            other => panic!("walled far route failed: {other:?}"),
        }
    }
    assert_eq!(polls_before_first_leg, Some(1));
    assert_eq!(tiles.last(), Some(&destination));
    assert_walkable_steps(&terrain, &tiles);
    assert!(tiles.iter().all(|tile| super::walkable(&terrain, *tile)));
    assert!(tiles.iter().any(|tile| tile.y < 140), "used the gateway");
    assert!(planner.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);
}
