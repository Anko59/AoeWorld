//! Per-poll memo of terrain answers. One search probes a tile up to a dozen
//! times (as a neighbour of several expanded tiles and as the side of their
//! diagonal steps), and each probe locks the walk cache and looks the tile up
//! in the resource overlay. Ground does not change during one poll, so the
//! first answer is reused; the search reads exactly what it read before.
//!
//! The answers last one poll; the maps that hold them last the thread. A tick
//! makes many polls, most of them the first poll of a new planner, and maps
//! built empty for each one regrew by rehash to thousands of entries.
use super::super::tile_hash::TileHasher;
use super::RouteTerrain;
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::TileCoord;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

// The persistent search records also use point lookups, never iteration to
// decide a route. Their canonical serialization sorts keys separately.
pub(super) type Map<K, V> = HashMap<K, V, BuildHasherDefault<TileHasher>>;

/// A map grown past this capacity is dropped instead of being kept for the
/// next poll. Emptying a map takes time in proportion to its capacity, and a
/// one-shot search may spend millions of work units in a single poll: every
/// later poll on its thread would pay for that table, and the thread would
/// hold its memory. An expansion of nine work units asks about at most
/// twenty-four steps, so a poll of up to twenty thousand units, more than a
/// simulation tick grants one, keeps its maps.
const KEPT_CAPACITY: usize = 1 << 16;

/// What one poll has been told about the ground.
#[derive(Default)]
struct Answers {
    walkable: Map<TileCoord, bool>,
    edges: Map<(TileCoord, TileCoord), bool>,
    costs: Map<(TileCoord, u8), u32>,
}

impl Answers {
    /// Forgets every answer. A map keeps the room it has grown to, unless
    /// that is more than `KEPT_CAPACITY`.
    fn forget(&mut self) {
        fn empty<K, V>(map: &mut Map<K, V>) {
            if map.capacity() > KEPT_CAPACITY {
                *map = Map::default();
            } else {
                map.clear();
            }
        }
        empty(&mut self.walkable);
        empty(&mut self.edges);
        empty(&mut self.costs);
    }
}

thread_local! {
    /// The emptied maps of the last poll on this thread, whichever planner
    /// made it. A poll takes them and hands them back, emptied, when it ends.
    static SPARE: Cell<Answers> = Cell::default();
}

pub(super) struct MemoTerrain<'a> {
    inner: &'a dyn RouteTerrain,
    /// Walkability is memoized for this overlay only. A search also asks
    /// about its proof snapshot (`with_proof_overlay`); keyed by tile alone,
    /// the memo answered the snapshot with the live overlay's answer, so a
    /// tile blocked in the snapshot but free now entered the search.
    overlay: &'a ResourceOverlay,
    answers: RefCell<Answers>,
}

impl<'a> MemoTerrain<'a> {
    pub(super) fn new(inner: &'a dyn RouteTerrain, overlay: &'a ResourceOverlay) -> Self {
        Self {
            inner,
            overlay,
            // A thread that is being torn down has no spare maps left.
            answers: RefCell::new(SPARE.try_with(Cell::take).unwrap_or_default()),
        }
    }
}

impl Drop for MemoTerrain<'_> {
    fn drop(&mut self) {
        // Ground may change before the next poll: the maps go back empty.
        let mut answers = std::mem::take(self.answers.get_mut());
        answers.forget();
        let _ = SPARE.try_with(|spare| spare.set(answers));
    }
}

impl RouteTerrain for MemoTerrain<'_> {
    fn prepare_navigation_chunk(&self) -> bool {
        self.inner.prepare_navigation_chunk()
    }
    fn route_generator(&self) -> Option<&crate::MapChunkGenerator> {
        self.inner.route_generator()
    }

    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if !std::ptr::eq(overlay, self.overlay) {
            return self.inner.route_walkable(overlay, tile, cancelled);
        }
        if let Some(known) = self.answers.borrow().walkable.get(&tile) {
            return Ok(*known);
        }
        let walkable = self.inner.route_walkable(overlay, tile, cancelled)?;
        self.answers.borrow_mut().walkable.insert(tile, walkable);
        Ok(walkable)
    }

    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if let Some(known) = self.answers.borrow().edges.get(&(from, to)) {
            return Ok(*known);
        }
        let clear = self.inner.route_edge_clear(from, to, cancelled)?;
        self.answers.borrow_mut().edges.insert((from, to), clear);
        Ok(clear)
    }

    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        // A step's cost depends on its target and on whether it is diagonal.
        let key = (to, u8::from(from.x != to.x && from.y != to.y));
        if let Some(known) = self.answers.borrow().costs.get(&key) {
            return Ok(*known);
        }
        let cost = self.inner.route_step_cost(from, to, cancelled)?;
        self.answers.borrow_mut().costs.insert(key, cost);
        Ok(cost)
    }
}
