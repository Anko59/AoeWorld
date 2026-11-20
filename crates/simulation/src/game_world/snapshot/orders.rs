//! Restore pending player intent after all current jobs have been reissued.
use super::{GameWorld, RestoreReport, UnitRecord, WorldSnapshot};
use aoe_core::EntityId;
use std::collections::BTreeSet;

impl GameWorld {
    /// Orders units back to what they were doing, through the ordinary order
    /// paths; a refused order leaves the unit idle.
    pub(super) fn resume_jobs(&mut self, snapshot: &WorldSnapshot, report: &mut RestoreReport) {
        let mut grouped = std::collections::BTreeSet::new();
        let mut failed_moves = std::collections::BTreeSet::new();
        for group in &snapshot.formations {
            if group.pace_limit.is_some_and(|(n, d)| n == 0 || d == 0) {
                continue;
            }
            let members: Vec<EntityId> = group
                .members
                .iter()
                .copied()
                .filter(|id| {
                    self.lookup.contains_key(id) && self.unit(*id).is_some_and(|u| u.hp > 0)
                })
                .collect();
            let Some(player) = members
                .first()
                .and_then(|id| self.unit(*id))
                .map(|u| u.player)
            else {
                continue;
            };
            let moved = self
                .issue_formation_move(
                    player,
                    &members,
                    group.form,
                    group.destination,
                    false,
                    group.attack_move,
                )
                .is_ok();
            if moved && let Some(pace) = group.pace_limit {
                let _ = self.issue_formation_pace(player, members[0], Some(pace));
            }
            // Each queued waypoint keeps its own attack-move flag.
            for &(at, attack_move) in &group.queue {
                let _ =
                    self.issue_formation_move(player, &members, group.form, at, true, attack_move);
            }
            if moved {
                grouped.extend(members);
                report.formations += 1;
            }
        }
        for record in &snapshot.units {
            let (id, player) = (record.unit.id, record.unit.player);
            if record.management.is_some() && self.restore_management(record) {
                report.managers += 1;
            }
            if record.garrison_in.is_some() || record.embarked_in.is_some() {
                continue;
            }
            let result = if let Some(building) = record.garrison_order {
                let result = self.issue_garrison(player, id, building);
                if result.is_ok() && record.shelter_intent != Default::default() {
                    self.economy
                        .shelter_intents
                        .insert(id, record.shelter_intent);
                }
                result
            } else if let Some((_, Some(farm))) = record.gather {
                self.issue_farm(player, id, farm)
            } else if let Some((node, None)) = record.gather {
                self.issue_gather(player, id, node)
            } else if let Some(building) = record.build {
                self.issue_construct(player, &[id], building)
            } else if let Some(animal) = record.hunt {
                self.issue_hunt(player, &[id], animal)
            } else if let Some(target) = record.unit.attack_target {
                let result = self.issue_attack(player, id, target);
                if let Some(attack) = self.combat.attackers.get_mut(&id) {
                    attack.resume = record.scalar_attack_move.map(|at| {
                        super::super::formation::Resume::new(
                            at,
                            crate::MovementWaypoint::individual(at, true),
                        )
                    });
                    attack.swing_started = record.swing_started.unwrap_or(0).min(self.tick.0);
                }
                result
            } else if let Some(at) = record.scalar_attack_move {
                self.issue_attack_move(id, at).map(|_| ())
            } else if record.unit.auto_scout {
                // Plan again from the restored position and explored terrain.
                self.issue_auto_scout(player, id, true)
            } else if let Some(at) = record.move_to.filter(|_| !grouped.contains(&id)) {
                let result = self
                    .issue_move(id, at)
                    .map(|_| ())
                    .map_err(crate::EconomyError::Movement);
                if result.is_err() {
                    failed_moves.insert(id);
                }
                result
            } else {
                continue;
            };
            match result {
                Ok(()) => report.jobs_resumed += 1,
                Err(_) => report.jobs_lost += 1,
            }
        }
        self.restore_player_queues(&snapshot.units, &failed_moves);
        for record in &snapshot.units {
            if let Some(patrol) = record
                .patrol
                .filter(|points| points.iter().all(|&at| self.config.valid_map_position(at)))
                && self.unit(record.unit.id).is_some_and(|u| u.hp > 0)
            {
                self.combat.patrols.insert(record.unit.id, patrol);
            }
        }
    }

    pub(super) fn restore_player_queues(
        &mut self,
        records: &[UnitRecord],
        failed_moves: &BTreeSet<EntityId>,
    ) {
        // A unit can retain pending player legs behind a formation or combat
        // job. Appending through issue_queued_move could instead start a new
        // leg immediately when it has no current movement route.
        for record in records {
            let id = record.unit.id;
            if record.queue.is_empty()
                || self.unit(id).is_none_or(|u| u.hp == 0)
                || self.embarked(id)
            {
                continue;
            }
            let occupied = self.formation_waypoints(id).map_or_else(
                || usize::from(self.movement_order(id).is_some()),
                |destinations| destinations.len(),
            );
            let available = Self::move_queue_width().saturating_sub(occupied.max(1));
            if available == 0 {
                continue;
            }
            let mut queue = crate::game_world::PlayerMoveQueue::default();
            queue
                .pending
                .extend(
                    record
                        .queue
                        .iter()
                        .take(available)
                        .map(|&(position, attack_move)| {
                            crate::game_world::MovementWaypoint::individual(position, attack_move)
                        }),
                );
            self.player_override(id);
            self.move_queues.insert(id, queue);
            // Only a refused scalar move needs this recovery: successful
            // attack, gather and other jobs must retain their current intent.
            // Generals retry their manager's current intent through decisions.
            if failed_moves.contains(&id) && !self.economy.generals.armies.contains_key(&id) {
                self.resume_queued_moves(&mut [id]);
            }
        }
    }
}
