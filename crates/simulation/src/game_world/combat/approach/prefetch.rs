//! Position searches for the attacker phase, computed side by side.
//!
//! The searches of one tick are the largest cost of the phase. They only
//! read the world, so those that the attacker loop is about to ask for are
//! computed first, in parallel, on the state at the start of the loop. The
//! loop then consumes them in canonical attacker order, so the outcome never
//! depends on the worker count.
use super::FAR_REUSE_TILES;
use crate::GameWorld;
use crate::game_world::combat::{MELEE_RANGE_SUBUNITS, tick::Goal};
use aoe_core::EntityId;

/// A smaller fight spends more on looking for searches to prefetch than the
/// few it has would save, so it runs inline.
const MIN_ATTACKERS: usize = 256;

/// Fewer predicted searches than this run inline: a fork-join and a second
/// look at each attacker would cost more than the searches save.
const MIN_PREFETCH: usize = 4;

impl GameWorld {
    pub(in crate::game_world::combat) fn prefetch_attack_searches(
        &mut self,
        fighters: &[EntityId],
        now: u64,
    ) {
        // Fewer than `MIN_ATTACKERS` fighters: no entry at the last index.
        if fighters.get(MIN_ATTACKERS - 1).is_none() {
            return;
        }
        let picked = fighters
            .iter()
            .filter_map(|&id| Some((id, self.search_goal(id, now)?)))
            .collect::<Vec<_>>();
        if picked.get(MIN_PREFETCH - 1).is_none() {
            return;
        }
        let world = &*self;
        let found = crate::game_world::parallel::par_map_each(&picked, |(id, goal)| {
            world.attack_search_core(*id, goal)
        });
        for (&(id, goal), search) in picked.iter().zip(found) {
            if let Ok(search) = search {
                self.combat.prefetched.insert(id, (goal, search));
            }
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
        Some(goal)
    }
}
