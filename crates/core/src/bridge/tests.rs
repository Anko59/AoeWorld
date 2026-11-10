use super::*;

#[test]
fn bridge_rasterization_accepts_every_slope_and_reversed_drags_are_identical() {
    let origin = TileCoord::new(60, 60);
    for dx in -41_i32..=41 {
        for dy in -41_i32..=41 {
            if dx.abs().max(dy.abs()) < 2 {
                continue;
            }
            let end = TileCoord::new(origin.x + dx, origin.y + dy);
            let span = BridgeSpan::new(origin, end).unwrap();
            assert_eq!(BridgeSpan::new(end, origin), Some(span));
            let tiles = span.tiles();
            assert_eq!(tiles.first(), Some(&span.from));
            assert_eq!(tiles.last(), Some(&span.to));
            assert_eq!(tiles.len(), dx.abs().max(dy.abs()) as usize + 1);
            for tile in &tiles {
                assert!(span.contains(*tile));
            }
            for pair in tiles.windows(2) {
                assert_eq!(
                    pair[0]
                        .x
                        .abs_diff(pair[1].x)
                        .max(pair[0].y.abs_diff(pair[1].y)),
                    1
                );
            }
        }
    }
    assert!(BridgeSpan::new(origin, TileCoord::new(102, 60)).is_none());
    assert!(BridgeSpan::new(origin, origin).is_none());
    assert_eq!(raster_bridge_line(origin, origin), Some(vec![origin]));
    assert!(raster_bridge_line(origin, TileCoord::new(i32::MAX, 0)).is_none());
}

#[test]
fn bridge_start_snaps_one_tile_inland_or_into_water_in_every_drag_direction() {
    for (from, to) in [
        (TileCoord::new(10, 10), TileCoord::new(20, 10)),
        (TileCoord::new(20, 10), TileCoord::new(10, 10)),
        (TileCoord::new(10, 10), TileCoord::new(10, 20)),
        (TileCoord::new(10, 20), TileCoord::new(10, 10)),
        (TileCoord::new(10, 10), TileCoord::new(20, 20)),
    ] {
        let expected = BridgeSpan::new(from, to).unwrap();
        for direction in [-1, 1] {
            let off = TileCoord::new(
                from.x + (to.x - from.x).signum() * direction,
                from.y + (to.y - from.y).signum() * direction,
            );
            assert_eq!(
                snap_bridge_start(off, to, |span| span == expected),
                Some(expected)
            );
        }
        assert_eq!(
            snap_bridge_start(from, to, |span| span == expected),
            Some(expected)
        );
        assert_eq!(snap_bridge_start(from, to, |_| false), None);
    }
}
