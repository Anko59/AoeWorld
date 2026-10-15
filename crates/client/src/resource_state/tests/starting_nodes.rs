use super::*;
use aoe_core::TileCoord;
use aoe_map::{ResourceKind, ResourceNode};

#[wasm_bindgen_test::wasm_bindgen_test]
fn starting_nodes_are_visible_without_chunks_and_follow_depletion_and_resync() {
    let node = ResourceNode::starting(TileCoord::new(12, 13), ResourceKind::Gold).unwrap();
    let mut cache = ResourceStateCache::default();
    let state = ResourceState {
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![ResourceAmount {
            id: node.id,
            remaining: node.initial_amount,
        }],
    };
    assert!(cache.apply(state.clone()));
    assert_eq!(cache.starting_nodes().collect::<Vec<_>>(), vec![node]);
    assert!(cache.apply(ResourceState {
        from_revision: Some(0),
        revision: 1,
        changes: vec![ResourceAmount {
            id: node.id,
            remaining: 0
        }],
        ..state.clone()
    }));
    assert_eq!(cache.starting_nodes().count(), 0);
    cache.clear();
    assert!(cache.apply(state));
    assert_eq!(cache.starting_nodes().collect::<Vec<_>>(), vec![node]);
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn emptied_natural_tree_stays_hidden_after_chunk_eviction_and_reconnect() {
    let mut cache = super::ResourceStateCache::default();
    let tree = (83 << 19) | (71 << 1);
    assert!(cache.apply(aoe_protocol::ResourceState {
        subscription_revision: 1,
        from_revision: None,
        revision: 0,
        changes: vec![],
    }));
    assert!(cache.visible(tree));
    assert!(cache.apply(aoe_protocol::ResourceState {
        subscription_revision: 1,
        from_revision: Some(0),
        revision: 1,
        changes: vec![aoe_protocol::ResourceAmount {
            id: tree,
            remaining: 0
        }],
    }));
    assert!(!cache.visible(tree));
    cache.clear();
    assert!(cache.apply(aoe_protocol::ResourceState {
        subscription_revision: 2,
        from_revision: None,
        revision: 1,
        changes: vec![aoe_protocol::ResourceAmount {
            id: tree,
            remaining: 0
        }],
    }));
    assert!(!cache.visible(tree));
}
