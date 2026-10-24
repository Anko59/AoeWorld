//! Per-poll memo of terrain answers. One search probes a tile up to a dozen
//! times (as a neighbour of several expanded tiles and as the side of their
//! diagonal steps), and each probe locks the walk cache and looks the tile up
//! in the resource overlay. Ground does not change during one poll, so the
//! first answer is reused; the search reads exactly what it read before.
use super::RouteTerrain;
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::TileCoord;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A multiply-rotate hasher for small integer keys (FxHash's mixing step).
#[derive(Default)]
struct TileHasher(u64);

impl Hasher for TileHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.write_u64(u64::from(*byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_i32(&mut self, value: i32) {
        self.write_u64(u64::from(value as u32));
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
}

type Map<K, V> = HashMap<K, V, BuildHasherDefault<TileHasher>>;

pub(super) struct MemoTerrain<'a> {
    inner: &'a dyn RouteTerrain,
    walkable: RefCell<Map<TileCoord, bool>>,
    edges: RefCell<Map<(TileCoord, TileCoord), bool>>,
    costs: RefCell<Map<(TileCoord, u8), u32>>,
}

impl<'a> MemoTerrain<'a> {
    pub(super) fn new(inner: &'a dyn RouteTerrain) -> Self {
        Self {
            inner,
            walkable: RefCell::default(),
            edges: RefCell::default(),
            costs: RefCell::default(),
        }
    }
}

impl RouteTerrain for MemoTerrain<'_> {
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
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
