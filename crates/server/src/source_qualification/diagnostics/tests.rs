use super::*;
use crate::PageResidency;
use aoe_core::Seed;
use aoe_map::MapRequest;

#[test]
fn bounded_start_diagnostic_describes_one_complete_chunk() {
    let directory = tempfile::tempdir().expect("package directory");
    let package =
        MapPackage::new(9, MapRequest::default(), Vec::new()).expect("source-free package");
    let generator = package.generator();
    let terrain = Terrain::from_package(&package);
    let provider =
        PageResidency::open(directory.path(), &package, &|| false).expect("empty residency");
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");

    let diagnostic = start_diagnostic(
        &terrain,
        &generator,
        &package,
        &provider,
        config,
        4,
        Some(TileCoord::new(31, 31)),
    )
    .expect("bounded diagnostic");

    assert!(diagnostic.contains("center=(31,31)"));
    assert!(diagnostic.contains("scanned_chunks=4"));
    assert!(diagnostic.contains("candidate_tiles=4096"));
    assert!(diagnostic.contains("clear_5x5_windows_reaching_256="));
    assert!(diagnostic.contains("start_component_diagnostic=eligible_5x5_candidates="));
    assert!(diagnostic.contains("local_worldcover=unavailable_no_source_lock"));
}

#[test]
fn component_diagnostics_group_candidates_and_classify_frontier_tiles() {
    let package =
        MapPackage::new(9, MapRequest::default(), Vec::new()).expect("source-free package");
    let generator = package.generator();
    let terrain = Terrain::from_package(&package);
    let config = WorldConfig::new(64, 64, Seed(1)).expect("config");
    let candidates = [TileCoord::new(10, 10), TileCoord::new(50, 50)];

    let component = start_component_diagnostic(
        &terrain,
        &generator,
        &package,
        &candidates,
        Some(TileCoord::new(10, 10)),
        TileCoord::new(62, 32),
        config,
    )
    .expect("component diagnostic");
    assert!(component.contains("eligible_5x5_candidates=2"));
    assert!(component.contains("candidates_in_selected_start_component="));
    assert!(component.contains("components=[root="));

    let tiles = BTreeSet::from([TileCoord::new(31, 31)]);
    let blocker = component_blocker_diagnostic(&terrain, &generator, &tiles, config)
        .expect("blocker diagnostic");
    assert!(blocker.contains("blocked_frontier_edges="));
    assert!(blocker.contains("distinct_blocker_tiles="));
}

#[test]
fn source_coordinates_round_trip_and_reject_impossible_axes() {
    assert_eq!(
        nearest_source_coordinate(TileCoord::new(31, 31), 64, 64).expect("source coordinate"),
        (31, 31)
    );
    assert_eq!(
        nearest_source_coordinate(TileCoord::new(-1, 64), 64, 64).expect("clamped coordinate"),
        (0, 63)
    );
    assert!(matches!(
        nearest_source_coordinate(TileCoord::new(0, 0), 0, 64),
        Err(EnvironmentPageError::Invalid)
    ));
    assert!(matches!(
        nearest_source_coordinate(TileCoord::new(0, 0), 64, 0),
        Err(EnvironmentPageError::Invalid)
    ));
}
