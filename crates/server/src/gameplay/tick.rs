use super::{GameplayService, MAX_ACK_HISTORY, MAX_SUBSCRIBED_UNITS, Subscription, unit_state};
use crate::gameplay_transport;
use aoe_core::{EntityId, PlayerId};
use aoe_protocol::{CommandResult, GameplayRole, GameplayServerMessage};
use aoe_protocol::{GameplayBuildingState, GameplayPlayerState};
use std::collections::{BTreeMap, BTreeSet};

/// Idle AI sessions still hear the clock this often (in ticks).
const AI_HEARTBEAT_TICKS: u64 = 10;
/// An AI's fog view is rebuilt at most this often (in ticks).
const AI_FOG_TICKS: u64 = 20;

/// A subscribed session's inputs to one tick, copied out of the session lock.
struct View {
    session_id: u64,
    subscription: Subscription,
    vision: Option<u64>,
    viewer: PlayerId,
    primary: EntityId,
    ai: bool,
}

/// Everything one player's sessions need, built once per tick however many
/// sessions look through that player.
struct ViewerState {
    players: BTreeMap<PlayerId, GameplayPlayerState>,
    buildings: BTreeMap<EntityId, GameplayBuildingState>,
    vision: u64,
}

impl GameplayService {
    pub async fn tick(&self) {
        let (pending, controllers) = {
            let mut ownership = self.ownership.lock().await;
            let mut sessions = self.sessions.lock().await;
            gameplay_transport::promote_expired(&mut ownership, &mut sessions, self.world_id);
            let mut commands = Vec::new();
            let controllers = sessions
                .iter()
                .filter_map(|(id, session)| {
                    (session.role == GameplayRole::Controller).then_some((*id, session.viewer))
                })
                .collect::<BTreeMap<_, _>>();
            for (session_id, session) in sessions.iter_mut() {
                while let Some(command) = session.pending.pop_front() {
                    commands.push((*session_id, command));
                }
            }
            commands.sort_by_key(|(session_id, command)| (*session_id, command.sequence));
            (commands, controllers)
        };
        let subscriptions = self
            .sessions
            .lock()
            .await
            .iter()
            .filter_map(|(id, session)| {
                session.subscription.map(|subscription| View {
                    session_id: *id,
                    subscription,
                    vision: session.vision_revision,
                    viewer: session.viewer,
                    primary: session.primary_unit_id,
                    ai: session.ai,
                })
            })
            .collect::<Vec<_>>();
        let mut world = self.world.clone().write_owned().await;
        let mut acknowledgements = Vec::new();
        for (session_id, command) in pending {
            let result = match controllers.get(&session_id) {
                None => CommandResult::RejectedNotController,
                Some(player) => super::commands::apply(&mut world, *player, command.action),
            };
            let tick = world.tick();
            acknowledgements.push((
                session_id,
                GameplayServerMessage::CommandAck {
                    sequence: command.sequence,
                    result,
                    applied_tick: tick,
                },
            ));
        }
        let Some(world) = self.advance_world(world).await else {
            tracing::error!("gameplay tick worker failed");
            return;
        };
        let all_players = super::economy::players(&world);
        let mut viewers = BTreeMap::<PlayerId, ViewerState>::new();
        for view in &subscriptions {
            viewers.entry(view.viewer).or_insert_with(|| ViewerState {
                players: all_players
                    .iter()
                    .filter(|(id, _)| **id == view.viewer || world.fog_disabled())
                    .map(|(id, state)| (*id, *state))
                    .collect(),
                buildings: super::economy::buildings(&world, view.viewer),
                vision: world.player_vision_revision(view.viewer),
            });
        }
        let tick = world.tick();
        let mut updates = Vec::new();
        let mut wildlife_updates = Vec::new();
        let mut projectile_updates = Vec::new();
        let mut fog_updates = Vec::new();
        for view in subscriptions {
            let View {
                session_id,
                subscription,
                vision: old_vision,
                viewer: player,
                primary,
                ai,
            } = view;
            let vision_revision = viewers[&player].vision;
            projectile_updates.push((
                session_id,
                subscription.revision,
                super::wildlife::projectiles(&world, subscription.region, player),
            ));
            // An AI gets a changed fog view at most once a second.
            if old_vision != Some(vision_revision) && (!ai || tick.0 % AI_FOG_TICKS == 0) {
                fog_updates.push((
                    session_id,
                    super::fog::state(&world, player, subscription.revision, subscription.region),
                    vision_revision,
                ));
            }
            wildlife_updates.push((
                session_id,
                subscription.revision,
                super::wildlife::animals(&world, subscription.region, player),
            ));
            let current = super::economy::query_units(&world, subscription.region, primary, player);
            if current.len() > MAX_SUBSCRIBED_UNITS {
                updates.push((
                    session_id,
                    GameplayServerMessage::Error {
                        code: 413,
                        message: "subscribed region exceeds gameplay entity limit".to_owned(),
                    },
                    BTreeMap::new(),
                ));
                continue;
            }
            let current = current
                .iter()
                .map(|unit| (unit.id, unit_state(&world, unit)))
                .collect::<BTreeMap<_, _>>();
            updates.push((
                session_id,
                GameplayServerMessage::Tick {
                    revision: subscription.revision,
                    tick,
                    changed_units: current.values().copied().collect(),
                    removals: Vec::new(),
                    changed_players: Vec::new(),
                    changed_buildings: Vec::new(),
                    building_removals: Vec::new(),
                },
                current,
            ));
        }
        drop(world);
        let mut sessions = self.sessions.lock().await;
        let mut failed = BTreeSet::new();
        for (id, fog, vision_revision) in fog_updates {
            let Some(session) = sessions.get_mut(&id) else {
                continue;
            };
            if session
                .subscription
                .is_none_or(|s| s.revision != fog.revision)
            {
                continue;
            }
            if session
                .sender
                .try_send(GameplayServerMessage::FogState(fog))
                .is_err()
            {
                failed.insert(id);
            } else {
                session.vision_revision = Some(vision_revision);
            }
        }
        for (session_id, message) in acknowledgements {
            if let Some(session) = sessions.get_mut(&session_id) {
                if let GameplayServerMessage::CommandAck { sequence, .. } = message {
                    session.acknowledgements.push_back(sequence);
                    while session.acknowledgements.len() > MAX_ACK_HISTORY {
                        session.acknowledgements.pop_front();
                    }
                }
                if session.sender.try_send(message).is_err() {
                    failed.insert(session_id);
                }
            }
        }
        for (session_id, mut message, current) in updates {
            if failed.contains(&session_id) {
                continue;
            }
            let is_tick = matches!(message, GameplayServerMessage::Tick { .. });
            let Some(session) = sessions.get_mut(&session_id) else {
                continue;
            };
            let Some(viewer) = viewers.get(&session.viewer) else {
                continue;
            };
            let (players, buildings) = (&viewer.players, &viewer.buildings);
            if let GameplayServerMessage::Tick { revision, .. } = &message
                && session.subscription.is_none_or(|s| s.revision != *revision)
            {
                continue;
            }
            if let GameplayServerMessage::Tick {
                changed_units,
                removals,
                changed_players,
                changed_buildings,
                building_removals,
                ..
            } = &mut message
            {
                let changed = current
                    .iter()
                    .filter_map(|(id, unit)| {
                        (session.resident.get(id) != Some(unit)).then_some(*unit)
                    })
                    .collect::<Vec<_>>();
                let removed = session
                    .resident
                    .keys()
                    .filter(|id| !current.contains_key(id))
                    .copied()
                    .collect::<Vec<_>>();
                *changed_players = players
                    .iter()
                    .filter_map(|(id, state)| {
                        (session.resident_players.get(id) != Some(state)).then_some(*state)
                    })
                    .collect();
                *changed_buildings = buildings
                    .iter()
                    .filter_map(|(id, state)| {
                        (session.resident_buildings.get(id) != Some(state)).then_some(state.clone())
                    })
                    .collect();
                *building_removals = session
                    .resident_buildings
                    .keys()
                    .filter(|id| !buildings.contains_key(id))
                    .copied()
                    .collect();
                *changed_units = changed;
                *removals = removed;
                if session.ai
                    && changed_units.is_empty()
                    && removals.is_empty()
                    && changed_players.is_empty()
                    && changed_buildings.is_empty()
                    && building_removals.is_empty()
                    && tick.0 % AI_HEARTBEAT_TICKS != 0
                {
                    // Nothing to tell an AI: it keeps time from the heartbeat.
                    continue;
                }
            }
            if session.sender.try_send(message).is_err() {
                failed.insert(session_id);
            } else if is_tick {
                session.resident_players = players.clone();
                session.resident_buildings = buildings.clone();
                session.resident = current;
            }
        }
        for (session_id, revision, animals) in wildlife_updates {
            if failed.contains(&session_id) {
                continue;
            }
            let Some(session) = sessions.get_mut(&session_id) else {
                continue;
            };
            // A concurrent subscription supersedes work collected for an old
            // revision; never overwrite its fresh residents with an old set.
            if session.subscription.is_none_or(|s| s.revision != revision) {
                continue;
            }
            if session.resident_animals != animals {
                let message = GameplayServerMessage::WildlifeState {
                    revision,
                    tick,
                    animals: animals.clone(),
                };
                if session.sender.try_send(message).is_err() {
                    failed.insert(session_id);
                } else {
                    session.resident_animals = animals;
                }
            }
        }
        drop(sessions);
        for (session_id, revision, projectiles) in projectile_updates {
            let mut sessions = self.sessions.lock().await;
            let Some(s) = sessions.get_mut(&session_id) else {
                continue;
            };
            if failed.contains(&session_id) || s.subscription.is_none_or(|v| v.revision != revision)
            {
                continue;
            }
            if s.resident_projectiles != projectiles {
                if s.sender
                    .try_send(GameplayServerMessage::Projectiles {
                        revision,
                        tick,
                        projectiles: projectiles.clone(),
                    })
                    .is_err()
                {
                    failed.insert(session_id);
                } else {
                    s.resident_projectiles = projectiles;
                }
            }
        }
        for id in failed {
            self.disconnect(id).await;
        }
        self.publish_resources().await;
    }
}
