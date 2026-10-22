use super::VILLAGE_RULES;
use crate::{GameWorld, UnitKind};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord};

impl GameWorld {
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
            economy.is_some_and(|p| p.age == crate::Age::Feudal),
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
