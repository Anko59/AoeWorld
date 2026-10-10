//! Base sampling without a bound environment fails closed.
use super::*;

#[test]
fn base_provider_without_environment_preserves_invalid_error() {
    let generator = MapChunkGenerator::new([23; 32], 17, 512);
    let tile = TileCoord::new(100, 100);
    assert_eq!(
        provider::sample_base_tile(&generator, tile, &|| false),
        Err(EnvironmentPageError::Invalid)
    );
}
