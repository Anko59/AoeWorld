//! Farms: a completed farm holds food that one villager works by standing on it.
use super::{BuildingKind, EconomyError, MAX_GATHERERS, UnitKind};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord};
use aoe_map::ResourceKind;

impl GameWorld {
    /// Order a villager to work `farm`. A farm takes one farmer at a time.
    pub fn issue_farm(
        &mut self,
        player: PlayerId,
        id: EntityId,
        farm: EntityId,
    ) -> Result<(), EconomyError> {
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
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
            && self.economy.gatherers.len() >= MAX_GATHERERS
        {
            return Err(EconomyError::Capacity);
        }
        let center = TileCoord::new(
            building.footprint.min.x + building.footprint.width() / 2,
            building.footprint.min.y + building.footprint.height() / 2,
        );
        self.economy.builders.remove(&id);
        self.begin_gather(id, 0, center, ResourceKind::Food, Some(farm))
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

    pub(super) fn farm_remaining(&self, farm: EntityId) -> Option<u16> {
        self.building(farm)?.food_remaining
    }
}
