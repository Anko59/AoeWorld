//! Bounded bulk reads of today's planner ground. Region construction visits
//! every tile, so calling the point cache for each edge is unnecessarily costly.
use super::{RegionChunk, RegionGround, RegionTile};
#[path = "terrain/chunk.rs"]
mod chunk;
use crate::{EnvironmentPageError, MapChunkGenerator, ResourceOverlay, WalkCell};
use aoe_core::TileCoord;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

type CachedChunk = ((i32, i32), Vec<Option<WalkCell>>);

pub struct MapRegionGround<'a> {
    generator: &'a MapChunkGenerator,
    overlay: &'a ResourceOverlay,
    cancelled: &'a dyn Fn() -> bool,
    /// The current chunk and its eight neighbours; never a map-sized tile cache.
    recent: RefCell<VecDeque<CachedChunk>>,
    error: Cell<Option<EnvironmentPageError>>,
}

impl<'a> MapRegionGround<'a> {
    pub fn new(
        generator: &'a MapChunkGenerator,
        overlay: &'a ResourceOverlay,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            generator,
            overlay,
            cancelled,
            recent: RefCell::default(),
            error: Cell::new(None),
        }
    }

    /// A failed read invalidates the query, never a proof of blocked ground.
    /// Callers must discard the derived regions when this returns an error.
    pub fn error(&self) -> Option<EnvironmentPageError> {
        self.error.get()
    }

    fn cell(&self, tile: TileCoord) -> Option<WalkCell> {
        if self.error.get().is_some() {
            return None;
        }
        if (self.cancelled)() {
            self.error.set(Some(EnvironmentPageError::Cancelled));
            return None;
        }
        if tile.x < 0
            || tile.y < 0
            || tile.x >= self.generator.width_tiles()
            || tile.y >= self.generator.width_tiles()
        {
            return None;
        }
        let key = (tile.x / super::CHUNK, tile.y / super::CHUNK);
        let at = super::label::index(tile);
        if let Some((_, cells)) = self
            .recent
            .borrow()
            .iter()
            .find(|(cached, _)| *cached == key)
        {
            return cells[at];
        }
        let cells = match self.generator.navigation_cells(key, self.cancelled) {
            Ok(cells) => cells,
            Err(error) => {
                self.error.set(Some(error));
                return None;
            }
        };
        let result = cells[at];
        let mut recent = self.recent.borrow_mut();
        if recent.len() == 9 {
            recent.pop_front();
        }
        recent.push_back((key, cells));
        result
    }
}

impl RegionGround for MapRegionGround<'_> {
    fn available(&self) -> bool {
        self.error.get().is_none()
    }
    fn chunk(&self, key: (i32, i32), size: (i32, i32)) -> Option<RegionChunk> {
        Some(chunk::read(self, key, size))
    }
    fn tile(&self, tile: TileCoord) -> RegionTile {
        if self.cell(tile).is_some_and(|cell| {
            !self.overlay.blocks_tile(tile)
                && (cell.passable || self.overlay.bridge_connections(tile).is_some())
                && cell.node.is_none_or(|node| !self.overlay.blocks_node(node))
        }) {
            RegionTile::Open
        } else {
            RegionTile::Blocked
        }
    }

    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        if let Some(clear) = self.overlay.bridge_edge(from, to) {
            return clear;
        }
        let (Some(a), Some(b)) = (self.cell(from), self.cell(to)) else {
            return false;
        };
        // The same predicate used by the planner's bulk Grid and cached_edge.
        a.passable
            && b.passable
            && a.walkable_surface
            && b.walkable_surface
            && (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() <= 1
    }

    fn diagonal_override(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        self.overlay.bridge_edge(from, to)
    }

    fn gate_owner(&self, _: u32) -> u32 {
        super::super::ANY_PLAYER
    }
}
