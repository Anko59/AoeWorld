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
}
