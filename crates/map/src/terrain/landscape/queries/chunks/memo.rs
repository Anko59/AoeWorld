//! Query-local lazy fixed halos. No generator mutation, global state, world
//! array, or speculative source read. Entries die with one point or chunk.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct Entry {
    base: Option<Tile>,
    candidate: Option<Option<ResourceNode>>,
}

/// One 192-tile cell and its wildness.
type CellWildness = ((i64, i64), u16);

pub(super) type PointMemo<'a> = Memo<'a, 25, 49>;
pub(super) type ChunkMemo<'a> = Memo<'a, 1296, 1444>;

/// Base tiles keep a two-tile halo. Water kinds keep three: the shore band
/// of a neighbor reads water two tiles beyond that neighbor.
const WATER_HALO: i64 = 1;

pub(super) struct Memo<'a, const N: usize, const W: usize> {
    generator: &'a MapChunkGenerator,
    origin: TileCoord,
    side: i64,
    cancelled: &'a dyn Fn() -> bool,
    entries: RefCell<[Entry; N]>,
    cells: RefCell<[Option<CellWildness>; 16]>,
    water: RefCell<[Option<crate::WaterKind>; W]>,
    shores: RefCell<[Option<Shore>; N]>,
    /// A point query asks the source whether any water is in reach before
    /// sampling its neighbors; a chunk shares those samples between tiles.
    probe_reach: bool,
    origins: RefCell<resources::Origins>,
}

impl<'a> PointMemo<'a> {
    pub(super) fn new(
        generator: &'a MapChunkGenerator,
        center: TileCoord,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            origin: TileCoord::new(center.x.saturating_sub(2), center.y.saturating_sub(2)),
            side: 5,
            cancelled,
            entries: RefCell::new([Entry::default(); 25]),
            cells: RefCell::new([None; 16]),
            water: RefCell::new([None; 49]),
            shores: RefCell::new([None; 25]),
            probe_reach: true,
            origins: RefCell::new(resources::Origins::new(generator)),
        }
    }
}

impl<'a> ChunkMemo<'a> {
    pub(super) fn new_chunk(
        generator: &'a MapChunkGenerator,
        origin: TileCoord,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            origin: TileCoord::new(origin.x.saturating_sub(2), origin.y.saturating_sub(2)),
            side: i64::from(CHUNK_TILES) + 4,
            cancelled,
            entries: RefCell::new([Entry::default(); 1296]),
            cells: RefCell::new([None; 16]),
            water: RefCell::new([None; 1444]),
            shores: RefCell::new([None; 1296]),
            probe_reach: false,
            origins: RefCell::new(resources::Origins::new(generator)),
        }
    }
}

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    fn index(&self, position: TileCoord) -> Option<usize> {
        let x = i64::from(position.x) - i64::from(self.origin.x);
        let y = i64::from(position.y) - i64::from(self.origin.y);
        ((0..self.side).contains(&x) && (0..self.side).contains(&y))
            .then_some((y * self.side + x) as usize)
    }
    /// Wildness of one 192-tile cell, direct-mapped: a point or chunk reads at
    /// most a 4x4 block of cells, so each is derived from the source once.
    pub(super) fn wildness(&self, cx: i64, cy: i64) -> Result<u16, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let slot = (cy.rem_euclid(4) * 4 + cx.rem_euclid(4)) as usize;
        if let Some((cell, value)) = self.cells.borrow()[slot]
            && cell == (cx, cy)
        {
            return Ok(value);
        }
        let value = self
            .generator
            .landscape_cell_wildness(cx, cy, self.cancelled)?;
        self.cells.borrow_mut()[slot] = Some(((cx, cy), value));
        Ok(value)
    }
    pub(super) fn base(&self, position: TileCoord) -> Result<Option<Tile>, EnvironmentPageError> {
        // Cancellation is observed even on a hit. Do not retain source errors.
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.index(position);
        if let Some(index) = index
            && let Some(base) = self.entries.borrow()[index].base
        {
            return Ok(Some(base));
        }
        let result = self.generator.landscape_base_at(position, self.cancelled)?;
        if let Some(index) = index
            && let Some(base) = result
        {
            self.entries.borrow_mut()[index].base = Some(base);
        }
        Ok(result)
    }
    fn candidate(&self, position: TileCoord) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.index(position);
        if let Some(index) = index
            && let Some(candidate) = self.entries.borrow()[index].candidate
        {
            return Ok(candidate);
        }
        let candidate = self
            .origins
            .borrow_mut()
            .candidate_with(position, || self.base(position))?;
        if let Some(index) = index {
            self.entries.borrow_mut()[index].candidate = Some(candidate);
        }
        Ok(candidate)
    }
    pub(super) fn reserved(&self, position: TileCoord) -> Result<bool, EnvironmentPageError> {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                if self.candidate(TileCoord::new(x, y))?.is_some() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    pub(super) fn resource(
        &self,
        position: TileCoord,
        base: Tile,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        let Some(node) = self.candidate(position)? else {
            return Ok(None);
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx == 0) == (dy == 0) {
                    continue;
                }
                let Some(x) = position.x.checked_add(dx) else {
                    continue;
                };
                let Some(y) = position.y.checked_add(dy) else {
                    continue;
                };
                let neighbor = TileCoord::new(x, y);
                if let Some(sample) = self.base(neighbor)?
                    && sample.passable
                    && sample.surface.walkable()
                    && (i32::from(base.game_height_level) - i32::from(sample.game_height_level))
                        .abs()
                        <= 1
                    && self.candidate(neighbor)?.is_none()
                {
                    return Ok(Some(node));
                }
            }
        }
        Ok(None)
    }
}

#[path = "memo/shore.rs"]
mod shore;
pub(super) use shore::Shore;

#[path = "memo/tests.rs"]
#[cfg(test)]
mod tests;
