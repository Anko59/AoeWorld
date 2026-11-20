//! Diffing built views against each session's baseline and sending the result.
//! This is the only fan-out step that holds the session lock.
use super::Session;
use super::fanout::{AI_HEARTBEAT_TICKS, Buildings, Built, SessionBuild, ViewerState};
use aoe_protocol::{FogState, GameplayServerMessage, GameplayUnitState};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use tokio::sync::mpsc::error::TrySendError;

#[path = "deliver/units.rs"]
mod units;

/// Sessions one worker task delivers at least.
const SESSIONS_PER_TASK: usize = 2;

/// The units a client holds, sorted by id, and the frame they were last
/// brought up to date from (`None` after a snapshot), so the next tick can
/// skip the units that frame says did not change.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Resident {
    pub units: Vec<GameplayUnitState>,
    pub frame: Option<u64>,
}

impl From<Vec<GameplayUnitState>> for Resident {
    fn from(units: Vec<GameplayUnitState>) -> Self {
        Self { units, frame: None }
    }
}

impl std::ops::Deref for Resident {
    type Target = [GameplayUnitState];
    fn deref(&self) -> &Self::Target {
        &self.units
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
    let diff = units::diff(&session.resident, slots, built, session.viewer);
    let changed_players = viewer
        .players
        .iter()
        .filter(|(id, state)| session.resident_players.get(*id) != Some(*state))
        .map(|(_, state)| state.clone())
        .collect::<Vec<_>>();
    let mut changed_buildings = Vec::new();
    let mut building_removals = Vec::new();
    match &viewer.buildings {
        Buildings::Full(known, private) => {
            let in_view = |state: &aoe_protocol::GameplayBuildingState| {
                !built.fog_disabled
                    || super::economy::building_in_camera(
                        state,
                        session.viewer,
                        subscription.region,
                    )
            };
            changed_buildings = private
                .iter()
                .chain(known.iter())
                .filter(|(id, state)| {
                    in_view(state) && session.resident_buildings.get(*id) != Some(*state)
                })
                .take(aoe_protocol::MAX_ECONOMY_BUILDINGS + 1)
                .map(|(_, state)| state.clone())
                .collect();
            let known_state = |id| private.get(id).or_else(|| known.get(id));
            building_removals = session
                .resident_buildings
                .keys()
                .filter(|id| known_state(*id).is_none_or(|state| !in_view(state)))
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
    if matches!(&viewer.buildings, Buildings::Full(_, private) if !private.is_empty()) {
        // Admit owned plans first when the batch is full, then put the
        // selected changes in the ID order required by the wire format.
        changed_buildings.sort_unstable_by_key(|state| state.id);
    }
    if session.ai
        && diff.changed.is_empty()
        && diff.removals.is_empty()
        && changed_players.is_empty()
        && changed_buildings.is_empty()
        && building_removals.is_empty()
        && !built.tick.0.is_multiple_of(AI_HEARTBEAT_TICKS)
    {
        // Nothing to tell an AI: it keeps time from the heartbeat. Its
        // baseline already equals a full rebuild, so that is settled too,
        // and its units equal this frame's.
        session.rebuild_buildings = false;
        session.resident.frame = Some(built.frame.generation());
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
    units::patch(&mut session.resident, &diff, slots, built, session.viewer);
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

/// One session's messages for the tick: whether it took the tick's building
/// changes into its baseline, and whether its queue closed.
fn deliver_session(
    session: &mut Session,
    build: &SessionBuild,
    built: &Built,
    new_acks: Vec<GameplayServerMessage>,
) -> (bool, BTreeSet<u64>) {
    let id = build.session_id;
    let mut failed = BTreeSet::new();
    let mut applied = false;
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
    flush_acks(session, new_acks, &mut failed, id);
    if failed.contains(&id) {
        return (applied, failed);
    }
    if let Some(viewer) = built.viewers.get(&build.viewer)
        && deliver_tick(session, build, viewer, built, &mut failed)
    {
        applied = true;
    }
    if failed.contains(&id) || !current {
        return (applied, failed);
    }
    if let Some(land) = built
        .viewers
        .get(&build.viewer)
        .and_then(|v| v.land.as_ref())
        && session
            .resident_land
            .as_ref()
            .is_none_or(|old| !std::sync::Arc::ptr_eq(old, land) && old.shapes != land.shapes)
    {
        if !send(
            session,
            super::economy::holdings::message(land, build.revision, built.tick),
            &mut failed,
            id,
        ) {
            return (applied, failed);
        }
        session.resident_land = Some(land.clone());
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
            return (applied, failed);
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
            return (applied, failed);
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
    (applied, failed)
}

/// Sends every session its messages for the tick, in the order a client has
/// always seen them: fog, command acknowledgements, units, wildlife,
/// projectiles. Returns the sessions whose queue was full.
#[cfg(test)]
pub(super) fn deliver(
    sessions: &mut BTreeMap<u64, Session>,
    built: &Built,
    acknowledgements: BTreeMap<u64, Vec<GameplayServerMessage>>,
) -> BTreeSet<u64> {
    deliver_on(sessions, built, acknowledgements, true)
}

/// `deliver`, on the worker pool when `parallel`; a delivery overlapping
/// the next tick runs serially so it never takes the simulation's workers.
pub(super) fn deliver_on(
    sessions: &mut BTreeMap<u64, Session>,
    built: &Built,
    mut acknowledgements: BTreeMap<u64, Vec<GameplayServerMessage>>,
    parallel: bool,
) -> BTreeSet<u64> {
    let mut failed = BTreeSet::new();
    // Sessions that took this tick's building changes into their baseline.
    let mut applied = BTreeSet::new();
    // Each session's delivery touches only that session, so they are diffed
    // and sent on the pool; the views and the sessions are both in id order.
    let mut builds = built.sessions.iter().collect::<Vec<_>>();
    builds.sort_by_key(|build| build.session_id);
    let mut work = Vec::with_capacity(builds.len());
    let mut builds = builds.into_iter().peekable();
    for (id, session) in sessions.iter_mut() {
        while builds.next_if(|build| build.session_id < *id).is_some() {}
        if let Some(build) = builds.next_if(|build| build.session_id == *id) {
            let acks = acknowledgements.remove(id).unwrap_or_default();
            work.push((build, session, acks));
        }
    }
    let one = |(build, session, acks): &mut (&SessionBuild, &mut Session, Vec<_>)| {
        deliver_session(session, build, built, std::mem::take(acks))
    };
    let results = if !parallel || work.len() < 2 * SESSIONS_PER_TASK {
        work.iter_mut().map(one).collect::<Vec<_>>()
    } else {
        work.par_iter_mut()
            .with_min_len(SESSIONS_PER_TASK)
            .map(one)
            .collect()
    };
    for ((build, _, _), (took, lost)) in work.iter().zip(results) {
        if took {
            applied.insert(build.session_id);
        }
        failed.extend(lost);
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
