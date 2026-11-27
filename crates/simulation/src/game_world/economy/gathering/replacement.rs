//! Bounded local resource replacement ranked from the actual drop-off.
use super::super::VILLAGE_RULES;
use super::*;
use aoe_map::ResourceNode;

/// Gatherers on one tree beyond which automatic retargeting avoids it.
pub(crate) const AUTO_TREE_CUTTERS: usize = 2;

impl GameWorld {
    /// Whether an automatic pick should skip this tree for a freer one.
    pub(crate) fn tree_is_crowded(&self, n: &ResourceNode, own: Option<WorkTarget>) -> bool {
        n.kind == ResourceKind::Wood
            && self.economy.gatherers.count_target(WorkTarget::Node(n.id))
                - usize::from(own == Some(WorkTarget::Node(n.id)))
                >= AUTO_TREE_CUTTERS
    }

    /// Re-pick before an obstructed approach gives up. Four searches per tick
    /// and one attempt per worker/second bound the local flood and route work.
    pub(crate) fn repick_obstructed_gatherer(&mut self, id: EntityId) -> bool {
        if self.economy.gather_repick_budget == 0 {
            return false;
        }
        let Some(task) = self.economy.gatherers.get(&id) else {
            return false;
        };
        if task.phase != GatherPhase::Approaching
            || !task.crowded
            || task.farm.is_some()
            || task.animal.is_some()
            || task.fish.is_some()
            || self.tick.0 < task.retry_tick
            || !self
                .unit(id)
                .is_some_and(|u| self.constrained_worksite(u.kind, task.tile))
        {
            return false;
        }
        self.economy.gather_repick_budget -= 1;
        let Some(mut task) = self.economy.gatherers.remove(&id) else {
            return false;
        };
        task.retry_tick = self.tick.0 + u64::from(self.config.tick_hz);
        let tile = task.tile;
        let changed = self.replacement_resource_in(id, &mut task, true, tile, false);
        self.economy.gatherers.insert(id, task);
        if changed {
            self.index_destination(id);
        }
        changed
    }

    pub(super) fn replacement_resource(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        self.replacement_resource_in(id, task, false, task.tile, false)
    }

    pub(super) fn replacement_initial_resource(
        &mut self,
        id: EntityId,
        task: &mut GatherTask,
    ) -> bool {
        // A refused click can redirect to a nearby alternative where the
        // worker already stands. A distant, unreachable click leaves its job.
        let nearby = !self.walking_resource_reachable(id, task.tile);
        self.replacement_resource_in(id, task, false, task.tile, nearby)
    }

    pub(super) fn replacement_failed_resource(
        &mut self,
        id: EntityId,
        task: &mut GatherTask,
    ) -> bool {
        let old = task.tile;
        self.replacement_resource(id, task)
            || self.unit(id).is_some_and(|unit| {
                let from = unit.position.tile_floor();
                from != old && self.replacement_resource_in(id, task, false, from, false)
            })
    }

    fn replacement_resource_in(
        &mut self,
        id: EntityId,
        task: &mut GatherTask,
        direct: bool,
        center: TileCoord,
        nearby: bool,
    ) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        let from = unit.position.tile_floor();
        let reserved = self.economy.gatherers.reserved_target(&id);
        let slots = jobs::slot_capacity(unit.kind);
        let dropoff = task
            .dropoff
            .or_else(|| self.nearest_dropoff(unit.player, from, task.kind))
            .and_then(|id| self.building(id))
            .filter(|b| b.player == unit.player && b.is_complete() && b.kind.accepts(task.kind))
            .map(|b| b.footprint);
        let radius = VILLAGE_RULES.replacement_radius_tiles;
        let radius_squared = (radius as u64).pow(2);
        // Probe one work area at a time, even when the drop-off is far away. At
        // most (2r+1)^2 tiles and their cached resource blocks are inspected.
        let mut candidates: Vec<ResourceNode> = Vec::new();
        if let Terrain::Map { generator, overlay } = &self.terrain {
            let low = TileCoord::new(center.x - radius, center.y - radius);
            let high = TileCoord::new(center.x + radius, center.y + radius);
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
                                n.id != task.node
                                    && n.kind == task.kind
                                    && (n.kind != ResourceKind::Food
                                        || n.object == aoe_map::ObjectKind::ForageBush)
                                    && distance_squared(n.tile, center) <= radius_squared
                                    && (!nearby || distance_squared(n.tile, from) <= radius_squared)
                                    && overlay.resource_available(*n)
                                    && (self.economy.gatherers.count_target(WorkTarget::Node(n.id))
                                        - usize::from(reserved == Some(WorkTarget::Node(n.id))))
                                        < slots
                                    && self.managed_target_allowed(
                                        id,
                                        position(n.tile).unwrap_or(unit.position),
                                    )
                            }),
                    );
                }
            }
        }
        // Automatic retargeting spreads cutters: trees that already carry
        // `AUTO_TREE_CUTTERS` gatherers rank after every freer tree in the
        // radius. Manual orders are not routed through here and may stack.
        self.count_gather(Count::Replacements, 1);
        self.count_gather(Count::ReplacementCandidates, candidates.len() as u64);
        let crowded = |n: &ResourceNode| self.tree_is_crowded(n, reserved);
        candidates.sort_unstable_by_key(|n| {
            (
                crowded(n),
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
        let mut choices = candidates
            .into_iter()
            .filter(|node| self.walking_resource_reachable(id, node.tile))
            .take(8)
            .filter_map(|node| {
                self.resource_work_spot(id, node.tile).map(|at| {
                    let key = (
                        crowded(&node),
                        dropoff.map_or(0, |rect| {
                            super::super::dropoff::ring_distance(rect, node.tile)
                        }),
                        !self.interaction_line_clear(unit.kind, unit.position, at),
                        crate::game_world::movement_step::slots::squared(unit.position, at),
                        node.id,
                    );
                    (key, node)
                })
            })
            .collect::<Vec<_>>();
        choices.sort_unstable_by_key(|(key, _)| *key);
        for (key, node) in choices {
            if direct
                && (key.2
                    || task.work_position.is_some_and(|at| {
                        key.3
                            > crate::game_world::movement_step::slots::squared(unit.position, at)
                                + 1024 * 1024
                    }))
            {
                continue;
            }
            if self.approach_resource_in(id, node.tile, true).is_ok() {
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
        }
        false
    }
}
