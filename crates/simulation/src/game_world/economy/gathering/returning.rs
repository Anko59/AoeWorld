//! Shared drop-off trips for map resources, farms and carcasses.
use super::*;

impl GameWorld {
    pub(super) fn at_dropoff(&self, unit: crate::GameUnit, task: &GatherTask) -> bool {
        task.dropoff
            // This predicate reads geometry and completion, not projected
            // work or queue progress. Avoid cloning an actively training TC.
            .and_then(|id| self.economy.buildings.get(&id))
            .is_some_and(|building| {
                building.player == unit.player
                    && building.is_complete()
                    && building.kind.accepts(task.kind)
                    && super::super::placement::adjacent(
                        building.footprint,
                        unit.position.tile_floor(),
                    )
            })
    }

    pub(super) fn begin_return(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        let from = unit.position.tile_floor();
        // Buildings come from the shared drop-off rule; only the failed or
        // blocked perimeter tiles of each are skipped.
        let mut order = self.dropoff_order(unit.player, from, task.kind);
        order.retain(|building_id| {
            self.building(*building_id).is_some_and(|b| {
                if !self
                    .managed_target_allowed(id, position(b.footprint.min).unwrap_or(unit.position))
                {
                    return false;
                }
                if unit.kind == UnitKind::FishingShip {
                    b.kind == super::super::BuildingKind::Dock
                } else {
                    b.kind != super::super::BuildingKind::Dock || task.fish.is_some()
                }
            })
        });
        let has_dropoff = !order.is_empty();
        self.count_gather(Count::Returns, 1);
        let farmer = u64::from(task.farm.is_some());
        self.count_gather(Count::FarmReturns, farmer);
        self.count_gather(Count::ReturnBuildings, order.len() as u64);
        // Ranking a building's perimeter sweeps nearby bodies per tile, so
        // buildings are ranked only as the attempts reach them, nearest first.
        // Tiles that failed before this call stay excluded; the same tile
        // listed by two adjacent buildings is still tried under each.
        let failed_before = task.failed_dropoffs.clone();
        let mut untried = false;
        let mut pocket = None;
        let mut probed_origin = false;
        'buildings: for (at, &building) in order.iter().enumerate() {
            let tiles = self.dropoff_tiles(id, building, &failed_before, from);
            for (n, &tile) in tiles.iter().enumerate() {
                let known = self.reject_known_unreachable_task_move(id, tile, pocket.as_ref());
                let issued = if known {
                    self.count_gather(Count::ReturnKnownUnreachable, 1);
                    Err(GameWorldError::Unreachable)
                } else {
                    self.count_gather(Count::ReturnRoutes, 1);
                    self.count_gather(Count::FarmReturnRoutes, farmer);
                    position(tile).and_then(|destination| self.issue_task_move(id, destination))
                };
                // Only a fresh failure with candidates left pays for a source
                // flood; cached failures stay cheap. The component lives for
                // this choice, so ground edits cannot leave a stale proof.
                if issued == Err(GameWorldError::Unreachable)
                    && !known
                    && !probed_origin
                    && (n + 1 < tiles.len() || at + 1 < order.len())
                {
                    probed_origin = true;
                    pocket = self
                        .route_origin_pocket(unit.kind, from, unit.player)
                        .ok()
                        .flatten();
                }
                if let Err(error) = issued
                    && !known
                {
                    self.count_gather(
                        match error {
                            GameWorldError::Unreachable => Count::ReturnUnreachable,
                            GameWorldError::InvalidPosition => Count::ReturnInvalid,
                            _ => Count::ReturnOther,
                        },
                        1,
                    );
                }
                if issued.is_ok() {
                    task.phase = GatherPhase::Returning;
                    task.dropoff = Some(building);
                    task.return_target = Some(tile);
                    if let Some(&index) = self.lookup.get(&id) {
                        self.units[index].state.task = UnitTask::Returning;
                    }
                    return true;
                }
                task.failed_dropoffs.insert(tile);
                if self.fast_route_allowance(unit.player) == 0 {
                    // Untried candidates keep the job: the rest of this
                    // perimeter or any later building's.
                    let failed = &task.failed_dropoffs;
                    untried = tiles[n + 1..].iter().any(|t| !failed.contains(t))
                        || order[at + 1..].iter().any(|&later| {
                            !self.dropoff_tiles_unranked(id, later, failed).is_empty()
                        });
                    break 'buildings;
                }
            }
        }
        task.phase = GatherPhase::WaitingDropoff;
        task.dropoff = None;
        task.return_target = None;
        task.retry_tick = self.tick.0.saturating_add(u64::from(self.config.tick_hz));
        if let Some(&index) = self.lookup.get(&id) {
            self.clear_planner(index);
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            self.units[index].state.task = UnitTask::Carrying;
            self.refresh_spacing_order(id);
        }
        // A full farmer keeps its paid plot while a blocked Town Center
        // waits for a route or replacement drop-off. Other exhausted node
        // jobs can release their reservations and choose another resource.
        task.farm.is_some() || !has_dropoff || untried
    }

    /// A drop-off's usable perimeter tiles: passable, unblocked, not failed
    /// before this trip and inside a managed worker's domain.
    fn dropoff_tiles_unranked(
        &self,
        id: EntityId,
        building: EntityId,
        failed: &std::collections::BTreeSet<TileCoord>,
    ) -> Vec<TileCoord> {
        let (Some(unit), Some(footprint)) =
            (self.unit(id), self.building(building).map(|b| b.footprint))
        else {
            return Vec::new();
        };
        let mut tiles = perimeter(footprint);
        tiles.retain(|tile| {
            self.unit_tile_passable(unit.kind, *tile, &|| false)
                .unwrap_or(false)
                && !self.economy.blocked_tiles.contains(tile)
                && !failed.contains(tile)
                && self.managed_target_allowed(id, position(*tile).unwrap_or(unit.position))
        });
        tiles
    }

    /// The same tiles, free work spots first, then nearest to `from`.
    fn dropoff_tiles(
        &self,
        id: EntityId,
        building: EntityId,
        failed: &std::collections::BTreeSet<TileCoord>,
        from: TileCoord,
    ) -> Vec<TileCoord> {
        let mut tiles = self.dropoff_tiles_unranked(id, building, failed);
        self.count_gather(Count::ReturnTiles, tiles.len() as u64);
        // Stable like `sort_by_key`, but a sweep of nearby bodies per tile
        // runs once instead of once per comparison.
        tiles.sort_by_cached_key(|tile| {
            (
                self.work_spot_busy(*tile, id),
                distance_squared(from, *tile),
                tile.y,
                tile.x,
            )
        });
        tiles
    }

    pub(super) fn task_remaining(&self, task: &GatherTask) -> Option<u16> {
        if let Some(animal) = task.animal {
            return Some(self.animal(animal).map_or(0, |a| a.food_remaining));
        }
        match task.farm {
            Some(farm) => self.farm_remaining(farm),
            None => self.resource_remaining(task.node),
        }
    }

    pub(super) fn resume_resource(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        if task.animal.is_some() {
            if self.task_remaining(task).is_some_and(|left| left > 0)
                && self.resource_has_work_spot(id, task.tile)
            {
                return self.restart_approach(id, task);
            }
            return self.replacement_animal(id, task);
        }
        if let Some(farm) = task.farm {
            // A spent farm is removed once its farmer has delivered; farms are
            // not replaced by a neighbour because they are worked by choice.
            if self.farm_remaining(farm).is_some_and(|food| food > 0) {
                return self.restart_approach(id, task);
            }
            if self.reseed_farm(id, farm) {
                return false;
            }
            self.remove_building(farm);
            return false;
        }
        if self.resource_remaining(task.node) == Some(0) {
            if task.fish.is_none() {
                return self.replacement_resource(id, task);
            }
            let Some(next) = (if task.fish.is_some() {
                self.nearest_fish(id, task.tile, task.node)
            } else {
                self.nearest_resource_matching(
                    task.tile,
                    task.kind,
                    super::super::VILLAGE_RULES.replacement_radius_tiles,
                    Some(task.node),
                    |n| {
                        self.managed_target_allowed(
                            id,
                            position(n.tile).unwrap_or(aoe_core::WorldPosition::new(0, 0)),
                        )
                    },
                )
            }) else {
                return false;
            };
            task.node = next.id;
            task.tile = next.tile;
            task.fish = self.fish_object(next.id);
        }
        self.restart_approach(id, task)
    }

    pub(super) fn restart_approach(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        if (task.farm.is_none()
            && task.animal.is_none()
            && task.fish.is_none()
            && !self.node_has_work_slot(task.node, id))
            || self.approach_resource(id, task.tile).is_err()
        {
            if !(task.farm.is_none() && task.animal.is_none() && task.fish.is_none()) {
                return false;
            }
            let queued = self.node_has_work_slot(task.node, id)
                && (self.units[self.lookup[&id]].last_movement_error.is_none()
                    || self.gather_traffic_blocked(id))
                && self.queue_resource_approach(id, task.tile);
            if !queued {
                return self.replacement_resource(id, task);
            }
        }
        task.work_position = self
            .movement_order(id)
            .map(|o| o.destination)
            .or_else(|| self.unit(id).map(|u| u.position));
        task.phase = GatherPhase::Approaching;
        task.dropoff = None;
        task.return_target = None;
        task.credits = 0;
        task.retry_tick = 0;
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task = task
                .animal
                .map_or(UnitTask::Moving, |animal| self.carcass_walk_task(animal));
        }
        true
    }

    pub(super) fn replacement_animal(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        let (Some(player), Some(kind), Some(animal)) = (
            self.unit(id).map(|u| u.player),
            task.carcass_kind,
            task.animal,
        ) else {
            return false;
        };
        let Some(next) = self.next_food_animal(player, task.tile, kind, animal, id) else {
            return false;
        };
        if next.hp == 0 && next.task == crate::AnimalTask::Carcass {
            task.animal = Some(next.id);
            task.carcass_kind = Some(next.kind);
            task.tile = next.position.tile_floor();
            return self.restart_approach(id, task);
        }
        // Hand the living target to the ordinary hunt/lead/kill state machine.
        let _ = self.issue_hunt(player, &[id], next.id);
        false
    }
}
