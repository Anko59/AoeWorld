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
