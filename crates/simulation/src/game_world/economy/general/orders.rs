//! Player orders that address the General alone rather than the whole army.
use super::*;
use crate::EconomyError;
use aoe_core::Formation;

impl GameWorld {
    /// A soldier selected without its commander leaves the army. Selecting
    /// the commander alongside its soldiers keeps their existing membership.
    pub fn general_player_order(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        destination: Option<WorldPosition>,
    ) {
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        let mut generals = Vec::new();
        for &id in &sorted {
            if self.unit(id).is_none_or(|u| u.player != player) {
                continue;
            }
            if self.economy.generals.armies.contains_key(&id) {
                generals.push(id);
            } else if let Some(ordered) = self.economy.generals.manual.get_mut(&id) {
                *ordered = self.tick.0;
            } else if let Some(commander) = self.general_controller(id) {
                if sorted.binary_search(&commander).is_ok() {
                    continue;
                }
                self.forget_general(id);
                self.set_sergeant(id, false);
                self.economy.generals.manual.insert(id, self.tick.0);
            } else if self
                .unit(id)
                .is_some_and(|u| u.hp > 0 && role(u.kind).is_some())
                && self.vassal_controller(id).is_none()
            {
                // Never in an army: it is detached all the same, so a General
                // created or arriving later cannot census it. One small entry
                // per living ordered soldier, cleared on death.
                self.economy.generals.manual.insert(id, self.tick.0);
            }
        }
        for general in generals {
            if self.economy.garrison_orders.contains_key(&general) || self.embarked(general) {
                self.hold_general_army(general);
            } else if let Some(destination) = destination {
                self.general_march(general, destination);
            }
        }
    }

    /// Resolve a mixed player selection into independent units and army
    /// commands. The roster owns its queued army waypoints.
    pub fn issue_player_formation_move(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        form: Formation,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        if !ids
            .iter()
            .any(|id| self.economy.generals.armies.contains_key(id))
        {
            self.issue_formation_move(player, ids, form, destination, queued, attack_move)?;
            self.general_player_order(player, ids, None);
            return Ok(());
        }
        let (ids, _) = self.formation_members(player, ids)?;
        let mut independent = Vec::new();
        let mut commanders = Vec::new();
        for &id in &ids {
            if self.economy.generals.armies.contains_key(&id) {
                commanders.push(id);
            } else if self
                .general_controller(id)
                .is_none_or(|g| ids.binary_search(&g).is_err())
            {
                independent.push(id);
            }
        }
        let mut accepted = false;
        let mut error = EconomyError::UnknownEntity;
        if !independent.is_empty() {
            match self.issue_formation_move(
                player,
                &independent,
                form,
                destination,
                queued,
                attack_move,
            ) {
                Ok(()) => {
                    self.general_player_order(player, &independent, None);
                    accepted = true;
                }
                Err(reason) => error = reason,
            }
        }
        for id in commanders {
            match self.issue_owner_general_move(id, destination, queued, attack_move) {
                Ok(()) => accepted = true,
                Err(reason) => error = reason,
            }
        }
        accepted.then_some(()).ok_or(error)
    }

    /// Focus the whole army when its commander is part of an attack.
    /// Other selected soldiers receive independent orders and leave their army
    /// only if their own attack was accepted.
    pub fn issue_player_attack(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        target: EntityId,
    ) -> Result<(), EconomyError> {
        let ids = Self::canonical_group_selection(ids)?;
        if ids.iter().any(|&id| {
            self.unit(id)
                .map(|u| u.player)
                .or_else(|| self.building(id).map(|b| b.player))
                .is_some_and(|owner| owner != player)
        }) {
            return Err(EconomyError::NotOwner);
        }
        let mut commanders = Vec::new();
        let mut expanded = ids.len();
        for &id in &ids {
            let Some(army) = self.economy.generals.armies.get(&id) else {
                continue;
            };
            if army.solo
                && !army
                    .members
                    .iter()
                    .any(|member| ids.binary_search(member).is_ok())
            {
                continue;
            }
            // The ordinary group order limit also bounds implicit members.
            expanded += army
                .members
                .iter()
                .filter(|member| ids.binary_search(member).is_err())
                .count();
            Self::validate_group_order_size(expanded)?;
            commanders.push(id);
        }
        let mut accepted = false;
        let mut error = EconomyError::UnknownEntity;
        for &id in &ids {
            if self
                .general_controller(id)
                .is_some_and(|g| ids.binary_search(&g).is_ok())
            {
                continue;
            }
            match self.issue_attack(player, id, target) {
                Ok(()) => {
                    self.cancel_patrol(id);
                    self.discard_move_queue_for_order(id);
                    if let Some(attack) = self.combat.attackers.get_mut(&id) {
                        attack.resume = None;
                    }
                    if commanders.binary_search(&id).is_ok() {
                        self.focus_general_army(id, target);
                    } else {
                        self.general_player_order(player, &[id], None);
                    }
                    accepted = true;
                }
                Err(reason) => error = reason,
            }
        }
        accepted.then_some(()).ok_or(error)
    }

    fn focus_general_army(&mut self, id: EntityId, target: EntityId) {
        let Some(commander) = self.unit(id) else {
            return;
        };
        self.hold_general_army(id);
        let Some(army) = self.economy.generals.armies.get_mut(&id) else {
            return;
        };
        army.solo = false;
        army.state = GeneralState::Fighting;
        let members: Vec<_> = army.members.iter().copied().collect();
        for member in members {
            let _ = self.issue_attack(commander.player, member, target);
        }
    }

    /// Move the General by himself. The army keeps its place and is not led,
    /// until the next army march or Stop. Movement and collision use the
    /// ordinary one-member offensive group order.
    pub fn issue_general_move(
        &mut self,
        player: PlayerId,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<(), EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::General || !self.economy.generals.armies.contains_key(&id) {
            return Err(EconomyError::WrongUnitKind);
        }
        self.issue_formation_move(
            player,
            &[id],
            aoe_core::Formation::Line,
            destination,
            false,
            true,
        )?;
        self.hold_general_army(id);
        Ok(())
    }

    fn owned_general(&self, player: PlayerId, id: EntityId) -> Result<(), EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::General || !self.economy.generals.armies.contains_key(&id) {
            return Err(EconomyError::WrongUnitKind);
        }
        Ok(())
    }

    /// The Train button: a General buys recruits only while it is active, and
    /// it switches itself off once the army is full.
    pub fn issue_general_training(
        &mut self,
        player: PlayerId,
        id: EntityId,
        enabled: bool,
    ) -> Result<(), EconomyError> {
        self.owned_general(player, id)?;
        if let Some(a) = self.economy.generals.armies.get_mut(&id) {
            a.training = enabled;
        }
        Ok(())
    }

    /// The unit-mix sliders, 0..=100 per role.
    pub fn issue_general_mix(
        &mut self,
        player: PlayerId,
        id: EntityId,
        mix: [u8; 5],
    ) -> Result<(), EconomyError> {
        self.owned_general(player, id)?;
        if mix.iter().any(|&share| share > 100) {
            return Err(EconomyError::InvalidTarget);
        }
        if let Some(a) = self.economy.generals.armies.get_mut(&id) {
            a.mix = mix;
        }
        Ok(())
    }

    fn hold_general_army(&mut self, id: EntityId) {
        self.restore_general_stances(id);
        self.cancel_general_recall(id);
        let Some(a) = self.economy.generals.armies.get_mut(&id) else {
            return;
        };
        a.destination = None;
        a.owner_march = None;
        a.waypoints.clear();
        a.routed = None;
        a.layout = None;
        a.commander_march = None;
        for squad in &mut a.squads {
            squad.march = None;
            squad.final_leg = false;
        }
        a.solo = true;
        a.retreat_until = 0;
        a.state = GeneralState::Idle;
        let mut members = [None; GENERAL_ARMY_LIMIT];
        for (slot, &m) in members.iter_mut().zip(&a.members) {
            *slot = Some(m);
        }
        for member in members.into_iter().flatten() {
            if let Some(&index) = self.lookup.get(&member) {
                self.stop_unit_orders(index);
            }
        }
    }
}
