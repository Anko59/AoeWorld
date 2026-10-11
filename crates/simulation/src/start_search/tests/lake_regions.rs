use super::*;

#[test]
fn lake_dominated_world_starts_on_the_largest_land_region() {
    let config = WorldConfig::new(512, 512, Seed(1)).unwrap();
    let mut coverage = vec![100; 32 * 32];
    // An attractive central island exceeds the area threshold but cannot
    // certify an exit. A distant mainland can; lake water separates them.
    for y in 15..=16 {
        for x in 15..=16 {
            coverage[y * 32 + x] = 0;
        }
    }
    for y in 4..=11 {
        for x in 20..=29 {
            coverage[y * 32 + x] = 0;
        }
    }
    let terrain = Terrain::Map {
        generator: flat_temperate_generator_with_water_field(32, coverage),
        overlay: ResourceOverlay::default(),
    };
    let result = terrain.search_start_checked(config, 64, || false).unwrap();
    let StartSearchResult::Found(tile) = result else {
        panic!("lake-rich map must find the mainland: {result:?}");
    };
    assert!(tile.x >= 320 && tile.y < 192, "mainland: {tile:?}");
    assert_start_contract(&terrain, config, result);
    let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
    assert!(cache.reaches_exit(tile).unwrap());
    assert!(cache.exit_visits <= START_EXIT_VISITS);
    assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    assert!(!terrain.passable(TileCoord::new(300, 255), config));
    assert_eq!(
        terrain.search_start_checked(config, 64, || false),
        Ok(result)
    );
}
