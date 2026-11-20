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
        let world = self.world.read().await;
        let mut sessions = self.sessions.lock().await;
        let failed = self.publish_resources_to(&world, &mut sessions);
        drop(sessions);
        drop(world);
        for id in failed {
            self.disconnect(id).await;
        }
    }

    /// Publishes under locks the caller already holds; returns the sessions
    /// whose queue closed, for the caller to disconnect once it unlocks.
    pub(in crate::gameplay) fn publish_resources_to(
        &self,
        world: &aoe_simulation::GameWorld,
        sessions: &mut BTreeMap<u64, super::super::Session>,
    ) -> Vec<u64> {
        let (ready, mut failed) = self.resource_updates(world, sessions);
        failed.extend(send_resources(sessions, ready));
        failed
    }

    /// Each session's resource frames as of `world`, not yet sent: a tick
    /// whose delivery overlaps the next one builds them while it still reads
    /// its own world and sends them after its views, as an ordinary tick does.
    pub(in crate::gameplay) fn resource_updates(
        &self,
        world: &aoe_simulation::GameWorld,
        sessions: &mut BTreeMap<u64, super::super::Session>,
    ) -> (Vec<ResourceUpdate>, Vec<u64>) {
        let Some(hash) = self.map_content_hash else {
            return (Vec::new(), Vec::new());
        };
        let Some(revision) = world.resource_revision() else {
            return (Vec::new(), Vec::new());
        };
        let mut ready = Vec::new();
        let mut known_by = BTreeMap::<aoe_core::PlayerId, Vec<ResourceAmount>>::new();
        {
            let journal = self
                .resource_journal
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut failed = Vec::new();
            let mut snapshot = None;
            let ai_due = world
                .tick()
                .0
                .is_multiple_of(super::super::tick::AI_VIEW_TICKS);
            for (id, session) in sessions.iter_mut() {
                if session.ai && !ai_due {
                    continue;
                }
                if session
                    .subscription_delivery
                    .as_ref()
                    .is_some_and(|d| d.pending())
                {
                    continue;
                }
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
                    // With fog disabled the shared snapshot/journal is authoritative;
                    // do not retain a million-entry fog-memory copy for every seat.
                    if world.fog_disabled() {
                        return Vec::new();
                    }
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
                    let from_revision = session.resource_revision.filter(|from| {
                        *from < revision
                            && !changes.is_empty()
                            && changes.len() <= aoe_protocol::MAX_RESOURCE_CHANGES
                    });
                    ResourceState {
                        snapshot_page: None,
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
                        snapshot_page: None,
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
                        snapshot_page: None,
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
                ready.push(ResourceUpdate {
                    session: *id,
                    frames: snapshot_frames(state),
                    revision,
                    vision_revision,
                    known: known.clone(),
                });
            }
            (ready, failed)
        }
    }
}

/// One session's resource frames waiting to be sent, and the baseline they
/// bring it to.
pub(in crate::gameplay) struct ResourceUpdate {
    session: u64,
    frames: Vec<ResourceState>,
    revision: u64,
    vision_revision: u64,
    known: Vec<ResourceAmount>,
}

/// Sends built resource frames; returns the sessions whose queue closed.
pub(in crate::gameplay) fn send_resources(
    sessions: &mut BTreeMap<u64, super::super::Session>,
    ready: Vec<ResourceUpdate>,
) -> Vec<u64> {
    let mut failed = Vec::new();
    for update in ready {
        let Some(session) = sessions.get_mut(&update.session) else {
            continue;
        };
        // A full queue skips this publication: the next one is computed
        // against what the session last received. Only a closed queue ends it.
        // Reserve all pages before publishing any: queue pressure never
        // leaves a client with an incomplete snapshot followed by deltas.
        match session.sender.try_reserve_many(update.frames.len()) {
            Ok(permits) => {
                for (permit, state) in permits.zip(update.frames) {
                    permit.send(GameplayServerMessage::ResourceState(state));
                }
                session.resource_revision = Some(update.revision);
                session.resource_vision_revision = Some(update.vision_revision);
                session.resident_resources = update.known;
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {}
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => failed.push(update.session),
        }
    }
    failed
}

/// A complete overlay is retained data, not a single wire frame. Keep the
/// message bound unchanged and carry explicit offsets for atomic reassembly.
fn snapshot_frames(state: ResourceState) -> Vec<ResourceState> {
    let limit = aoe_protocol::MAX_RESOURCE_CHANGES;
    if state.changes.len() <= limit {
        return vec![state];
    }
    debug_assert!(state.from_revision.is_none());
    let total = state.changes.len() as u32;
    state
        .changes
        .chunks(limit)
        .enumerate()
        .map(|(page, changes)| ResourceState {
            subscription_revision: state.subscription_revision,
            from_revision: None,
            revision: state.revision,
            changes: changes.to_vec(),
            snapshot_page: Some(((page * limit) as u32, total)),
        })
        .collect()
}

#[path = "feed/tests.rs"]
#[cfg(test)]
mod tests;
