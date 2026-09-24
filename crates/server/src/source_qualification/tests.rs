use super::*;
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, PageLayer};

#[test]
fn fixed_centerline_route_uses_opposite_interior_edges() {
    let axis = QUALIFICATION_AXIS_TILES as i32;
    let y = (axis - 1) / 2;
    let points = [TileCoord::new(axis - 2, y), TileCoord::new(1, y)];
    assert_eq!(points[0], TileCoord::new(49_998, 24_999));
    assert_eq!(points[1], TileCoord::new(1, 24_999));
    assert_eq!(points[0].x.abs_diff(points[1].x), 49_997);
    assert!(points.windows(2).all(|pair| pair[0] != pair[1]));
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
