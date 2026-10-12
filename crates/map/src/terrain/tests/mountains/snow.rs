use super::*;
use crate::{GroundMaterial, MapPackage, MapRequest, ObjectKind};

fn materials(
    generator: &MapChunkGenerator,
    rows: std::ops::Range<i32>,
) -> Vec<Vec<GroundMaterial>> {
    rows.map(|y| {
        (0..SIDE)
            .map(|x| {
                generator
                    .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                    .expect("point")
                    .expect("in bounds")
                    .tile
                    .terrain
                    .material
            })
            .collect()
    })
    .collect()
}

/// Uniform ground at one height, class and latitude (degrees).
fn level(height: i32, class: u8, latitude: i32) -> MapChunkGenerator {
    relief(1, class, move |_, _| height).with_latitude(latitude * 10_000_000, 60_000)
}

fn snow_share(generator: &MapChunkGenerator) -> usize {
    let rows = materials(generator, 600..664);
    let snow = rows
        .iter()
        .flatten()
        .filter(|material| **material == GroundMaterial::Snow)
        .count();
    snow * 100 / (rows.len() * SIDE as usize)
}

#[test]
fn snow_lies_above_a_line_with_a_natural_edge_and_rock_below_it() {
    // A gentle 4% slope from 1,400 m in the east to 3,700 m in the west.
    let generator = plane(-115, 0).with_latitude(460_000_000, 60_000);
    assert_eq!(
        generator.snow_line_centimeters(TileCoord::new(384, 384)) / 1_000,
        276
    );
    let rows = materials(&generator, 600..744);
    let height = |x: i32| {
        generator
            .sample_base_tile(TileCoord::new(x, 700))
            .geographic_height_centimeters
    };
    let mut edges = Vec::new();
    for row in &rows {
        for (x, material) in row.iter().enumerate() {
            let metres = height(x as i32) / 100;
            let snow = *material == GroundMaterial::Snow;
            assert!(!snow || metres > 2_769 - 400, "snow at {metres} m");
            assert!(snow || metres < 2_769 + 400, "bare at {metres} m");
            if *material == GroundMaterial::Rock {
                assert!(
                    (2_100 - 400..2_600 + 400).contains(&metres),
                    "rock at {metres} m"
                );
            }
            assert!(
                metres > 1_700
                    || matches!(
                        material,
                        GroundMaterial::TemperateGrass
                            | GroundMaterial::WornGrass
                            | GroundMaterial::GrassDirt
                            | GroundMaterial::LushGrass
                            | GroundMaterial::Dirt
                    )
            );
        }
        // Easternmost snow of the row: where the snow line crosses it.
        edges.push(
            row.iter()
                .rposition(|m| *m == GroundMaterial::Snow)
                .expect("snow"),
        );
        assert!(row.contains(&GroundMaterial::Rock));
    }
    let (west, east) = (*edges.iter().min().unwrap(), *edges.iter().max().unwrap());
    let mut distinct = edges.clone();
    distinct.sort_unstable();
    distinct.dedup();
    eprintln!(
        "snow edge wanders over {} tiles, {} positions",
        east - west,
        distinct.len()
    );
    // The line wanders, with no straight run of a chunk side along the slope.
    assert!(east - west >= 24 && distinct.len() >= 16);
    let longest_straight = edges
        .chunk_by(|left, right| left == right)
        .map(<[usize]>::len)
        .max()
        .unwrap();
    assert!(
        longest_straight < 16,
        "straight edge of {longest_straight} tiles"
    );
    // Nor does it sit on a chunk (32) or elevation sample (48) boundary.
    for grid in [32, 48] {
        let on_grid = edges.iter().filter(|x| (**x + 1) % grid == 0).count();
        assert!(on_grid * 10 < edges.len(), "edge on the {grid}-tile grid");
    }
}

#[test]
fn the_snow_line_falls_toward_the_poles() {
    // Tens of metres at the map centre.
    let line = |latitude: i32| {
        (level(0, 13, latitude).snow_line_centimeters(TileCoord::new(384, 384)) + 500) / 1_000
    };
    assert_eq!(
        [line(20), line(36), line(46), line(62), line(72)],
        [520, 377, 277, 117, 17]
    );
    assert!(line(80) < -50);
    assert_eq!(line(-62), 117);
    assert_eq!(snow_share(&level(1_600, 13, 46)), 0);
    assert_eq!(snow_share(&level(1_600, 13, 62)), 100);
    assert_eq!(snow_share(&level(3_200, 13, 46)), 100);
    assert_eq!(snow_share(&level(3_200, 13, 30)), 0);
    // Without a geographic request the line is the 46° one.
    assert_eq!(snow_share(&relief(1, 13, |_, _| 3_200)), 100);
    assert_eq!(snow_share(&relief(1, 13, |_, _| 1_600)), 0);
}

#[test]
fn a_package_binds_its_latitude_and_north_is_up() {
    let request = MapRequest {
        center_latitude_e7: 615_000_000,
        requested_side_meters: 600_000,
        ..MapRequest::default()
    };
    let package = MapPackage::new(1, request, Vec::new()).expect("package");
    let generator = package.generator();
    let side = generator.width_tiles;
    let at = |y: i32| generator.latitude_e7(TileCoord::new(side / 2, y));
    assert!((at(side / 2) - 615_000_000).abs() < 100_000);
    // 600 km is about 5.4 degrees of latitude.
    let span = at(0) - at(side - 1);
    assert!((53_000_000..55_000_000).contains(&span), "{span}");
    assert!(
        generator.snow_line_centimeters(TileCoord::new(0, 0))
            < generator.snow_line_centimeters(TileCoord::new(0, side - 1))
    );
}

#[test]
fn cold_lowlands_are_summer_country_not_snow_squares() {
    // Boreal forest (15), tundra (28) and steppe (20) lowlands at 61.5° N.
    for class in [15, 28, 20] {
        let generator = level(150, class, 61);
        let rows = materials(&generator, 600..632);
        assert!(
            rows.iter().flatten().all(|material| matches!(
                material,
                GroundMaterial::TemperateGrass
                    | GroundMaterial::DryGrass
                    | GroundMaterial::ForestFloor
                    | GroundMaterial::WornGrass
                    | GroundMaterial::GrassDirt
                    | GroundMaterial::LushGrass
                    | GroundMaterial::Dirt
            )),
            "class {class}"
        );
    }
    // The same tundra in the high arctic is under snow.
    assert_eq!(snow_share(&level(150, 28, 80)), 100);
    // Near the snow line tundra vegetation is the colder signal, and no tree
    // stands on snow or bare rock at any latitude.
    let (tundra, temperate) = (level(1_150, 28, 62), level(1_150, 13, 62));
    assert!(snow_share(&tundra) >= snow_share(&temperate) + 10);
    for y in (600..664).step_by(2) {
        for x in (0..SIDE).step_by(2) {
            let point = temperate
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .expect("point")
                .expect("in bounds");
            let bare = matches!(
                point.tile.terrain.material,
                GroundMaterial::Snow | GroundMaterial::Rock
            );
            assert!(
                !(bare
                    && point
                        .resource
                        .is_some_and(|r| r.node.object == ObjectKind::Tree))
            );
        }
    }
}

#[test]
fn alpine_country_has_forest_then_rock_then_snow_and_no_tree_in_the_snow() {
    let generator = alpine(1).with_latitude(459_000_000, 60_000);
    let (mut snow, mut rock, mut trees) = (0, 0, 0);
    let (mut lowest_snow, mut highest_tree) = (i32::MAX, i32::MIN);
    for y in (0..SIDE).step_by(3) {
        for x in (0..SIDE).step_by(3) {
            let point = generator
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .expect("point")
                .expect("in bounds");
            let height = point.tile.terrain.geographic_height_centimeters / 100;
            let tree = point
                .resource
                .is_some_and(|resource| resource.node.object == ObjectKind::Tree);
            match point.tile.terrain.material {
                GroundMaterial::Snow => {
                    snow += 1;
                    lowest_snow = lowest_snow.min(height);
                    assert!(!tree, "tree in the snow at {height} m");
                }
                GroundMaterial::Rock => rock += 1,
                _ => {}
            }
            if tree {
                trees += 1;
                highest_tree = highest_tree.max(height);
            }
        }
    }
    eprintln!(
        "snow={snow} rock={rock} trees={trees} lowest snow={lowest_snow} m highest tree={highest_tree} m"
    );
    assert!(snow > 1_000 && rock > 1_000 && trees > 1_000);
    assert!(lowest_snow > 2_350 && highest_tree < 2_300);
}

#[test]
fn forage_bushes_follow_the_composed_ground_and_never_stand_on_rock_or_snow() {
    let (mut bushes, mut covered, mut ores) = (0, 0, 0);
    for height in [0, 2400, 3300] {
        let generator = level(height, 13, 46);
        for y in 18..22 {
            for x in 6..10 {
                let chunk = generator
                    .landscape_chunk_with_cancel(x, y, &|| false)
                    .expect("chunk");
                covered += chunk
                    .tiles
                    .iter()
                    .filter(|tile| {
                        matches!(
                            tile.terrain.material,
                            GroundMaterial::Rock | GroundMaterial::Snow
                        )
                    })
                    .count();
                for resource in &chunk.resources {
                    let ground = chunk
                        .tiles
                        .iter()
                        .find(|tile| tile.tile == resource.node.tile)
                        .expect("resource terrain")
                        .terrain
                        .material;
                    if resource.node.object == ObjectKind::ForageBush {
                        assert!(
                            !matches!(
                                ground,
                                GroundMaterial::Rock | GroundMaterial::Snow | GroundMaterial::Ice
                            ),
                            "bush on {ground:?} at {:?}",
                            resource.node.tile
                        );
                        bushes += 1;
                    } else if matches!(
                        resource.node.object,
                        ObjectKind::GoldDeposit | ObjectKind::StoneDeposit
                    ) {
                        ores += 1;
                    }
                }
            }
        }
    }
    assert!(covered > 10_000, "substantial exposed mountain ground");
    assert!(
        bushes > 0 && ores > 0,
        "vegetated forage and mineral deposits remain"
    );
}

#[test]
fn visual_variety_keeps_climatic_rock_and_snow_cover_and_no_forage_in_it() {
    use crate::terrain::relief::Cover;
    let mut counts = [0; 2];
    for height in [2400, 3300] {
        let generator = level(height, 13, 46);
        for y in 18..22 {
            for x in 6..10 {
                let chunk = generator
                    .landscape_chunk_with_cancel(x, y, &|| false)
                    .expect("chunk");
                for tile in &chunk.tiles {
                    let Some(cover) = generator.ground_cover(
                        tile.tile,
                        tile.terrain.geographic_height_centimeters,
                        tile.terrain.biome,
                    ) else {
                        continue;
                    };
                    let (slot, expected) = match cover {
                        Cover::Rock => (0, GroundMaterial::Rock),
                        Cover::Snow => (1, GroundMaterial::Snow),
                    };
                    counts[slot] += 1;
                    assert_eq!(
                        tile.terrain.material, expected,
                        "climatic cover at {:?}",
                        tile.tile
                    );
                    assert!(
                        !chunk
                            .resources
                            .iter()
                            .any(|resource| resource.node.tile == tile.tile
                                && resource.node.object == ObjectKind::ForageBush)
                    );
                }
            }
        }
    }
    assert!(
        counts.into_iter().all(|count| count > 10_000),
        "rock and snow controls: {counts:?}"
    );
}
