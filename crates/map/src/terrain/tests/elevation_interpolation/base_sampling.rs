//! Provider base callbacks preserve source failures before appearance decoration.
use super::*;

#[test]
fn provider_base_forwards_missing_pages_and_cancellation_without_fallback() {
    let (environment, _, _) = small_plane();
    let generator = MapChunkGenerator::new([4; 32], 3, 5)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(Pages(BTreeMap::new())),
        )
        .expect("source provider");
    for (cancelled, error) in [
        (false, EnvironmentPageError::Missing),
        (true, EnvironmentPageError::Cancelled),
    ] {
        let tile = TileCoord::new(2, 2);
        assert_eq!(
            provider::sample_base_tile(&generator, tile, &|| cancelled),
            Err(error)
        );
        assert_eq!(
            generator.tile_at_with_cancel(tile, &|| cancelled),
            Err(error)
        );
    }
}
