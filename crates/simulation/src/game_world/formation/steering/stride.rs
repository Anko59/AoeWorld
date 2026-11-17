//! Experiment (#865 Q4, never merged as is): a mover no human sees steps on
//! every `STRIDE`-th tick only, banking each skipped tick's movement in its
//! carry so that it covers the same ground in a longer step.
use super::*;

/// Ticks between two steps of an unseen mover; 1 disables the experiment.
pub(crate) const STRIDE: u64 = 4;
/// The one human seat of the scoreboard save.
const HUMAN: aoe_core::PlayerId = aoe_core::PlayerId(0);

impl GameWorld {
    /// Removes the movers that sit out this tick and returns them.
    pub(crate) fn park_unseen_movers(&mut self, movers: &mut Vec<EntityId>) -> Vec<EntityId> {
        let mut parked = Vec::new();
        movers.retain(|&id| {
            let park = self.park_unseen(id);
            if park {
                parked.push(id);
            }
            !park
        });
        parked
    }

    /// True when `id` sits out this tick: it is ordered to move, no human
    /// sees it and its turn is not due. Its skipped movement is banked.
    pub(crate) fn park_unseen(&mut self, id: EntityId) -> bool {
        if STRIDE <= 1 || (self.tick.0 + u64::from(id.0)).is_multiple_of(STRIDE) {
            return false;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        // Fighters keep every tick: an attacker that moves in jumps strikes
        // less often (attempt 1 lost 5-7% of kills per game tick).
        if self.unit_visible_to(HUMAN, id)
            || self.combat.attackers.contains_key(&id)
            || self.units[index].state.attack_target.is_some()
        {
            return false;
        }
        // A formation member has no order of its own: it banks the credit
        // `formation_motion` would give it this tick.
        if let Some(leader) = self.units[index].formation
            && leader != id
        {
            let unit = self.units[index].state;
            let own = self.movement_speed(unit.kind, unit.player);
            let road = self.road_speed(unit.kind, unit.position, own);
            let Some(group) = self.formations.get_mut(&leader) else {
                return false;
            };
            let Some(member) = group.member_index(id) else {
                return false;
            };
            let credit = super::super::matched_speed(group.pace, road);
            group.members[member].carry = group.members[member].carry.saturating_add(credit);
            return true;
        }
        let Some(mut order) = self.units[index].order else {
            return false;
        };
        let unit = self.units[index].state;
        let banked = match self.formations.get(&id) {
            Some(group) => group.pace.0,
            None => {
                self.road_speed(
                    unit.kind,
                    unit.position,
                    self.movement_speed(unit.kind, unit.player),
                )
                .0
            }
        };
        order.speed_carry = order.speed_carry.saturating_add(banked);
        self.units[index].order = Some(order);
        true
    }
}
