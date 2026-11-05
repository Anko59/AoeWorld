//! Player orders that address the General alone rather than the whole army.
use super::*;
use crate::EconomyError;

impl GameWorld {
    /// A player's accepted order on `ids`. A General never overwrites it: each
    /// soldier ordered without his General leaves the army and stays out of it
    /// until the player next orders a General nearby. An order that includes
    /// the General himself is an army order, and `destination` is his march.
    pub fn general_player_order(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        destination: Option<WorldPosition>,
    ) {
        if self.economy.generals.armies.is_empty() {
            return;
        }
        let mut sorted = ids.to_vec();
        sorted.sort_unstable();
        for &id in &sorted {
            if self.unit(id).is_none_or(|u| u.player != player) {
                continue;
            }
            if self.economy.generals.armies.contains_key(&id) {
                if let Some(destination) = destination {
                    self.general_march(id, destination);
                }
            } else if let Some(general) = self.general_controller(id)
                && sorted.binary_search(&general).is_err()
            {
                self.forget_general(id);
                self.set_sergeant(id, false);
                self.economy.generals.manual.insert(id);
            }
        }
    }

    /// The player ordered this General: soldiers around him that finished what
    /// they were told may answer his next census again.
    pub(super) fn readmit_manual_soldiers(&mut self, general: EntityId) {
        let Some(centre) = self.unit(general).map(|u| u.position) else {
            return;
        };
        let manual = std::mem::take(&mut self.economy.generals.manual);
        self.economy.generals.manual = manual
            .into_iter()
            .filter(|&id| {
                self.unit(id).is_some_and(|u| {
                    u.moving
                        || u.planning
                        || u.attack_target.is_some()
                        || distance(centre, u.position) > (24 * 1024_i64).pow(2)
                })
            })
            .collect();
    }

    /// Move the General by himself. The army keeps its place and is not led,
    /// until the next army march or Stop. Friendly followers yield to him in
    /// collision (movement_step/collision.rs).
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
        self.issue_move(id, destination)
            .map_err(EconomyError::Movement)?;
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
        let Some(centre) = self.unit(id).map(|u| u.position) else {
            return;
        };
        self.restore_general_stances(id);
        let Some(a) = self.economy.generals.armies.get_mut(&id) else {
            return;
        };
        a.destination = None;
        a.routed = None;
        a.layout = None;
        a.solo = true;
        a.retreat_until = 0;
        a.state = GeneralState::Idle;
        let mut members = [None; GENERAL_ARMY_LIMIT];
        for (slot, &m) in members.iter_mut().zip(&a.members) {
            *slot = Some(m);
        }
        for member in members.into_iter().flatten() {
            if let Some(&index) = self.lookup.get(&member)
                && in_influence(centre, self.units[index].state.position)
            {
                self.stop_unit_orders(index);
            }
        }
    }
}
