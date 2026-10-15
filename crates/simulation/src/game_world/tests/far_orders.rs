//! Orders hundreds or thousands of tiles away must start on the tick they are
//! issued and keep moving: planning is never a precondition for the first step.
use super::*;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, PotentialBiomePage, PreparedEnvironment,
    PyramidLevel, Ratio, WaterPage, ordered_biome_page_root, ordered_page_root,
    ordered_water_page_root,
};

const WIDTH: i32 = 4_096;
/// Barren ground: no generated trees or other blocking resources.
const BARREN: u8 = 28;

fn wide_barren_terrain() -> Terrain {
    prepared_terrain(vec![0; 4], vec![BARREN; 4])
}

pub(super) fn prepared_terrain(water_samples: Vec<u8>, biome_samples: Vec<u8>) -> Terrain {
    let elevation = |level, side: u8| ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![0; usize::from(side) * usize::from(side)],
    };
    let water = |level, side: u8| WaterPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        ocean_coverage_percent: if level == 0 {
            water_samples.clone()
        } else {
            vec![water_samples.iter().copied().max().unwrap(); 1]
        },
        inland_coverage_percent: vec![0; usize::from(side) * usize::from(side)],
    };
    let biome = |level, side: u8| PotentialBiomePage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        potential_biome_class: if level == 0 {
            biome_samples.clone()
        } else {
            vec![biome_samples[0]]
        },
    };
    let elevations = vec![elevation(0, 2), elevation(1, 1)];
    let waters = vec![water(0, 2), water(1, 1)];
    let biomes = vec![biome(0, 2), biome(1, 1)];
    let levels = |roots: [[u8; 32]; 2]| FieldPyramid {
        levels: vec![
            PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: roots[0],
            },
            PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: roots[1],
            },
        ],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: levels([
            ordered_page_root(&elevations[..1]).expect("elevation root"),
            ordered_page_root(&elevations[1..]).expect("elevation overview root"),
        ]),
        water: Some(levels([
            ordered_water_page_root(&waters[..1]).expect("water root"),
            ordered_water_page_root(&waters[1..]).expect("water overview root"),
        ])),
        vegetation: Some(levels([
            ordered_biome_page_root(&biomes[..1]).expect("biome root"),
            ordered_biome_page_root(&biomes[1..]).expect("biome overview root"),
        ])),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let generator = MapChunkGenerator::new([0; 32], 0, WIDTH)
        .with_prepared_elevation(
            Ratio::new(1, 1).expect("compression"),
            &environment,
            elevations,
        )
        .expect("flat elevation")
        .with_prepared_water(&environment, waters)
        .expect("dry water")
        .with_prepared_biomes(&environment, biomes)
        .expect("barren biome");
    Terrain::Map {
        generator,
        overlay: ResourceOverlay::default(),
    }
}

fn wide_world() -> GameWorld {
    let config = WorldConfig::new(WIDTH, WIDTH, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    world.terrain = wide_barren_terrain();
    world
}

fn center(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).expect("tile center")
}

#[test]
fn far_order_moves_on_the_tick_it_is_issued_and_never_waits_for_planning() {
    let mut world = wide_world();
    let mut replay = wide_world();
    let origin = center(1, 10);
    let target = center(1_500, 700);
    let unit = world.spawn_unit(PlayerId(0), origin).expect("unit");
    let twin = replay.spawn_unit(PlayerId(0), origin).expect("twin");

    assert!(world.issue_move(unit, target).expect("far order"));
    assert!(replay.issue_move(twin, target).expect("far replay order"));
    let issued = world.unit(unit).expect("unit");
    assert!(issued.moving, "a far order starts without a planning wait");
    assert!(!issued.planning);

    world.advance();
    replay.advance();
    assert_ne!(world.unit(unit).expect("unit").position, origin);

    let mut arrived_after = None;
    for tick in 2..=20_000_u32 {
        world.advance();
        replay.advance();
        let state = world.unit(unit).expect("unit");
        assert!(!state.planning, "planning pause at tick {tick}");
        assert_eq!(world.movement_failure(unit), None);
        if tick % 512 == 0 {
            assert_eq!(world.canonical_hash(), replay.canonical_hash());
        }
        if !state.moving {
            arrived_after = Some(tick);
            break;
        }
    }
    assert!(arrived_after.is_some(), "the far order never arrived");
    assert_eq!(world.unit(unit).expect("unit").position, target);
    assert_eq!(world.canonical_hash(), replay.canonical_hash());
    assert_eq!(world.active_planner_count, 0);
    assert_eq!(world.active_route_searches, 0);
}

#[test]
fn simultaneous_far_orders_all_move_on_the_first_tick() {
    let mut world = wide_world();
    let units = (0..12)
        .map(|index| {
            let row = 10 + 3 * index;
            let unit = world.spawn_unit(PlayerId(0), center(1, row)).expect("unit");
            (unit, center(1, row), center(3_000, row + 400))
        })
        .collect::<Vec<_>>();
    for (unit, _, target) in &units {
        assert!(world.issue_move(*unit, *target).expect("far order"));
    }
    world.advance();
    for (unit, origin, _) in &units {
        let state = world.unit(*unit).expect("unit");
        assert!(state.moving, "unit {unit:?} waits for a planner");
        assert!(!state.planning);
        assert_ne!(state.position, *origin);
    }
    for tick in 0..1_024 {
        world.advance();
        for (unit, _, _) in &units {
            let state = world.unit(*unit).expect("unit");
            assert!(
                state.moving && !state.planning,
                "unit {unit:?} paused at tick {tick}"
            );
        }
    }
    // All twelve left their first spatial chunk together; each must still be
    // indexed exactly once (a stale bucket slot used to panic here).
    let (indexed, _) = world.query(TileRect::from_xywh(0, 0, WIDTH, WIDTH));
    assert_eq!(indexed.len(), units.len());
}

#[test]
fn redirected_far_order_moves_toward_the_new_destination_at_once() {
    let mut world = wide_world();
    let origin = center(2_000, 2_000);
    let unit = world.spawn_unit(PlayerId(0), origin).expect("unit");
    assert!(world.issue_move(unit, center(4_000, 2_000)).expect("east"));
    for _ in 0..200 {
        world.advance();
    }
    let turned_at = world.unit(unit).expect("unit").position;
    assert!(turned_at.x > origin.x);
    assert!(world.issue_move(unit, center(10, 2_000)).expect("west"));
    let state = world.unit(unit).expect("unit");
    assert!(state.moving && !state.planning);
    for _ in 0..200 {
        world.advance();
        let state = world.unit(unit).expect("unit");
        assert!(state.moving && !state.planning);
    }
    assert!(world.unit(unit).expect("unit").position.x < turned_at.x);
}
