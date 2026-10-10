use super::*;

#[test]
fn source_water_is_not_cleared_to_certify_a_start() {
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    let terrain = Terrain::Map {
        generator: flat_temperate_generator_with_water(100),
        overlay: ResourceOverlay::default(),
    };
    assert_eq!(
        terrain.search_start_checked(config, 64, || false),
        Ok(StartSearchResult::Unavailable)
    );
    assert!(!terrain.passable(TileCoord::new(31, 31), config));
}

#[test]
fn small_worlds_certify_a_bounded_route_toward_the_edge() {
    let terrain = Terrain::uniform(1);
    for width in [32, 64, 128, 512] {
        let config = WorldConfig::new(width, width, Seed(1)).expect("config");
        let center = TileCoord::new((width - 1) / 2, (width - 1) / 2);
        assert_eq!(
            terrain.search_start_checked(config, 64, || false),
            Ok(StartSearchResult::Found(center))
        );
        let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
        assert!(cache.reaches_exit(center).expect("bounded exit"));
        assert!(cache.exit_visits <= START_EXIT_VISITS);
        assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    }
}

#[test]
fn isolated_large_world_glade_is_not_a_certified_start() {
    let config = WorldConfig::new(512, 512, Seed(1)).expect("config");
    let center = TileCoord::new(255, 255);
    // Four central source cells cover a roughly 33-tile land island in ocean.
    // Its component exceeds 256 tiles, but no point can reach 64 tiles away.
    // A power-of-two axis has a complete 32/16/8/4/2/1 verified mip chain.
    let mut coverage = vec![100; 32 * 32];
    for y in 15..=16 {
        for x in 15..=16 {
            coverage[y * 32 + x] = 0;
        }
    }
    let terrain = Terrain::Map {
        generator: flat_temperate_generator_with_water_field(32, coverage),
        overlay: ResourceOverlay::default(),
    };
    let mut cache = StartPassabilityCache::new(&terrain, config, &|| false);
    assert!(clear_starting_area(&mut cache, center).expect("footprint"));
    assert!(
        cache
            .reaches_required_tiles(center)
            .expect("reachable component")
    );
    assert!(
        !cache
            .reaches_exit(center)
            .expect("isolated source component")
    );
    assert!(cache.exit_visits <= START_EXIT_VISITS);
    assert!(cache.exit_chunks.len() <= START_SEARCH_CHUNKS);
    assert!(!matches!(
        terrain.search_start_checked(config, 64, || false),
        Ok(StartSearchResult::Found(_))
    ));
}
