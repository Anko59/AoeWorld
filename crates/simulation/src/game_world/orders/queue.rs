//! A bounded player-command FIFO, separate from a pathfinder's tile route.
use super::orders::diagnostics::near_goal;
use crate::{EconomyError, GameWorld, GameWorldError};
use aoe_core::{EntityId, MAX_MOVE_DESTINATIONS, WorldPosition};

impl GameWorld {
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
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0 && !self.embarked(u.id))
            .ok_or(EconomyError::UnknownEntity)?;
        if !self.config.valid_map_position(destination) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        let destination = self.config.snap_ground_position(destination);
        if !self
            .unit_tile_passable(unit.kind, destination.tile_floor(), &|| false)
            .map_err(GameWorldError::Environment)?
            || !self.route_tile_admitted(unit.player, destination.tile_floor())
        {
            return Err(GameWorldError::InvalidPosition.into());
        }
        if !self.queued_move_active(id) {
            let moving = self.start_leg(id, destination, attack_move)?;
            if moving {
                self.move_queues.entry(id).or_default();
            }
            return Ok(moving);
        }
        let formation_pending = self.formation_queue_summary(id).map_or(0, |(_, n)| n);
        if self.move_queues.get(&id).map_or(1, |q| q.len() + 1) + formation_pending
            >= Self::move_queue_width()
        {
            return Err(EconomyError::OrderTooLarge);
        }
        // An accepted manual waypoint takes ownership of the current route;
        // autonomous frontier selection must not replace the queued legs.
        self.player_override(id);
        self.cancel_auto_scout(id);
        self.move_queues
            .entry(id)
            .or_default()
            .push_back((destination, attack_move));
        Ok(true)
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

    /// Authoritative marker list, combining formation and player-command legs.
    pub fn move_destinations(&self, id: EntityId) -> Vec<WorldPosition> {
        if let Some(mut destinations) = self.formation_destinations(id) {
            if let Some(queue) = self.move_queues.get(&id) {
                destinations.extend(queue.iter().map(|&(at, _)| at));
            }
            return destinations;
        }
        let Some(queue) = self.move_queues.get(&id) else {
            return Vec::new();
        };
        self.movement_order(id)
            .map(|o| o.destination)
            .into_iter()
            .chain(queue.iter().map(|&(at, _)| at))
            .collect()
    }

    fn queued_move_active(&self, id: EntityId) -> bool {
        self.movement_order(id).is_some()
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
        if self.move_queues.is_empty() {
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
            if !self.move_queues.contains_key(&id) || self.queued_move_active(id) {
                continue;
            }
            let Some(mut queue) = self.move_queues.remove(&id) else {
                continue;
            };
            while let Some((next, attack_move)) = queue.pop_front() {
                // A building may have blocked a previously valid destination.
                // Skip that failed leg; never discard the later commands.
                if self
                    .start_leg(id, next, attack_move)
                    .is_ok_and(|moving| moving)
                {
                    // An army march turns on this leg; a solo commander keeps
                    // his army holding throughout the automatic tick phases.
                    if self
                        .economy
                        .generals
                        .armies
                        .get(&id)
                        .is_some_and(|army| !army.solo)
                    {
                        self.general_march(id, next);
                    }
                    self.move_queues.insert(id, queue);
                    break;
                }
            }
        }
    }
}
