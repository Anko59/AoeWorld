//! Movement order validation and issuance, shared by player and task orders.
use super::*;
pub(crate) mod diagnostics;
pub use diagnostics::{ISSUER_SPANS, OrderDemand, OrderIssuer, OrderRedundancy};

/// Area commands tolerate a tile; interaction contacts need the exact point.
#[derive(Clone, Copy)]
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
        self.moving_to_goal(id, destination, GoalPrecision::Area)
    }

    pub(crate) fn moving_to_goal(
        &self,
        id: EntityId,
        destination: WorldPosition,
        precision: GoalPrecision,
    ) -> bool {
        self.lookup.get(&id).is_some_and(|&index| {
            let unit = &self.units[index];
            unit.formation.is_none()
                && unit.order.is_some_and(|order| match precision {
                    GoalPrecision::Contact => order.destination == destination,
                    GoalPrecision::Area => {
                        diagnostics::near_goal(order.destination, destination)
                            // A soft arrival reservation cannot discard an
                            // admitted frontier or renew a waiting deadline.
                            && (unit.state.planning
                                || unit.planner.is_some()
                                || !self.destination_point_reserved(order.destination, id))
                    }
                })
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
        self.issue_move_order(id, destination, GoalPrecision::Area, repeated)
    }

    /// A manager selected an exact free arrival contact, rather than an area.
    #[track_caller]
    pub(crate) fn issue_contact_order(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let repeated = self.moving_to_goal(id, destination, GoalPrecision::Contact);
        self.issue_move_order(id, destination, GoalPrecision::Contact, repeated)
    }

    #[track_caller]
    fn issue_move_order(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
        precision: GoalPrecision,
        repeated: bool,
    ) -> Result<bool, GameWorldError> {
        let result = self.issue_move_goal(id, destination, precision)?;
        // A kept route skips the goal's queue reset; a player's plain order still replaces it.
        if repeated {
            self.discard_move_queue_for_order(id);
        }
        self.cancel_transport_journey(id);
        self.cancel_auto_scout(id);
        self.stop_gathering(id);
        self.cancel_attack(id);
        self.economy.builders.remove(&id);
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
