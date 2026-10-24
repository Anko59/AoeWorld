use super::{
    EconomyError, TrainingEntry, UnitKind, VILLAGE_RULES,
    placement::{perimeter, position},
};
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId};

pub(crate) mod rally;

impl GameWorld {
    pub fn issue_train(
        &mut self,
        player: PlayerId,
        center: EntityId,
        kind: UnitKind,
    ) -> Result<(), EconomyError> {
        // The Militia line has one current trainable identity after its upgrade.
        let kind =
            if kind == UnitKind::Militia && self.has_researched(player, crate::Tech::ManAtArms) {
                UnitKind::ManAtArms
            } else {
                kind
            };
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
        if matches!(
            kind,
            UnitKind::Archer
                | UnitKind::Skirmisher
                | UnitKind::Spearman
                | UnitKind::Scout
                | UnitKind::ManAtArms
        ) && !self.is_feudal(player)
        {
            return Err(EconomyError::RequiresFeudalAge);
        }
        if kind == UnitKind::ManAtArms && !self.has_researched(player, crate::Tech::ManAtArms) {
            return Err(EconomyError::TechRequirementMissing);
        }
        if building.research.is_some()
            || building.training_queue.len() >= VILLAGE_RULES.training_queue_limit
        {
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
        let seconds = if kind == UnitKind::Villager {
            if state.stockpile.food < VILLAGE_RULES.villager_food_cost {
                return Err(EconomyError::InsufficientFood);
            }
            if let Some(state) = self.economy.players.get_mut(&player) {
                state.stockpile.food -= VILLAGE_RULES.villager_food_cost;
            }
            VILLAGE_RULES.villager_training_seconds
        } else {
            // Combat units pay the combat table's cost, all or nothing.
            let training = Self::unit_training(kind).ok_or(EconomyError::WrongUnitKind)?;
            self.pay_unit_cost(player, kind)?;
            training.seconds
        };
        // No fallible work follows these mutations.
        if let Some(building) = self.economy.buildings.get_mut(&center) {
            building.training_queue.push_back(TrainingEntry {
                kind,
                progress_ticks: 0,
                total_ticks: u64::from(self.config.tick_hz) * u64::from(seconds),
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
        let Some(entry) = building.training_queue.pop_front() else {
            return Ok(());
        };
        if entry.kind == UnitKind::Villager {
            if let Some(state) = self.economy.players.get_mut(&player) {
                state.stockpile.food = state
                    .stockpile
                    .food
                    .saturating_add(VILLAGE_RULES.villager_food_cost);
            }
        } else {
            self.refund_unit_cost(player, entry.kind);
        }
        Ok(())
    }

    pub(super) fn advance_training(&mut self, changed: &mut Vec<EntityId>) {
        let centers = self
            .economy
            .buildings
            .iter()
            .filter(|(_, building)| !building.training_queue.is_empty())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in centers {
            let Some(building) = self.economy.buildings.get_mut(&id) else {
                continue;
            };
            let Some(entry) = building.training_queue.front_mut() else {
                continue;
            };
            let kind = entry.kind;
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(1)
                .min(entry.total_ticks);
            if entry.progress_ticks != entry.total_ticks {
                continue;
            }
            let rally = building.rally_point;
            let footprint = building.footprint;
            let player = building.player;
            if self
                .player_economy(player)
                .is_none_or(|state| state.population_used >= state.population_cap)
            {
                continue;
            }
            let spawn = perimeter(footprint).into_iter().find(|tile| {
                self.unit_tile_passable(kind, *tile, &|| false)
                    .unwrap_or(false)
                    && !self.economy.blocked_tiles.contains(tile)
                    && !self.units().any(|u| u.position.tile_floor() == *tile)
            });
            let Some(tile) = spawn else {
                continue;
            };
            let Ok(position) = position(tile) else {
                continue;
            };
            let spawned = match kind {
                UnitKind::Militia => self.spawn_militia(player, position),
                _ => self.spawn_kind(player, kind, position),
            };
            if let Ok(unit) = spawned {
                if let Some(building) = self.economy.buildings.get_mut(&id) {
                    building.training_queue.pop_front();
                }
                if let Some(rally) = rally {
                    self.apply_rally(unit, rally);
                }
                changed.push(unit);
            }
        }
    }
}
