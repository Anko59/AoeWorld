use super::{
    EconomyError, TrainingEntry, UnitKind, VILLAGE_RULES,
    placement::{perimeter, position},
};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId};

impl GameWorld {
    pub fn issue_train(
        &mut self,
        player: PlayerId,
        center: EntityId,
        kind: UnitKind,
    ) -> Result<(), EconomyError> {
        let building = self
            .economy
            .buildings
            .get(&center)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if !building.is_complete() {
            return Err(EconomyError::UnderConstruction);
        }
        if !super::trainable_units(building.kind).contains(&kind) {
            return Err(EconomyError::WrongUnitKind);
        }
        if building.training_queue.len() >= VILLAGE_RULES.training_queue_limit {
            return Err(EconomyError::TrainingQueueFull);
        }
        let state = self
            .economy
            .players
            .get(&player)
            .ok_or(EconomyError::NotOwner)?;
        let reserved = self
            .buildings()
            .filter(|b| b.player == player)
            .map(|b| b.training_queue.len())
            .sum::<usize>();
        if usize::from(state.population_used) + reserved >= usize::from(state.population_cap) {
            return Err(EconomyError::PopulationCap);
        }
        if state.stockpile.food < VILLAGE_RULES.villager_food_cost {
            return Err(EconomyError::InsufficientFood);
        }
        // No fallible work follows these mutations.
        if let Some(state) = self.economy.players.get_mut(&player) {
            state.stockpile.food -= VILLAGE_RULES.villager_food_cost;
        }
        if let Some(building) = self.economy.buildings.get_mut(&center) {
            building.training_queue.push_back(TrainingEntry {
                kind,
                progress_ticks: 0,
                total_ticks: u64::from(self.config.tick_hz)
                    * u64::from(VILLAGE_RULES.villager_training_seconds),
            });
        }
        Ok(())
    }

    /// Cancel the active entry, refund its food, and release its reserved slot.
    /// Repeating cancellation on an empty queue cannot create resources.
    pub fn issue_cancel_training(
        &mut self,
        player: PlayerId,
        center: EntityId,
    ) -> Result<(), EconomyError> {
        let building = self
            .economy
            .buildings
            .get_mut(&center)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        if building.training_queue.pop_front().is_some()
            && let Some(state) = self.economy.players.get_mut(&player)
        {
            state.stockpile.food = state
                .stockpile
                .food
                .saturating_add(VILLAGE_RULES.villager_food_cost);
        }
        Ok(())
    }

    pub(super) fn advance_training(&mut self, changed: &mut Vec<crate::GameUnit>) {
        let centers = self.economy.buildings.keys().copied().collect::<Vec<_>>();
        for id in centers {
            let Some(building) = self.economy.buildings.get_mut(&id) else {
                continue;
            };
            let Some(entry) = building.training_queue.front_mut() else {
                continue;
            };
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(1)
                .min(entry.total_ticks);
            if entry.progress_ticks != entry.total_ticks {
                continue;
            }
            let footprint = building.footprint;
            let player = building.player;
            if self
                .player_economy(player)
                .is_none_or(|state| state.population_used >= state.population_cap)
            {
                continue;
            }
            let spawn = perimeter(footprint)
                .into_iter()
                .find(|tile| self.tile_free(*tile));
            let Some(tile) = spawn else {
                continue;
            };
            let Ok(position) = position(tile) else {
                continue;
            };
            if let Ok(unit) = self.spawn_villager(player, position) {
                if let Some(building) = self.economy.buildings.get_mut(&id) {
                    building.training_queue.pop_front();
                }
                if let Some(unit) = self.unit(unit) {
                    changed.push(unit);
                }
            }
        }
    }
}
