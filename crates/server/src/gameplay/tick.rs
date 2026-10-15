use super::{GameplayService, MAX_ACK_HISTORY, MAX_SUBSCRIBED_UNITS, unit_state};
use crate::gameplay_transport;
use aoe_protocol::{CommandResult, GameplayRole, GameplayServerMessage};
use std::collections::{BTreeMap, BTreeSet};

impl GameplayService {
    pub async fn tick(&self) {
        let (pending, controllers) = {
            let mut ownership = self.ownership.lock().await;
            let mut sessions = self.sessions.lock().await;
            gameplay_transport::promote_expired(
                &mut ownership,
                &mut sessions,
                self.world_id,
                self.primary_unit_id,
            );
            let mut commands = Vec::new();
            let controllers = sessions
                .iter()
                .filter_map(|(id, session)| {
                    (session.role == GameplayRole::Controller).then_some(*id)
                })
                .collect::<std::collections::BTreeSet<_>>();
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
                session.subscription.map(|subscription| (*id, subscription))
            })
            .collect::<Vec<_>>();
        let mut world = self.world.clone().write_owned().await;
        let mut acknowledgements = Vec::new();
        for (session_id, command) in pending {
            let accepted = controllers.contains(&session_id);
            let player = world
                .unit(self.primary_unit_id)
                .map_or(aoe_core::PlayerId(0), |unit| unit.player);
            let result = if !accepted {
                CommandResult::RejectedNotController
            } else {
                super::commands::apply(&mut world, player, command.action)
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
        let players = super::economy::players(&world);
        let buildings = super::economy::buildings(&world);
        let tick = world.tick();
        let mut updates = Vec::new();
        for (session_id, subscription) in subscriptions {
            let current =
                super::economy::query_units(&world, subscription.region, self.primary_unit_id);
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
            }
            if session.sender.try_send(message).is_err() {
                failed.insert(session_id);
            } else if is_tick {
                session.resident_players = players.clone();
                session.resident_buildings = buildings.clone();
                session.resident = current;
            }
        }
        drop(sessions);
        for id in failed {
            self.disconnect(id).await;
        }
        self.publish_resources().await;
    }
}
