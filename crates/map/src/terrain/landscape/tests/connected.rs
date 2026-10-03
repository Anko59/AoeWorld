use super::*;

#[test]
fn recipe_eight_links_every_local_node_and_the_central_glade() {
    for key in [3_u8, 17, 71, 255] {
        for width in [512, 1_024, 70_000] {
            let terrain = MapChunkGenerator::new([key; 32], 600, width);
            let coordinate = (width - 1) / 2;
            let center = TileCoord::new(coordinate, coordinate);
            let cell = (
                coordinate.div_euclid(OPENING_GRID_TILES),
                coordinate.div_euclid(OPENING_GRID_TILES),
            );
            let end = shape_center(&terrain, cell);
            assert!(starting_connector_contains(&terrain, center));
            assert!(starting_connector_contains(&terrain, end));
            let distance = center
                .x
                .abs_diff(end.x)
                .max(center.y.abs_diff(end.y))
                .max(1);
            for step in 0..=distance {
                let tile = TileCoord::new(
                    center.x
                        + ((i64::from(end.x - center.x) * i64::from(step)) / i64::from(distance))
                            as i32,
                    center.y
                        + ((i64::from(end.y - center.y) * i64::from(step)) / i64::from(distance))
                            as i32,
                );
                assert!(procedural_trail_contains(&terrain, tile));
                assert!(clearing::suppresses_objects(
                    &terrain,
                    tile,
                    Biome::Temperate
                ));
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cell = (cell.0 + dx, cell.1 + dy);
                    assert!(opening_geometry_for_cell(&terrain, cell).is_some());
                    for axis in 0..=1 {
                        let segment = trail_segment(&terrain, cell, axis).expect("mandatory edge");
                        assert!(opening_contains(&terrain, segment.start));
                        assert!(opening_contains(&terrain, segment.end));
                    }
                }
            }
        }
    }
}

#[test]
fn recipe_seven_remains_optional_and_has_no_new_start_connector() {
    let old = generator(crate::PRIOR_FOREST_GENERATION_RECIPE_VERSION);
    assert!(!starting_connector_contains(&old, TileCoord::new(511, 511)));
    assert!((0..12).any(|y| (0..12).any(|x| opening_geometry_for_cell(&old, (x, y)).is_none())));
    assert!((0..12).any(|y| (0..12).any(|x| trail_segment(&old, (x, y), 0).is_none())));
    let new = generator(crate::GENERATION_RECIPE_VERSION);
    for y in (0..512).step_by(3) {
        for x in (0..512).step_by(3) {
            let tile = TileCoord::new(x, y);
            let before = old.tile_at(tile).expect("old");
            let after = new.tile_at(tile).expect("new");
            assert_eq!(
                before.geographic_height_centimeters,
                after.geographic_height_centimeters
            );
            assert_eq!(before.game_height_level, after.game_height_level);
            assert_eq!(before.surface, after.surface);
            assert_eq!(before.biome, after.biome);
            assert_eq!(before.water, after.water);
            assert_eq!(before.passable, after.passable);
            assert_eq!(before.elevation_provenance, after.elevation_provenance);
            assert_eq!(before.water_provenance, after.water_provenance);
            assert_eq!(before.vegetation_provenance, after.vegetation_provenance);
        }
    }
}
