use aoe_protocol::{MAX_RESOURCE_CHANGES, ResourceAmount, ResourceState};

/// Independent of immutable chunk residency, so evicting terrain never
/// resurrects a depleted resource. Unknown state is hidden until synchronized.
#[derive(Default)]
pub struct ResourceStateCache {
    revision: Option<u64>,
    amounts: Vec<ResourceAmount>,
}

impl ResourceStateCache {
    pub fn clear(&mut self) {
        self.revision = None;
        self.amounts.clear();
    }
    pub fn visible(&self, id: u64) -> bool {
        self.revision.is_some()
            && self
                .amounts
                .binary_search_by_key(&id, |value| value.id)
                .ok()
                .is_none_or(|index| self.amounts[index].remaining > 0)
    }
    pub fn apply(&mut self, state: ResourceState) -> bool {
        if !state.valid()
            || self
                .revision
                .is_some_and(|revision| state.revision < revision)
            || state
                .from_revision
                .is_some_and(|from| self.revision != Some(from))
        {
            return false;
        }
        if state.from_revision.is_none() {
            self.amounts = state.changes;
        } else {
            if state.changes.iter().any(|change| {
                self.amounts
                    .binary_search_by_key(&change.id, |value| value.id)
                    .is_ok_and(|index| change.remaining > self.amounts[index].remaining)
            }) {
                return false;
            }
            let added = state
                .changes
                .iter()
                .filter(|change| {
                    self.amounts
                        .binary_search_by_key(&change.id, |value| value.id)
                        .is_err()
                })
                .count();
            if self.amounts.len() + added > MAX_RESOURCE_CHANGES {
                return false;
            }
            if added == 0 {
                let mut next = 0;
                for current in &mut self.amounts {
                    if next < state.changes.len() && current.id == state.changes[next].id {
                        *current = state.changes[next];
                        next += 1;
                    }
                }
            } else {
                let mut merged = Vec::with_capacity(self.amounts.len() + added);
                let mut old = 0;
                for change in state.changes {
                    while old < self.amounts.len() && self.amounts[old].id < change.id {
                        merged.push(self.amounts[old]);
                        old += 1;
                    }
                    if old < self.amounts.len() && self.amounts[old].id == change.id {
                        old += 1;
                    }
                    merged.push(change);
                }
                merged.extend_from_slice(&self.amounts[old..]);
                self.amounts = merged;
            }
        }
        self.revision = Some(state.revision);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_delta_reconnect_and_out_of_order_updates_are_atomic() {
        let mut cache = ResourceStateCache::default();
        assert!(!cache.visible(0));
        assert!(cache.apply(ResourceState {
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
        assert!(!cache.apply(ResourceState {
            subscription_revision: 1,
            from_revision: Some(MAX_RESOURCE_CHANGES as u64),
            revision: MAX_RESOURCE_CHANGES as u64 + 1,
            changes: vec![ResourceAmount {
                id: MAX_RESOURCE_CHANGES as u64 * 2,
                remaining: 0
            }]
        }));
        assert_eq!(cache.revision, Some(MAX_RESOURCE_CHANGES as u64));
        assert_eq!(cache.amounts.len(), MAX_RESOURCE_CHANGES);
    }
}
