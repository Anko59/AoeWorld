use super::{
    CarriedResource, EconomyError, GatherPhase, GatherTask, MAX_GATHERERS, UnitKind, UnitTask,
    VILLAGE_RULES,
    placement::{distance_squared, perimeter, position},
};
use crate::{GameUnit, GameWorld, GameWorldError, PreparedResourceDepletion, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_map::{ResourceKind, ResourceNode};

impl GameWorld {
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
        let mut nearest = None;
        let mut score = (u64::MAX, u64::MAX);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                let tile = TileCoord::new(origin.x.saturating_add(dx), origin.y.saturating_add(dy));
                let distance = distance_squared(origin, tile);
                if distance > squared {
                    continue;
                }
                if let Some(node) = generator.object_at(tile)
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
        let node = generator
            .resource_by_id_with_cancel(node_id, &|| false)
            .map_err(GameWorldError::Environment)?
            .filter(|node| overlay.blocks_node(*node))
            .ok_or(EconomyError::InvalidResource)?;
        self.approach_resource(id, node.tile)?;
        let Some(&index) = self.lookup.get(&id) else {
            return Err(EconomyError::UnknownEntity);
        };
        if self.units[index]
            .state
            .carried
            .is_some_and(|load| load.kind != node.kind)
        {
            self.units[index].state.carried = None;
        }
        let mut task = GatherTask {
            node: node.id,
            tile: node.tile,
            kind: node.kind,
            phase: GatherPhase::Approaching,
            credits: 0,
            dropoff: None,
            retry_tick: 0,
        };
        self.units[index].state.task = if self.units[index].state.carried.is_some() {
            UnitTask::Carrying
        } else {
            UnitTask::Moving
        };
        if self.units[index]
            .state
            .carried
            .is_some_and(|load| load.amount == VILLAGE_RULES.villager_capacity)
        {
            self.begin_return(id, &mut task);
        }
        self.economy.gatherers.insert(id, task);
        Ok(())
    }

    fn approach_resource(&mut self, id: EntityId, target: TileCoord) -> Result<(), EconomyError> {
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

    fn begin_return(&mut self, id: EntityId, task: &mut GatherTask) {
        let Some(unit) = self.unit(id) else {
            return;
        };
        let mut candidates = self
            .buildings()
            .filter(|building| building.player == unit.player)
            .flat_map(|building| {
                perimeter(building.footprint)
                    .into_iter()
                    .map(move |tile| (building.id, tile))
            })
            .collect::<Vec<_>>();
        candidates.retain(|(_, tile)| self.tile_passable(*tile));
        candidates.sort_by_key(|(center, tile)| {
            (
                distance_squared(unit.position.tile_floor(), *tile),
                *center,
                tile.y,
                tile.x,
            )
        });
        for (center, tile) in candidates {
            if let Ok(destination) = position(tile)
                && self.issue_task_move(id, destination).is_ok()
            {
                task.phase = GatherPhase::Returning;
                task.dropoff = Some(center);
                if let Some(&index) = self.lookup.get(&id) {
                    self.units[index].state.task = UnitTask::Returning;
                }
                return;
            }
            if self.planning_budget == 0 {
                break;
            }
        }
        task.phase = GatherPhase::WaitingDropoff;
        task.dropoff = None;
        task.retry_tick = self.tick.0.saturating_add(u64::from(self.config.tick_hz));
        if let Some(&index) = self.lookup.get(&id) {
            self.clear_planner(index);
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            self.units[index].state.task = UnitTask::Carrying;
        }
    }

    fn resume_resource(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        if self.resource_remaining(task.node) == Some(0) {
            let Some(next) = self.nearest_resource(
                task.tile,
                task.kind,
                VILLAGE_RULES.replacement_radius_tiles,
                Some(task.node),
            ) else {
                return false;
            };
            task.node = next.id;
            task.tile = next.tile;
        }
        if self.approach_resource(id, task.tile).is_err() {
            return false;
        }
        task.phase = GatherPhase::Approaching;
        task.dropoff = None;
        task.credits = 0;
        task.retry_tick = 0;
        if let Some(&index) = self.lookup.get(&id) {
            self.units[index].state.task = UnitTask::Moving;
        }
        true
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
            if self.tick.0 >= task.retry_tick {
                self.begin_return(id, task);
            }
            return true;
        }
        if self.units[index].order.is_some() {
            return true;
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
                self.begin_return(id, task);
                return true;
            }
            if let Some(load) = unit.carried
                && let Some(player) = self.economy.players.get_mut(&unit.player)
            {
                player.stockpile.deposit(load);
                self.units[index].state.carried = None;
            }
            return self.resume_resource(id, task);
        }
        let tile = self.units[index].state.position.tile_floor();
        if tile.x.abs_diff(task.tile.x) > 1 || tile.y.abs_diff(task.tile.y) > 1 {
            return self.resume_resource(id, task);
        }
        task.phase = GatherPhase::Gathering;
        self.units[index].state.task = UnitTask::Gathering;
        let Some(remaining) = self.resource_remaining(task.node) else {
            return false;
        };
        if remaining == 0
            || self.units[index]
                .state
                .carried
                .is_some_and(|load| load.amount >= VILLAGE_RULES.villager_capacity)
        {
            if self.units[index].state.carried.is_some() {
                self.begin_return(id, task);
                return true;
            }
            return self.resume_resource(id, task);
        }
        if self.tick.0 < task.retry_tick {
            return true;
        }
        task.credits += u64::from(VILLAGE_RULES.gather_hundredths_per_second[task.kind as usize]);
        let threshold = u64::from(self.config.tick_hz) * 100;
        if task.credits < threshold {
            return true;
        }
        task.credits -= threshold;
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
                self.begin_return(id, task);
            }
        }
        true
    }
}
