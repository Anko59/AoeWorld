//! Position searches for the attacker phase, computed side by side.
//!
//! The searches of one tick are bounded by the shared position budget and
//! are the largest cost of the phase. They only read the world, so those that
//! the attacker loop is about to ask for are computed first, in parallel, on
//! the state at the start of the loop. The loop then consumes them in
//! canonical attacker order and charges the budget there, so the outcome
//! never depends on the worker count.
use super::FAR_REUSE_TILES;
use crate::GameWorld;
use crate::game_world::combat::{MELEE_RANGE_SUBUNITS, tick::Goal};
use aoe_core::EntityId;

impl GameWorld {
    pub(in crate::game_world::combat) fn prefetch_attack_searches(
        &mut self,
        ids: &[EntityId],
        now: u64,
    ) {
        let room = self.attack_search_room() as usize;
        let mut jobs = Vec::with_capacity(room);
        for &id in ids {
            if jobs.len() == room {
                break;
            }
            if self.combat.pursuit_floor.is_some_and(|floor| id <= floor) {
                continue;
            }
            if let Some(goal) = self.search_goal(id, now) {
                jobs.push((id, goal));
            }
        }
        // Waves of at most the remaining position budget, so a search whose
        // turn would come after the budget is spent is never run in vain.
        let mut remaining = self.combat.position_searches as usize;
        let mut at = 0;
        while at < jobs.len() {
            let open = remaining > 0;
            let wave = if open {
                remaining.min(jobs.len() - at)
            } else {
                jobs.len() - at
            };
            let world = &*self;
            let found =
                crate::game_world::parallel::par_map_each(&jobs[at..at + wave], |(id, goal)| {
                    world.attack_search_core(*id, goal, open)
                });
            for (&(id, goal), outcome) in jobs[at..at + wave].iter().zip(found) {
                if let Ok(outcome) = outcome {
                    remaining -= usize::from(outcome.spends);
                    self.combat.prefetched.insert(id, (goal, outcome));
                }
            }
            at += wave;
        }
    }

    /// The goal of the position search `approach` would start for this
    /// attacker now, or `None` when it would not search. It mirrors the
    /// conditions of `approach`; a wrong guess only costs an unused result.
    fn search_goal(&self, id: EntityId, now: u64) -> Option<Goal> {
        let index = *self.lookup.get(&id)?;
        let attacker = *self.combat.attackers.get(&id)?;
        let unit = self.units[index].state;
        if unit.hp == 0 || now < attacker.repath_tick || self.waiting_for_route(index) {
            return None;
        }
        let goal = self.attack_goal(attacker.target, unit.player)?;
        let stats = self.unit_state_stats(&unit);
        let range = crate::game_world::navy::attack_range(&stats);
        let distance = goal.distance_squared(unit.position);
        // A fighter already in reach that is only drifting to a free slot
        // is left to the loop: telling would cost its slot test twice.
        if distance <= range * range {
            return None;
        }
        let order = self.units[index].order;
        if order.is_none() && self.units[index].last_movement_error.is_some() {
            return None;
        }
        let centre = goal.centre().tile_floor();
        let from = unit.position.tile_floor();
        let away = i64::from((from.x - centre.x).abs().max((from.y - centre.y).abs()));
        let far = attacker.far.is_some() || away > FAR_REUSE_TILES;
        if attacker.far.is_some_and(|f| f.centre == centre) && away > FAR_REUSE_TILES {
            return None;
        }
        if let Some(o) = order
            && !far
        {
            let kept = if Self::uses_melee_position(unit.kind) {
                attacker.goal_tile == Some(centre) && self.interaction_point_free(id, o.destination)
            } else {
                let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
                (min * min..=range * range).contains(&goal.distance_squared(o.destination))
            };
            if kept {
                return None;
            }
        }
        (!far || !self.position_searches_closed()).then_some(goal)
    }
}
