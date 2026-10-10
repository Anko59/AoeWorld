use super::*;

fn rectangle(origin: TileCoord, destination: TileCoord, budget: u32) -> MovementOutcome {
    search_path(
        origin,
        destination,
        budget,
        |tile| (0..32).contains(&tile.x) && (0..32).contains(&tile.y),
        |tile| {
            let mut neighbors = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = TileCoord::new(tile.x + dx, tile.y + dy);
                    if (0..32).contains(&next.x) && (0..32).contains(&next.y) {
                        neighbors.push((
                            next,
                            if dx != 0 && dy != 0 {
                                DIAGONAL_COST
                            } else {
                                ORTHOGONAL_COST
                            },
                        ));
                    }
                }
            }
            neighbors
        },
    )
}

#[test]
fn progress_priority_crosses_octile_plateau_with_same_small_expansion_budget_and_optimal_cost() {
    let origin = TileCoord::new(1, 1);
    let target = TileCoord::new(25, 18);
    let MovementOutcome::Path(path) = rectangle(origin, target, 30) else {
        panic!("progress priority must reach goal within unchanged30 expansions");
    };
    assert_eq!(
        path.cost,
        17 * u64::from(DIAGONAL_COST) + 7 * u64::from(ORTHOGONAL_COST)
    );
    assert_eq!(path.tiles.len(), 25);
    assert_eq!(path.tiles.first(), Some(&origin));
    assert_eq!(path.tiles.last(), Some(&target));
}

#[test]
fn equal_f_prefers_greater_g_with_a_canonical_route() {
    let origin = TileCoord::new(1, 1);
    let target = TileCoord::new(3, 2);
    let MovementOutcome::Path(path) = rectangle(origin, target, 100) else {
        panic!("progress path");
    };
    assert_eq!(path.tiles, vec![origin, TileCoord::new(2, 2), target]);
    assert_eq!(
        path.cost,
        u64::from(DIAGONAL_COST) + u64::from(ORTHOGONAL_COST)
    );
}

#[test]
fn incremental_planner_keeps_actual_cost_and_bounded_work() {
    use crate::{
        ElevationPage, FieldPyramid, PotentialBiomePage, PreparedEnvironment, PyramidLevel, Ratio,
        WaterPage,
    };
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![9],
    };
    let field = |root| FieldPyramid {
        levels: vec![PyramidLevel {
            samples_per_axis: 1,
            ordered_page_root: root,
        }],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: field(crate::ordered_page_root(std::slice::from_ref(&elevation)).unwrap()),
        water: Some(field(
            crate::ordered_water_page_root(std::slice::from_ref(&water)).unwrap(),
        )),
        vegetation: Some(field(
            crate::ordered_biome_page_root(std::slice::from_ref(&biome)).unwrap(),
        )),
        ..PreparedEnvironment::default()
    };
    let terrain = MapChunkGenerator::new([17; 32], 1, 64)
        .with_prepared_elevation(Ratio::new(1, 1).unwrap(), &environment, vec![elevation])
        .unwrap()
        .with_prepared_water(&environment, vec![water])
        .unwrap()
        .with_prepared_biomes(&environment, vec![biome])
        .unwrap();
    let origin = TileCoord::new(27, 27);
    let target = TileCoord::new(37, 35);
    let overlay = ResourceOverlay::default();
    let expected = heuristic(origin, target);
    let mut planner = RoutePlanner::new(origin, target, 256);
    let mut repeated = planner.clone();
    loop {
        let result = planner.poll(&terrain, &overlay, 7, &|| false);
        assert_eq!(result, repeated.poll(&terrain, &overlay, 7, &|| false));
        assert!(planner.work() <= 256);
        match result {
            RoutePlannerPoll::Pending => continue,
            RoutePlannerPoll::Path(path) => {
                assert_eq!(path.cost, expected);
                assert_eq!(path.tiles.last(), Some(&target));
                break;
            }
            other => panic!("bounded incremental route failed: {other:?}"),
        }
    }
    assert_eq!(planner.work(), repeated.work());
}

#[test]
fn complemented_queue_keys_keep_total_order_eq_consistency_and_actual_cost_round_trip() {
    for actual in [0, 1, 1024, 1448, u64::MAX - 1, u64::MAX] {
        assert_eq!(priority_cost(priority_cost(actual)), actual);
    }
    let keys = [0, 1, u64::MAX - 1, u64::MAX];
    for a in keys {
        for b in keys {
            let first = OpenNode::new(50, a, TileCoord::new(1, 2));
            let second = OpenNode::new(50, b, TileCoord::new(1, 2));
            assert_eq!(first.cmp(&second) == Ordering::Equal, first == second);
        }
    }
    assert_eq!(
        std::mem::size_of::<OpenNode>(),
        std::mem::size_of::<(u64, u64, TileCoord)>()
    );
}
