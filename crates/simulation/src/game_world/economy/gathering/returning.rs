//! Shared drop-off trips for map resources, farms and carcasses.
use super::*;

impl GameWorld {
    pub(super) fn at_dropoff(&self, unit: crate::GameUnit, task: &GatherTask) -> bool {
        task.dropoff
            .and_then(|id| self.building(id))
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
        let mut candidates = Vec::new();
        for building in order {
            let Some(footprint) = self.building(building).map(|b| b.footprint) else {
                continue;
            };
            let mut tiles = perimeter(footprint);
            tiles.retain(|tile| {
                self.unit_tile_passable(unit.kind, *tile, &|| false)
                    .unwrap_or(false)
                    && !self.economy.blocked_tiles.contains(tile)
                    && !task.failed_dropoffs.contains(tile)
                    && self.managed_target_allowed(id, position(*tile).unwrap_or(unit.position))
            });
            tiles.sort_by_key(|tile| {
                (
                    self.work_spot_busy(*tile, id),
                    distance_squared(from, *tile),
                    tile.y,
                    tile.x,
                )
            });
            candidates.extend(tiles.into_iter().map(|tile| (building, tile)));
        }
        for &(center, tile) in &candidates {
            if let Ok(destination) = position(tile)
                && self.issue_task_move(id, destination).is_ok()
            {
                task.phase = GatherPhase::Returning;
                task.dropoff = Some(center);
                task.return_target = Some(tile);
                if let Some(&index) = self.lookup.get(&id) {
                    self.units[index].state.task = UnitTask::Returning;
                }
                return true;
            }
            task.failed_dropoffs.insert(tile);
            if self.fast_route_allowance(unit.player) == 0 {
                break;
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
        // At most the owned centers' bounded perimeters are attempted once
        // per trip. No center yet may appear later; exhausted choices end it.
        !has_dropoff
            || candidates
                .iter()
                .any(|(_, tile)| !task.failed_dropoffs.contains(tile))
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
            && !self.node_has_work_slot(task.node, task.kind, id))
            || self.approach_resource(id, task.tile).is_err()
        {
            return task.farm.is_none()
                && task.animal.is_none()
                && task.fish.is_none()
                && self.replacement_resource(id, task);
        }
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
