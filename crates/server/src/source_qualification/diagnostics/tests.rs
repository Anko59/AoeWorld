use super::components::{
    BoundaryTerrainEvidence, ForestPatternEvidence, diagnostic_conclusion, forest_pattern,
};
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
    assert!(blocker.contains("geographic_cliff_gradient="));
    assert!(blocker.contains("forest_pattern="));
    assert!(blocker.contains("diagnostic_conclusion="));
}

#[test]
fn obstruction_conclusion_distinguishes_elevation_and_fragmented_tree_barriers() {
    let supported_cliffs = BoundaryTerrainEvidence {
        cliff_surface_edges: 4,
        cliff_edges_supported_by_geographic_rise: 3,
        ..BoundaryTerrainEvidence::default()
    };
    assert_eq!(
        diagnostic_conclusion(&supported_cliffs, &ForestPatternEvidence::default()),
        "geographic_elevation_supports_most_cliff_frontier_edges"
    );
    let weak_cliffs = BoundaryTerrainEvidence {
        cliff_surface_edges: 4,
        cliff_edges_supported_by_geographic_rise: 1,
        ..BoundaryTerrainEvidence::default()
    };
    assert!(
        diagnostic_conclusion(&weak_cliffs, &ForestPatternEvidence::default())
            .contains("review_raster_or_quantization_fragmentation")
    );
    let fragmented_forest = ForestPatternEvidence {
        tree_blocker_tiles: 4,
        isolated_tree_tiles: 3,
        ..ForestPatternEvidence::default()
    };
    assert!(
        diagnostic_conclusion(&BoundaryTerrainEvidence::default(), &fragmented_forest)
            .contains("review_procedural_forest_fragmentation")
    );
}

#[test]
fn boundary_tree_objects_are_grouped_as_eight_connected_forest_clumps() {
    let blocker_cells = BTreeMap::from([
        (TileCoord::new(1, 1), "tree_object"),
        (TileCoord::new(2, 1), "tree_object"),
        (TileCoord::new(2, 2), "tree_object"),
        (TileCoord::new(10, 10), "tree_object"),
    ]);
    let evidence = forest_pattern(&blocker_cells);
    assert_eq!(evidence.tree_blocker_tiles, 4);
    assert_eq!(evidence.eight_connected_clumps, 2);
    assert_eq!(evidence.largest_clump_tiles, 3);
    assert_eq!(evidence.isolated_tree_tiles, 1);
}

#[test]
fn bounded_connectivity_probe_distinguishes_proven_and_exhausted_components() {
    let config = WorldConfig::new(8, 8, Seed(1)).expect("config");
    let terrain = Terrain::uniform(1);
    let connected = bounded_connectivity_diagnostic(
        &terrain,
        config,
        TileCoord::new(3, 1),
        TileCoord::new(3, 5),
    )
    .expect("connected probe");
    assert_eq!(
        connected.outcome,
        super::super::navigation_report::ConnectivityProbeOutcome::Connected
    );
    assert_eq!(connected.connected_parallel_corridor_x_offset, Some(0));
    assert!(connected.visited_tiles < connected.node_limit);

    let disconnected = bounded_connectivity_diagnostic(
        &terrain,
        config,
        TileCoord::new(3, 1),
        TileCoord::new(10, 1),
    )
    .expect("exhausted probe");
    assert_eq!(
        disconnected.outcome,
        super::super::navigation_report::ConnectivityProbeOutcome::Disconnected
    );
    assert_eq!(disconnected.frontier_tiles_at_stop, 0);
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
