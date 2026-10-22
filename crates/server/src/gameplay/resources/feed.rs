use super::super::GameplayService;
use aoe_protocol::{GameplayServerMessage, ResourceAmount, ResourceState};
use std::collections::{BTreeMap, VecDeque};

const MAX_JOURNAL_CHANGES: usize = 1_024;

#[derive(Default)]
pub(in crate::gameplay) struct Journal {
    changes: VecDeque<(u64, ResourceAmount)>,
}

impl Journal {
    pub(in crate::gameplay) fn record(&mut self, change: aoe_map::Depletion) {
        if change.removed == 0 {
            return;
        }
        self.changes.push_back((
            change.revision,
            ResourceAmount {
                id: change.id,
                remaining: change.remaining,
            },
        ));
        if self.changes.len() > MAX_JOURNAL_CHANGES {
            self.changes.pop_front();
        }
    }

    fn since(&self, from: u64, current: u64) -> Option<Vec<ResourceAmount>> {
        if from >= current {
            return None;
        }
        let mut expected = from.checked_add(1)?;
        let mut amounts = BTreeMap::new();
        for (revision, change) in &self.changes {
            if *revision <= from {
                continue;
            }
            if *revision != expected {
                return None;
            }
            amounts.insert(change.id, *change);
            expected = expected.checked_add(1)?;
        }
        (expected.checked_sub(1)? == current).then(|| amounts.into_values().collect())
    }
}

impl GameplayService {
    /// Called after releasing the tick world lock. Snapshot and journal reads
    /// share the world read lock with depletion's exclusive publication.
    pub(in crate::gameplay) async fn publish_resources(&self) {
        let Some(hash) = self.map_content_hash else {
            return;
        };
        let world = self.world.read().await;
        let Some(revision) = world.resource_revision() else {
            return;
        };
        let mut known_by = BTreeMap::<aoe_core::PlayerId, Vec<ResourceAmount>>::new();
        let mut sessions = self.sessions.lock().await;
        let failed = {
            let journal = self
                .resource_journal
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut failed = Vec::new();
            let mut snapshot = None;
            for (id, session) in sessions.iter_mut() {
                let Some(subscription) = session.subscription else {
                    continue;
                };
                let vision_revision = world.player_vision_revision(session.viewer);
                if session.resource_revision == Some(revision)
                    && session.resource_vision_revision == Some(vision_revision)
                {
                    continue;
                }
                let known = known_by.entry(session.viewer).or_insert_with(|| {
                    world
                        .known_resource_amounts(session.viewer)
                        .into_iter()
                        .map(|(id, remaining)| ResourceAmount { id, remaining })
                        .collect()
                });
                if !world.fog_disabled()
                    && session.resource_revision.is_some()
                    && session.resident_resources == *known
                {
                    // Keep the last delivered resource revision: a later delta
                    // must still start at the revision the client received.
                    session.resource_vision_revision = Some(vision_revision);
                    continue;
                }
                let delta = session
                    .resource_revision
                    .and_then(|from| journal.since(from, revision));
                let state = if !world.fog_disabled() {
                    let changes = known
                        .iter()
                        .filter(|c| {
                            session
                                .resident_resources
                                .binary_search_by_key(&c.id, |old| old.id)
                                .map_or(true, |i| session.resident_resources[i] != **c)
                        })
                        .copied()
                        .collect::<Vec<_>>();
                    let from_revision = session
                        .resource_revision
                        .filter(|from| *from < revision && !changes.is_empty());
                    ResourceState {
                        subscription_revision: subscription.revision,
                        from_revision,
                        revision,
                        changes: if from_revision.is_some() {
                            changes
                        } else {
                            known.clone()
                        },
                    }
                } else if let Some(changes) = delta {
                    ResourceState {
                        subscription_revision: subscription.revision,
                        from_revision: session.resource_revision,
                        revision,
                        changes,
                    }
                } else {
                    if snapshot.is_none() {
                        snapshot = world.resource_snapshot(hash);
                    }
                    let Some(snapshot) = &snapshot else {
                        failed.push(*id);
                        continue;
                    };
                    ResourceState {
                        subscription_revision: subscription.revision,
                        from_revision: None,
                        revision,
                        changes: snapshot
                            .changes
                            .iter()
                            .map(|change| ResourceAmount {
                                id: change.id,
                                remaining: change.remaining,
                            })
                            .collect(),
                    }
                };
                // A full queue skips this publication: the next one is computed
                // against what the session last received. Only a closed queue ends it.
                match session
                    .sender
                    .try_send(GameplayServerMessage::ResourceState(state))
                {
                    Ok(()) => {
                        session.resource_revision = Some(revision);
                        session.resource_vision_revision = Some(vision_revision);
                        session.resident_resources = known.clone();
                    }
                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {}
                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => failed.push(*id),
                }
            }
            failed
        };
        drop(sessions);
        drop(world);
        for id in failed {
            self.disconnect(id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn change(revision: u64) -> aoe_map::Depletion {
        aoe_map::Depletion {
            id: 2,
            removed: 1,
            remaining: 10,
            became_nonblocking: false,
            revision,
        }
    }
    #[test]
    fn retained_journal_deduplicates_and_lag_requires_snapshot() {
        let mut journal = Journal::default();
        for revision in 1..=1025 {
            journal.record(change(revision));
        }
        assert_eq!(journal.changes.len(), MAX_JOURNAL_CHANGES);
        assert!(journal.since(0, 1025).is_none());
        assert_eq!(journal.since(1, 1025).unwrap().len(), 1);
        assert!(journal.since(1025, 1025).is_none());
        journal.record(change(1027));
        assert!(journal.since(1025, 1027).is_none());
    }
}
