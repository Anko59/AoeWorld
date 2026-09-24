use super::*;
use crate::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, RoutePlanner, RoutePlannerPoll, WaterPage, ordered_biome_page_root,
    ordered_page_root, ordered_water_page_root,
};

fn flat_terrain_with_water_wall(width: i32, wall_x: i32, gap_y: i32) -> MapChunkGenerator {
    let mut elevation_pages = Vec::new();
    let mut elevation_levels = Vec::new();
    let mut water_pages = Vec::new();
    let mut water_levels = Vec::new();
    let mut biome_pages = Vec::new();
    let mut biome_levels = Vec::new();
    let mut axis = width.min(64) as u16;
    let mut level = 0_u8;
    loop {
        let side = axis as u8;
        let sample_count = usize::from(axis) * usize::from(axis);
        let elevation = ElevationPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            geographic_height_centimeters: vec![0; sample_count],
        };
        elevation_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                .expect("elevation root"),
        });
        elevation_pages.push(elevation);

        let mut ocean = vec![0; sample_count];
        if level == 0 && wall_x >= 0 {
            for y in 0..i32::from(axis) {
                if y != gap_y {
                    ocean[(y * i32::from(axis) + wall_x) as usize] = 100;
                }
            }
        }
        let water = WaterPage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            ocean_coverage_percent: ocean,
            inland_coverage_percent: vec![0; sample_count],
        };
        water_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                .expect("water root"),
        });
        water_pages.push(water);

        let biome = PotentialBiomePage {
            level,
            x: 0,
            y: 0,
            width: side,
            height: side,
            potential_biome_class: vec![28; sample_count],
        };
        biome_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&biome))
                .expect("biome root"),
        });
        biome_pages.push(biome);
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
        level += 1;
    }
    let environment = PreparedEnvironment {
        samples_per_axis: width.min(64) as u16,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: elevation_levels,
        },
        water: Some(FieldPyramid {
            levels: water_levels,
        }),
        vegetation: Some(FieldPyramid {
            levels: biome_levels,
        }),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    MapChunkGenerator::new([0; 32], 0, width)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            elevation_pages,
        )
        .expect("flat elevation")
        .with_prepared_water(&environment, water_pages)
        .expect("wall coverage")
        .with_prepared_biomes(&environment, biome_pages)
        .expect("barren biome")
}

fn collect_route(
    planner: &mut RoutePlanner,
    terrain: &MapChunkGenerator,
    budget: u32,
) -> Vec<TileCoord> {
    let overlay = ResourceOverlay::default();
    let mut tiles = Vec::new();
    for _ in 0..512 {
        match planner.poll(terrain, &overlay, budget, &|| false) {
            RoutePlannerPoll::Pending => {}
            RoutePlannerPoll::Path(path) => {
                if let Some(previous) = tiles.last() {
                    assert_eq!(path.tiles.first(), Some(previous));
                }
                tiles.extend(path.tiles.into_iter().skip(usize::from(!tiles.is_empty())));
                if planner.is_terminal() {
                    return tiles;
                }
            }
            other => panic!("fixed long route failed: {other:?}"),
        }
    }
    panic!("fixed route did not finish within the poll limit")
}

#[test]
fn fixed_128_fine_cell_route_continues_across_segments() {
    const WIDTH: i32 = 4_096;
    const START_X: i32 = 1;
    const END_X: i32 = WIDTH - 2;
    const ROW: i32 = 10;
    let terrain = flat_terrain_with_water_wall(WIDTH, -1, -1);
    let origin = TileCoord::new(START_X, ROW);
    let destination = TileCoord::new(END_X, ROW);
    let mut planner = RoutePlanner::new(origin, destination, MAX_ROUTE_PLANNER_WORK);
    let tiles = collect_route(&mut planner, &terrain, 16_384);
    assert_eq!(tiles.first(), Some(&origin));
    assert_eq!(tiles.last(), Some(&destination));
    assert_eq!(tiles.len(), (END_X - START_X + 1) as usize);
    assert!(tiles.iter().all(|tile| tile.y == ROW));
    assert!(tiles.windows(2).all(|pair| {
        terrain
            .edge_between(pair[0], pair[1])
            .eq(&EdgePassability::Passable)
    }));
    assert!(planner.work() > (END_X - START_X) as u32);
    assert!(planner.work() < 100_000);
    assert!(planner.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);
}

#[test]
fn fixed_wall_detour_uses_its_only_proven_gateway() {
    let terrain = flat_terrain_with_water_wall(64, 32, 5);
    let origin = TileCoord::new(1, 32);
    let destination = TileCoord::new(62, 32);
    let mut planner = RoutePlanner::new(origin, destination, 100_000);
    let tiles = collect_route(&mut planner, &terrain, 4_096);
    assert!(tiles.contains(&TileCoord::new(32, 5)));
    assert!(tiles.iter().all(|tile| tile.x != 32 || tile.y == 5));
    assert_eq!(tiles.first(), Some(&origin));
    assert_eq!(tiles.last(), Some(&destination));
    assert!(tiles.windows(2).all(|pair| {
        terrain
            .edge_between(pair[0], pair[1])
            .eq(&EdgePassability::Passable)
    }));
    assert!(planner.work() > 0);
    assert!(planner.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);
}

#[test]
fn fixed_wall_without_a_gateway_is_proven_disconnected() {
    let terrain = flat_terrain_with_water_wall(64, 32, -1);
    let origin = TileCoord::new(1, 32);
    let destination = TileCoord::new(62, 32);
    let mut planner = RoutePlanner::new(origin, destination, 100_000);
    let overlay = ResourceOverlay::default();
    let result = loop {
        match planner.poll(&terrain, &overlay, 4_096, &|| false) {
            RoutePlannerPoll::Pending => {}
            outcome => break outcome,
        }
    };
    assert_eq!(result, RoutePlannerPoll::Unreachable);
    assert!(planner.is_terminal());
    assert_eq!(
        planner.poll(&terrain, &overlay, 4_096, &|| false),
        RoutePlannerPoll::Unreachable
    );
    assert!(planner.work() < 100_000);
    assert!(planner.peak_retained_entries() <= MAX_ROUTE_PLANNER_NODES);
}
