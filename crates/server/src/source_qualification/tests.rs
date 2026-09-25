use super::*;
use aoe_core::WorldConfig;
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, PageLayer};

#[test]
fn ordinary_activation_search_is_unchanged_and_route_extent_is_local() {
    assert_eq!(ORDINARY_ACTIVATION_SEARCH_CHUNKS, 64);
    let config = WorldConfig::new(64, 64, aoe_core::Seed(5)).expect("config");
    let start = TileCoord::new(32, 32);
    let fixed = route::plan_fixed_repeated_route(
        &aoe_simulation::Terrain::uniform(config.seed.0),
        config,
        start,
    )
    .expect("fixed local route");
    assert_eq!(fixed.start, start);
    assert_eq!(fixed.spatial_extent_tiles(), [2, 0]);
    assert!(fixed.spatial_extent_tiles()[0] < config.width_tiles);
}

#[test]
fn provider_page_key_enumeration_is_bounded_by_supported_pyramids() {
    // Bound the walker from the largest supported prepared pyramid and typed
    // evidence grids, rather than one fixture's 1,024-sample fields.
    assert_eq!(pyramid_page_count(1_024), 347);
    assert_eq!(pyramid_page_count(4_096), 5_467);
    let maximum_prepared_pages = pyramid_page_count(aoe_map::MAX_ENVIRONMENT_SAMPLES_PER_AXIS);
    let maximum_typed_pages = 2 * usize::from(
        aoe_map::MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES)),
    )
    .pow(2);
    let maximum_walked_pages = 4 * maximum_prepared_pages + maximum_typed_pages;
    assert_eq!(maximum_prepared_pages, 87_387);
    assert_eq!(maximum_typed_pages, 512);
    assert_eq!(maximum_walked_pages, 350_060);
    assert!(maximum_walked_pages * 96 < 64 * 1024 * 1024);
    assert_eq!(MAX_RESIDENT_PAGES, 128);
    const { assert!(MAX_ROUTE_TICKS <= 1_200_000) };
}

#[test]
fn typed_evidence_pages_are_included_in_eviction_walk() {
    let mut package =
        MapPackage::new(1, aoe_map::MapRequest::default(), Vec::new()).expect("package");
    let base_pages = page_keys(&package).len();
    package.environment.hydrology_evidence = Some(aoe_map::HydrologyEvidenceIndex {
        samples_per_axis: 65,
        page_samples: 64,
        world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
        policy: aoe_map::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [1; 32],
        modern_land_cover_page_root: [2; 32],
    });
    let keys = page_keys(&package);
    assert_eq!(keys.len(), base_pages + 8);
    for layer in [PageLayer::HydrologyEvidence, PageLayer::ModernLandCover] {
        let matching: Vec<_> = keys.iter().filter(|key| key.layer == layer).collect();
        assert_eq!(matching.len(), 4);
        assert!(matching.iter().all(|key| key.level == 0));
        assert!(matching.iter().any(|key| key.x == 1 && key.y == 1));
    }
}

#[test]
fn resource_search_is_a_fixed_bounded_center_window() {
    assert_eq!(MAX_RESOURCE_SCAN_SIDE, 64);
    let center = (50_000 - 1) / 2;
    let half = MAX_RESOURCE_SCAN_SIDE / 2;
    assert_eq!((center - half, center + half), (24_967, 25_031));
    assert_eq!((2 * half) * (2 * half), 4_096);
}

#[test]
fn center_resource_search_and_temporary_state_are_bounded() {
    let empty = aoe_map::MapChunkGenerator::new([3; 32], 1, 0);
    assert!(matches!(
        find_center_resource(&empty, 0),
        Err(SourceQualificationError::NoResource)
    ));

    let node = (0_u64..32)
        .find_map(|seed| {
            find_center_resource(&aoe_map::MapChunkGenerator::new([3; 32], seed, 64), 64).ok()
        })
        .expect("bounded procedural resource");
    assert!(node.tile.x >= 0 && node.tile.x < 64);
    assert!(node.tile.y >= 0 && node.tile.y < 64);

    let scratch = QualificationDirectory::new().expect("scratch directory");
    let path = scratch.path.clone();
    assert!(path.is_dir());
    drop(scratch);
    assert!(!path.exists());
    assert_eq!(hex(&[0, 1, 15, 255]), "00010fff");
}

#[tokio::test]
async fn qualification_rejects_unbounded_tick_requests_before_source_access() {
    for max_ticks in [0, MAX_ROUTE_TICKS + 1] {
        assert!(matches!(
            run_source_qualification(Path::new("missing"), "missing", max_ticks, |_| {}).await,
            Err(SourceQualificationError::TickLimit)
        ));
    }
}
