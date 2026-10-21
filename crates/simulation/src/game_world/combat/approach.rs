//! Bounded, canonical search for a firing tile in the attacker's component.
use super::{MELEE_RANGE_SUBUNITS, tick::Goal};
use crate::{GameWorld, GameWorldError};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
};

/// Bounds both search work and retained frontier entries, independent of map size.
const MAX_ATTACK_SEARCH_TILES: usize = 8_192;

impl GameWorld {
    /// Search outward from the attacker, prioritizing travel distance plus a
    /// lower bound on distance still needed to reach attack range. Reaching a
    /// tile proves connectivity; merely being passable near the target does not.
    pub(crate) fn attack_destination(
        &self,
        id: EntityId,
        goal: &Goal,
    ) -> Result<Option<WorldPosition>, GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let stats = self.unit_stats(id).ok_or(GameWorldError::UnknownEntity)?;
        let range = i64::from(stats.range) * MELEE_RANGE_SUBUNITS;
        let min = i64::from(stats.min_range) * MELEE_RANGE_SUBUNITS;
        let origin = unit.position.tile_floor();
        let estimate = |at: WorldPosition| {
            // Diagonal steps cost 1448 rather than sqrt(2)*1024. This small
            // discount keeps the Euclidean lower bound below that grid cost.
            (goal.distance_squared(at) as u64)
                .isqrt()
                .saturating_sub(range as u64)
                * 999
                / 1000
        };
        let mut reached = BTreeMap::from([(origin, 0_u64)]);
        let mut frontier = BinaryHeap::from([Reverse((
            estimate(unit.position),
            0_u64,
            origin.y,
            origin.x,
        ))]);
        let mut expanded = 0;
        while let Some(Reverse((_, cost, y, x))) = frontier.pop() {
            let tile = TileCoord::new(x, y);
            if reached.get(&tile) != Some(&cost) {
                continue;
            }
            let at = WorldPosition::from_tile_center(tile)
                .map_err(|_| GameWorldError::InvalidPosition)?;
            let distance = goal.distance_squared(at);
            if distance <= range * range && distance >= min * min {
                return Ok(Some(at));
            }
            expanded += 1;
            if expanded >= MAX_ATTACK_SEARCH_TILES {
                break;
            }
            for (dx, dy) in [
                (0, -1),
                (-1, 0),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(x + dx, y + dy);
                let next_cost = cost + if dx == 0 || dy == 0 { 1024 } else { 1448 };
                if reached.get(&next).is_some_and(|&old| old <= next_cost)
                    || self.economy.blocked_tiles.contains(&next)
                    || (dx != 0
                        && dy != 0
                        && [TileCoord::new(x + dx, y), TileCoord::new(x, y + dy)]
                            .iter()
                            .any(|t| self.economy.blocked_tiles.contains(t)))
                    || !self
                        .unit_step_clear(unit.kind, tile, next)
                        .map_err(GameWorldError::Environment)?
                {
                    continue;
                }
                // Count inserted entries as well as expansions, including stale
                // heap entries, so the retained memory has a fixed ceiling.
                if reached.len() >= MAX_ATTACK_SEARCH_TILES
                    || frontier.len() >= MAX_ATTACK_SEARCH_TILES
                {
                    return Ok(None);
                }
                let at = WorldPosition::from_tile_center(next)
                    .map_err(|_| GameWorldError::InvalidPosition)?;
                reached.insert(next, next_cost);
                frontier.push(Reverse((
                    next_cost + estimate(at),
                    next_cost,
                    next.y,
                    next.x,
                )));
            }
        }
        Ok(None)
    }
}
