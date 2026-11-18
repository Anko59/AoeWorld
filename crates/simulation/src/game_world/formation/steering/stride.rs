//! A mover no human seat sees steps on every `STRIDE`-th tick only and banks
//! each skipped tick's movement in its carry, so it covers the same ground in
//! fewer, longer steps. Fighters and anything a person can see step every tick.
//! A formation strides on its leader's phase, so an unseen group steps as one
//! block, and its targets are prepared, on one tick in four.
use super::*;

/// Ticks between two steps of an unseen mover.
pub(crate) const STRIDE: u64 = 4;

impl GameWorld {
    /// Removes the movers that sit out this tick and returns them.
    pub(crate) fn park_unseen_movers(&mut self, movers: &mut Vec<EntityId>) -> Vec<EntityId> {
        let mut parked = Vec::new();
        if self.vision.human_seats.is_empty() {
            return parked;
        }
        movers.retain(|&id| {
            let park = self.park_unseen(id);
            if park {
                parked.push(id);
            }
            !park
        });
        parked
    }

    fn seen_by_a_person(&self, id: EntityId) -> bool {
        self.vision
            .human_seats
            .iter()
            .any(|&player| self.unit_visible_to(player, id))
    }

    /// Whether the unit at `index` sits out this tick: no person sees it, it
    /// is not fighting and its turn (its leader's, in a formation) is not due.
    fn sits_out(&self, index: usize) -> bool {
        let id = self.units[index].state.id;
        let phase = self.units[index].formation.unwrap_or(id);
        !self.vision.human_seats.is_empty()
            && !(self.tick.0 + u64::from(phase.0)).is_multiple_of(STRIDE)
            // Fighters keep every tick: an attacker that moves in jumps strikes
            // less often (the first experiment lost 5-7% of kills per tick).
            && !self.combat.attackers.contains_key(&id)
            && self.units[index].state.attack_target.is_none()
            && !self.seen_by_a_person(id)
    }

    /// Whether every member the group steers sits out this tick; its targets
    /// then stay as they are, since its progress cannot move. Detached
    /// members fight or walk on their own and never read those targets.
    pub(crate) fn group_sits_out(&self, leader: EntityId, group: &Group) -> bool {
        !self.vision.human_seats.is_empty()
            && !(self.tick.0 + u64::from(leader.0)).is_multiple_of(STRIDE)
            && group.members.iter().filter(|m| !m.detached).all(|m| {
                self.lookup
                    .get(&m.id)
                    .is_some_and(|&index| self.sits_out(index))
            })
    }

    /// True when `id` sits out this tick (see `sits_out`). Its skipped
    /// movement is banked.
    pub(crate) fn park_unseen(&mut self, id: EntityId) -> bool {
        if self.vision.human_seats.is_empty() {
            return false;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        self.sits_out(index) && self.bank_rest(index, id)
    }

    /// Banks the movement the unit at `index` skips this tick.
    pub(crate) fn bank_rest(&mut self, index: usize, id: EntityId) -> bool {
        let unit = self.units[index].state;
        // A formation member has no order of its own: it banks the credit
        // `formation_motion` would give it this tick.
        if let Some(leader) = self.units[index].formation
            && leader != id
        {
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

#[path = "stride/tests.rs"]
#[cfg(test)]
mod tests;
