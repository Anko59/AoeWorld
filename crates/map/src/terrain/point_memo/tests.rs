use super::super::super::MapChunkGenerator;
use aoe_core::TileCoord;

#[test]
fn memoized_answers_equal_direct_evaluation_and_reset_with_inputs() {
    let generator = MapChunkGenerator::new([7; 32], 11, 256);
    for y in (0..256).step_by(7) {
        for x in (0..256).step_by(5) {
            let tile = TileCoord::new(x, y);
            let direct = generator
                .landscape_point_with_cancel(tile, &|| false)
                .expect("direct")
                .expect("inside");
            for _ in 0..2 {
                assert_eq!(generator.tile_at(tile), Some(direct.tile.terrain));
                assert_eq!(
                    generator.object_at(tile),
                    direct.resource.map(|resource| resource.node)
                );
            }
        }
    }
    let moved = generator.clone().with_latitude(466_000_000, 60_000);
    let tile = TileCoord::new(100, 100);
    assert_eq!(
        moved.tile_at(tile),
        moved
            .landscape_point_with_cancel(tile, &|| false)
            .expect("direct")
            .map(|point| point.tile.terrain)
    );
}

#[test]
fn cancellation_and_outside_tiles_are_never_memoized_as_answers() {
    let generator = MapChunkGenerator::new([3; 32], 5, 64);
    let tile = TileCoord::new(10, 10);
    assert!(generator.tile_at_with_cancel(tile, &|| true).is_err());
    assert!(generator.tile_at(tile).is_some());
    assert!(generator.tile_at(TileCoord::new(-1, 0)).is_none());
    assert!(generator.tile_at(TileCoord::new(64, 0)).is_none());
}

#[test]
fn base_table_returns_what_was_inserted_and_matches_edge_evaluation() {
    use super::{PointMemo, SLOTS};
    let generator = MapChunkGenerator::new([9; 32], 3, 128);
    let tile = TileCoord::new(20, 30);
    let base = generator
        .base_physical_tile_with_cancel(tile, &|| false)
        .expect("base");
    let mut memo = PointMemo::default();
    assert_eq!(memo.get_base(tile), None);
    memo.insert_base(tile, base);
    if SLOTS > 0 {
        assert_eq!(memo.get_base(tile), Some(base));
    } else {
        assert_eq!(memo.get_base(tile), None);
    }
    // A cold generator and a warmed one give identical edge answers.
    for y in 0..8 {
        for x in 0..8 {
            let from = TileCoord::new(x, y);
            for to in [TileCoord::new(x + 1, y), TileCoord::new(x + 1, y + 1)] {
                let first = generator.edge_between_with_cancel(from, to, &|| false);
                let again = generator.edge_between_with_cancel(from, to, &|| false);
                let fresh = MapChunkGenerator::new([9; 32], 3, 128).edge_between_with_cancel(
                    from,
                    to,
                    &|| false,
                );
                assert_eq!(first, again);
                assert_eq!(first, fresh);
            }
        }
    }
}

#[test]
fn memo_memory_is_a_fixed_budget() {
    use super::{Point, SLOTS};
    let per_table = SLOTS
        * std::mem::size_of::<Option<(TileCoord, Point)>>().max(std::mem::size_of::<
            Option<(TileCoord, Option<super::super::super::Tile>)>,
        >());
    // Two tables per generator, never more than 40 MiB, and none in the browser.
    assert!(2 * per_table <= 40 << 20, "{per_table}");
    if cfg!(target_arch = "wasm32") {
        assert_eq!(SLOTS, 0);
    }
}
