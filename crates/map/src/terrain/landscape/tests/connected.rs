use super::*;

#[test]
fn every_local_node_links_to_the_central_glade() {
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
                assert!(terrain.landscape_reservations_at(tile).route);
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let cell = (cell.0 + dx, cell.1 + dy);
                    let shape = opening_geometry_for_cell(&terrain, cell);
                    assert!(geometry::point_in_polygon(
                        &shape.vertices,
                        shape_center(&terrain, cell)
                    ));
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
