//! Opt-in impact stream for offline measurement. Never read by gameplay.
use crate::GameWorld;
use aoe_core::EntityId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DamageObservation {
    pub attacker: EntityId,
    pub target: EntityId,
    /// Effective damage removed, including quarter HP, capped at target health.
    pub amount_quarters: u32,
}
impl GameWorld {
    /// Opt-in buffering: one predictable branch per resolved hit while disabled,
    /// no per-tick scan. Drain each tick when enabled to bound recording memory.
    /// Observations are excluded from snapshots and canonical gameplay hashes.
    pub fn set_damage_observation(&mut self, enabled: bool) {
        self.combat.telemetry.observed = enabled.then(Vec::new);
    }
    pub fn take_damage_observations(&mut self) -> Vec<DamageObservation> {
        self.combat
            .telemetry
            .observed
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }
    pub(in crate::game_world) fn observe_damage(
        &mut self,
        attacker: EntityId,
        target: EntityId,
        amount_quarters: u32,
    ) {
        if let Some(observed) = self.combat.telemetry.observed.as_mut() {
            observed.push(DamageObservation {
                attacker,
                target,
                amount_quarters,
            });
        }
    }
    /// Whether this tick encountered an obstruction/recovery state. Standing
    /// idle, waiting for a planner and being at the destination do not imply it.
    pub fn movement_obstructed(&self, id: EntityId) -> bool {
        self.quality_member_obstructed(id)
            || self.lookup.get(&id).is_some_and(|&index| {
                self.units[index]
                    .obstruction
                    .is_some_and(|o| o.last_tick != u64::MAX && o.last_tick + 1 >= self.tick.0)
            })
    }
}
