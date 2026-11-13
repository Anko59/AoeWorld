//! Fine bodies and conservative coarse population bounds share one index.
//! Both live in flat sparse grids (`grid`), not hash maps: a cell read is two
//! array loads. Each cell's body order (insertion with swap removals) is the
//! order the hash map's cells had, so sampling windows are unchanged.
use crate::UnitKind;
use aoe_core::{EntityId, TileCoord, WorldPosition};
use grid::Grid;
use std::hash::Hasher;

mod grid;
mod neighborhood;
pub(crate) use neighborhood::Neighborhood;

const CELL_LIMIT: usize = 32;
const NO_CELL: u32 = u32::MAX;

/// The non-empty 1-tile cells of one layer: a grid of slots into an arena of
/// body lists. A freed list keeps its capacity for the next cell.
#[derive(Debug)]
struct Cells {
    grid: Grid,
    bodies: Vec<Vec<CellBody>>,
    free: Vec<u32>,
}

impl Default for Cells {
    fn default() -> Self {
        Self {
            grid: Grid::new(NO_CELL),
            bodies: Vec::new(),
            free: Vec::new(),
        }
    }
}

impl Cells {
    #[inline]
    fn get(&self, tile: TileCoord) -> Option<&Vec<CellBody>> {
        let slot = self.grid.get(tile.x, tile.y);
        (slot != NO_CELL).then(|| &self.bodies[slot as usize])
    }

    #[inline]
    fn get_mut(&mut self, tile: TileCoord) -> Option<&mut Vec<CellBody>> {
        let slot = self.grid.get(tile.x, tile.y);
        (slot != NO_CELL).then(|| &mut self.bodies[slot as usize])
    }

    fn entry(&mut self, tile: TileCoord) -> &mut Vec<CellBody> {
        let mut slot = self.grid.get(tile.x, tile.y);
        if slot == NO_CELL {
            slot = self.free.pop().unwrap_or_else(|| {
                self.bodies.push(Vec::new());
                (self.bodies.len() - 1) as u32
            });
            self.grid.set(tile.x, tile.y, slot);
        }
        &mut self.bodies[slot as usize]
    }

    /// Frees an emptied cell.
    fn release(&mut self, tile: TileCoord) {
        let slot = self.grid.get(tile.x, tile.y);
        if slot != NO_CELL {
            self.grid.set(tile.x, tile.y, NO_CELL);
            self.free.push(slot);
        }
    }

    #[cfg(test)]
    fn entries(&self) -> impl Iterator<Item = (TileCoord, &Vec<CellBody>)> + '_ {
        self.grid
            .entries()
            .map(|(x, y, slot)| (TileCoord::new(x, y), &self.bodies[slot as usize]))
    }
}

/// A body's centre beside its id: most bodies of a queried cell are outside
/// the query's box, and rejecting them must not touch the large unit records.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CellBody {
    id: EntityId,
    at: WorldPosition,
}

/// Centres a query can answer for, inclusive: `(min x, max x, min y, max y)`.
pub(crate) type BodyBox = (i32, i32, i32, i32);

pub(super) struct BodyQuery {
    pub(super) at: WorldPosition,
    pub(super) reach: i32,
    pub(super) window: BodyBox,
    pub(super) naval: bool,
    pub(super) salt: u64,
}

// Coordinates are internal integer keys. Mix the packed pair once instead of
// running the general-purpose keyed byte hasher on every neighbour probe.
#[derive(Debug, Default)]
pub(crate) struct CellHasher(u64);
impl Hasher for CellHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(23);
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3);
        }
    }
}

pub(crate) fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}

#[derive(Debug, Default)]
pub(crate) struct CollisionIndex {
    cells: [Cells; 2],
    slots: Vec<usize>,
    indices: Vec<usize>,
    overfull_cells: usize,
    maximum_radii: [i32; 2],
    /// Land bodies per 2×2-tile cell.
    land_counts: LandCounts,
}

#[derive(Debug)]
struct LandCounts(Grid);

impl Default for LandCounts {
    fn default() -> Self {
        Self(Grid::new(0))
    }
}

impl CollisionIndex {
    /// Kinds are fixed at spawn. Retain the largest observed radius after
    /// removal: an upper bound avoids a population scan on every group tick.
    pub(crate) fn observe_kind(&mut self, kind: UnitKind) {
        let radius = &mut self.maximum_radii[usize::from(kind.is_naval())];
        *radius = (*radius).max(kind.collision_radius());
    }

    pub(crate) fn maximum_radii(&self) -> [i32; 2] {
        self.maximum_radii
    }

    pub(crate) fn unit_index(&self, id: EntityId) -> usize {
        self.indices[id.0 as usize]
    }

    pub(crate) fn reindex(&mut self, id: EntityId, index: usize) {
        if let Some(slot) = self.indices.get_mut(id.0 as usize) {
            *slot = index;
        }
    }

    pub(crate) fn sampling_slot(&self, id: EntityId) -> u64 {
        self.slots
            .get(id.0 as usize)
            .filter(|&&n| n != usize::MAX)
            .map_or(u64::MAX, |&n| n as u64)
    }

    pub(crate) fn insert(
        &mut self,
        id: EntityId,
        position: WorldPosition,
        naval: bool,
        index: usize,
    ) {
        let tile = position.tile_floor();
        let at = id.0 as usize;
        if self.slots.get(at).is_some_and(|&slot| slot != usize::MAX) {
            return;
        }
        // Bodies are on the map: the grids hold no negative tiles.
        debug_assert!(tile.x >= 0 && tile.y >= 0, "{tile:?}");
        if tile.x < 0 || tile.y < 0 {
            return;
        }
        if at >= self.slots.len() {
            self.slots.resize(at + 1, usize::MAX);
            self.indices.resize(at + 1, usize::MAX);
        }
        self.indices[at] = index;
        let cell = self.cells[usize::from(naval)].entry(tile);
        self.slots[at] = cell.len();
        cell.push(CellBody { id, at: position });
        self.overfull_cells += usize::from(cell.len() == CELL_LIMIT + 1);
        if !naval {
            let (x, y) = coarse(tile);
            let count = self.land_counts.0.get(x, y);
            self.land_counts.0.set(x, y, count + 1);
        }
    }

    pub(crate) fn remove(&mut self, id: EntityId, tile: TileCoord, naval: bool) {
        let Some(&slot) = self
            .slots
            .get(id.0 as usize)
            .filter(|&&slot| slot != usize::MAX)
        else {
            return;
        };
        let cells = &mut self.cells[usize::from(naval)];
        let Some(cell) = cells.get_mut(tile) else {
            return;
        };
        if cell.get(slot).is_none_or(|body| body.id != id) {
            return;
        }
        if !naval {
            let (x, y) = coarse(tile);
            let count = self.land_counts.0.get(x, y);
            self.land_counts.0.set(x, y, count.saturating_sub(1));
        }
        self.overfull_cells -= usize::from(cell.len() == CELL_LIMIT + 1);
        cell.swap_remove(slot);
        self.slots[id.0 as usize] = usize::MAX;
        if let Some(displaced) = cell.get(slot) {
            self.slots[displaced.id.0 as usize] = slot;
        }
        if cell.is_empty() {
            cells.release(tile);
        }
    }

    /// Follows every step of a body, including those inside one tile.
    pub(crate) fn relocate(
        &mut self,
        id: EntityId,
        from: TileCoord,
        position: WorldPosition,
        naval: bool,
    ) {
        let to = position.tile_floor();
        if from != to {
            let index = self.unit_index(id);
            self.remove(id, from, naval);
            self.insert(id, position, naval, index);
        } else if let Some(&slot) = self.slots.get(id.0 as usize)
            && let Some(cell) = self.cells[usize::from(naval)].get_mut(to)
            && let Some(body) = cell.get_mut(slot).filter(|body| body.id == id)
        {
            body.at = position;
        }
    }

    /// The indexed centre of a body, wherever its cell.
    #[cfg(test)]
    pub(crate) fn body_at(
        &self,
        id: EntityId,
        tile: TileCoord,
        naval: bool,
    ) -> Option<WorldPosition> {
        let slot = *self.slots.get(id.0 as usize)?;
        let body = self.cells[usize::from(naval)].get(tile)?.get(slot)?;
        (body.id == id).then_some(body.at)
    }

    /// Takes the centres tests wrote straight into unit records between ticks.
    #[cfg(test)]
    pub(crate) fn resync_bodies(&mut self, centre: impl Fn(usize) -> WorldPosition) {
        let indices = &self.indices;
        for body in self
            .cells
            .iter_mut()
            .flat_map(|c| c.bodies.iter_mut())
            .flatten()
        {
            body.at = centre(indices[body.id.0 as usize]);
        }
    }

    /// A test tick must leave every indexed centre equal to its unit's. A test
    /// that wrote a unit into another tile left it in its old cell: not checked.
    #[cfg(test)]
    pub(crate) fn assert_bodies(&self, centre: impl Fn(usize) -> WorldPosition) {
        for (tile, cell) in self.cells.iter().flat_map(Cells::entries) {
            for body in cell {
                let at = centre(self.indices[body.id.0 as usize]);
                if at.tile_floor() == tile {
                    assert_eq!(body.at, at, "{:?}", body.id);
                }
            }
        }
    }

    pub(crate) fn has_overfull_cells(&self) -> bool {
        self.overfull_cells != 0
    }

    /// Uncapped observer census, including every body in overfull cells.
    /// This is never used for movement decisions.
    pub(crate) fn census_any(
        &self,
        min: TileCoord,
        max: TileCoord,
        naval: bool,
        mut test: impl FnMut(EntityId, WorldPosition) -> bool,
    ) -> bool {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(cell) = self.cells[usize::from(naval)].get(TileCoord::new(x, y))
                    && cell.iter().any(|body| test(body.id, body.at))
                {
                    return true;
                }
            }
        }
        false
    }

    /// Cell-level absence proof, independent of cyclic sampling. A singleton
    /// actor cell is empty for its own query; every other body vetoes it.
    pub(crate) fn cells_empty_except(
        &self,
        min: TileCoord,
        max: TileCoord,
        naval: bool,
        actor: EntityId,
    ) -> bool {
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                if let Some(cell) = self.cells[usize::from(naval)].get(TileCoord::new(x, y))
                    && (cell.len() != 1 || cell[0].id != actor)
                {
                    return false;
                }
            }
        }
        true
    }

    /// Coarse counts conservatively reject sparse contact windows in at most
    /// nine cell reads. Dead/enemy bodies overestimate demand, never hide it.
    pub(crate) fn contact_may_be_crowded(
        &self,
        at: WorldPosition,
        capacity: usize,
        work: &crate::game_world::route_scheduler::GroupWork,
    ) -> bool {
        let min =
            WorldPosition::new(at.x.saturating_sub(1536), at.y.saturating_sub(1536)).tile_floor();
        let max =
            WorldPosition::new(at.x.saturating_add(1536), at.y.saturating_add(1536)).tile_floor();
        let needed = capacity.saturating_sub(1).min(CELL_LIMIT + 1);
        let mut count = 0;
        for y in min.y.div_euclid(2)..=max.y.div_euclid(2) {
            for x in min.x.div_euclid(2)..=max.x.div_euclid(2) {
                work.collision(1);
                count += self.land_counts.0.get(x, y) as usize;
                if count >= needed {
                    return true;
                }
            }
        }
        false
    }

    /// At most sixteen cells, with 32 body tests per cell. Hash iteration never
    /// determines a decision: cells and each cell's cyclic window are canonical.
    pub(crate) fn blocked(
        &self,
        query: (WorldPosition, i32),
        naval: bool,
        salt: u64,
        work: &crate::game_world::route_scheduler::GroupWork,
        test: impl FnMut(EntityId) -> bool,
        overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        let everywhere = (i32::MIN, i32::MAX, i32::MIN, i32::MAX);
        self.blocked_in(query, everywhere, naval, salt, work, test, overflow)
    }

    /// `blocked` for a query that rejects every centre outside `window`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn blocked_in(
        &self,
        query: (WorldPosition, i32),
        window: BodyBox,
        naval: bool,
        salt: u64,
        work: &crate::game_world::route_scheduler::GroupWork,
        test: impl FnMut(EntityId) -> bool,
        overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        neighborhood::blocked_cells(query, window, work, test, overflow, |tile| {
            self.sample(tile, naval, salt)
        })
    }

    pub(crate) fn neighborhood(
        &self,
        at: WorldPosition,
        naval: bool,
        salt: u64,
    ) -> Neighborhood<'_> {
        Neighborhood::new(self, at.tile_floor(), naval, salt)
    }

    pub(super) fn blocked_in_with(
        &self,
        query: BodyQuery,
        neighbors: Option<&Neighborhood<'_>>,
        work: &crate::game_world::route_scheduler::GroupWork,
        test: impl FnMut(EntityId) -> bool,
        overflow: impl FnMut(TileCoord) -> bool,
    ) -> bool {
        let BodyQuery {
            at,
            reach,
            window,
            naval,
            salt,
        } = query;
        match neighbors {
            Some(neighbors) => neighbors.blocked_in((at, reach), window, work, test, overflow),
            None => self.blocked_in((at, reach), window, naval, salt, work, test, overflow),
        }
    }

    #[inline(always)]
    fn sample(&self, tile: TileCoord, naval: bool, salt: u64) -> Option<neighborhood::Sample<'_>> {
        self.cells[usize::from(naval)].get(tile).map(|cell| {
            neighborhood::Sample::new(
                cell,
                salt,
                cell.len().min(CELL_LIMIT),
                cell.len() > CELL_LIMIT,
            )
        })
    }
}

fn coarse(tile: TileCoord) -> (i32, i32) {
    (tile.x.div_euclid(2), tile.y.div_euclid(2))
}

#[cfg(test)]
mod tests;
