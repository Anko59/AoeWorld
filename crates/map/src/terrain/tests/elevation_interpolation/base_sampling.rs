//! Provider base callbacks preserve source failures before appearance decoration.
use super::*;

#[test]
fn provider_base_and_legacy_samples_share_all_nonappearance_fields() {
    for recipe in 3..=8 {
        let (environment, _, pages) = small_plane();
        let generator = MapChunkGenerator::new([4; 32], 3, 5)
            .with_elevation_sampling_recipe(recipe)
            .with_page_provider(
                Ratio::new(1, 1).expect("ratio"),
                environment,
                Arc::new(pages),
            )
            .expect("source provider");
        for y in 0..5 {
            for x in 0..5 {
                let tile = TileCoord::new(x, y);
                let base = provider::sample_base_tile(&generator, tile, &|| false)
                    .expect("undecorated source");
                let legacy =
                    provider::sample_tile(&generator, tile, &|| false).expect("decorated source");
                let mut expected = base;
                if base.water == WaterKind::None && base.surface.walkable() {
                    expected.material =
                        landscape::material_for_tile(&generator, tile, base.biome, base.material);
                }
                expected.passable = expected.water == WaterKind::None
                    && expected.material != GroundMaterial::Ice
                    && expected.surface.walkable();
                assert_eq!(legacy, expected);
            }
        }
    }
}

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
            provider::sample_tile(&generator, tile, &|| cancelled),
            Err(error)
        );
    }
}
