//! A bounded player-command FIFO, separate from a pathfinder's tile route.
use crate::{EconomyError, GameWorld, GameWorldError};
use aoe_core::{EntityId, MAX_MOVE_DESTINATIONS, WorldPosition};

impl GameWorld {
    pub fn issue_queued_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
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
            || self
                .economy
                .blocked_tiles
                .contains(&destination.tile_floor())
        {
            return Err(GameWorldError::InvalidPosition.into());
        }
        if self.movement_order(id).is_none() {
            let moving = self.issue_move(id, destination)?;
            if moving {
                self.move_queues.entry(id).or_default();
            }
            return Ok(moving);
        }
        if self.move_queues.get(&id).map_or(1, |q| q.len() + 1) >= MAX_MOVE_DESTINATIONS {
            return Err(EconomyError::OrderTooLarge);
        }
        // An accepted manual waypoint takes ownership of the current route;
        // autonomous frontier selection must not replace the queued legs.
        self.cancel_auto_scout(id);
        self.move_queues
            .entry(id)
            .or_default()
            .push_back(destination);
        Ok(true)
    }

    /// Authoritative marker list; ordinary, unqueued movement has no markers.
    pub fn move_destinations(&self, id: EntityId) -> Vec<WorldPosition> {
        if let Some(destinations) = self.formation_destinations(id) {
            return destinations;
        }
        let Some(queue) = self.move_queues.get(&id) else {
            return Vec::new();
        };
        self.movement_order(id)
            .map(|o| o.destination)
            .into_iter()
            .chain(queue.iter().copied())
            .collect()
    }

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
            let Some(mut queue) = self.move_queues.remove(&id) else {
                continue;
            };
            while let Some(next) = queue.pop_front() {
                // A building may have blocked a previously valid destination.
                // Skip that failed leg; never discard the later commands.
                if self.issue_move(id, next).is_ok_and(|moving| moving) {
                    self.move_queues.insert(id, queue);
                    break;
                }
            }
        }
    }
}
