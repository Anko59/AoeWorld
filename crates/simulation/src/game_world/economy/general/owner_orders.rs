//! A player's army waypoints belong to the command roster. Individual squads
//! can pause to fight or finish at different times without consuming the FIFO.
use super::*;
use crate::{EconomyError, GameWorldError, MovementWaypoint};

impl GameWorld {
    pub(super) fn issue_owner_general_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        let destination = self.validate_queued_leg(id, destination, attack_move)?;
        let army = self
            .economy
            .generals
            .armies
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?;
        if queued && army.owner_march.is_some() {
            if army.waypoints.len() + 1 >= Self::move_queue_width() {
                return Err(EconomyError::OrderTooLarge);
            }
            army.waypoints.push_back((destination, attack_move));
            return Ok(());
        }
        self.start_owner_general_move(id, destination, attack_move)?;
        self.economy
            .generals
            .armies
            .get_mut(&id)
            .ok_or(EconomyError::UnknownEntity)?
            .waypoints
            .clear();
        Ok(())
    }

    fn start_owner_general_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        let prior = &self.economy.generals.armies[&id];
        let changed =
            prior.destination != Some(destination) || prior.owner_march != Some(attack_move);
        if changed {
            if attack_move {
                let player = self.unit(id).ok_or(GameWorldError::UnknownEntity)?.player;
                self.issue_formation_move(
                    player,
                    &[id],
                    aoe_core::Formation::Line,
                    destination,
                    false,
                    true,
                )?;
                self.cancel_transport_journey(id);
            } else {
                self.issue_move(id, destination)?;
            }
            self.move_queues.remove(&id);
            self.restore_general_stances(id);
            let army = self
                .economy
                .generals
                .armies
                .get_mut(&id)
                .ok_or(GameWorldError::UnknownEntity)?;
            army.destination = Some(destination);
            army.routed = None;
            army.layout = None;
            army.commander_march = None;
            for squad in &mut army.squads {
                squad.march = None;
                squad.final_leg = false;
            }
        }
        let army = self
            .economy
            .generals
            .armies
            .get_mut(&id)
            .ok_or(GameWorldError::UnknownEntity)?;
        army.owner_march = Some(attack_move);
        army.solo = false;
        army.state = GeneralState::Marching;
        army.retreat_until = 0;
        self.cancel_general_recall(id);
        Ok(())
    }

    pub(crate) fn owner_general_waypoints(&self, id: EntityId) -> Option<Vec<MovementWaypoint>> {
        let army = self.economy.generals.armies.get(&id)?;
        let at = army.destination?;
        let attack_move = army.owner_march?;
        Some(
            std::iter::once((at, attack_move))
                .chain(army.waypoints.iter().copied())
                .map(|(at, attack_move)| MovementWaypoint::individual(at, attack_move))
                .collect(),
        )
    }

    pub(super) fn advance_owner_general_move(
        &mut self,
        general: GameUnit,
        changed: &mut Vec<EntityId>,
        work: &mut GeneralWork,
    ) {
        let id = general.id;
        let army = &self.economy.generals.armies[&id];
        let Some(goal) = army.destination else { return };
        let mut available: Vec<_> = army
            .members
            .iter()
            .filter_map(|&id| self.unit(id))
            .filter(|u| u.hp > 0 && !self.embarked(u.id))
            .collect();
        available.push(general);
        let reached_or_failed = |id| {
            self.unit(id).is_none_or(|u| u.hp == 0 || self.embarked(id))
                || self.movement_failure(id).is_some()
        };
        let finished = (army.routed == Some(goal) || reached_or_failed(id))
            && army.squads.iter().all(|s| {
                (s.final_leg && s.march.is_some_and(|(objective, _)| objective == goal))
                    || s.members.iter().copied().all(reached_or_failed)
            })
            && available.iter().all(|u| !self.general_unit_busy(u));
        if !finished {
            self.march_squads(id, &available, goal, false, changed, work);
            return;
        }
        let Some(army) = self.economy.generals.armies.get_mut(&id) else {
            return;
        };
        let mut waypoints = std::mem::take(&mut army.waypoints);
        while let Some((destination, attack_move)) = waypoints.pop_front() {
            if self
                .start_owner_general_move(id, destination, attack_move)
                .is_ok()
            {
                if let Some(army) = self.economy.generals.armies.get_mut(&id) {
                    army.waypoints = waypoints;
                }
                return;
            }
        }
        if let Some(army) = self.economy.generals.armies.get_mut(&id) {
            army.owner_march = None;
            army.destination = None;
            army.state = GeneralState::Idle;
        }
    }

    fn general_unit_busy(&self, u: &GameUnit) -> bool {
        u.moving
            || u.planning
            || u.attack_target.is_some()
            || self.combat.resuming.contains_key(&u.id)
            || self.movement_goal(u.id).is_some()
    }

    pub(crate) fn general_has_owner_order(&self, u: GameUnit) -> bool {
        let army = &self.economy.generals.armies[&u.id];
        army.owner_march.is_some()
            || self.economy.garrison_orders.contains_key(&u.id)
            || (army.solo && self.general_unit_busy(&u))
            || self.general_has_explicit_attack(u.id)
    }

    pub(super) fn general_has_explicit_attack(&self, id: EntityId) -> bool {
        let explicit = |id| {
            self.combat
                .attackers
                .get(&id)
                .is_some_and(|attack| attack.origin.is_none() && attack.resume.is_none())
        };
        let army = &self.economy.generals.armies[&id];
        explicit(id)
            || (army.state == GeneralState::Fighting
                && army.destination.is_none()
                && army.members.iter().copied().any(explicit))
    }
}
