use super::*;

#[test]
fn unshaped_grids_reproduce_the_rounded_source_lookup() {
    let grid = CellGrid::source(128, 200);
    assert!(!grid.shaped());
    for tile in 0..200_i64 {
        assert_eq!(grid.nearest(tile as i32), (tile * 127 + 99) / 199);
    }
    let blocks = CellGrid::blocks(16);
    for tile in -40..40_i32 {
        assert_eq!(blocks.nearest(tile), i64::from(tile.div_euclid(16)));
    }
    assert!(blocks.shaped() && CellGrid::blocks(4).shaped());
    assert!(!CellGrid::source(1, 512).shaped());
}

#[test]
fn threshold_noise_is_bounded_and_the_core_protects_the_nearest_tile_row() {
    for cell in [3 * UNIT, 4 * UNIT, 8 * UNIT, 40 * UNIT, 315 * UNIT] {
        let limit = core(cell) - FULL / 2;
        let mut extremes = (0, 0);
        for y in -80..80 {
            for x in -80..80 {
                let step = (cell / (8 * UNIT)).max(1) as i32;
                let tile = TileCoord::new(x * 3 * step, y * 7 * step);
                let noise = threshold_noise(0x51ed_270b, cell, tile);
                assert!(noise.abs() <= limit);
                extremes = (extremes.0.min(noise), extremes.1.max(noise));
            }
        }
        assert!(
            extremes.0 < -limit / 3 && extremes.1 > limit / 3,
            "{cell}: {extremes:?}"
        );
        // Half a tile beside a fully wet segment, stretched by the largest
        // warp gradient (two reaches per lattice), stays in the core.
        let gradient = 2 * warp_reach(cell) * FULL / WARP_LATTICE;
        let row = (UNIT / 2) * (FULL + gradient) / FULL;
        assert!(gradient <= FULL * 32 / 100);
        assert!(FULL - FULL * row / cell >= core(cell), "{cell}");
        assert!(core(cell) > FULL * 3 / 4);
        for y in -40..40 {
            for x in -40..40 {
                let tile = TileCoord::new(x * 5, y * 3);
                let (wx, wy) = warped(0x51ed_270b, cell, tile);
                assert!((wx - i64::from(tile.x) * UNIT).abs() <= warp_reach(cell));
                assert!((wy - i64::from(tile.y) * UNIT).abs() <= warp_reach(cell));
            }
        }
    }
}
