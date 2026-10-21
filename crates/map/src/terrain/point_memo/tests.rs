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
