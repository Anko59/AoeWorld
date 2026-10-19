use super::{
    CarriedResource, EconomyError, GatherPhase, GatherTask, MAX_GATHERERS, UnitKind, UnitTask,
    VILLAGE_RULES,
    placement::{distance_squared, perimeter, position},
};
use crate::{GameUnit, GameWorld, GameWorldError, PreparedResourceDepletion, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_map::{ResourceKind, ResourceNode, ResourceOverlay};
use std::collections::BTreeMap;

mod returning;

#[cfg(test)]
mod tests;

// Starting-history entries that replacement searches on this thread touched.
#[cfg(test)]
thread_local! {
    static HISTORY_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn starting_history(
    overlay: &ResourceOverlay,
    min: TileCoord,
    max: TileCoord,
) -> impl Iterator<Item = ResourceNode> + '_ {
    overlay.starting_nodes_in(min, max).inspect(|_| {
        HISTORY_VISITS.with(|visits| visits.set(visits.get() + 1));
    })
}

#[cfg(not(test))]
fn starting_history(
    overlay: &ResourceOverlay,
    min: TileCoord,
    max: TileCoord,
) -> impl Iterator<Item = ResourceNode> + '_ {
    overlay.starting_nodes_in(min, max)
}

impl GameWorld {
    /// Wildlife food uses exactly the normal return/drop-off state machine.
    pub(crate) fn begin_carcass_gather(
        &mut self,
        id: EntityId,
        animal: EntityId,
    ) -> Result<(), EconomyError> {
        if !self.economy.gatherers.contains_key(&id)
            && self.economy.gatherers.len() >= MAX_GATHERERS
        {
            return Err(EconomyError::Capacity);
        }
        let a = self.animal(animal).ok_or(EconomyError::InvalidResource)?;
        if a.hp > 0 || a.task != crate::AnimalTask::Carcass || a.food_remaining == 0 {
            return Err(EconomyError::InvalidResource);
        }
        self.begin_gather(id, 0, a.position.tile_floor(), ResourceKind::Food, None)?;
        if let Some(task) = self.economy.gatherers.get_mut(&id) {
            task.animal = Some(animal);
        }
        Ok(())
    }
    pub fn resource_remaining(&self, id: u64) -> Option<u16> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        overlay.remaining(generator, id)
    }

    pub fn nearest_resource(
        &self,
        origin: TileCoord,
        kind: ResourceKind,
        radius: i32,
        exclude: Option<u64>,
    ) -> Option<ResourceNode> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        // Public callers cannot enlarge the rule's bounded replacement search.
        let radius = radius.clamp(0, VILLAGE_RULES.replacement_radius_tiles);
        let squared = (radius as u64).pow(2);
        // One window read per search. Each tile keeps its first starting entry
        // in identity order, matching the former per-tile scan.
        let min = TileCoord::new(
            origin.x.saturating_sub(radius),
            origin.y.saturating_sub(radius),
        );
        let max = TileCoord::new(
            origin.x.saturating_add(radius),
            origin.y.saturating_add(radius),
        );
        let mut history = BTreeMap::new();
        for node in starting_history(overlay, min, max) {
            history.entry(node.tile).or_insert(node);
        }
        let mut nearest = None;
        let mut score = (u64::MAX, u64::MAX);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let tile = TileCoord::new(origin.x.saturating_add(dx), origin.y.saturating_add(dy));
                let distance = distance_squared(origin, tile);
                if distance > squared {
                    continue;
                }
                if let Some(node) = history
                    .get(&tile)
                    .copied()
                    .or_else(|| generator.object_at(tile))
                    && node.kind == kind
                    && Some(node.id) != exclude
                    && overlay.blocks_node(node)
                    && (distance, node.id) < score
                {
                    nearest = Some(node);
                    score = (distance, node.id);
                }
            }
        }
        nearest
    }

    pub fn issue_gather(
        &mut self,
        player: PlayerId,
        id: EntityId,
        node_id: u64,
    ) -> Result<(), EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::Villager {
            return Err(EconomyError::WrongUnitKind);
        }
        if !self.economy.gatherers.contains_key(&id)
            && self.economy.gatherers.len() >= MAX_GATHERERS
        {
            return Err(EconomyError::Capacity);
        }
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(EconomyError::InvalidResource);
        };
        let node = overlay
            .resource_by_id_with_cancel(generator, node_id, &|| false)
            .map_err(GameWorldError::Environment)?
            .filter(|node| overlay.blocks_node(*node))
            .ok_or(EconomyError::InvalidResource)?;
        // A refused order leaves the unit's current job alone.
        self.begin_gather(id, node.id, node.tile, node.kind, None)?;
        self.economy.builders.remove(&id);
        Ok(())
    }

    /// Start a gather job on a map node or, with `farm`, on a farm building.
    pub(super) fn begin_gather(
        &mut self,
        id: EntityId,
        node: u64,
        tile: TileCoord,
        kind: ResourceKind,
        farm: Option<EntityId>,
    ) -> Result<(), EconomyError> {
        self.approach_resource(id, tile)?;
        self.cancel_hunt(id);
        self.cancel_attack(id);
        let previous_farm = self.economy.gatherers.get(&id).and_then(|task| task.farm);
        let Some(&index) = self.lookup.get(&id) else {
            return Err(EconomyError::UnknownEntity);
        };
        if self.units[index]
            .state
            .carried
            .is_some_and(|load| load.kind != kind)
        {
            self.units[index].state.carried = None;
        }
        let mut task = GatherTask {
            node,
            farm,
            animal: None,
            tile,
            kind,
            phase: GatherPhase::Approaching,
            credits: 0,
            dropoff: None,
            return_target: None,
            failed_dropoffs: Default::default(),
            retry_tick: 0,
        };
        self.units[index].state.task = if self.units[index].state.carried.is_some() {
            UnitTask::Carrying
        } else {
            UnitTask::Moving
        };
        let keep = self.units[index]
            .state
            .carried
            .is_none_or(|load| load.amount != VILLAGE_RULES.villager_capacity)
            || self.begin_return(id, &mut task);
        if keep {
            self.economy.gatherers.insert(id, task);
        } else {
            self.economy.gatherers.remove(&id);
        }
        if let Some(old) = previous_farm
            && Some(old) != farm
        {
            self.remove_spent_farm(old);
        }
        Ok(())
    }

    pub(super) fn approach_resource(
        &mut self,
        id: EntityId,
        target: TileCoord,
    ) -> Result<(), EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        let mut candidates = perimeter(aoe_core::TileRect::new(
            target,
            TileCoord::new(target.x + 1, target.y + 1),
        ));
        candidates.retain(|tile| self.tile_passable(*tile));
        candidates.sort_by_key(|tile| {
            (
                distance_squared(unit.position.tile_floor(), *tile),
                tile.y,
                tile.x,
            )
        });
        let mut failure = GameWorldError::Unreachable;
        for tile in candidates {
            match self.issue_task_move(id, position(tile)?) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    failure = error;
                    if matches!(
                        error,
                        GameWorldError::PathBudgetExceeded | GameWorldError::Environment(_)
                    ) {
                        break;
                    }
                }
            }
        }
        Err(failure.into())
    }

    pub(super) fn advance_gathering(
        &mut self,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
        changed: &mut Vec<GameUnit>,
    ) {
        let ids = self.economy.gatherers.keys().copied().collect::<Vec<_>>();
        for id in ids {
            let Some(mut task) = self.economy.gatherers.remove(&id) else {
                continue;
            };
            let Some(before) = self.unit(id) else {
                continue;
            };
            let keep = self.gather_step(id, &mut task, sink);
            if keep {
                self.economy.gatherers.insert(id, task);
            } else if let Some(&index) = self.lookup.get(&id) {
                self.units[index].state.task = if self.units[index].state.carried.is_some() {
                    UnitTask::Carrying
                } else {
                    UnitTask::Idle
                };
            }
            if let Some(after) = self.unit(id)
                && after != before
            {
                changed.push(after);
            }
        }
    }

    fn gather_step(
        &mut self,
        id: EntityId,
        task: &mut GatherTask,
        sink: &mut impl FnMut(&PreparedResourceDepletion<'_>) -> bool,
    ) -> bool {
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        if task.phase == GatherPhase::WaitingDropoff {
            return self.tick.0 < task.retry_tick || self.begin_return(id, task);
        }
        if self.units[index].order.is_some() {
            return true;
        }
        if task.phase == GatherPhase::Approaching && self.units[index].last_movement_error.is_some()
        {
            return false;
        }
        if task.phase == GatherPhase::Returning {
            let unit = self.units[index].state;
            let adjacent = task
                .dropoff
                .and_then(|id| self.building(id))
                .is_some_and(|center| {
                    center.player == unit.player
                        && perimeter(center.footprint).contains(&unit.position.tile_floor())
                });
            if !adjacent {
                if let Some(tile) = task.return_target.take() {
                    task.failed_dropoffs.insert(tile);
                }
                return self.begin_return(id, task);
            }
            if let Some(load) = unit.carried
                && let Some(player) = self.economy.players.get_mut(&unit.player)
            {
                player.stockpile.deposit(load);
                self.units[index].state.carried = None;
                task.failed_dropoffs.clear();
            }
            return self.resume_resource(id, task);
        }
        let tile = self.units[index].state.position.tile_floor();
        if tile.x.abs_diff(task.tile.x) > 1 || tile.y.abs_diff(task.tile.y) > 1 {
            return self.resume_resource(id, task);
        }
        task.phase = GatherPhase::Gathering;
        self.units[index].state.task = if task.farm.is_some() {
            UnitTask::Farming
        } else if let Some(animal) = task.animal {
            if self
                .animal(animal)
                .is_some_and(|a| a.kind == crate::AnimalKind::Sheep)
            {
                UnitTask::Shepherding
            } else {
                UnitTask::Hunting
            }
        } else {
            UnitTask::Gathering
        };
        let Some(remaining) = self.task_remaining(task) else {
            return false;
        };
        if remaining == 0
            || self.units[index]
                .state
                .carried
                .is_some_and(|load| load.amount >= VILLAGE_RULES.villager_capacity)
        {
            if self.units[index].state.carried.is_some() {
                return self.begin_return(id, task);
            }
            return self.resume_resource(id, task);
        }
        if self.tick.0 < task.retry_tick {
            return true;
        }
        task.credits += u64::from(task.animal.and_then(|id| self.animal(id)).map_or(
            VILLAGE_RULES.gather_hundredths_per_second[task.kind as usize],
            |a| a.kind.rules().gather_hundredths,
        ));
        let threshold = u64::from(self.config.tick_hz) * 100;
        if task.credits < threshold {
            return true;
        }
        task.credits -= threshold;
        if task.farm.is_some() || task.animal.is_some() {
            let left = if let Some(animal) = task.animal {
                self.deplete_carcass(animal)
            } else {
                task.farm.and_then(|farm| self.deplete_farm(farm))
            };
            let Some(left) = left else {
                return false;
            };
            let amount = self.units[index]
                .state
                .carried
                .map_or(0, |load| load.amount)
                + 1;
            self.units[index].state.carried = Some(CarriedResource {
                kind: task.kind,
                amount,
            });
            if amount == VILLAGE_RULES.villager_capacity || left == 0 {
                return self.begin_return(id, task);
            }
            return true;
        }
        let depletion = match self.prepare_resource_depletion(task.node, 1) {
            Ok(mutation) if sink(&mutation) => mutation.commit(),
            _ => {
                // Failed storage earns no delayed burst of resources. Retry
                // after a bounded game-time pause, without accruing credits.
                task.credits = 0;
                task.retry_tick = self.tick.0.saturating_add(u64::from(self.config.tick_hz));
                return true;
            }
        };
        if depletion.removed > 0 {
            let amount = self.units[index]
                .state
                .carried
                .map_or(0, |load| load.amount)
                + depletion.removed;
            self.units[index].state.carried = Some(CarriedResource {
                kind: task.kind,
                amount,
            });
            self.economy.depletions.push(depletion);
            if amount == VILLAGE_RULES.villager_capacity || depletion.remaining == 0 {
                return self.begin_return(id, task);
            }
        }
        true
    }
}
