use super::*;

fn amounts(from: usize, count: usize) -> Vec<ResourceAmount> {
    (from..from + count)
        .map(|id| ResourceAmount {
            id: id as u64 * 2,
            remaining: 0,
        })
        .collect()
}

#[test]
fn a_live_cache_can_cross_the_per_message_resource_limit_and_reconnect_in_pages() {
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: MAX_RESOURCE_CHANGES as u64,
        changes: amounts(0, MAX_RESOURCE_CHANGES)
    }));
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(MAX_RESOURCE_CHANGES as u64),
        revision: MAX_RESOURCE_CHANGES as u64 + 1,
        changes: amounts(MAX_RESOURCE_CHANGES, 1)
    }));
    assert_eq!(cache.amounts.len(), MAX_RESOURCE_CHANGES + 1);
}

#[test]
fn paged_reconnect_is_atomic_and_missing_or_reordered_pages_are_refused() {
    let total = MAX_RESOURCE_CHANGES + 1;
    let first = ResourceState {
        subscription_revision: 2,
        from_revision: None,
        revision: total as u64,
        changes: amounts(0, MAX_RESOURCE_CHANGES),
        snapshot_page: Some((0, total as u32)),
    };
    let last = ResourceState {
        subscription_revision: 2,
        from_revision: None,
        revision: total as u64,
        changes: amounts(MAX_RESOURCE_CHANGES, 1),
        snapshot_page: Some((MAX_RESOURCE_CHANGES as u32, total as u32)),
    };
    let mut cache = ResourceStateCache::default();
    assert!(!cache.apply(last.clone()));
    assert!(cache.apply(first.clone()));
    assert!(!cache.visible(total as u64 * 2));
    assert!(
        cache.amounts.is_empty(),
        "incomplete snapshots remain unobserved"
    );
    let mut bad = last.clone();
    bad.snapshot_page = Some((1, total as u32));
    assert!(!cache.apply(bad));
    assert!(cache.apply(first));
    assert!(cache.apply(last));
    assert_eq!(cache.amounts.len(), total);
    assert_eq!(cache.revision, Some(total as u64));
}

#[test]
fn complete_overlay_pages_fit_retained_cap_and_an_extra_delta_is_atomic() {
    let mut cache = ResourceStateCache::default();
    for offset in (0..MAX_RESOURCE_CACHE).step_by(MAX_RESOURCE_CHANGES) {
        assert!(cache.apply(ResourceState {
            subscription_revision: 1,
            from_revision: None,
            revision: MAX_RESOURCE_CACHE as u64,
            changes: amounts(offset, MAX_RESOURCE_CHANGES),
            snapshot_page: Some((offset as u32, MAX_RESOURCE_CACHE as u32)),
        }));
    }
    assert_eq!(cache.amounts.len(), MAX_RESOURCE_CACHE);
    assert!(!cache.apply(ResourceState {
        subscription_revision: 1,
        from_revision: Some(MAX_RESOURCE_CACHE as u64),
        revision: MAX_RESOURCE_CACHE as u64 + 1,
        changes: amounts(MAX_RESOURCE_CACHE, 1),
        snapshot_page: None,
    }));
    assert_eq!(cache.revision, Some(MAX_RESOURCE_CACHE as u64));
    assert_eq!(cache.amounts.len(), MAX_RESOURCE_CACHE);
}
