use super::deliver::deliver;
use super::fanout::{self, View};
use super::{GameplayService, Session};
use crate::gameplay_transport;
use aoe_protocol::{CommandResult, GameplayRole, GameplayServerMessage};
use std::collections::BTreeMap;

impl GameplayService {
    pub async fn tick(&self) {
        let (pending, controllers, views) = {
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
            let views = sessions
                .iter()
                .filter_map(|(id, session)| view(*id, session))
                .collect::<Vec<_>>();
            (commands, controllers, views)
        };
        let mut world = self.world.clone().write_owned().await;
        let orders = pending.len();
        let mut acknowledgements = BTreeMap::<u64, Vec<GameplayServerMessage>>::new();
        for (session_id, command) in pending {
            let result = match controllers.get(&session_id) {
                None => CommandResult::RejectedNotController,
                Some(player) => super::commands::apply(&mut world, *player, command.action),
            };
            let tick = world.tick();
            acknowledgements.entry(session_id).or_default().push(
                GameplayServerMessage::CommandAck {
                    sequence: command.sequence,
                    result,
                    applied_tick: tick,
                },
            );
        }
        let advance_started = std::time::Instant::now();
        let advanced = self.advance_world(world).await;
        self.probe.record(advance_started.elapsed(), orders);
        let Some(mut world) = advanced else {
            tracing::error!("gameplay tick worker failed");
            return;
        };
        let changes = world.take_remembered_building_changes();
        // The simulation has stepped: from here the world is only read, so
        // commands and map work may queue for the write lock while the views
        // are built on worker threads.
        let world = world.downgrade();
        let built = tokio::task::spawn_blocking(move || fanout::build(&world, &views, &changes))
            .await
            .ok()
            .flatten();
        let Some(built) = built else {
            tracing::error!("gameplay fan-out worker failed");
            // The tick's building changes were taken and are lost: every
            // session rebuilds its building baseline.
            for session in self.sessions.lock().await.values_mut() {
                session.rebuild_buildings = true;
            }
            return;
        };
        let delivery_started = std::time::Instant::now();
        let failed = {
            let mut sessions = self.sessions.lock().await;
            deliver(&mut sessions, &built, acknowledgements)
        };
        for id in failed {
            self.disconnect(id).await;
        }
        let [viewers, frame, sessions] = built.phases;
        self.fanout_probe
            .record([viewers, frame, sessions, delivery_started.elapsed()]);
        self.publish_resources().await;
    }
}

fn view(session_id: u64, session: &Session) -> Option<View> {
    session.subscription.map(|subscription| View {
        session_id,
        subscription,
        vision: session.vision_revision,
        viewer: session.viewer,
        primary: session.primary_unit_id,
        ai: session.ai,
        rebuild: session.rebuild_buildings,
    })
}

#[path = "tick/tests/rebuild.rs"]
#[cfg(test)]
mod rebuild;
#[path = "tick/tests/scenario.rs"]
#[cfg(test)]
mod scenario;
#[path = "tick/tests.rs"]
#[cfg(test)]
mod tests;
