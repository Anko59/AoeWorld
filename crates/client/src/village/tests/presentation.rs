use super::super::presentation::starting_selection;

#[wasm_bindgen_test::wasm_bindgen_test]
fn selected_fish_have_resource_labels_separate_from_berry_bushes() {
    use super::super::presentation::resource_selection_kind;
    use aoe_map::{ObjectKind, ResourceKind};
    assert_eq!(
        resource_selection_kind(ObjectKind::ShoreFish, ResourceKind::Food),
        4.0
    );
    assert_eq!(
        resource_selection_kind(ObjectKind::DeepFish, ResourceKind::Food),
        5.0
    );
    assert_eq!(
        resource_selection_kind(ObjectKind::ForageBush, ResourceKind::Food),
        0.0
    );
}
use crate::resource_state::ResourceStateCache;
use aoe_core::TileCoord;
use aoe_map::{ResourceKind, ResourceNode};
use aoe_protocol::{ResourceAmount, ResourceState};
use wasm_bindgen_test::wasm_bindgen_test;

fn state(revision: u64, id: u64, remaining: u16) -> ResourceState {
    ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: (revision > 1).then_some(revision - 1),
        revision,
        changes: vec![ResourceAmount { id, remaining }],
    }
}

#[wasm_bindgen_test]
fn a_selected_starting_resource_reports_zero_when_exhausted_instead_of_vanishing() {
    let gold = ResourceNode::starting(TileCoord::new(10, 12), ResourceKind::Gold).unwrap();
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply(state(1, gold.id, 300)));
    assert_eq!(starting_selection(gold.id, &cache), Some((2.0, 300.0)));
    assert!(cache.apply(state(2, gold.id, 0)));
    // The overlay drops an empty node, but the selection keeps its kind and
    // shows the authoritative zero, matching natural resources.
    assert_eq!(cache.starting_nodes().count(), 0);
    assert_eq!(starting_selection(gold.id, &cache), Some((2.0, 0.0)));
    assert_eq!(starting_selection(7, &cache), None);
}

#[wasm_bindgen_test]
fn strategic_resource_records_preserve_fish_type_and_full_node_identity() {
    use super::super::presentation::resource_marker;
    use aoe_map::ObjectKind;

    let node = ResourceNode {
        id: 0x1234_5678_9abc_def0,
        tile: TileCoord::new(30, 40),
        kind: ResourceKind::Food,
        object: ObjectKind::DeepFish,
        initial_amount: 225,
        visual_variant: 0,
    };
    let mut records = Vec::new();
    resource_marker(&mut records, node);
    resource_marker(
        &mut records,
        ResourceNode {
            object: ObjectKind::ForageBush,
            ..node
        },
    );
    let position_and_id = [
        30.5,
        40.5,
        f64::from(0x9abc_def0_u32),
        f64::from(0x1234_5678_u32),
    ];
    assert_eq!(records[0], 5.0);
    assert_eq!(records[5], 0.0);
    assert_eq!(records[1..5], position_and_id);
    assert_eq!(records[6..], position_and_id);
}
