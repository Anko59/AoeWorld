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

fn update(from: Option<u64>, revision: u64, changes: Vec<ResourceAmount>) -> ResourceState {
    ResourceState {
        subscription_revision: 1,
        from_revision: from,
        revision,
        changes,
        snapshot_page: None,
    }
}

fn amount(id: u64, remaining: u16) -> ResourceAmount {
    ResourceAmount { id, remaining }
}

#[test]
fn accepted_update_reclaims_expired_stump_metadata_at_three_seconds() {
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply_at(update(None, 1, vec![amount(42, 100)]), 0.0));
    assert!(cache.apply_at(update(Some(1), 2, vec![amount(42, 0)]), 200.0));
    assert_eq!(cache.tree_stage(42, 100, 3199.0), TreeStage::Stump);
    assert!(cache.apply_at(update(Some(2), 3, vec![amount(44, 0)]), 3200.0));

    assert_eq!(cache.remaining(42, 100), 0);
    assert_eq!(cache.tree_stage(42, 100, 3200.0), TreeStage::Hidden);
    assert_eq!(cache.tree_stage(44, 100, 3200.0), TreeStage::Stump);
    assert!(
        !cache.exhausted_at.contains_key(&42),
        "hidden stumps must release their transient effect metadata"
    );
    assert_eq!(cache.exhausted_at.len(), 1);
}

#[test]
fn long_depletion_stream_retains_only_live_effects_and_all_authoritative_amounts() {
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply_at(update(None, 1, vec![amount(0, 100)]), 0.0));
    let mut peak_effects = 0;
    for id in 0..128 {
        assert!(cache.apply_at(
            update(Some(id + 1), id + 2, vec![amount(id * 2, 0)]),
            id as f64 * 1000.0,
        ));
        peak_effects = peak_effects.max(cache.exhausted_at.len());
    }

    for id in 0..128 {
        assert_eq!(cache.remaining(id * 2, 100), 0);
        assert!(!cache.visible(id * 2));
    }
    assert_eq!(cache.amounts.len(), 128);
    assert!(
        peak_effects <= 3,
        "one depletion per second needs at most three live effects, retained {peak_effects}"
    );
}

#[test]
fn simultaneous_depletion_burst_has_a_fixed_effect_budget() {
    // Cosmetic stumps may be dropped under pressure; depleted resources may not
    // resurrect when their transient timestamp is evicted.
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply_at(update(None, 1, vec![amount(0, 100)]), 0.0));
    assert!(cache.apply_at(
        update(Some(1), 2, (0..8192).map(|id| amount(id * 2, 0)).collect()),
        100.0,
    ));
    assert!(cache.apply_at(update(Some(2), 3, vec![amount(16384, 0)]), 200.0,));

    assert_eq!(cache.amounts.len(), 8193);
    assert_eq!(cache.remaining(0, 100), 0);
    assert_eq!(cache.remaining(16384, 100), 0);
    assert_eq!(cache.tree_stage(16384, 100, 200.0), TreeStage::Stump);
    assert!(cache.effect_order.capacity() <= 4096);
    assert!(
        cache.exhausted_at.len() <= 4096,
        "cosmetic effect metadata exceeded its 4096-entry budget: {}",
        cache.exhausted_at.len()
    );
}

#[test]
fn idle_client_reclaims_effects_without_forgetting_depletion() {
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply_at(state(None, 1, 100), 0.0));
    assert!(cache.apply_at(state(Some(1), 2, 0), 100.0));
    cache.expire_effects(3100.0);
    assert!(cache.exhausted_at.is_empty());
    assert!(cache.effect_order.is_empty());
    assert_eq!(cache.remaining(42, 100), 0);
}
