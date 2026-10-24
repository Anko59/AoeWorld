use super::*;

fn state(from: Option<u64>, revision: u64, remaining: u16) -> ResourceState {
    ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: from,
        revision,
        changes: vec![ResourceAmount { id: 42, remaining }],
    }
}

#[test]
fn tree_stages_follow_authoritative_amounts_and_only_new_exhaustion_leaves_a_stump() {
    let mut cache = ResourceStateCache::default();
    assert_eq!(cache.tree_stage(42, 100, 0.0), TreeStage::Hidden);
    assert!(cache.apply_at(state(None, 1, 100), 0.0));
    assert_eq!(cache.tree_stage(42, 100, 0.0), TreeStage::Full);
    assert!(cache.apply_at(state(Some(1), 2, 99), 50.0));
    assert_eq!(cache.tree_stage(42, 100, 50.0), TreeStage::Chopped);
    assert!(cache.apply_at(state(Some(2), 3, 1), 100.0));
    assert_eq!(cache.tree_stage(42, 100, 100.0), TreeStage::Chopped);
    assert!(cache.apply_at(state(Some(3), 4, 0), 200.0));
    assert_eq!(cache.tree_stage(42, 100, 3199.0), TreeStage::Stump);
    assert_eq!(cache.tree_stage(42, 100, 3200.0), TreeStage::Hidden);
    assert!(
        !cache.visible(42),
        "stumps cannot be gathered or block orders"
    );
    assert!(!cache.apply_at(state(Some(3), 5, 10), 3000.0));
    assert_eq!(cache.tree_stage(42, 100, 3200.0), TreeStage::Hidden);
    cache.clear();
    assert!(cache.apply_at(state(None, 4, 0), 4000.0));
    assert_eq!(cache.tree_stage(42, 100, 4000.0), TreeStage::Hidden);
    assert!(cache.apply_at(state(None, 5, 20), 5000.0));
    assert_eq!(cache.tree_stage(42, 100, 5000.0), TreeStage::Chopped);
}
