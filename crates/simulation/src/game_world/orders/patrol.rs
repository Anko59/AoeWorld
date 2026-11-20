//! Repeat ordinary attack-move legs between two endpoints.
use crate::{EconomyError, GameWorld, GameWorldError, UnitKind, unit_combat};
use aoe_core::{EntityId, PlayerId, WorldPosition};

impl GameWorld {
    pub fn issue_patrol(
        &mut self,
        player: PlayerId,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), EconomyError> {
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0 && !self.embarked(id))
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if matches!(unit.kind, UnitKind::Villager | UnitKind::General)
            || unit_combat(unit.kind).attack == 0
        {
            return Err(EconomyError::WrongUnitKind);
        }
        if !self.config.valid_map_position(destination) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        let destination = self.config.clamp_ground_position(destination);
        if crate::game_world::orders::diagnostics::near_goal(unit.position, destination) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        if self.issue_attack_move(id, destination)? {
            self.combat.patrols.insert(id, [unit.position, destination]);
        }
        Ok(())
    }

    pub fn cancel_patrol(&mut self, id: EntityId) {
        self.combat.patrols.remove(&id);
    }

    pub(in crate::game_world) fn resume_attack_move(
        &mut self,
        id: EntityId,
        at: WorldPosition,
    ) -> bool {
        let patrol = self.combat.patrols.remove(&id);
        let moving = self.issue_attack_move(id, at).is_ok_and(|moving| moving);
        if moving && let Some(pair) = patrol {
            self.combat.patrols.insert(id, pair);
        }
        moving
    }

    pub(in crate::game_world) fn turn_patrol(&mut self, id: EntityId) {
        let Some(&[home, destination]) = self.combat.patrols.get(&id) else {
            return;
        };
        if self.combat.resuming.contains_key(&id) || self.combat.attackers.contains_key(&id) {
            return;
        }
        let Some(unit) = self.unit(id).filter(|unit| unit.hp > 0) else {
            self.cancel_patrol(id);
            return;
        };
        self.cancel_patrol(id);
        if crate::game_world::orders::diagnostics::near_goal(unit.position, destination)
            && self.issue_attack_move(id, home).is_ok_and(|moving| moving)
        {
            self.combat.patrols.insert(id, [destination, home]);
        }
    }
}
