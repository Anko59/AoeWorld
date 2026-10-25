//! Diffing built views against each session's baseline and sending the result.
//! This is the only fan-out step that holds the session lock.
use super::Session;
use super::fanout::{AI_HEARTBEAT_TICKS, Buildings, Built, SessionBuild, ViewerState};
use aoe_core::EntityId;
use aoe_protocol::{FogState, GameplayServerMessage, GameplayUnitState};
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::mpsc::error::TrySendError;

/// What changed between a session's baseline and the units now in view.
struct UnitDiff {
    changed: Vec<GameplayUnitState>,
    removals: Vec<EntityId>,
    /// The same ids are in view, so the baseline can be patched in place.
    same_ids: bool,
}

fn diff_units(resident: &[GameplayUnitState], current: &[u32], built: &Built) -> UnitDiff {
    let mut diff = UnitDiff {
        changed: Vec::new(),
        removals: Vec::new(),
        same_ids: true,
    };
    let mut old = resident.iter().peekable();
    for slot in current {
        let now = built.frame.state(*slot);
        while let Some(before) = old.next_if(|before| before.id < now.id) {
            diff.removals.push(before.id);
            diff.same_ids = false;
        }
        match old.next_if(|before| before.id == now.id) {
            Some(before) if before == now => {}
            Some(_) => diff.changed.push(*now),
            None => {
                diff.changed.push(*now);
                diff.same_ids = false;
            }
        }
    }
    for before in old {
        diff.removals.push(before.id);
        diff.same_ids = false;
    }
    diff
}

fn patch_units(
    resident: &mut Vec<GameplayUnitState>,
    diff: &UnitDiff,
    current: &[u32],
    built: &Built,
) {
    if diff.same_ids {
        for unit in &diff.changed {
            if let Ok(at) = resident.binary_search_by_key(&unit.id, |u| u.id) {
                resident[at] = *unit;
            }
        }
    } else {
        *resident = current
            .iter()
            .map(|slot| *built.frame.state(*slot))
            .collect();
    }
}

/// Same fog as the client already holds? The tick it was built at does not
/// matter, so a rebuilt but unchanged view is not sent again.
fn same_fog(held: &FogState, fresh: &FogState) -> bool {
    held.revision == fresh.revision
        && held.region == fresh.region
        && held.disabled == fresh.disabled
        && held.tiles == fresh.tiles
        && held.overview == fresh.overview
}

/// Acks a session may have waiting for room before it counts as lost.
const MAX_PENDING_ACKS: usize = 4_096;

/// Offers `message` without waiting. A full queue is not a failure: every
/// delta is computed against what the session last received, so a message
/// that does not fit is folded into the next tick's, and the caller sees
/// `false` (not delivered; `deliver` then also marks the session
/// `rebuild_buildings`, as for every other skip). Only a closed queue, a
/// session that is gone, is recorded in `failed`.
fn send(
    session: &mut Session,
    message: GameplayServerMessage,
    failed: &mut BTreeSet<u64>,
    id: u64,
) -> bool {
    match session.sender.try_send(message) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Closed(_)) => {
            failed.insert(id);
            false
        }
    }
}

/// Holds `acks` (one-shot verdicts, not deltas the next tick recomputes) and
/// sends the held ones in order as far as the queue has room. An ack is never
/// dropped; a session too far behind is reset (the SDK rejoins).
fn flush_acks(
    session: &mut Session,
    acks: impl IntoIterator<Item = GameplayServerMessage>,
    failed: &mut BTreeSet<u64>,
    id: u64,
) {
    for message in acks {
        if let GameplayServerMessage::CommandAck { sequence, .. } = message {
            session.acknowledgements.push_back(sequence);
            while session.acknowledgements.len() > super::MAX_ACK_HISTORY {
                session.acknowledgements.pop_front();
            }
        }
        session.pending_acks.push_back(message);
    }
    while let Some(ack) = session.pending_acks.pop_front() {
        match session.sender.try_send(ack) {
            Ok(()) => {}
            Err(TrySendError::Full(ack)) => {
                session.pending_acks.push_front(ack);
                break;
            }
            Err(TrySendError::Closed(_)) => {
                failed.insert(id);
                break;
            }
        }
    }
    if session.pending_acks.len() > MAX_PENDING_ACKS {
        failed.insert(id);
    }
}

fn deliver_tick(
    session: &mut Session,
    build: &SessionBuild,
    viewer: &ViewerState,
    built: &Built,
    failed: &mut BTreeSet<u64>,
) -> bool {
    let id = build.session_id;
    let Some(subscription) = session.subscription else {
        return false;
    };
    let Some(slots) = &build.units else {
        let _ = send(
            session,
            GameplayServerMessage::Error {
                code: 413,
                message: "subscribed region exceeds gameplay entity limit".to_owned(),
            },
            failed,
            id,
        );
        return false;
    };
    if session
        .subscription_delivery
        .as_ref()
        .is_some_and(|d| d.pending())
        || session.viewer != build.viewer
        || session
            .subscription
            .is_none_or(|s| s.revision != build.revision)
    {
        return false;
    }
    let diff = diff_units(&session.resident, slots, built);
    let changed_players = viewer
        .players
        .iter()
        .filter(|(id, state)| session.resident_players.get(*id) != Some(*state))
        .map(|(_, state)| *state)
        .collect::<Vec<_>>();
    let mut changed_buildings = Vec::new();
    let mut building_removals = Vec::new();
    match &viewer.buildings {
        Buildings::Full(known) => {
            let in_view = |state: &aoe_protocol::GameplayBuildingState| {
                !built.fog_disabled
                    || super::economy::building_in_camera(
                        state,
                        session.viewer,
                        subscription.region,
                    )
            };
            changed_buildings = known
                .iter()
                .filter(|(id, state)| {
                    in_view(state) && session.resident_buildings.get(*id) != Some(*state)
                })
                .take(aoe_protocol::MAX_ECONOMY_BUILDINGS + 1)
                .map(|(_, state)| state.clone())
                .collect();
            building_removals = session
                .resident_buildings
                .keys()
                .filter(|id| known.get(*id).is_none_or(|state| !in_view(state)))
                .take(aoe_protocol::MAX_ECONOMY_BUILDINGS + 1)
                .copied()
                .collect();
        }
        Buildings::Changes(changes) => {
            for (id, known) in changes {
                match known {
                    Some(state) if session.resident_buildings.get(id) != Some(state) => {
                        changed_buildings.push(state.clone());
                    }
                    None if session.resident_buildings.contains_key(id) => {
                        building_removals.push(*id);
                    }
                    _ => {}
                }
            }
        }
    }
    // One delivery stays within the wire bound. Retain only what was sent;
    // a full rebuild next tick supplies the rest, including after queue pressure.
    let limit = aoe_protocol::MAX_ECONOMY_BUILDINGS;
    let more_buildings = changed_buildings.len() + building_removals.len() > limit;
    building_removals.truncate(limit);
    changed_buildings.truncate(limit - building_removals.len());
    if session.ai
        && diff.changed.is_empty()
        && diff.removals.is_empty()
        && changed_players.is_empty()
        && changed_buildings.is_empty()
        && building_removals.is_empty()
        && !built.tick.0.is_multiple_of(AI_HEARTBEAT_TICKS)
    {
        // Nothing to tell an AI: it keeps time from the heartbeat. Its
        // baseline already equals a full rebuild, so that is settled too.
        session.rebuild_buildings = false;
        return true;
    }
    let message = GameplayServerMessage::Tick {
        revision: build.revision,
        tick: built.tick,
        changed_units: diff.changed.clone(),
        removals: diff.removals.clone(),
        changed_players,
        changed_buildings: changed_buildings.clone(),
        building_removals: building_removals.clone(),
    };
    if !send(session, message, failed, id) {
        return false;
    }
    patch_units(&mut session.resident, &diff, slots, built);
    if session.resident_players != viewer.players {
        session.resident_players = viewer.players.clone();
    }
    // Either way the baseline now equals what the viewer knows.
    for state in changed_buildings {
        session.resident_buildings.insert(state.id, state);
    }
    for id in building_removals {
        session.resident_buildings.remove(&id);
    }
    session.rebuild_buildings = more_buildings;
    true
}

/// Sends every session its messages for the tick, in the order a client has
/// always seen them: fog, command acknowledgements, units, wildlife,
/// projectiles. Returns the sessions whose queue was full.
pub(super) fn deliver(
    sessions: &mut BTreeMap<u64, Session>,
    built: &Built,
    mut acknowledgements: BTreeMap<u64, Vec<GameplayServerMessage>>,
) -> BTreeSet<u64> {
    let mut failed = BTreeSet::new();
    // Sessions that took this tick's building changes into their baseline.
    let mut applied = BTreeSet::new();
    for build in &built.sessions {
        let id = build.session_id;
        let Some(session) = sessions.get_mut(&id) else {
            continue;
        };
        let current = session
            .subscription
            .is_some_and(|s| s.revision == build.revision)
            // Every revision-scoped frame must follow its initial snapshot.
            // Leave skipped baselines untouched so an unchanged final view
            // remains eligible for delivery after the worker drains.
            && !session
                .subscription_delivery
                .as_ref()
                .is_some_and(|delivery| delivery.pending());
        if let Some((fog, vision)) = &build.fog
            && current
        {
            let held = session
                .resident_fog
                .as_ref()
                .is_some_and(|held| same_fog(held, fog));
            if held {
                session.vision_revision = Some(*vision);
            } else if send(
                session,
                GameplayServerMessage::FogState(fog.clone()),
                &mut failed,
                id,
            ) {
                session.resident_fog = Some(fog.clone());
                session.vision_revision = Some(*vision);
            }
        }
        let new_acks = acknowledgements.remove(&id).unwrap_or_default();
        flush_acks(session, new_acks, &mut failed, id);
        if failed.contains(&id) {
            continue;
        }
        if let Some(viewer) = built.viewers.get(&build.viewer)
            && deliver_tick(session, build, viewer, built, &mut failed)
        {
            applied.insert(id);
        }
        if failed.contains(&id) || !current {
            continue;
        }
        if session.resident_move_queues != build.move_queues {
            let message = GameplayServerMessage::MoveQueues {
                revision: build.revision,
                tick: built.tick,
                queues: build.move_queues.clone(),
            };
            if send(session, message, &mut failed, id) {
                session.resident_move_queues = build.move_queues.clone();
            } else {
                continue;
            }
        }
        // A concurrent subscription supersedes work collected for an old
        // revision; never overwrite its fresh residents with an old set.
        if session.resident_animals != build.animals {
            let message = GameplayServerMessage::WildlifeState {
                revision: build.revision,
                tick: built.tick,
                animals: build.animals.clone(),
            };
            if send(session, message, &mut failed, id) {
                session.resident_animals = build.animals.clone();
            } else {
                continue;
            }
        }
        if session.resident_projectiles != build.projectiles {
            let message = GameplayServerMessage::Projectiles {
                revision: build.revision,
                tick: built.tick,
                projectiles: build.projectiles.clone(),
            };
            if send(session, message, &mut failed, id) {
                session.resident_projectiles = build.projectiles.clone();
            }
        }
    }
    // The tick's building changes were taken from the world once. A session
    // that did not apply them (skipped, superseded by a subscription that
    // raced the tick, or not even in this tick's views) rebuilds its building
    // baseline from the full view on its next delivery.
    for (id, session) in sessions.iter_mut() {
        if !applied.contains(id) {
            session.rebuild_buildings = true;
        }
    }
    // Sessions that were not subscribed still hear their acknowledgements.
    for (id, messages) in acknowledgements {
        let Some(session) = sessions.get_mut(&id) else {
            continue;
        };
        flush_acks(session, messages, &mut failed, id);
    }
    // Acks held back by a full queue go out once it has room, whether or not
    // this tick brought new ones.
    for (id, session) in sessions.iter_mut() {
        if !session.pending_acks.is_empty() {
            flush_acks(session, Vec::new(), &mut failed, *id);
        }
    }
    failed
}
