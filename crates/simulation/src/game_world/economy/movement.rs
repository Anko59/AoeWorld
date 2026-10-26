use super::VILLAGE_RULES;
use crate::{GameWorld, UnitKind};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord};

impl GameWorld {
    /// Work approaches share continuous escape steps and local corner exits.
    pub(crate) fn has_work_task(&self, id: aoe_core::EntityId) -> bool {
        self.economy.gatherers.contains_key(&id) || self.economy.builders.contains_key(&id)
    }

    /// Reassign an obstructed work approach to a physically free legal slot.
    /// `Some(false)` proves this bounded job perimeter is currently full.
    pub(crate) fn redirect_gather_arrival(&mut self, index: usize) -> Option<bool> {
        let unit = self.units[index].state;
        let task = self.economy.gatherers.get(&unit.id)?;
        let footprint = match task.phase {
            super::GatherPhase::Approaching => {
                aoe_core::TileRect::new(task.tile, TileCoord::new(task.tile.x + 1, task.tile.y + 1))
            }
            super::GatherPhase::Returning => self.building(task.dropoff?)?.footprint,
            _ => return None,
        };
        let mut slots = super::placement::perimeter(footprint);
        slots.sort_by_key(|tile| {
            (
                super::placement::distance_squared(unit.position.tile_floor(), *tile),
                tile.y,
                tile.x,
            )
        });
        let destination = slots.iter().copied().find_map(|tile| {
            let at = aoe_core::WorldPosition::from_tile_center(tile).ok()?;
            (self.config.valid_ground_position(at)
                && !self.economy.blocked_tiles.contains(&tile)
                && !self.work_spot_busy(tile, unit.id)
                && self
                    .unit_tile_passable(unit.kind, tile, &|| false)
                    .unwrap_or(false)
                && self.arrival_point_free(index, at))
            .then_some(at)
        });
        let Some(destination) = destination else {
            // Moving traffic may clear a slot: only settled occupants prove
            // that the worksite has no capacity for another worker.
            let full = slots.into_iter().all(|tile| {
                let Ok(at) = aoe_core::WorldPosition::from_tile_center(tile) else {
                    return true;
                };
                !self.config.valid_ground_position(at)
                    || self.economy.blocked_tiles.contains(&tile)
                    || !self
                        .unit_tile_passable(unit.kind, tile, &|| false)
                        .unwrap_or(false)
                    || self.arrival_tile_has_idle(index, at)
            });
            return Some(!full);
        };
        if self.units[index]
            .order
            .is_none_or(|o| o.destination != destination)
        {
            let obstruction = self.units[index].obstruction;
            let queue = self.move_queues.remove(&unit.id);
            let _ = self.issue_task_move(unit.id, destination);
            self.units[index].obstruction = obstruction;
            if let Some(queue) = queue {
                self.move_queues.insert(unit.id, queue);
            }
        }
        Some(true)
    }

    /// A blocked approach may finish on any legal spot for this exact job.
    pub(crate) fn at_gather_arrival(&self, index: usize) -> bool {
        let unit = self.units[index].state;
        let tile = unit.position.tile_floor();
        self.economy
            .gatherers
            .get(&unit.id)
            .is_some_and(|task| match task.phase {
                super::GatherPhase::Approaching => {
                    tile.x.abs_diff(task.tile.x) <= 1 && tile.y.abs_diff(task.tile.y) <= 1
                }
                super::GatherPhase::Returning => task
                    .dropoff
                    .and_then(|id| self.building(id))
                    .is_some_and(|b| {
                        b.player == unit.player
                            && super::placement::perimeter(b.footprint).contains(&tile)
                    }),
                _ => false,
            })
    }

    pub(crate) fn movement_speed(&self, kind: UnitKind, player: aoe_core::PlayerId) -> (u64, u64) {
        self.movement_speed_with(kind, self.player_economy(player))
    }

    /// `movement_speed` for an already fetched owner economy.
    pub(crate) fn movement_speed_with(
        &self,
        kind: UnitKind,
        economy: Option<super::PlayerEconomy>,
    ) -> (u64, u64) {
        if kind == UnitKind::Scout && economy.is_none() {
            return (
                self.config.move_speed_subunits_per_tick as u64,
                self.config.move_speed_subunits_per_tick_denominator,
            );
        }
        if kind == UnitKind::FishingShip {
            return (
                126 * FIXED_SUBUNITS_PER_TILE as u64,
                100 * u64::from(self.config.tick_hz),
            );
        }
        let researched = economy.map_or(0, |p| p.researched);
        let stats = super::super::combat::military::researched_stats(
            kind,
            economy.is_some_and(|p| p.age >= crate::Age::Feudal),
            researched,
        );
        if let Some(hundredths) = stats.speed_hundredths {
            (
                u64::from(hundredths) * FIXED_SUBUNITS_PER_TILE as u64,
                100 * u64::from(self.config.tick_hz),
            )
        } else if kind == UnitKind::Scout {
            (
                self.config.move_speed_subunits_per_tick as u64,
                self.config.move_speed_subunits_per_tick_denominator,
            )
        } else {
            (
                u64::from(VILLAGE_RULES.villager_speed_numerator)
                    * FIXED_SUBUNITS_PER_TILE as u64
                    * u64::from(crate::game_world::research::villager_speed_percent(
                        researched,
                    )),
                u64::from(VILLAGE_RULES.villager_speed_denominator)
                    * u64::from(self.config.tick_hz)
                    * 100,
            )
        }
    }

    pub(crate) fn work_separation_clear(&self, id: aoe_core::EntityId, tile: TileCoord) -> bool {
        self.economy.gatherers.get(&id).is_none_or(|task| {
            !matches!(
                task.phase,
                super::GatherPhase::Gathering | super::GatherPhase::Approaching
            ) || (tile.x.abs_diff(task.tile.x) <= 1 && tile.y.abs_diff(task.tile.y) <= 1)
        })
    }

    pub(crate) fn building_step_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        if self.economy.blocked_tiles.is_empty() {
            return true;
        }
        !self.economy.blocked_tiles.contains(&to)
            && (from.x == to.x
                || from.y == to.y
                || (!self
                    .economy
                    .blocked_tiles
                    .contains(&TileCoord::new(from.x, to.y))
                    && !self
                        .economy
                        .blocked_tiles
                        .contains(&TileCoord::new(to.x, from.y))))
    }
}
