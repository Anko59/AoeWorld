//! Player orders that address the General alone rather than the whole army.
use super::*;
use crate::EconomyError;

/// Ticks a detached soldier must be past its player's last order before a
/// General's fresh order can take it back.
const MANUAL_IDLE_TICKS: u64 = 300;

impl GameWorld {
    /// The rule (docs/movement/real-terrain.md, "A player's order beats a
    /// General"). Per player a soldier is in an army or detached. Every player
    /// order that names a soldier detaches it and restarts its clock, with or
    /// without armies, and also when its own General is named in the same
    /// order: the soldier follows the order it was given, not his march. An
    /// order that names a General then readmits detached soldiers (see
    /// `readmit_manual_soldiers`), and `destination` is his march.
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
            } else if self.general_controller(id).is_some() {
                self.forget_general(id);
                self.set_sergeant(id, false);
                self.economy.generals.manual.insert(id, self.tick.0);
            }
        }
        for general in generals {
            if let Some(destination) = destination {
                self.general_march(general, destination);
            }
            self.readmit_manual_soldiers(general);
        }
    }

    /// A detached soldier of the General's own player returns to the census
    /// only when it is idle (not moving, planning, attacking or auto-scouting),
    /// within 24 tiles and `MANUAL_IDLE_TICKS` past its last order. A garrisoned
    /// soldier has no map record and stays detached. Only a player's order to
    /// the General calls this, never an automatic march.
    fn readmit_manual_soldiers(&mut self, general: EntityId) {
        let Some((centre, player)) = self.unit(general).map(|u| (u.position, u.player)) else {
            return;
        };
        let now = self.tick.0;
        let manual = std::mem::take(&mut self.economy.generals.manual);
        self.economy.generals.manual = manual
            .into_iter()
            .filter(|&(id, ordered)| match self.unit(id) {
                Some(u) => {
                    u.player != player
                        || u.moving
                        || u.planning
                        || u.attack_target.is_some()
                        || u.auto_scout
                        || now.saturating_sub(ordered) < MANUAL_IDLE_TICKS
                        || distance(centre, u.position) > (24 * 1024_i64).pow(2)
                }
                None => self.is_garrisoned(id),
            })
            .collect();
    }

    fn is_garrisoned(&self, id: EntityId) -> bool {
        self.economy
            .buildings
            .values()
            .any(|b| b.garrison.binary_search_by_key(&id, |g| g.unit.id).is_ok())
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
