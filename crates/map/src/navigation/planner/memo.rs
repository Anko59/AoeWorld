//! Per-poll memo of terrain answers. One search probes a tile up to a dozen
//! times (as a neighbour of several expanded tiles and as the side of their
//! diagonal steps), and each probe locks the walk cache and looks the tile up
//! in the resource overlay. Ground does not change during one poll, so the
//! first answer is reused; the search reads exactly what it read before.
use super::super::tile_hash::TileHasher;
use super::RouteTerrain;
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::TileCoord;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;

// The persistent search records also use point lookups, never iteration to
// decide a route. Their canonical serialization sorts keys separately.
pub(super) type Map<K, V> = HashMap<K, V, BuildHasherDefault<TileHasher>>;

pub(super) struct MemoTerrain<'a> {
    inner: &'a dyn RouteTerrain,
    /// Walkability is memoized for this overlay only. A search also asks
    /// about its proof snapshot (`with_proof_overlay`); keyed by tile alone,
    /// the memo answered the snapshot with the live overlay's answer, so a
    /// tile blocked in the snapshot but free now entered the search.
    overlay: &'a ResourceOverlay,
    walkable: RefCell<Map<TileCoord, bool>>,
    edges: RefCell<Map<(TileCoord, TileCoord), bool>>,
    costs: RefCell<Map<(TileCoord, u8), u32>>,
}

impl<'a> MemoTerrain<'a> {
    pub(super) fn new(inner: &'a dyn RouteTerrain, overlay: &'a ResourceOverlay) -> Self {
        Self {
            inner,
            overlay,
            walkable: RefCell::default(),
            edges: RefCell::default(),
            costs: RefCell::default(),
        }
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
        if let Some(known) = self.walkable.borrow().get(&tile) {
            return Ok(*known);
        }
        let walkable = self.inner.route_walkable(overlay, tile, cancelled)?;
        self.walkable.borrow_mut().insert(tile, walkable);
        Ok(walkable)
    }

    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if let Some(known) = self.edges.borrow().get(&(from, to)) {
            return Ok(*known);
        }
        let clear = self.inner.route_edge_clear(from, to, cancelled)?;
        self.edges.borrow_mut().insert((from, to), clear);
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
        if let Some(known) = self.costs.borrow().get(&key) {
            return Ok(*known);
        }
        let cost = self.inner.route_step_cost(from, to, cancelled)?;
        self.costs.borrow_mut().insert(key, cost);
        Ok(cost)
    }
}
