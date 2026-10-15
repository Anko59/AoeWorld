use super::VILLAGE_RULES;
use crate::{GameWorld, UnitKind};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord};

impl GameWorld {
    pub(crate) fn movement_speed(&self, kind: UnitKind) -> (u64, u64) {
        if kind == UnitKind::Scout {
            (
                self.config.move_speed_subunits_per_tick as u64,
                self.config.move_speed_subunits_per_tick_denominator,
            )
        } else {
            (
                u64::from(VILLAGE_RULES.villager_speed_numerator) * FIXED_SUBUNITS_PER_TILE as u64,
                u64::from(VILLAGE_RULES.villager_speed_denominator)
                    * u64::from(self.config.tick_hz),
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
