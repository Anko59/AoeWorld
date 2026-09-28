use super::*;
use crate::{
    EnvironmentPage, EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid,
    PreparedEnvironment, PyramidLevel, Ratio, ResourceNode,
};
use std::sync::Arc;

fn resource() -> (MapChunkGenerator, ResourceNode) {
    super::super::tests::resource()
}

fn state() -> (MapChunkGenerator, ResourceNode, ResourceOverlay) {
    let (terrain, node) = resource();
    let mut overlay = ResourceOverlay::default();
    overlay
        .deplete(&terrain, node.id, 1)
        .expect("partial depletion");
    (terrain, node, overlay)
}

#[test]
fn snapshot_roundtrip_preserves_depletion_collision_and_future_revisions() {
    let (terrain, node, mut original) = state();
    let bytes = serde_json::to_vec(&original.snapshot([1; 32])).expect("snapshot bytes");
    let snapshot = serde_json::from_slice(&bytes).expect("snapshot");
    let mut restored =
        ResourceOverlay::from_snapshot(&snapshot, [1; 32], &terrain, &|| false).expect("restore");
    assert_eq!(restored, original);
    assert_eq!(
        original.deplete(&terrain, node.id, u16::MAX),
        restored.deplete(&terrain, node.id, u16::MAX)
    );
    assert!(!restored.blocks_node(node));
    assert_eq!(restored, original);
}

#[test]
fn snapshot_rejects_wrong_map_schema_amount_order_and_impossible_revision() {
    let (terrain, node, overlay) = state();
    let snapshot = overlay.snapshot([1; 32]);
    assert_eq!(
        ResourceOverlay::from_snapshot(&snapshot, [2; 32], &terrain, &|| false),
        Err(ResourceOverlayError::WrongMap)
    );
    let mut corruptions = Vec::new();
    let mut invalid = snapshot.clone();
    invalid.schema_version += 1;
    corruptions.push(invalid);
    let mut invalid = snapshot.clone();
    invalid.changes[0].remaining = node.initial_amount;
    corruptions.push(invalid);
    let mut invalid = snapshot.clone();
    invalid.revision = 0;
    corruptions.push(invalid);
    let mut invalid = snapshot.clone();
    invalid.revision = 2;
    corruptions.push(invalid);
    let mut invalid = snapshot.clone();
    invalid.revision = 2;
    invalid.changes.push(invalid.changes[0]);
    corruptions.push(invalid);
    let mut invalid = snapshot.clone();
    invalid.changes.clear();
    corruptions.push(invalid);
    for invalid in corruptions {
        assert_eq!(
            ResourceOverlay::from_snapshot(&invalid, [1; 32], &terrain, &|| false),
            Err(ResourceOverlayError::InvalidSnapshot)
        );
    }
    let mut unknown = snapshot;
    unknown.changes[0].id = u64::MAX;
    assert_eq!(
        ResourceOverlay::from_snapshot(&unknown, [1; 32], &terrain, &|| false),
        Err(ResourceOverlayError::UnknownResource)
    );
    assert_eq!(overlay.revision(), 1);
}

#[test]
fn snapshot_deserialization_rejects_oversize_input_before_retaining_it() {
    let snapshot = ResourceOverlaySnapshot {
        schema_version: RESOURCE_OVERLAY_SCHEMA_VERSION,
        map_content_hash: [0; 32],
        revision: 0,
        changes: vec![
            ResourceChange {
                id: 0,
                remaining: 0
            };
            MAX_RESOURCE_OVERLAY_CHANGES + 1
        ],
    };
    let bytes = serde_json::to_vec(&snapshot).expect("oversize fixture");
    assert!(
        serde_json::from_slice::<ResourceOverlaySnapshot>(&bytes)
            .expect_err("bounded parser")
            .to_string()
            .contains("too many resource changes")
    );
}

#[test]
fn capacity_and_revision_failures_never_partially_deplete() {
    let (terrain, node) = resource();
    let mut overlay = ResourceOverlay::default();
    // Fill private state with unique sentinel IDs to test the entry bound
    // without searching a large procedural world for thousands of resources.
    for id in 0..MAX_RESOURCE_OVERLAY_CHANGES as u64 {
        overlay.remaining.insert((1 << 63) | id, 0);
    }
    let before = overlay.clone();
    assert_eq!(
        overlay.deplete(&terrain, node.id, 1),
        Err(ResourceOverlayError::Capacity)
    );
    assert_eq!(overlay, before);
    assert_eq!(
        overlay
            .deplete(&terrain, node.id, 0)
            .expect("no-op")
            .removed,
        0
    );
    let mut exhausted = ResourceOverlay {
        revision: u64::MAX,
        ..ResourceOverlay::default()
    };
    let before = exhausted.clone();
    assert_eq!(
        exhausted.deplete(&terrain, node.id, 1),
        Err(ResourceOverlayError::RevisionExhausted)
    );
    assert_eq!(exhausted, before);
}

#[derive(Debug)]
struct Unavailable;
impl EnvironmentPageProvider for Unavailable {
    fn page(
        &self,
        _: EnvironmentPageKey,
        _: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        Err(EnvironmentPageError::Unavailable)
    }
}

#[test]
fn restore_and_deplete_keep_provider_errors_and_cancellation_distinct() {
    let (terrain, node, overlay) = state();
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [1; 32],
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [2; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    let unavailable = terrain
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(Unavailable),
        )
        .expect("provider");
    let snapshot = overlay.snapshot([1; 32]);
    assert_eq!(
        ResourceOverlay::from_snapshot(&snapshot, [1; 32], &unavailable, &|| false),
        Err(ResourceOverlayError::Environment(
            EnvironmentPageError::Unavailable
        ))
    );
    assert_eq!(
        ResourceOverlay::from_snapshot(&snapshot, [1; 32], &unavailable, &|| true),
        Err(ResourceOverlayError::Environment(
            EnvironmentPageError::Cancelled
        ))
    );
    let mut empty = ResourceOverlay::default();
    assert_eq!(
        empty.deplete(&unavailable, node.id, 1),
        Err(ResourceOverlayError::Environment(
            EnvironmentPageError::Unavailable
        ))
    );
    assert_eq!(empty, ResourceOverlay::default());
}
