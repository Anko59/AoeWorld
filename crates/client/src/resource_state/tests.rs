use super::*;
#[test]
fn snapshot_delta_reconnect_and_out_of_order_updates_are_atomic() {
    let mut cache = ResourceStateCache::default();
    assert!(!cache.visible(0));
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 1,
        changes: vec![ResourceAmount {
            id: 0,
            remaining: 1
        }]
    }));
    assert!(cache.visible(0));
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(1),
        revision: 2,
        changes: vec![ResourceAmount {
            id: 0,
            remaining: 0
        }]
    }));
    assert!(!cache.visible(0));
    assert!(!cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(1),
        revision: 3,
        changes: vec![ResourceAmount {
            id: 0,
            remaining: 10
        }]
    }));
    assert!(!cache.visible(0));
    for changes in [
        Vec::new(),
        vec![ResourceAmount {
            id: 0,
            remaining: 1,
        }],
    ] {
        assert!(!cache.apply(ResourceState {
            snapshot_page: None,
            subscription_revision: 1,
            from_revision: Some(2),
            revision: 3,
            changes,
        }));
        assert_eq!(cache.revision, Some(2));
        assert!(!cache.visible(0));
    }
    cache.clear();
    assert!(!cache.visible(2));
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 2,
        from_revision: None,
        revision: 2,
        changes: vec![ResourceAmount {
            id: 0,
            remaining: 0
        }]
    }));
    assert!(!cache.visible(0));
    assert!(cache.visible(2));
}
#[test]
fn merged_amounts_stay_sorted_and_overflow_preserves_existing_state() {
    let mut cache = ResourceStateCache::default();
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: None,
        revision: 2,
        changes: vec![
            ResourceAmount {
                id: 2,
                remaining: 5
            },
            ResourceAmount {
                id: 6,
                remaining: 0
            }
        ]
    }));
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(2),
        revision: 5,
        changes: vec![
            ResourceAmount {
                id: 0,
                remaining: 0
            },
            ResourceAmount {
                id: 2,
                remaining: 0
            },
            ResourceAmount {
                id: 4,
                remaining: 1
            }
        ]
    }));
    assert_eq!(
        cache
            .amounts
            .iter()
            .map(|value| value.id)
            .collect::<Vec<_>>(),
        [0, 2, 4, 6]
    );
    assert!(!cache.visible(2));
    assert!(cache.visible(4));
    assert!(
        cache.apply(ResourceState {
            snapshot_page: None,
            subscription_revision: 1,
            from_revision: None,
            revision: MAX_RESOURCE_CHANGES as u64,
            changes: (0..MAX_RESOURCE_CHANGES)
                .map(|id| ResourceAmount {
                    id: id as u64 * 2,
                    remaining: 0
                })
                .collect()
        })
    );
    assert!(cache.apply(ResourceState {
        snapshot_page: None,
        subscription_revision: 1,
        from_revision: Some(MAX_RESOURCE_CHANGES as u64),
        revision: MAX_RESOURCE_CHANGES as u64 + 1,
        changes: vec![ResourceAmount {
            id: MAX_RESOURCE_CHANGES as u64 * 2,
            remaining: 0
        }]
    }));
    assert_eq!(cache.revision, Some(MAX_RESOURCE_CHANGES as u64 + 1));
    assert_eq!(cache.amounts.len(), MAX_RESOURCE_CHANGES + 1);
}
