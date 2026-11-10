//! Movement order validation and issuance, shared by player and task orders.
use super::*;
pub(crate) mod diagnostics;
pub use diagnostics::{OrderDemand, OrderIssuer};

/// Area commands tolerate a tile; interaction contacts need the exact point.
pub(crate) enum GoalPrecision {
    Contact,
    Area,
}

impl GameWorld {
    #[track_caller]
    pub(crate) fn issue_task_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        self.issue_move_goal(id, destination, GoalPrecision::Contact)
    }

    pub(crate) fn moving_towards(&self, id: EntityId, destination: WorldPosition) -> bool {
        self.lookup.get(&id).is_some_and(|&index| {
            let unit = &self.units[index];
            unit.formation.is_none()
                && unit
                    .order
                    .is_some_and(|order| diagnostics::near_goal(order.destination, destination))
        })
    }

    #[track_caller]
    pub fn issue_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let repeated = self.moving_towards(id, destination);
        let destination = if repeated {
            destination
        } else {
            self.spread_destination(id, destination)?
        };
        let result = self.issue_move_goal(id, destination, GoalPrecision::Area)?;
        let retained_queue = if repeated {
            self.move_queues.remove(&id)
        } else {
            None
        };
        self.cancel_transport_journey(id);
        self.cancel_auto_scout(id);
        self.stop_gathering(id);
        self.cancel_attack(id);
        self.economy.builders.remove(&id);
        if let Some(queue) = retained_queue {
            self.move_queues.insert(id, queue);
        }
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task = if result {
                UnitTask::Moving
            } else if self.units[index].state.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        if !self.economy.stewards.automatic
            && self.unit(id).is_some_and(|u| u.kind == UnitKind::General)
        {
            self.general_march(id, destination);
        }
        Ok(result)
    }
}

#[cfg(test)]
#[path = "orders/tests/noop.rs"]
mod noop_tests;
