//! Experiment (#865 Q4, never merged as is): a mover no human sees steps on
//! every `STRIDE`-th tick only, banking each skipped tick's movement in its
//! carry so that it covers the same ground in a longer step.
use super::*;

/// Ticks between two steps of an unseen mover; 1 disables the experiment.
pub(crate) const STRIDE: u64 = 4;
/// The one human seat of the scoreboard save.
const HUMAN: aoe_core::PlayerId = aoe_core::PlayerId(0);

impl GameWorld {
    /// True when `id` sits out this tick: it is ordered to move, no human
    /// sees it and its turn is not due. Its skipped movement is banked.
    pub(crate) fn park_unseen(&mut self, id: EntityId) -> bool {
        if STRIDE <= 1 || (self.tick.0 + u64::from(id.0)).is_multiple_of(STRIDE) {
            return false;
        }
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        let Some(mut order) = self.units[index].order else {
            return false;
        };
        if self.unit_visible_to(HUMAN, id) {
            return false;
        }
        let unit = self.units[index].state;
        let banked = match self.formations.get(&id) {
            Some(group) => group.pace.0,
            None => {
                self.road_speed(unit.kind, unit.position, self.movement_speed(unit.kind, unit.player))
                    .0
            }
        };
        order.speed_carry = order.speed_carry.saturating_add(banked);
        self.units[index].order = Some(order);
        true
    }
}
