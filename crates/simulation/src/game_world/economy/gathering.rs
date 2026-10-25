use super::{
    CarriedResource, EconomyError, GatherPhase, GatherTask, UnitKind, UnitTask,
    placement::{distance_squared, perimeter, position},
};
use crate::{GameWorld, GameWorldError, PreparedResourceDepletion, Terrain};
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_map::ResourceKind;

mod chunks;
mod replacement;
mod returning;

#[cfg(test)]
mod tests;

impl GameWorld {
    /// Wildlife food uses exactly the normal return/drop-off state machine.
    pub(crate) fn begin_carcass_gather(
        &mut self,
        id: EntityId,
        animal: EntityId,
    ) -> Result<(), EconomyError> {
        if !self.economy.gatherers.contains_key(&id)
            && self.economy.gatherers.len() >= self.economy.limits.gatherers
        {
            return Err(EconomyError::Capacity);
        }
        let a = *self.animal(animal).ok_or(EconomyError::InvalidResource)?;
        if a.hp > 0 || a.task != crate::AnimalTask::Carcass || a.food_remaining == 0 {
            return Err(EconomyError::InvalidResource);
        }
        self.begin_gather(id, 0, a.position.tile_floor(), ResourceKind::Food, None)?;
        if let Some(task) = self.economy.gatherers.get_mut(&id) {
            task.animal = Some(animal);
            task.carcass_kind = Some(a.kind);
            task.fish = None;
        }
        let index = self.lookup[&id];
        if self.units[index].state.task == UnitTask::Moving {
            self.units[index].state.task = self.carcass_walk_task(animal);
        }
        Ok(())
    }

    fn carcass_task(&self, animal: EntityId) -> UnitTask {
        if self
            .animal(animal)
            .is_some_and(|a| a.kind == crate::AnimalKind::Sheep)
        {
            UnitTask::Shepherding
        } else {
            UnitTask::Hunting
        }
    }

    /// Hunters keep their profession on the way to a carcass; a shepherd only
    /// herds while leading a living sheep, so it walks to the carcass.
    pub(super) fn carcass_walk_task(&self, animal: EntityId) -> UnitTask {
        match self.carcass_task(animal) {
            UnitTask::Shepherding => UnitTask::Moving,
            task => task,
        }
    }

    pub fn resource_remaining(&self, id: u64) -> Option<u16> {
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return None;
        };
        overlay.remaining(generator, id)
    }

    pub fn issue_gather(
        &mut self,
        player: PlayerId,
        id: EntityId,
        node_id: u64,
    ) -> Result<(), EconomyError> {
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0 && !self.embarked(u.id))
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !matches!(unit.kind, UnitKind::Villager | UnitKind::FishingShip) {
            return Err(EconomyError::WrongUnitKind);
        }
        if !self.economy.gatherers.contains_key(&id)
            && self.economy.gatherers.len() >= self.economy.limits.gatherers
        {
            return Err(EconomyError::Capacity);
        }
        let Terrain::Map { generator, overlay } = &self.terrain else {
            return Err(EconomyError::InvalidResource);
        };
        let node = overlay
            .resource_by_id_with_cancel(generator, node_id, &|| false)
            .map_err(GameWorldError::Environment)?
            .filter(|node| overlay.resource_available(*node))
            .ok_or(EconomyError::InvalidResource)?;
        if (unit.kind == UnitKind::FishingShip
            && !matches!(
                node.object,
                aoe_map::ObjectKind::ShoreFish | aoe_map::ObjectKind::DeepFish
            ))
            || (unit.kind == UnitKind::Villager && node.object == aoe_map::ObjectKind::DeepFish)
        {
            return Err(EconomyError::InvalidResource);
        }
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
        self.cancel_garrison_order(id);
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
            work_tile: None,
            node,
            farm,
            animal: None,
            carcass_kind: None,
            fish: if farm.is_none() {
                self.fish_object(node)
            } else {
                None
            },
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
        let keep = self.units[index].state.carried.is_none_or(|load| {
            load.amount < self.gather_capacity(&self.units[index].state, task.farm.is_some())
        }) || self.begin_return(id, &mut task);
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
        candidates.retain(|tile| {
            self.unit_tile_passable(unit.kind, *tile, &|| false)
                .unwrap_or(false)
                && !self.economy.blocked_tiles.contains(tile)
        });
        candidates.sort_by_key(|tile| {
            (
                self.work_spot_busy(*tile, id),
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
        changed: &mut Vec<EntityId>,
    ) {
        let ids = self.economy.gatherers.keys().copied().collect::<Vec<_>>();
        for id in ids {
            // A gatherer that is walking under a movement order, with no
            // failed route, changes nothing in `gather_step`: skip the
            // remove/insert round trip through the task map.
            if let (Some(&index), Some(task)) =
                (self.lookup.get(&id), self.economy.gatherers.get(&id))
                && task.phase != GatherPhase::WaitingDropoff
                && self.units[index].order.is_some()
                && !(task.phase == GatherPhase::Approaching
                    && self.units[index].last_movement_error.is_some())
            {
                continue;
            }
            let Some(mut task) = self.economy.gatherers.remove(&id) else {
                continue;
            };
            let Some(&index) = self.lookup.get(&id) else {
                continue;
            };
            let before = self.units[index].state;
            let keep = self.gather_step(id, &mut task, sink);
            if keep {
                self.economy.gatherers.insert(id, task);
            } else if self.hunting_target(id).is_none()
                && !self.economy.builders.contains_key(&id)
                && let Some(&index) = self.lookup.get(&id)
            {
                self.units[index].state.task = if self.units[index].state.carried.is_some() {
                    UnitTask::Carrying
                } else {
                    UnitTask::Idle
                };
            }
            if let Some(after) = self.unit(id)
                && after != before
            {
                changed.push(after.id);
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
        if task.phase == GatherPhase::Approaching && task.farm.is_none() {
            task.work_tile = Some(tile);
            self.step_up_to_work(id, index, task.tile);
        }
        task.phase = GatherPhase::Gathering;
        if let Some(target) = task
            .animal
            .and_then(|id| self.animal(id))
            .map(|a| a.position)
        {
            self.units[index].state.face_towards(target);
        } else {
            self.units[index].state.face_tile(task.tile);
        }
        self.units[index].state.task = if task.fish.is_some() {
            UnitTask::Fishing
        } else if task.farm.is_some() {
            UnitTask::Farming
        } else if let Some(animal) = task.animal {
            self.carcass_task(animal)
        } else {
            UnitTask::Gathering
        };
        let Some(remaining) = self.task_remaining(task) else {
            return false;
        };
        if remaining == 0
            || self.units[index].state.carried.is_some_and(|load| {
                load.amount >= self.gather_capacity(&self.units[index].state, task.farm.is_some())
            })
        {
            if self.units[index].state.carried.is_some() {
                return self.begin_return(id, task);
            }
            return self.resume_resource(id, task);
        }
        if self.tick.0 < task.retry_tick {
            return true;
        }
        // Fish rates use current DE naval balance, independent of land technologies.
        let rate = match (task.fish, self.units[index].state.kind) {
            (Some(aoe_map::ObjectKind::DeepFish), UnitKind::FishingShip) => 416500,
            (Some(aoe_map::ObjectKind::ShoreFish), UnitKind::FishingShip) => 238000,
            (Some(_), _) => 430000,
            _ => task.animal.and_then(|id| self.animal(id)).map_or_else(
                || {
                    let (numerator, denominator) =
                        self.gather_rate(self.units[index].state.player, task.kind);
                    numerator * 10_000 / denominator
                },
                |a| u32::from(a.kind.rules().gather_hundredths) * 10_000,
            ),
        };
        task.credits += u64::from(rate);
        // Millionths retain stacked 1.15 * 1.15 = 1.3225 exactly.
        let threshold = u64::from(self.config.tick_hz) * 1_000_000;
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
            if amount >= self.gather_capacity(&self.units[index].state, task.farm.is_some())
                || left == 0
            {
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
            if amount >= self.gather_capacity(&self.units[index].state, task.farm.is_some())
                || depletion.remaining == 0
            {
                return self.begin_return(id, task);
            }
        }
        true
    }

    /// Routes end at tile centres, a full tile from the node. As in AoE2 the
    /// worker then steps up against the bush, tree, mine, fish or carcass,
    /// inside its own tile. Facing is left to the facing rules.
    fn step_up_to_work(&mut self, id: EntityId, index: usize, target: TileCoord) {
        let unit = self.units[index].state;
        let stand = unit.position.tile_floor();
        let Ok(centre) = position(stand) else {
            return;
        };
        let (dx, dy) = (target.x - stand.x, target.y - stand.y);
        if unit.position != centre {
            return;
        }
        let state = &mut self.units[index].state;
        state.previous_position = unit.position;
        state.position = aoe_core::WorldPosition::new(
            centre.x + dx.signum() * WORK_STEP,
            centre.y + dy.signum() * WORK_STEP,
        );
        let moved = *state;
        self.track_unit_step(id, stand, &moved);
    }
}

/// How far a worker steps from its tile centre toward the node: it then
/// stands 0.7 tiles from a cardinal neighbour's centre, touching a one-tile
/// bush or trunk, and stays inside its own (passable) tile.
pub(crate) const WORK_STEP: i32 = aoe_core::FIXED_SUBUNITS_PER_TILE * 3 / 10;
