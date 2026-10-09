//! Base ecology never recursively applies forest-floor/object decoration.
use super::*;

#[test]
fn base_sampling_preserves_every_nonappearance_field_of_published_recipes() {
    let mut changed = 0;
    for recipe in 3..=8 {
        let generator =
            MapChunkGenerator::new([23; 32], 17, 512).with_elevation_sampling_recipe(recipe);
        for y in 0..512 {
            for x in (0..512).step_by(7) {
                let position = TileCoord::new(x, y);
                let base = generator.sample_base_tile(position);
                let decorated = generator.tile_at(position).expect("bounded published tile");
                changed += usize::from(base.material != decorated.material);
                let mut expected = base;
                if base.water == WaterKind::None && base.surface.walkable() {
                    expected.material = landscape::material_for_tile(
                        &generator,
                        position,
                        base.biome,
                        base.material,
                    );
                }
                expected.passable = expected.water == WaterKind::None
                    && expected.material != GroundMaterial::Ice
                    && expected.surface.walkable();
                assert_eq!(decorated, expected, "recipe {recipe}, tile {position:?}");
            }
        }
        assert!(generator.tile_at(TileCoord::new(-1, 0)).is_none());
        assert!(generator.tile_at(TileCoord::new(512, 0)).is_none());
    }
    assert!(changed > 0, "fixture must observe real legacy decoration");
}

#[test]
fn base_provider_without_environment_preserves_invalid_error() {
    let generator = MapChunkGenerator::new([23; 32], 17, 512);
    let tile = TileCoord::new(100, 100);
    assert_eq!(
        provider::sample_base_tile(&generator, tile, &|| false),
        Err(EnvironmentPageError::Invalid)
    );
    assert_eq!(
        provider::sample_tile(&generator, tile, &|| false),
        Err(EnvironmentPageError::Invalid)
    );
}
