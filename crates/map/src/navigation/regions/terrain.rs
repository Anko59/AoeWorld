//! Bounded bulk reads of today's planner ground. Region construction visits
//! every tile, so calling the point cache for each edge is unnecessarily costly.
use super::{RegionChunk, RegionGround, RegionTile};
#[path = "terrain/chunk.rs"]
mod chunk;
use crate::{EnvironmentPageError, MapChunkGenerator, ResourceOverlay, WalkCell};
use aoe_core::TileCoord;
use std::cell::Cell;

pub struct MapRegionGround<'a> {
    generator: &'a MapChunkGenerator,
    overlay: &'a ResourceOverlay,
    cancelled: &'a dyn Fn() -> bool,
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
            error: Cell::new(None),
        }
    }

    /// Capture terrain and bridge steps before the caller's territory barriers.
    /// Ordinary buildings and natural resources are left to unit routing.
    pub fn territory_chunk(&self, key: (i32, i32), size: (i32, i32)) -> [Option<u8>; super::CELLS] {
        chunk::read(self, key, size, |_, _| true)
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
        match self.generator.walk_at_with_cancel(tile, self.cancelled) {
            Ok(cell) => cell,
            Err(error) => {
                self.error.set(Some(error));
                None
            }
        }
    }

    fn chunk_cells(&self, key: (i32, i32)) -> Option<Vec<Option<WalkCell>>> {
        if self.error.get().is_some() {
            return None;
        }
        match self.generator.navigation_cells(key, self.cancelled) {
            Ok(cells) => Some(cells),
            Err(error) => {
                self.error.set(Some(error));
                None
            }
        }
    }
}

impl RegionGround for MapRegionGround<'_> {
    fn available(&self) -> bool {
        self.error.get().is_none()
    }
    fn chunk(&self, key: (i32, i32), size: (i32, i32)) -> Option<RegionChunk> {
        let steps = chunk::read(self, key, size, |tile, cell| {
            !self.overlay.blocks_tile(tile)
                && cell.node.is_none_or(|node| !self.overlay.blocks_node(node))
        });
        let classes = steps
            .iter()
            .map(|step| {
                if step.is_some() {
                    RegionTile::Open
                } else {
                    RegionTile::Blocked
                }
            })
            .collect();
        Some(RegionChunk::new(
            classes,
            steps.into_iter().map(|step| step.unwrap_or(0)).collect(),
        ))
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
