use super::VILLAGE_RULES;
use crate::{GameWorld, UnitKind};
use aoe_core::{FIXED_SUBUNITS_PER_TILE, TileCoord};
use aoe_map::{MovementOutcome, Path};
use std::collections::{BTreeMap, BTreeSet};

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

    /// Uniform compatibility worlds retain direct stepping until a building
    /// is present. Their short village routes use bounded canonical A*.
    pub(crate) fn uniform_building_route(
        &mut self,
        origin: TileCoord,
        target: TileCoord,
    ) -> Option<MovementOutcome> {
        if self.economy.blocked_tiles.is_empty() {
            return None;
        }
        if !self.tile_passable(target) {
            return Some(MovementOutcome::InvalidDestination);
        }
        let mut open = BTreeSet::from([(heuristic(origin, target), 0_u64, origin.y, origin.x)]);
        let mut records = BTreeMap::from([(origin, (0_u64, None))]);
        let budget = self.planning_budget.min(16_384);
        for _ in 0..budget {
            self.planning_budget -= 1;
            let Some((_, cost, y, x)) = open.pop_first() else {
                return Some(MovementOutcome::Unreachable);
            };
            let tile = TileCoord::new(x, y);
            if tile == target {
                let mut path = vec![tile];
                let mut cursor = tile;
                while let Some((_, Some(parent))) = records.get(&cursor) {
                    path.push(*parent);
                    cursor = *parent;
                }
                path.reverse();
                return Some(MovementOutcome::Path(Path { tiles: path, cost }));
            }
            if records.get(&tile).is_some_and(|entry| entry.0 < cost) {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let next = TileCoord::new(x + dx, y + dy);
                    if !self.tile_passable(next) || !self.building_step_clear(tile, next) {
                        continue;
                    }
                    let next_cost = cost + if dx == 0 || dy == 0 { 1024 } else { 1448 };
                    if records.get(&next).is_some_and(|entry| entry.0 <= next_cost) {
                        continue;
                    }
                    records.insert(next, (next_cost, Some(tile)));
                    open.insert((
                        next_cost + heuristic(next, target),
                        next_cost,
                        next.y,
                        next.x,
                    ));
                }
            }
        }
        Some(MovementOutcome::BudgetExceeded)
    }
}
fn heuristic(from: TileCoord, to: TileCoord) -> u64 {
    let x = u64::from(from.x.abs_diff(to.x));
    let y = u64::from(from.y.abs_diff(to.y));
    x.min(y) * 1448 + (x.max(y) - x.min(y)) * 1024
}
