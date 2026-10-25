use super::deliver::deliver;
use super::fanout::{self, View};
use super::{GameplayService, Session};
use crate::gameplay_transport;
use aoe_protocol::{CommandResult, GameplayRole, GameplayServerMessage};
use std::collections::BTreeMap;

impl GameplayService {
    pub async fn tick(&self) {
        let started = std::time::Instant::now();
        let (pending, controllers, views) = {
            let mut ownership = self.ownership.lock().await;
            let mut sessions = self.sessions.lock().await;
            gameplay_transport::promote_expired(&mut ownership, &mut sessions, self.world_id);
            let controllers = sessions
                .iter()
                .filter_map(|(id, session)| {
                    (session.role == GameplayRole::Controller).then_some((*id, session.viewer))
                })
                .collect::<BTreeMap<_, _>>();
            let start = self.command_cursor.fetch_add(
                MAX_COMMANDS_PER_TICK as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            let mut commands = take_pending(&mut sessions, start as usize);
            commands.sort_by_key(|(session_id, command)| (*session_id, command.sequence));
            let views = sessions
                .iter()
                .filter_map(|(id, session)| view(*id, session))
                .collect::<Vec<_>>();
            (commands, controllers, views)
        };
        self.probe.phase("sessions", started.elapsed());
        let started = std::time::Instant::now();
        let mut world = self.world.clone().write_owned().await;
        self.probe.phase("world_lock", started.elapsed());
        let started = std::time::Instant::now();
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
        self.probe.phase("orders", started.elapsed());
        let advance_started = std::time::Instant::now();
        let advanced = self.advance_world(world).await;
        self.probe.record(advance_started.elapsed(), orders);
        let Some(mut world) = advanced else {
            tracing::error!("gameplay tick worker failed");
            return;
        };
        if world.tick().0.is_multiple_of(ROUTE_LOG_TICKS) {
            log_route_scheduler(&world);
        }
        let changes = world.take_remembered_building_changes();
        // The simulation has stepped: from here the world is only read, so
        // commands and map work may queue for the write lock while the views
        // are built on worker threads.
        let world = world.downgrade();
        let fanout_started = std::time::Instant::now();
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
        self.probe.phase("fanout_wait", fanout_started.elapsed());
        let delivery_started = std::time::Instant::now();
        let failed = {
            let mut sessions = self.sessions.lock().await;
            deliver(&mut sessions, &built, acknowledgements)
        };
        for id in failed {
            self.disconnect(id).await;
        }
        let [viewers, frame, sessions] = built.phases;
        self.probe.phase("viewers", viewers);
        self.probe.phase("frame", frame);
        self.probe.phase("session_views", sessions);
        self.probe.phase("delivery", delivery_started.elapsed());
        self.fanout_probe
            .record([viewers, frame, sessions, delivery_started.elapsed()]);
        let started = std::time::Instant::now();
        self.publish_resources().await;
        self.probe.phase("resources", started.elapsed());
        let started = std::time::Instant::now();
        self.prefetch_routes().await;
        self.probe.phase("prefetch", started.elapsed());
    }
}

/// A join burst cannot turn thousands of pending actions into one long tick.
const MAX_COMMANDS_PER_TICK: usize = 32;
fn take_pending(
    sessions: &mut BTreeMap<u64, Session>,
    start: usize,
) -> Vec<(u64, super::PendingCommand)> {
    let ids = sessions.keys().copied().collect::<Vec<_>>();
    let mut commands = Vec::new();
    if ids.is_empty() {
        return commands;
    }
    loop {
        let before = commands.len();
        for turn in 0..ids.len() {
            if commands.len() == MAX_COMMANDS_PER_TICK {
                return commands;
            }
            let id = ids[(start + turn) % ids.len()];
            let Some(session) = sessions.get_mut(&id) else {
                continue;
            };
            if session
                .pending
                .front()
                .is_some_and(super::PendingCommand::ready)
                && let Some(command) = session.pending.pop_front()
            {
                commands.push((id, command));
            }
        }
        if commands.len() == before {
            return commands;
        }
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

/// Route-scheduler occupancy is logged once a minute at 20 Hz.
const ROUTE_LOG_TICKS: u64 = 1_200;

/// A stalled player shows up in the server log: units that wait for a route,
/// the slots held, and any drift between the counters and a recount.
fn log_route_scheduler(world: &aoe_simulation::GameWorld) {
    let stats = world.route_scheduler_stats();
    tracing::info!(
        tick = world.tick().0,
        planners = stats.planners,
        searches = stats.searches,
        waiting = stats.waiting_units,
        waiting_without_planner = stats.waiting_without_planner,
        waiting_players = stats.waiting_players,
        most_waiting = stats.most_waiting,
        most_waiting_player = ?stats.most_waiting_player.map(|player| player.0),
        oldest_wait_ticks = stats.oldest_wait_ticks,
        players_at_planner_cap = stats.players_at_planner_cap,
        players_at_search_cap = stats.players_at_search_cap,
        world_capacity_exhausted = stats.world_capacity_exhausted,
        orphan_planners = stats.orphan_planners,
        oldest_waiting = stats.oldest_waiting.as_deref().unwrap_or("none"),
        cache_entries = world.navigation_cache_usage().entries,
        units = world.units().count(),
        metrics = %stats.metrics,
        "route scheduler"
    );
    if let Some(error) = world.route_accounting_error() {
        tracing::warn!(%error, "route scheduler");
    }
}
