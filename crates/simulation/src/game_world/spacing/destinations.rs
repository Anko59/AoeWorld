//! Radius reservations for move endpoints and persistent work positions.
use super::*;
use crate::game_world::movement_step::{CellHasher, tile_key};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::BuildHasherDefault;

#[derive(Debug, Default)]
pub(crate) struct Destinations {
    /// Probed for every nearby tile of every reservation query. Only
    /// membership questions are asked of it, so hash order decides nothing.
    by_tile: HashMap<u64, BTreeSet<EntityId>, BuildHasherDefault<CellHasher>>,
    by_unit: BTreeMap<EntityId, [Option<TileCoord>; 2]>,
}
impl Destinations {
    pub(crate) fn remove(&mut self, id: EntityId) {
        if let Some(tiles) = self.by_unit.remove(&id) {
            for tile in tiles.into_iter().flatten() {
                if let Some(ids) = self.by_tile.get_mut(&tile_key(tile)) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        self.by_tile.remove(&tile_key(tile));
                    }
                }
            }
        }
    }
    fn insert(&mut self, id: EntityId, tiles: [Option<TileCoord>; 2]) {
        if self.by_unit.get(&id) == Some(&tiles) {
            return;
        }
        self.remove(id);
        if tiles == [None, None] {
            return;
        }
        self.by_unit.insert(id, tiles);
        for tile in tiles.into_iter().flatten() {
            self.by_tile.entry(tile_key(tile)).or_default().insert(id);
        }
    }
}

impl GameWorld {
    fn reserved_positions(&self, id: EntityId) -> [Option<WorldPosition>; 2] {
        [
            self.movement_order(id).map(|o| o.destination),
            self.economy
                .gatherers
                .get(&id)
                .and_then(|t| t.work_position),
        ]
    }

    pub(crate) fn index_destination(&mut self, id: EntityId) {
        let tiles = self
            .reserved_positions(id)
            .map(|p| p.map(WorldPosition::tile_floor));
        self.destinations.insert(id, tiles);
        self.refresh_spacing_order(id);
    }

    pub(crate) fn destination_point_reserved(&self, at: WorldPosition, except: EntityId) -> bool {
        let Some(&index) = self.lookup.get(&except) else {
            return true;
        };
        let unit = &self.units[index].state;
        let reach = unit.kind.collision_radius() + 512;
        let min = WorldPosition::new(at.x - reach, at.y - reach).tile_floor();
        let max = WorldPosition::new(at.x + reach, at.y + reach).tile_floor();
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let Some(ids) = self
                    .destinations
                    .by_tile
                    .get(&tile_key(TileCoord::new(x, y)))
                else {
                    continue;
                };
                // An overfull arrival cell is unavailable, never sampled clear.
                if ids.len() > 32 {
                    return true;
                }
                for &id in ids {
                    if id == except {
                        continue;
                    }
                    let Some(other) = self.unit(id) else {
                        continue;
                    };
                    if unit.kind.is_naval() != other.kind.is_naval() {
                        continue;
                    }
                    let r = i64::from(unit.kind.collision_radius() + other.kind.collision_radius());
                    if self
                        .reserved_positions(id)
                        .into_iter()
                        .flatten()
                        .any(|p| super::super::movement_step::slots::squared(at, p) < r * r)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn spread_destination(
        &self,
        id: EntityId,
        click: WorldPosition,
    ) -> Result<WorldPosition, GameWorldError> {
        self.destination_slot(id, click, false)
    }

    pub(crate) fn free_arrival_destination(
        &self,
        id: EntityId,
        click: WorldPosition,
    ) -> Result<WorldPosition, GameWorldError> {
        self.destination_slot(id, click, true)
    }

    /// Half-tile slots allow several infantry to share a tile. A bounded flood
    /// certifies connectivity to the click, so slots cannot jump across a wall.
    fn destination_slot(
        &self,
        id: EntityId,
        click: WorldPosition,
        occupants: bool,
    ) -> Result<WorldPosition, GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let click = self.config.snap_ground_position(click);
        let target = click.tile_floor();
        if !self.unit_tile_passable(unit.kind, target, &|| false)?
            || self.economy.blocked_tiles.contains(&target)
        {
            return Err(GameWorldError::InvalidPosition);
        }
        let busy = |at| {
            self.destination_point_reserved(at, id)
                || (occupants
                    && (!self.arrival_point_free(self.lookup[&id], at) || at == unit.position))
        };
        if !busy(click) {
            return Ok(click);
        }
        let mut seen = BTreeSet::from([target]);
        let mut queue = std::collections::VecDeque::from([target]);
        let mut best = None;
        while let Some(tile) = queue.pop_front() {
            let center = WorldPosition::from_tile_center(tile)?;
            for (dx, dy) in [(0, 0), (-256, -256), (256, -256), (-256, 256), (256, 256)] {
                let at = WorldPosition::new(center.x + dx, center.y + dy);
                if !busy(at) && self.body_position_clear(unit.kind, at) {
                    let key = (
                        super::super::movement_step::slots::squared(at, click),
                        at.y,
                        at.x,
                    );
                    if best.is_none_or(|(old, _)| key < old) {
                        best = Some((key, at));
                    }
                }
            }
            // Once a near slot is found, no farther ring can improve it.
            if seen.len() >= 289 {
                continue;
            }
            for (dx, dy) in [(-1, 0), (0, -1), (1, 0), (0, 1)] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if next.x.abs_diff(target.x) > 8
                    || next.y.abs_diff(target.y) > 8
                    || seen.contains(&next)
                {
                    continue;
                }
                let Ok(next_position) = WorldPosition::from_tile_center(next) else {
                    continue;
                };
                if best.is_some_and(|(key, _)| {
                    super::super::movement_step::slots::squared(next_position, click)
                        .saturating_sub(1024 * 1024)
                        > key.0
                }) {
                    continue;
                }
                if self.config.valid_ground_position(next_position)
                    && self.building_step_clear(tile, next)
                    && self.unit_step_clear(unit.kind, tile, next)?
                {
                    seen.insert(next);
                    queue.push_back(next);
                }
            }
        }
        Ok(best.map_or(click, |(_, at)| at))
    }

    /// Older tile callers (construction/drop-off) ask about the centre only;
    /// movement and resource work always reserve the actual sub-tile point.
    pub(crate) fn work_spot_busy(&self, tile: TileCoord, except: EntityId) -> bool {
        let Ok(at) = WorldPosition::from_tile_center(tile) else {
            return true;
        };
        self.destination_point_reserved(at, except)
            || self
                .lookup
                .get(&except)
                .is_none_or(|&i| self.position_occupied(i, at, true))
    }

    pub(crate) fn arrival_position_free(&self, index: usize) -> bool {
        self.arrival_point_free(index, self.units[index].state.position)
    }
    pub(crate) fn arrival_point_free(&self, index: usize, at: WorldPosition) -> bool {
        !self.local_position_occupied(index, at)
    }
    pub(crate) fn arrival_tile_has_idle(&self, index: usize, at: WorldPosition) -> bool {
        self.position_occupied(index, at, true)
    }
}
