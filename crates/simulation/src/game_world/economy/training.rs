use super::{
    EconomyError, TrainingEntry, UnitKind, VILLAGE_RULES,
    placement::{distance_squared, perimeter, position},
};
use crate::GameWorld;
use aoe_core::{ChunkCoord, EntityId, PlayerId, SPATIAL_CHUNK_TILES};

pub(crate) mod rally;
pub(crate) mod timers;

impl GameWorld {
    pub fn issue_train(
        &mut self,
        player: PlayerId,
        center: EntityId,
        kind: UnitKind,
    ) -> Result<(), EconomyError> {
        let researched = self.player_economy(player).map_or(0, |p| p.researched);
        let kind = kind.upgraded(researched);
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
        if kind == UnitKind::Steward && self.steward_reserved(building, None) {
            return Err(EconomyError::StewardExists);
        }
        self.require_age(player, kind.required_age())?;
        if kind
            .required_tech()
            .is_some_and(|tech| !self.has_researched(player, tech))
        {
            return Err(EconomyError::TechRequirementMissing);
        }
        if kind == UnitKind::WarGalley && !self.has_researched(player, crate::Tech::WarGalley) {
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
        self.wake_queues(center);
        self.mark_building_changed(center);
        if let Some(building) = self.economy.buildings.get_mut(&center) {
            building.training_queue.push_back(TrainingEntry {
                kind,
                progress_ticks: 0,
                total_ticks: u64::from(self.config.tick_hz) * u64::from(seconds),
            });
            self.economy.active_training.insert(center);
            self.economy.queues.training.awake.insert(center);
            self.economy.queues.register_training(center, player);
        }
        Ok(())
    }

    /// Cancel the active entry and refund its prepaid cost.
    /// Repeating cancellation on an empty queue cannot create resources.
    pub fn issue_cancel_training(
        &mut self,
        player: PlayerId,
        center: EntityId,
    ) -> Result<(), EconomyError> {
        let building = self
            .building_mut(center)
            .ok_or(EconomyError::UnknownEntity)?;
        if building.player != player {
            return Err(EconomyError::NotOwner);
        }
        let Some(entry) = building.training_queue.pop_front() else {
            return Ok(());
        };
        if building.training_queue.is_empty() {
            self.economy.active_training.remove(&center);
            self.economy.queues.training.awake.remove(&center);
            self.economy.queues.unregister_training(center, player);
        }
        self.cancel_general_recruit(center);
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
        let centers = self.begin_queues(true);
        #[cfg(test)]
        {
            self.economy.tick_work.training_buildings = centers.len();
            self.economy.tick_work.spawn_units = 0;
        }
        for id in centers {
            self.economy.queues.cursor = Some(id);
            self.mark_building_changed(id);
            let Some(building) = self.economy.buildings.get_mut(&id) else {
                continue;
            };
            // Housing blocks progress, while the prepaid queue remains intact.
            if self
                .economy
                .players
                .get(&building.player)
                .is_none_or(|state| state.population_used >= state.population_cap)
            {
                self.sleep_training(id, false);
                continue;
            }
            let Some(entry) = building.training_queue.front_mut() else {
                continue;
            };
            let kind = entry.kind;
            entry.progress_ticks = entry
                .progress_ticks
                .saturating_add(1)
                .min(entry.total_ticks);
            if entry.progress_ticks != entry.total_ticks {
                self.sleep_training(id, false);
                continue;
            }
            let rally = building.rally_point;
            let footprint = building.footprint;
            let player = building.player;
            // The isometric screen's bottom tip is max/max (screen y grows
            // with world x+y). A rally point takes priority over that exit.
            let front = footprint.max;
            let toward = rally.map_or(front, |rally| rally.position.tile_floor());
            let mut exits = perimeter(footprint);
            exits.sort_by_key(|tile| {
                (
                    distance_squared(*tile, toward),
                    distance_squared(*tile, front),
                    tile.y,
                    tile.x,
                )
            });
            #[cfg(test)]
            let mut inspected = 0;
            let mut environment_pending = false;
            let spawn = exits.into_iter().find(|tile| {
                self.unit_tile_passable(kind, *tile, &|| false)
                    .unwrap_or_else(|_| {
                        environment_pending = true;
                        false
                    })
                    && !self.economy.blocked_tiles.contains(tile)
                    && !self
                        .chunks
                        .get(&ChunkCoord {
                            x: tile.x.div_euclid(SPATIAL_CHUNK_TILES),
                            y: tile.y.div_euclid(SPATIAL_CHUNK_TILES),
                        })
                        .into_iter()
                        .flatten()
                        .any(|id| {
                            #[cfg(test)]
                            {
                                inspected += 1;
                            }
                            !self.embarked(*id)
                                && self
                                    .unit(*id)
                                    .is_some_and(|u| u.position.tile_floor() == *tile)
                        })
            });
            #[cfg(test)]
            {
                self.economy.tick_work.spawn_units += inspected;
            }
            let Some(tile) = spawn else {
                if !environment_pending {
                    self.sleep_training(id, true);
                }
                continue;
            };
            let Ok(position) = position(tile) else {
                continue;
            };
            let kind = kind.upgraded(self.player_economy(player).map_or(0, |p| p.researched));
            let spawned = self.spawn_kind(player, kind, position);
            if let Ok(unit) = spawned {
                if kind == UnitKind::General {
                    self.bind_general_castle(unit, id);
                }
                if let Some(building) = self.building_mut(id) {
                    building.training_queue.pop_front();
                    if building.training_queue.is_empty() {
                        self.economy.active_training.remove(&id);
                        self.economy.queues.training.awake.remove(&id);
                        self.economy.queues.unregister_training(id, player);
                    }
                }
                self.finish_general_recruit(id, unit);
                if let Some(rally) = rally {
                    self.apply_rally(unit, rally);
                }
                changed.push(unit);
                self.sleep_training(id, false);
            }
        }
        self.economy.queues.cursor = None;
        self.economy.queues.training.tick = self.tick.0;
    }
}
