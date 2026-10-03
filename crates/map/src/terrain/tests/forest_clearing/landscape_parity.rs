use super::super::super::{clearing, landscape};
use super::*;

#[test]
fn recipe_seven_center_glade_reserves_a_playable_start_footprint() {
    let terrain = recipe_generator([17; 32], 5, crate::GENERATION_RECIPE_VERSION);
    let center = TileCoord::new(255, 255);
    for offset_y in -2..=2 {
        for offset_x in -2..=2 {
            let tile = TileCoord::new(center.x + offset_x, center.y + offset_y);
            assert!(landscape::opening_contains(&terrain, tile));
            assert!(clearing::contains(&terrain, tile));
            assert!(matches!(
                landscape::material_for_tile(
                    &terrain,
                    tile,
                    crate::biome_rules::Biome::Temperate,
                    crate::GroundMaterial::TemperateGrass,
                ),
                crate::GroundMaterial::Dirt | crate::GroundMaterial::DryGrass
            ));
        }
    }
}

#[test]
fn recipe_seven_prepared_and_provider_chunks_match_forest_landscape() {
    let key = [23; 32];
    let recipe = crate::GENERATION_RECIPE_VERSION;
    let (mut environment, elevation, water, mut biome, mut pages) = constant_pages();
    biome[0].potential_biome_class[0] = 8;
    environment
        .vegetation
        .as_mut()
        .expect("vegetation index")
        .levels[0]
        .ordered_page_root = crate::ordered_biome_page_root(&biome).expect("temperate root");
    let vegetation = EnvironmentPage::Vegetation(biome[0].clone());
    pages.0.insert(vegetation.key(), Arc::new(vegetation));

    let dense = prepared_generator(key, 11, recipe, &environment, elevation, water, biome);
    let provider = provider_generator(key, 11, recipe, environment, pages);
    let mut forest_floor = 0;
    let mut dry_grass = 0;
    let mut clearings = 0;
    let mut trails = 0;

    for chunk_y in 0..9 {
        for chunk_x in 0..9 {
            let dense_chunk = dense.chunk(chunk_x, chunk_y).expect("prepared chunk");
            let provider_chunk = provider.chunk(chunk_x, chunk_y).expect("provider chunk");
            assert_eq!(dense_chunk, provider_chunk, "chunk ({chunk_x}, {chunk_y})");

            for (index, tile) in dense_chunk.tiles.iter().enumerate() {
                let coordinate = TileCoord::new(
                    chunk_x * crate::CHUNK_TILES + (index % crate::CHUNK_TILES as usize) as i32,
                    chunk_y * crate::CHUNK_TILES + (index / crate::CHUNK_TILES as usize) as i32,
                );
                let in_clearing = clearing::contains(&dense, coordinate);
                forest_floor += usize::from(tile.material == crate::GroundMaterial::ForestFloor);
                dry_grass += usize::from(tile.material == crate::GroundMaterial::DryGrass);
                clearings += usize::from(in_clearing);
                trails += usize::from(
                    tile.material == crate::GroundMaterial::Dirt
                        && !in_clearing
                        && landscape::procedural_trail_contains(&dense, coordinate),
                );
            }
        }
    }

    assert!(
        forest_floor > 0,
        "dense canopy cores use forest-floor ground"
    );
    assert!(dry_grass > 0, "clearings include dry-grass patches");
    assert!(clearings > 0, "recipe seven emits open clearings");
    assert!(trails > 0, "recipe seven emits dirt procedural trails");
}
