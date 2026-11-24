//! Query-local lazy fixed halos. No generator mutation, global state, world
//! array, or speculative source read. Chunk and one-off point entries die
//! with their query; a thread's shared point tables are tagged by tile, so
//! they serve later default-policy points with the same pure per-tile values.
use super::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Default)]
struct Entry {
    /// The tile a tagged (shared) table slot currently holds.
    at: Option<TileCoord>,
    base: Option<Tile>,
    candidate: Option<Option<ResourceNode>>,
    dressing_roll: Option<u64>,
    landscape: Option<LandscapeSample>,
    input: Option<crate::landscape_patches::Input>,
    /// Unfiltered density, keyed by the explicit wildness it was read with.
    density: Option<(Option<u16>, crate::landscape_patches::DensitySample)>,
    reserved: Option<bool>,
    reservations: Option<Reservations>,
    shore: Option<Shore>,
}

/// One 192-tile cell and its wildness.
type CliffCell = ((i32, i32), Option<crate::terrain::relief::CliffPiece>);
/// A rock anchor can sit one tile outside a chunk and probe two more tiles
/// for scree. The resulting 38 tiles can intersect 14 three-tile cliff cells.
const CLIFF_CACHE_SIDE: i32 = 14;
const CLIFF_CACHE_SIZE: usize = (CLIFF_CACHE_SIDE * CLIFF_CACHE_SIDE) as usize;

type CellWildness = ((i64, i64), u16);

/// A water kind, with its dense provenance when no provider produced it.
type MemoWater = (crate::WaterKind, Option<crate::terrain::Provenance>);
/// A water slot's tile and kind.
type TileWater = (TileCoord, MemoWater);
/// Patch membership bits of one area: its origin and side.
type Members = (TileCoord, i32, Vec<u64>);

pub(super) type PointMemo<'a> = Memo<'a, 25, 361>;
pub(super) type ChunkMemo<'a> = Memo<'a, 1296, 2500>;
/// Tagged tables, one slot per tile of a 64-tile window: every read of one
/// point evaluation lies within ten tiles of it, so no two collide.
pub(super) type SharedMemo<'a> = Memo<'a, SHARED_SLOTS, SHARED_SLOTS>;
const SHARED_SIDE: i32 = 64;
pub(super) const SHARED_SLOTS: usize = (SHARED_SIDE * SHARED_SIDE) as usize;
/// Shared tables keep at most this many resource patch origins.
const SHARED_ORIGINS: usize = 1024;

/// Base tiles keep a two-tile halo. Water kinds keep seven more: the depth
/// scan of a water tile reads water up to seven tiles away.
const WATER_HALO: i64 = shore::DEPTH_REACH as i64;

/// The per-tile tables of a memo, owned by one query or lent by a thread.
pub(super) struct Tables<const N: usize, const W: usize> {
    entries: RefCell<Box<[Entry]>>,
    cells: RefCell<[Option<CellWildness>; 16]>,
    cliffs: RefCell<[Option<CliffCell>; CLIFF_CACHE_SIZE]>,
    /// Water kinds by tile; the dense provenance spares a base tile a second sample.
    water: RefCell<Box<[Option<TileWater>]>>,
    origins: RefCell<resources::Origins>,
    /// Patch membership of one area (its origin and side), built on demand.
    members: RefCell<Option<Members>>,
    /// Ground variety lattice corners, shared along the chunk's rows.
    pub(super) corners: RefCell<variety::Corners>,
}

impl<const N: usize, const W: usize> Tables<N, W> {
    pub(super) fn new(generator: &MapChunkGenerator) -> Self {
        Self {
            entries: RefCell::new(vec![Entry::default(); N].into_boxed_slice()),
            cells: RefCell::new([None; 16]),
            cliffs: RefCell::new([None; CLIFF_CACHE_SIZE]),
            water: RefCell::new(vec![None; W].into_boxed_slice()),
            origins: RefCell::new(resources::Origins::new(generator)),
            members: RefCell::new(None),
            corners: RefCell::new(variety::Corners::default()),
        }
    }
}

enum Hold<'a, T> {
    Own(T),
    Lent(&'a T),
}

pub(super) struct Memo<'a, const N: usize, const W: usize> {
    generator: &'a MapChunkGenerator,
    origin: TileCoord,
    side: i64,
    cancelled: &'a dyn Fn() -> bool,
    /// A point query asks the source whether any water is in reach before
    /// sampling its neighbors; a chunk shares those samples between tiles.
    probe_reach: bool,
    /// Slots are addressed by tile modulo the shared window and tagged.
    tagged: bool,
    tables: Hold<'a, Tables<N, W>>,
}

impl<const N: usize, const W: usize> std::ops::Deref for Memo<'_, N, W> {
    type Target = Tables<N, W>;
    fn deref(&self) -> &Tables<N, W> {
        match &self.tables {
            Hold::Own(tables) => tables,
            Hold::Lent(tables) => tables,
        }
    }
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
            probe_reach: true,
            tagged: false,
            tables: Hold::Own(Tables::new(generator)),
        }
    }
}

impl<'a> SharedMemo<'a> {
    /// A point memo over a thread's tables for this generator's default policy.
    pub(super) fn shared(
        generator: &'a MapChunkGenerator,
        cancelled: &'a dyn Fn() -> bool,
        tables: &'a Tables<SHARED_SLOTS, SHARED_SLOTS>,
    ) -> Self {
        {
            let mut origins = tables.origins.borrow_mut();
            if origins.len() > SHARED_ORIGINS {
                *origins = resources::Origins::new(generator);
            }
        }
        Self {
            generator,
            origin: TileCoord::new(0, 0),
            side: i64::from(SHARED_SIDE),
            cancelled,
            probe_reach: true,
            tagged: true,
            tables: Hold::Lent(tables),
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
            probe_reach: false,
            tagged: false,
            tables: Hold::Own(Tables::new(generator)),
        }
    }
}

impl<const N: usize, const W: usize> Memo<'_, N, W> {
    fn index(&self, position: TileCoord) -> Option<usize> {
        if self.tagged {
            let index = tagged_slot(position);
            let mut entries = self.entries.borrow_mut();
            if entries[index].at != Some(position) {
                entries[index] = Entry {
                    at: Some(position),
                    ..Entry::default()
                };
            }
            return Some(index);
        }
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
    pub(super) fn landscape(
        &self,
        position: TileCoord,
        evaluate: impl FnOnce() -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError>,
    ) -> Result<Option<(LandscapeSample, Tile)>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let index = self.index(position);
        if let Some(index) = index {
            let entry = &self.entries.borrow()[index];
            if let (Some(sample), Some(base)) = (entry.landscape, entry.base) {
                return Ok(Some((sample, base)));
            }
        }
        let result = evaluate()?;
        if let (Some(index), Some((sample, _))) = (index, result) {
            self.put(index, position, |entry| entry.landscape = Some(sample));
        }
        Ok(result)
    }
    pub(super) fn dressing_roll(&self, position: TileCoord) -> u64 {
        let index = self.index(position);
        if let Some(index) = index
            && let Some(roll) = self.entries.borrow()[index].dressing_roll
        {
            return roll;
        }
        let roll = unsigned_noise(
            self.generator.geography_key,
            b"landscape-dressing-v9",
            position.x,
            position.y,
        ) ^ self.generator.procedural_seed.rotate_left(29);
        if let Some(index) = index {
            self.put(index, position, |entry| entry.dressing_roll = Some(roll));
        }
        roll
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
        let in_bounds = self.generator.landscape_in_bounds(position);
        let result =
            if self.generator.provider.is_some() && in_bounds {
                Some(provider::sample_base_tile_with_cliff(
                    self.generator,
                    position,
                    self.cancelled,
                    &|tile| self.cliff(tile),
                )?)
            } else if in_bounds {
                let water = self.dense_water(position);
                // The memo's cliff cell, as the provider path uses; a cancelled
                // read falls back to the uncancellable sample the base takes.
                Some(
                    self.generator
                        .sample_base_tile_with(position, water, &|tile| match self.cliff(tile) {
                            Ok(piece) => piece,
                            Err(_) => self
                                .generator
                                .cliff_piece_with_cancel(tile, &|| false)
                                .ok()
                                .flatten(),
                        }),
                )
            } else {
                self.generator.landscape_base_at(position, self.cancelled)?
            };
        if let Some(index) = index
            && let Some(base) = result
        {
            self.put(index, position, |entry| entry.base = Some(base));
        }
        Ok(result)
    }
    /// Direct mapping is collision-free in the composition and scree halo.
    /// Cache dry and non-cliff decisions too, but never errors or cancellation.
    pub(super) fn cliff(
        &self,
        position: TileCoord,
    ) -> Result<Option<crate::terrain::relief::CliffPiece>, EnvironmentPageError> {
        if (self.cancelled)() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let side = crate::terrain::relief::CLIFF_CELL_TILES;
        let cell = (position.x.div_euclid(side), position.y.div_euclid(side));
        let slot = (cell.1.rem_euclid(CLIFF_CACHE_SIDE) * CLIFF_CACHE_SIDE
            + cell.0.rem_euclid(CLIFF_CACHE_SIDE)) as usize;
        let cached = self.cliffs.borrow()[slot];
        let piece = if let Some((key, piece)) = cached
            && key == cell
        {
            piece
        } else {
            let piece = self
                .generator
                .cliff_piece_with_cancel(position, self.cancelled)?;
            self.cliffs.borrow_mut()[slot] = Some((cell, piece));
            piece
        };
        Ok(piece.map(|mut piece| {
            piece.anchor = position.x.rem_euclid(side) == 1 && position.y.rem_euclid(side) == 1;
            piece
        }))
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
        if let Some(index) = index
            && !self.member(position)
        {
            self.put(index, position, |entry| entry.candidate = Some(None));
            return Ok(None);
        }
        let candidate = self
            .origins
            .borrow_mut()
            .candidate_with(position, || self.base(position))?;
        if let Some(index) = index {
            self.put(index, position, |entry| entry.candidate = Some(candidate));
        }
        Ok(candidate)
    }
    /// Reservations of a tile, computed once: callers keep them coordinate-stable.
    pub(super) fn reservations_with(
        &self,
        position: TileCoord,
        compute: impl FnOnce() -> Result<Reservations, EnvironmentPageError>,
    ) -> Result<Reservations, EnvironmentPageError> {
        let index = self.index(position);
        if let Some(index) = index
            && let Some(reservations) = self.entries.borrow()[index].reservations
        {
            return Ok(reservations);
        }
        let reservations = compute()?;
        if let Some(index) = index {
            self.put(index, position, |entry| {
                entry.reservations = Some(reservations)
            });
        }
        Ok(reservations)
    }
    /// Whether a patch may own the tile; `true` when it is not in the area.
    fn member(&self, position: TileCoord) -> bool {
        let (origin, side) = if self.tagged {
            let corner = |v: i32| v.div_euclid(SHARED_SIDE).saturating_mul(SHARED_SIDE);
            (
                TileCoord::new(corner(position.x), corner(position.y)),
                SHARED_SIDE,
            )
        } else {
            (self.origin, i32::try_from(self.side).unwrap_or(0))
        };
        let mut members = self.members.borrow_mut();
        if members
            .as_ref()
            .is_none_or(|(at, known, _)| (*at, *known) != (origin, side))
        {
            let bits = self.origins.borrow_mut().members(origin, side);
            *members = Some((origin, side, bits));
        }
        let x = i64::from(position.x) - i64::from(origin.x);
        let y = i64::from(position.y) - i64::from(origin.y);
        let side = i64::from(side);
        if !(0..side).contains(&x) || !(0..side).contains(&y) {
            return true;
        }
        let index = (y * side + x) as usize;
        members
            .as_ref()
            .is_some_and(|(_, _, bits)| bits[index / 64] & (1 << (index % 64)) != 0)
    }
    /// Store into a slot only while it still holds `position`.
    fn put(&self, index: usize, position: TileCoord, update: impl FnOnce(&mut Entry)) {
        let mut entries = self.entries.borrow_mut();
        if !self.tagged || entries[index].at == Some(position) {
            update(&mut entries[index]);
        }
    }
    pub(super) fn reserved(&self, position: TileCoord) -> Result<bool, EnvironmentPageError> {
        let index = self.index(position);
        if let Some(index) = index
            && let Some(reserved) = self.entries.borrow()[index].reserved
        {
            return Ok(reserved);
        }
        let reserved = self.reserved_uncached(position)?;
        if let Some(index) = index {
            self.put(index, position, |entry| entry.reserved = Some(reserved));
        }
        Ok(reserved)
    }
    fn reserved_uncached(&self, position: TileCoord) -> Result<bool, EnvironmentPageError> {
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

/// A tile's slot in the shared window.
fn tagged_slot(position: TileCoord) -> usize {
    (position.y.rem_euclid(SHARED_SIDE) * SHARED_SIDE + position.x.rem_euclid(SHARED_SIDE)) as usize
}

#[path = "memo/cache.rs"]
mod cache;
#[path = "memo/fishing.rs"]
mod fishing;
#[path = "memo/shore.rs"]
mod shore;
pub(super) use shore::Shore;

#[path = "memo/tests.rs"]
#[cfg(test)]
mod tests;
