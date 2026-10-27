//! Bounded local resource replacement ranked from the actual drop-off.
use super::super::VILLAGE_RULES;
use super::*;
use aoe_map::ResourceNode;
use std::collections::{BTreeSet, VecDeque};

impl GameWorld {
    pub(super) fn replacement_resource(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        let from = unit.position.tile_floor();
        let dropoff = task
            .dropoff
            .or_else(|| self.nearest_dropoff(unit.player, from, task.kind))
            .and_then(|id| self.building(id))
            .filter(|b| b.player == unit.player && b.is_complete() && b.kind.accepts(task.kind))
            .map(|b| b.footprint);
        let radius = VILLAGE_RULES.replacement_radius_tiles;
        // Stay in the old work area, even when the drop-off is far away. At
        // most (2r+1)^2 tiles and their cached resource blocks are inspected.
        let mut candidates: Vec<ResourceNode> = Vec::new();
        if let Terrain::Map { generator, overlay } = &self.terrain {
            let low = TileCoord::new(task.tile.x - radius, task.tile.y - radius);
            let high = TileCoord::new(task.tile.x + radius, task.tile.y + radius);
            for by in low.y.div_euclid(chunks::BLOCK_TILES)..=high.y.div_euclid(chunks::BLOCK_TILES)
            {
                for bx in
                    low.x.div_euclid(chunks::BLOCK_TILES)..=high.x.div_euclid(chunks::BLOCK_TILES)
                {
                    candidates.extend(
                        self.block_resource_nodes(generator, overlay, (bx, by))
                            .iter()
                            .copied()
                            .filter(|n| {
                                self.managed_target_allowed(
                                    id,
                                    position(n.tile).unwrap_or(unit.position),
                                ) && n.id != task.node
                                    && n.kind == task.kind
                                    && (n.kind != ResourceKind::Food
                                        || n.object == aoe_map::ObjectKind::ForageBush)
                                    && overlay.resource_available(*n)
                                    && self.node_has_work_slot(n.id, id)
                                    && distance_squared(n.tile, task.tile) <= (radius as u64).pow(2)
                            }),
                    );
                }
            }
        }
        candidates.sort_unstable_by_key(|n| {
            (
                dropoff.map_or_else(
                    || distance_squared(n.tile, from),
                    |rect| super::super::dropoff::ring_distance(rect, n.tile),
                ),
                n.id,
            )
        });
        if candidates.is_empty() {
            return false;
        }
        // A small flood from the last reached work spot certifies local tiles,
        // even when the worker has just made a long trip to a Town Center.
        // respecting terrain edges and building corners. No candidate behind
        // a river or a closed wall can win just by being close to the camp.
        let work = task
            .work_tile
            .filter(|tile| self.tile_passable(*tile))
            .or_else(|| {
                // A first order may come from across the map. Certify the
                // clicked work area locally; route admission still starts at
                // the actual worker, with the same eight-node bound.
                perimeter(aoe_core::TileRect::from_xywh(
                    task.tile.x,
                    task.tile.y,
                    1,
                    1,
                ))
                .into_iter()
                .filter(|tile| self.tile_passable(*tile))
                .min_by_key(|tile| (distance_squared(from, *tile), tile.y, tile.x))
            })
            .unwrap_or(from);
        let mut seen = BTreeSet::from([work]);
        let mut queue = VecDeque::from([work]);
        while let Some(tile) = queue.pop_front() {
            for (dx, dy) in [
                (0, -1),
                (-1, 0),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if seen.contains(&next) || seen.len() >= 1024 {
                    continue;
                }
                if self.building_step_clear(tile, next)
                    && self.unit_step_clear(unit.kind, tile, next).unwrap_or(false)
                {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
        let mut attempts = 0;
        for node in candidates {
            let rect = aoe_core::TileRect::new(
                node.tile,
                TileCoord::new(node.tile.x + 1, node.tile.y + 1),
            );
            if !perimeter(rect).iter().any(|t| seen.contains(t)) {
                continue;
            }
            attempts += 1;
            if self
                .approach_resource_in(id, node.tile, Some(&seen))
                .is_ok()
            {
                task.work_position = self
                    .movement_order(id)
                    .map(|o| o.destination)
                    .or_else(|| self.unit(id).map(|u| u.position));
                task.node = node.id;
                task.tile = node.tile;
                task.fish = self.fish_object(node.id);
                task.phase = GatherPhase::Approaching;
                task.return_target = None;
                task.credits = 0;
                task.retry_tick = 0;
                if let Some(&index) = self.lookup.get(&id) {
                    self.units[index].state.task = UnitTask::Moving;
                }
                return true;
            }
            if attempts == 8 {
                break;
            }
        }
        false
    }
}
