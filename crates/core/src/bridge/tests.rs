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
        let water = |tile| {
            let tiles = expected.tiles();
            Some(tiles[1..tiles.len() - 1].contains(&tile))
        };
        for direction in [-1, 1] {
            let off = TileCoord::new(
                from.x + (to.x - from.x).signum() * direction,
                from.y + (to.y - from.y).signum() * direction,
            );
            assert_eq!(
                snap_bridge_start(off, to, water, |span| span == expected),
                Some(expected)
            );
        }
        assert_eq!(
            snap_bridge_start(from, to, water, |span| span == expected),
            Some(expected)
        );
        assert_eq!(snap_bridge_start(from, to, water, |_| false), None);
    }
}

#[test]
fn a_valid_bank_start_does_not_rotate_to_rescue_a_lateral_end() {
    // The browser fixture has just three water cells; both side banks are land.
    let water = |tile: TileCoord| tile.y == 8189 && (8189..=8191).contains(&tile.x);
    let valid = |span: BridgeSpan| {
        let tiles = span.tiles();
        !water(span.from) && !water(span.to) && tiles[1..tiles.len() - 1].iter().copied().all(water)
    };
    for (start, end, side) in [(8188, 8192, 8189), (8192, 8188, 8191)] {
        let from = TileCoord::new(start, 8189);
        let lateral = TileCoord::new(end, 8188);
        // This side bank fits the terrain, but changes the chosen crossing.
        assert!(valid(
            BridgeSpan::new(TileCoord::new(side, 8190), lateral).unwrap()
        ));
        assert_eq!(
            snap_bridge_start(from, lateral, |t| Some(water(t)), valid),
            None
        );
        let to = TileCoord::new(end, 8189);
        for offset in [-1, 1] {
            assert_eq!(
                snap_bridge_start(
                    TileCoord::new(start + offset, 8189),
                    to,
                    |t| Some(water(t)),
                    valid,
                ),
                BridgeSpan::new(from, to)
            );
        }
    }
}
