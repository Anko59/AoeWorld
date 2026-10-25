//! Farms: a completed farm holds food that one villager works by standing on it.
use super::{BuildingKind, EconomyError, UnitKind};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_map::ResourceKind;

impl GameWorld {
    /// Prepay one reseed at an owned completed Mill, or cancel and refund one.
    pub fn issue_farm_reseed(
        &mut self,
        player: PlayerId,
        mill: EntityId,
        add: bool,
    ) -> Result<(), EconomyError> {
        let building = self
            .economy
            .buildings
            .get_mut(&mill)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if building.kind != BuildingKind::Mill {
            return Err(EconomyError::FarmUnavailable);
        }
        if !building.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        let count = building.farm_reseeds;
        let wood = BuildingKind::Farm.rules().cost.wood;
        if add {
            if count >= 99 {
                return Err(EconomyError::Capacity);
            }
            let stock = &mut self
                .economy
                .players
                .get_mut(&player)
                .ok_or(EconomyError::NotOwner)?
                .stockpile;
            if stock.wood < wood {
                return Err(EconomyError::InsufficientResources);
            }
            stock.wood -= wood;
        } else {
            if count == 0 {
                return Err(EconomyError::FarmUnavailable);
            }
            let stock = &mut self
                .economy
                .players
                .get_mut(&player)
                .ok_or(EconomyError::NotOwner)?
                .stockpile;
            stock.wood = stock.wood.saturating_add(wood);
        }
        building.farm_reseeds = if add { count + 1 } else { count - 1 };
        self.remember_visible_buildings();
        Ok(())
    }

    /// The last load has been delivered. Rebuild in place with normal work
    /// time, then construction's usual completion path restores this farmer.
    pub(super) fn reseed_farm(&mut self, worker: EntityId, farm: EntityId) -> bool {
        let Some(old) = self.building(farm).cloned() else {
            return false;
        };
        let center = TileCoord::new(old.footprint.min.x + 1, old.footprint.min.y + 1);
        let mill = self
            .player_buildings_of(old.player, BuildingKind::Mill)
            .filter(|b| b.is_complete() && b.farm_reseeds > 0)
            .min_by_key(|b| (super::dropoff::ring_distance(b.footprint, center), b.id))
            .map(|b| b.id);
        let Some(mill) = mill else {
            return false;
        };
        let Some(queue) = self.economy.buildings.get_mut(&mill) else {
            return false;
        };
        if queue.farm_reseeds == 0 {
            return false;
        }
        queue.farm_reseeds -= 1;
        let food = self.farm_food(old.player);
        let Some(building) = self.economy.buildings.get_mut(&farm) else {
            self.refund_reseed(mill);
            return false;
        };
        building.state = super::ConstructionState::UnderConstruction;
        building.work_done = 0;
        building.hp = 1;
        building.food_remaining = Some(food);
        if self.issue_construct(old.player, &[worker], farm).is_err() {
            self.economy.buildings.insert(farm, old);
            self.refund_reseed(mill);
            return false;
        }
        self.update_building_vision(farm);
        true
    }

    fn refund_reseed(&mut self, mill: EntityId) {
        if let Some(building) = self.economy.buildings.get_mut(&mill) {
            building.farm_reseeds = building.farm_reseeds.saturating_add(1);
        }
    }

    /// Order a villager to work `farm`. A farm takes one farmer at a time.
    pub fn issue_farm(
        &mut self,
        player: PlayerId,
        id: EntityId,
        farm: EntityId,
    ) -> Result<(), EconomyError> {
        let unit = self
            .unit(id)
            .filter(|u| u.hp > 0 && !self.embarked(u.id))
            .ok_or(EconomyError::UnknownEntity)?;
        if unit.player != player {
            return Err(EconomyError::NotOwner);
        }
        if unit.kind != UnitKind::Villager {
            return Err(EconomyError::WrongUnitKind);
        }
        let building = self.building(farm).ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if building.kind != BuildingKind::Farm {
            return Err(EconomyError::FarmUnavailable);
        }
        if !building.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        if building.food_remaining.is_none_or(|food| food == 0)
            || self
                .economy
                .gatherers
                .iter()
                .any(|(worker, task)| task.farm == Some(farm) && *worker != id)
        {
            return Err(EconomyError::FarmUnavailable);
        }
        if !self.economy.gatherers.contains_key(&id)
            && self.economy.gatherers.len() >= self.economy.limits.gatherers
        {
            return Err(EconomyError::Capacity);
        }
        let center = TileCoord::new(
            building.footprint.min.x + building.footprint.width() / 2,
            building.footprint.min.y + building.footprint.height() / 2,
        );
        self.begin_gather(id, 0, center, ResourceKind::Food, Some(farm))?;
        self.economy.builders.remove(&id);
        Ok(())
    }

    /// Take one food from a farm, returning what is left.
    pub(super) fn deplete_farm(&mut self, farm: EntityId) -> Option<u16> {
        let food = self
            .economy
            .buildings
            .get_mut(&farm)?
            .food_remaining
            .as_mut()?;
        *food = food.saturating_sub(1);
        Some(*food)
    }

    /// End `id`'s gather job. A farm whose last food is already on the farmer
    /// would otherwise linger empty and unworkable, so it is removed here.
    pub(crate) fn stop_gathering(&mut self, id: EntityId) {
        self.cancel_garrison_order(id);
        self.move_queues.remove(&id);
        self.cancel_hunt(id);
        if let Some(task) = self.economy.gatherers.remove(&id)
            && let Some(farm) = task.farm
        {
            self.remove_spent_farm(farm);
        }
    }

    pub(super) fn remove_spent_farm(&mut self, farm: EntityId) {
        if self.farm_remaining(farm) == Some(0) {
            self.remove_building(farm);
        }
    }

    pub(super) fn farm_remaining(&self, farm: EntityId) -> Option<u16> {
        self.building(farm)?.food_remaining
    }
}
