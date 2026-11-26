//! Near fast attempts precede far work without changing players' fair shares.
use super::{GameWorld, far_planner, far_route};
use aoe_core::{EntityId, PlayerId};
use std::collections::BTreeMap;

impl GameWorld {
    pub(super) fn serve_fast_near_routes(
        &mut self,
        waiting: &BTreeMap<PlayerId, Vec<EntityId>>,
        changed: &mut Vec<EntityId>,
    ) {
        let players = waiting.keys().copied().collect::<Vec<_>>();
        let start = (self.tick.0 % players.len() as u64) as usize;
        let mut batch = Vec::new();
        let mut reserved = super::batch::Reserved::default();
        for turn in 0..players.len() {
            let player = players[(start + turn) % players.len()];
            for &id in &waiting[&player] {
                let Some(&index) = self.lookup.get(&id) else {
                    continue;
                };
                let unit = &self.units[index];
                let far = unit.planner.as_deref().map_or_else(
                    || {
                        unit.order.is_some_and(|order| {
                            far_route(order.waypoint.tile_floor(), order.target_tile)
                        })
                    },
                    far_planner,
                );
                if !far
                    && !unit.fast_route_tried
                    && let Some(search) = self.prepare_search(player, id, 0, &mut reserved)
                {
                    batch.push(search);
                }
            }
        }
        self.run_searches(batch, reserved, changed);
    }

    /// Far formation routes are served before the fair pass, each carrier
    /// its own `ROUTE_SLICE_WORK` a tick like every other search; returns the
    /// work spent. With 99 AIs the fair share per
    /// player was once a few hundred work a tick, and no probe group found
    /// its route within 3,600 ticks.
    pub(super) fn serve_far_groups(
        &mut self,
        waiting: &BTreeMap<PlayerId, Vec<EntityId>>,
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
        let mut spent = 0_u32;
        for (player, id) in carriers {
            let work = self.search_waiting_route(player, id, super::ROUTE_SLICE_WORK, changed);
            spent = spent.saturating_add(work);
            self.route_metrics.attribution.group_pool_work += u64::from(work);
        }
        spent
    }
}
