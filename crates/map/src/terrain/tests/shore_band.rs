//! Shore band between shaped water and land: walkable beach or river bank on
//! land, shallow water on the lake/ocean side. Band widths are modelling
//! choices, not observed geography.
use super::shoreline::{CELL, Pages, WIDTH, at, bind, coast, composed, environment, wet};
use super::*;
use crate::{
    EnvironmentPage, HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind,
    HydrologyWaterModelPage, HydrologyWaterPolicy, LandscapeTile, ModernLandCoverPage,
    MovementOutcome, WaterFlowDirection, WaterModelProvenance,
};

/// Beach and bank reach: water within this squared tile distance (two tiles
/// straight, or a knight's step).
const BAND_DISTANCE_SQUARED: i32 = 5;

fn lake(key: u8) -> MapChunkGenerator {
    coast(key, &|sx, sy| {
        (
            0,
            if (12..=17).contains(&sx) && (12..=16).contains(&sy) {
                100
            } else {
                0
            },
        )
    })
}

fn nearest_water(tiles: &[LandscapeTile], x: i32, y: i32, reach: i32) -> Option<(i32, WaterKind)> {
    let mut nearest: Option<(i32, WaterKind)> = None;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let distance = dx * dx + dy * dy;
            if let Some(tile) = at(tiles, x + dx, y + dy)
                && tile.terrain.water != WaterKind::None
                && nearest.is_none_or(|(best, _)| distance < best)
            {
                nearest = Some((distance, tile.terrain.water));
            }
        }
    }
    nearest
}

fn land_adjacent(tiles: &[LandscapeTile], x: i32, y: i32) -> bool {
    (-1..=1).any(|dy| {
        (-1..=1).any(|dx| at(tiles, x + dx, y + dy).is_some() && !wet(tiles, x + dx, y + dy))
    })
}

#[test]
fn lakes_get_a_walkable_sand_beach_of_the_modelled_width_and_a_shallow_rim() {
    let generator = lake(51);
    let tiles = composed(&generator);
    let chunks = (WIDTH + crate::CHUNK_TILES - 1) / crate::CHUNK_TILES;
    let mut trees = std::collections::BTreeSet::new();
    for y in 0..chunks {
        for x in 0..chunks {
            let chunk = generator.chunk(x, y).expect("chunk");
            trees.extend(
                chunk
                    .resources
                    .iter()
                    .filter(|node| node.object == ObjectKind::Tree)
                    .map(|node| (node.tile.x, node.tile.y)),
            );
        }
    }
    assert!(!trees.is_empty(), "temperate fixture grows forest");
    let (mut beach, mut shallow, mut deep) = (0, 0, 0);
    for y in 0..WIDTH {
        for x in 0..WIDTH {
            let tile = at(&tiles, x, y).expect("tile").terrain;
            if tile.water == WaterKind::None {
                let near = nearest_water(&tiles, x, y, 2)
                    .is_some_and(|(distance, _)| distance <= BAND_DISTANCE_SQUARED);
                assert!(tile.passable, "flat shore land is walkable at {x},{y}");
                assert_eq!(
                    tile.material == GroundMaterial::Sand,
                    near,
                    "beach band mismatch at {x},{y}"
                );
                if near {
                    beach += 1;
                    assert!(!trees.contains(&(x, y)), "tree on the beach at {x},{y}");
                    let appearance = at(&tiles, x, y).expect("tile").appearance;
                    assert_eq!(
                        (appearance.canopy_strength, appearance.floor_strength),
                        (0, 0)
                    );
                }
            } else {
                assert!(!tile.passable, "water is never walkable at {x},{y}");
                if land_adjacent(&tiles, x, y) {
                    shallow += 1;
                    assert_eq!(tile.water, WaterKind::Shallow, "rim at {x},{y}");
                    assert_eq!(tile.material, GroundMaterial::Shore);
                } else {
                    deep += 1;
                    assert_eq!(tile.water, WaterKind::Lake, "open water at {x},{y}");
                    assert_eq!(tile.material, GroundMaterial::Water);
                }
            }
        }
    }
    assert!(
        beach > 150 && shallow > 100 && deep > 1_000,
        "{beach} {shallow} {deep}"
    );
}

#[test]
fn units_walk_the_beach_around_a_lake_and_never_through_it() {
    let generator = lake(51);
    let tiles = composed(&generator);
    // A beach tile on the west shore of the lake row y = 112.
    let shore_x = (0..WIDTH)
        .find(|x| wet(&tiles, *x, 112))
        .expect("west shore")
        - 1;
    let origin = TileCoord::new(shore_x, 112);
    assert_eq!(
        generator
            .tile_at(origin)
            .map(|tile| (tile.material, tile.passable)),
        Some((GroundMaterial::Sand, true))
    );
    let east_shore = (0..WIDTH)
        .rev()
        .find(|x| wet(&tiles, *x, 112))
        .expect("east shore")
        + 1;
    let destination = TileCoord::new(east_shore, 112);
    let MovementOutcome::Path(path) = crate::find_path(&generator, origin, destination, 65_536)
    else {
        panic!("no walkable route around the lake");
    };
    assert!(
        path.tiles.len() as i32 > east_shore - shore_x,
        "route went around"
    );
    for tile in &path.tiles {
        assert!(
            !wet(&tiles, tile.x, tile.y),
            "route crossed water at {tile:?}"
        );
    }
    let middle = TileCoord::new(116, 112);
    assert!(matches!(
        crate::find_path(&generator, origin, middle, 4_096),
        MovementOutcome::InvalidDestination | MovementOutcome::Unreachable
    ));
}

/// One modeled river column (evidence axis 33) on dry categorical water.
fn modeled_river(key: u8) -> MapChunkGenerator {
    let (mut environment, mut pages) = environment(33, 8, &|_, _| (0, 0));
    let river = |sx: u16, sy: u16| (sx == 16 && sy <= 20) || (sy == 20 && (16..=24).contains(&sx));
    let cells = (0..33_u16).flat_map(|sy| (0..33_u16).map(move |sx| (sx, sy)));
    let kind = |wet: bool| if wet { HydrologyKind::River } else { HydrologyKind::Land } as u8;
    let hydrology = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        kind: cells.clone().map(|(sx, sy)| kind(river(sx, sy))).collect(),
        method: cells
            .clone()
            .map(|(sx, sy)| {
                if river(sx, sy) {
                    HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8
                } else {
                    HydrologyEvidenceMethod::WorldCoverClass as u8
                }
            })
            .collect(),
        water_model: Some(HydrologyWaterModelPage {
            kind: cells.clone().map(|(sx, sy)| kind(river(sx, sy))).collect(),
            surface_level_centimeters: cells
                .clone()
                .map(|(sx, sy)| river(sx, sy).then_some(-300))
                .collect(),
            flow_direction: vec![WaterFlowDirection::Unknown as u8; 33 * 33],
            provenance: cells
                .map(|(sx, sy)| {
                    if river(sx, sy) {
                        WaterModelProvenance::ModelledRiverSurface as u8
                    } else {
                        WaterModelProvenance::EvidenceOnly as u8
                    }
                })
                .collect(),
        }),
    };
    hydrology.validate().expect("modeled river page");
    let cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 33,
        height: 33,
        worldcover_class: vec![30; 33 * 33],
    };
    for page in [
        EnvironmentPage::HydrologyEvidence(hydrology),
        EnvironmentPage::ModernLandCover(cover),
    ] {
        pages.0.insert(page.key(), Arc::new(page));
    }
    environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
        samples_per_axis: 33,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [3; 32],
        modern_land_cover_page_root: [4; 32],
        water_model: None,
    });
    bind(key, environment, Pages(pages.0))
}

#[test]
fn modeled_rivers_keep_their_level_stay_connected_and_get_mud_banks() {
    for key in [81, 82, 83] {
        let tiles = composed(&modeled_river(key));
        // Cell centres are protected core water of one connected river.
        let component = super::shoreline::water_component(&tiles, (16 * CELL, 0));
        for (sx, sy) in [(16, 10), (16, 20), (20, 20), (24, 20)] {
            assert!(
                component[(sy * CELL * WIDTH + sx * CELL) as usize],
                "key {key}: modeled river is cut before cell {sx},{sy}"
            );
        }
        let mut banks = 0;
        let mut straight_rows = std::collections::BTreeSet::new();
        for y in 0..WIDTH {
            if y < 18 * CELL {
                straight_rows.insert((0..WIDTH).find(|x| wet(&tiles, *x, y)).expect("river row"));
            }
            for x in 0..WIDTH {
                let tile = at(&tiles, x, y).expect("tile").terrain;
                if tile.water == WaterKind::None {
                    let near = nearest_water(&tiles, x, y, 2)
                        .is_some_and(|(distance, _)| distance <= BAND_DISTANCE_SQUARED);
                    assert_eq!(
                        tile.material == GroundMaterial::Mud,
                        near,
                        "bank at {x},{y}"
                    );
                    assert!(tile.passable);
                    banks += i32::from(near);
                } else {
                    // Rivers keep their kind to the bank: no shallow rim.
                    assert_eq!(tile.water, WaterKind::River, "river kind at {x},{y}");
                    assert_eq!(tile.water_provenance, Provenance::ModelDerived);
                    assert_eq!(tile.game_height_level, -3);
                    assert_eq!(tile.surface.corner_game_height_levels, [-3; 4]);
                    assert!(!tile.passable);
                }
            }
        }
        assert!(banks > 200, "key {key}: {banks} bank tiles");
        assert!(
            straight_rows.len() >= 3,
            "key {key}: river bank is a straight line"
        );
    }
}

#[test]
fn seeded_fallback_rivers_have_mud_banks_only_beside_river_water() {
    let generator = MapChunkGenerator::new([23; 32], 1, 512);
    let mut tiles = std::collections::BTreeMap::new();
    for cy in 0..16 {
        for cx in 0..16 {
            let chunk = generator.chunk(cx, cy).expect("fallback chunk");
            for (index, tile) in chunk.tiles.into_iter().enumerate() {
                let index = index as i32;
                tiles.insert((cx * 32 + index % 32, cy * 32 + index / 32), tile);
            }
        }
    }
    let mut mud = 0;
    for (&(x, y), tile) in &tiles {
        if tile.material != GroundMaterial::Mud {
            continue;
        }
        mud += 1;
        assert!(tile.passable && tile.water == WaterKind::None);
        let river_near = (-2..=2).any(|dy: i32| {
            (-2..=2).any(|dx: i32| {
                dx * dx + dy * dy <= BAND_DISTANCE_SQUARED
                    && tiles
                        .get(&(x + dx, y + dy))
                        .is_some_and(|near| near.water == WaterKind::River)
            })
        });
        assert!(river_near, "mud without a river at {x},{y}");
    }
    assert!(mud > 0);
}
