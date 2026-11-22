//! A bounded player-command FIFO, separate from a pathfinder's tile route.
use super::orders::diagnostics::near_goal;
use crate::{EconomyError, GameWorld, GameWorldError};
use aoe_core::{EntityId, MAX_MOVE_DESTINATIONS, WorldPosition};
use std::collections::VecDeque;

/// The destination shown for a player order. An ID of zero is private to one unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementWaypoint {
    pub position: WorldPosition,
    pub attack_move: bool,
    pub order_id: u32,
}

impl MovementWaypoint {
    pub(crate) fn individual(position: WorldPosition, attack_move: bool) -> Self {
        Self {
            position,
            attack_move,
            order_id: 0,
        }
    }

    pub(crate) fn leg(self) -> (WorldPosition, bool) {
        (self.position, self.attack_move)
    }
}

#[derive(Debug, Default)]
pub(crate) struct PlayerMoveQueue {
    pub(crate) pending: VecDeque<MovementWaypoint>,
    pub(crate) current: Option<MovementWaypoint>,
}

impl GameWorld {
    /// Allocate an identity only after admitting a shared order.
    pub(crate) fn shared_waypoint(
        &mut self,
        position: WorldPosition,
        attack_move: bool,
    ) -> MovementWaypoint {
        self.waypoint_order = self.waypoint_order.wrapping_add(1).max(1);
        MovementWaypoint {
            position,
            attack_move,
            order_id: self.waypoint_order,
        }
    }

    /// Live admission and save restoration share this command FIFO width.
    pub(crate) const fn move_queue_width() -> usize {
        MAX_MOVE_DESTINATIONS
    }

    /// Temporary automatic orders cannot replace a living commander's FIFO.
    pub(crate) fn discard_move_queue_for_order(&mut self, id: EntityId) {
        if !self.move_queues.contains_key(&id) {
            return;
        }
        if !self.economy.stewards.automatic
            || !self.economy.generals.armies.contains_key(&id)
            || self.unit(id).is_none_or(|unit| unit.hp == 0)
        {
            self.move_queues.remove(&id);
        }
    }

    pub fn issue_queued_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, EconomyError> {
        self.issue_queued_leg(id, destination, false)
    }

    /// Append one player leg; an attack-move leg fights along its own route.
    pub fn issue_queued_leg(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) -> Result<bool, EconomyError> {
        let destination = self.validate_queued_leg(id, destination, attack_move)?;
        if !self.queued_move_active(id) {
            let moving = self.start_leg(id, destination, attack_move)?;
            if moving {
                self.move_queues.entry(id).or_default();
            }
            return Ok(moving);
        }
        let formation_pending = self.formation_queue_summary(id).map_or(0, |(_, n)| n);
        if self.move_queues.get(&id).map_or(1, |q| q.pending.len() + 1) + formation_pending
            >= Self::move_queue_width()
        {
            return Err(EconomyError::OrderTooLarge);
        }
        // An accepted manual waypoint takes ownership of the current route;
        // autonomous frontier selection must not replace the queued legs.
        self.player_override(id);
        self.cancel_auto_scout(id);
        self.cancel_patrol(id);
        self.move_queues
            .entry(id)
            .or_default()
            .pending
            .push_back(MovementWaypoint::individual(destination, attack_move));
        Ok(true)
    }

    pub(crate) fn validate_queued_leg(
        &self,
        id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) -> Result<WorldPosition, EconomyError> {
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0 && !self.embarked(u.id))
            .ok_or(EconomyError::UnknownEntity)?;
        if !self.config.valid_map_position(destination) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        let destination = self.config.snap_ground_position(destination);
        let tile = destination.tile_floor();
        let breach = attack_move && self.hostile_ground_blocker(unit.player, tile).is_some();
        if !breach
            && (!self
                .unit_tile_passable(unit.kind, tile, &|| false)
                .map_err(GameWorldError::Environment)?
                || !self.route_tile_admitted(unit.player, tile))
        {
            return Err(GameWorldError::InvalidPosition.into());
        }
        Ok(destination)
    }

    /// Replace the unit's order by an attack-move: the same one-member group a
    /// multi-unit attack-move uses, so its engage look does the fighting.
    pub fn issue_attack_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, EconomyError> {
        self.start_leg(id, destination, true)
    }

    pub(crate) fn scalar_march_destination(&self, id: EntityId) -> Option<WorldPosition> {
        let unit = self.unit(id)?;
        if !unit.kind.is_naval() && unit.kind != crate::UnitKind::Villager {
            return None;
        }
        self.offensive_resume(id)
            .or_else(|| self.offensive_continuation(id))
            .map(|resume| resume.slot)
    }

    /// Begin a player leg now. Attack-move runs as the unit's own one-member
    /// formation, the route type that acquires targets on the way; Generals
    /// keep their army march and ordinary moves stay scalar.
    fn start_leg(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) -> Result<bool, EconomyError> {
        if attack_move
            && let Some(unit) = self.unit(id)
            && (unit.kind.is_naval() || unit.kind == crate::UnitKind::Villager)
            && crate::unit_combat(unit.kind).attack > 0
        {
            if self.config.valid_map_position(destination)
                && let Some(target) =
                    self.hostile_ground_blocker(unit.player, destination.tile_floor())
                && self
                    .building(target)
                    .is_some_and(|b| self.building_visible_to(unit.player, b.footprint))
            {
                self.issue_attack(unit.player, id, target)?;
                if let Some(attack) = self.combat.attackers.get_mut(&id) {
                    attack.resume = Some(super::formation::Resume::new(
                        destination,
                        MovementWaypoint::individual(destination, true),
                    ));
                }
                return Ok(true);
            }
            let moving = self.issue_move(id, destination)?;
            if moving {
                self.combat.scalar_marches.insert(id);
                if unit.kind == crate::UnitKind::Villager {
                    self.combat.seekers.insert(id);
                    self.combat.quiet.join(id);
                }
            }
            return Ok(moving);
        }
        if attack_move
            && !self.economy.generals.armies.contains_key(&id)
            && let Some(player) = self.unit(id).map(|u| u.player)
            && self
                .issue_formation_move(
                    player,
                    &[id],
                    aoe_core::Formation::Line,
                    destination,
                    false,
                    true,
                )
                .is_ok()
        {
            // A repeated identical group order returns early, so the scalar
            // move's replacement cleanup is done here for every outcome.
            self.discard_move_queue_for_order(id);
            self.cancel_transport_journey(id);
            return Ok(true);
        }
        Ok(self.issue_move(id, destination)?)
    }

    /// Assign one display identity after a mixed selection's scalar orders
    /// have been admitted. Physical destinations can have different offsets.
    pub fn share_selection_waypoint(
        &mut self,
        ids: &[EntityId],
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    ) {
        let destination = self.config.snap_ground_position(destination);
        let marker = self.shared_waypoint(destination, attack_move);
        for &id in ids {
            if queued
                && let Some(last) = self
                    .move_queues
                    .get_mut(&id)
                    .and_then(|queue| queue.pending.back_mut())
                && last.position == destination
                && last.order_id == 0
            {
                *last = marker;
                continue;
            }
            if let Some(resume) = self
                .combat
                .attackers
                .get_mut(&id)
                .and_then(|attack| attack.resume.as_mut())
                .filter(|resume| resume.marker.position == destination)
            {
                resume.marker = marker;
                continue;
            }
            if self.formation_waypoints(id).is_some_and(|waypoints| {
                waypoints
                    .first()
                    .is_some_and(|waypoint| waypoint.position == destination)
            }) {
                self.set_formation_marker(id, marker);
            } else if self.movement_order(id).is_some() {
                self.move_queues.entry(id).or_default().current = Some(marker);
            }
        }
    }

    pub fn move_destinations(&self, id: EntityId) -> Vec<WorldPosition> {
        self.move_waypoints(id)
            .into_iter()
            .map(|(position, _)| position)
            .collect()
    }

    /// Authoritative marker list with each destination's attack-move intent.
    pub fn move_waypoints(&self, id: EntityId) -> Vec<(WorldPosition, bool)> {
        self.movement_waypoints(id)
            .into_iter()
            .map(MovementWaypoint::leg)
            .collect()
    }

    /// Visible waypoint identities survive a formation member leaving to fight.
    pub fn movement_waypoints(&self, id: EntityId) -> Vec<MovementWaypoint> {
        if !self.economy.generals.armies.is_empty()
            && let Some(waypoints) = self.owner_general_waypoints(id)
        {
            return waypoints;
        }
        let queue = self.move_queues.get(&id);
        let pending = queue
            .into_iter()
            .flat_map(|queue| queue.pending.iter().copied());
        if let Some(resume) = self.offensive_resume(id).or_else(|| {
            self.combat
                .scalar_marches
                .contains(&id)
                .then(|| self.offensive_continuation(id))
                .flatten()
        }) {
            return std::iter::once(resume.marker).chain(pending).collect();
        }
        if let Some(mut markers) = self.formation_waypoints(id) {
            markers.extend(pending);
            return markers;
        }
        queue.map_or_else(Vec::new, |queue| {
            self.movement_order(id)
                .map(|order| {
                    queue
                        .current
                        .unwrap_or_else(|| MovementWaypoint::individual(order.destination, false))
                })
                .into_iter()
                .chain(pending)
                .collect()
        })
    }

    fn queued_move_active(&self, id: EntityId) -> bool {
        self.movement_order(id).is_some()
            || self.offensive_resume(id).is_some()
            || self
                .lookup
                .get(&id)
                .and_then(|&index| self.units[index].formation)
                .and_then(|leader| self.formation_movement_goal(id, leader))
                .is_some()
            || self.economy.generals.armies.get(&id).is_some_and(|army| {
                !army.solo
                    && (matches!(
                        army.state,
                        crate::GeneralState::Fighting | crate::GeneralState::Retreating
                    ) || army.destination.is_some_and(|goal| {
                        // A temporary or absent route after restore does not
                        // complete the retained player march. Terminal route
                        // failures can still skip to a later queued leg.
                        match self.formation_queue_summary(id) {
                            Some((current, _)) => !near_goal(current, goal),
                            None => {
                                self.movement_failure(id).is_none()
                                    && self
                                        .unit(id)
                                        .is_some_and(|unit| !near_goal(unit.position, goal))
                            }
                        }
                    }))
            })
    }

    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn resume_queued_moves(&mut self, changed: &mut [EntityId]) {
        if self.move_queues.is_empty() && self.combat.patrols.is_empty() {
            return;
        }
        // Movement proposals may finish in a rotated order. New searches must
        // enter the shared deterministic budget in canonical entity order.
        changed.sort_unstable();
        for &id in changed.iter() {
            if self.movement_order(id).is_some() {
                continue;
            }
            // Only queued units need the formation/army completion check.
            if (!self.move_queues.contains_key(&id) && !self.combat.patrols.contains_key(&id))
                || self.queued_move_active(id)
            {
                continue;
            }
            self.turn_patrol(id);
            let Some(mut queue) = self.move_queues.remove(&id) else {
                continue;
            };
            while let Some(leg) = queue.pending.pop_front() {
                let (next, attack_move) = leg.leg();
                // A building may have blocked a previously valid destination.
                // Skip that failed leg; never discard the later commands.
                if self
                    .start_leg(id, next, attack_move)
                    .is_ok_and(|moving| moving)
                {
                    // Owner army waypoints advance only after the squads settle.
                    if self
                        .economy
                        .generals
                        .armies
                        .get(&id)
                        .is_some_and(|army| !army.solo && army.owner_march.is_none())
                    {
                        self.general_march(id, next);
                    }
                    queue.current = (leg.order_id != 0).then_some(leg);
                    if leg.order_id != 0 {
                        self.set_formation_marker(id, leg);
                    }
                    self.move_queues.insert(id, queue);
                    break;
                }
            }
        }
    }
}
