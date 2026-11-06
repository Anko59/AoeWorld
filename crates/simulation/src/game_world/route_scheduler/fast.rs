//! Near fast attempts precede far work without changing players' fair shares.
use super::{GameWorld, far_planner, far_route};
use aoe_core::{EntityId, PlayerId};
use std::collections::BTreeMap;

/// Part of the tick's fair budget reserved for far formation routes (at most
/// half of it), shared by their carriers; what they leave goes to the fair pass.
pub(crate) const GROUP_FAR_ROUTE_WORK_PER_TICK: u32 = 8_192;

impl GameWorld {
    pub(super) fn serve_fast_near_routes(
        &mut self,
        waiting: &BTreeMap<PlayerId, Vec<EntityId>>,
        changed: &mut Vec<EntityId>,
    ) {
        let players = waiting.keys().copied().collect::<Vec<_>>();
        let start = (self.tick.0 % players.len() as u64) as usize;
        for turn in 0..players.len() {
            if self.fast_route_budget == 0 {
                break;
            }
            let player = players[(start + turn) % players.len()];
            for &id in &waiting[&player] {
                let Some(&index) = self.lookup.get(&id) else {
                    continue;
                };
                let unit = &self.units[index];
                let far = unit.planner.as_ref().map_or_else(
                    || {
                        unit.order.is_some_and(|order| {
                            far_route(order.waypoint.tile_floor(), order.target_tile)
                        })
                    },
                    far_planner,
                );
                if !far && !unit.fast_route_tried {
                    self.search_waiting_route(player, id, 0, changed);
                }
            }
        }
    }

    /// Far formation routes get a reserved part of `budget` before the fair
    /// pass; returns the work spent from it. With
    /// 99 AIs the fair share per player was a few hundred work a tick, a
    /// quarter of it for far routes, and far routes got 0.03% of all fair
    /// work: no probe group found its route within 3,600 ticks.
    pub(super) fn serve_far_groups(
        &mut self,
        waiting: &BTreeMap<PlayerId, Vec<EntityId>>,
        budget: u32,
        changed: &mut Vec<EntityId>,
    ) -> u32 {
        let carriers: Vec<(PlayerId, EntityId)> = waiting
            .iter()
            .flat_map(|(&player, ids)| ids.iter().map(move |&id| (player, id)))
            .filter(|&(_, id)| {
                self.lookup.get(&id).is_some_and(|&index| {
                    self.units[index].formation == Some(id) && self.waits_for_far_route(index)
                })
            })
            .collect();
        if carriers.is_empty() {
            return 0;
        }
        let reserved = GROUP_FAR_ROUTE_WORK_PER_TICK.min(budget / 2);
        let mut budget = reserved;
        let start = (self.tick.0 % carriers.len() as u64) as usize;
        for turn in 0..carriers.len() {
            if budget == 0 {
                break;
            }
            let (player, id) = carriers[(start + turn) % carriers.len()];
            let share = (budget / (carriers.len() - turn) as u32).max(1);
            let spent = self.search_waiting_route(player, id, share, changed);
            budget -= spent.min(budget);
            self.route_metrics.attribution.group_pool_work += u64::from(spent);
        }
        reserved - budget
    }
}
