//! Exact walking distances from one tile over a rectangle: the same passability,
//! diagonal and bridge rules as `find_path_with_overlay`, off tick only. A
//! measuring tool: a march is judged against the truth, not against the planner.
use super::{neighbors, walkable_with_overlay};
use crate::{MapChunkGenerator, ResourceOverlay};
use aoe_core::TileCoord;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const UNREACHED: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct WalkingField {
    min: TileCoord,
    side: (usize, usize),
    costs: Vec<u32>,
}

impl WalkingField {
    pub fn cost(&self, tile: TileCoord) -> Option<u32> {
        let (x, y) = (
            usize::try_from(tile.x - self.min.x).ok()?,
            usize::try_from(tile.y - self.min.y).ok()?,
        );
        if x >= self.side.0 || y >= self.side.1 {
            return None;
        }
        Some(self.costs[y * self.side.0 + x]).filter(|&cost| cost != UNREACHED)
    }
}

/// Dijkstra outward from `origin`, inside `min..=max`.
pub fn walking_field(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    min: TileCoord,
    max: TileCoord,
) -> WalkingField {
    let side = ((max.x - min.x + 1) as usize, (max.y - min.y + 1) as usize);
    let mut field = WalkingField {
        min,
        side,
        costs: vec![UNREACHED; side.0 * side.1],
    };
    let slot = |tile: TileCoord| {
        (tile.x >= min.x && tile.y >= min.y && tile.x <= max.x && tile.y <= max.y)
            .then(|| (tile.y - min.y) as usize * side.0 + (tile.x - min.x) as usize)
    };
    let passable = |tile| walkable_with_overlay(terrain, overlay, tile);
    let Some(start) = slot(origin) else {
        return field;
    };
    field.costs[start] = 0;
    let mut open = BinaryHeap::from([Reverse((0_u32, origin.y, origin.x))]);
    while let Some(Reverse((cost, y, x))) = open.pop() {
        let tile = TileCoord::new(x, y);
        if slot(tile).is_none_or(|at| field.costs[at] != cost) {
            continue;
        }
        for (next, step) in neighbors(terrain, tile, &passable, Some(overlay)) {
            let Some(at) = slot(next) else { continue };
            if cost + step < field.costs[at] {
                field.costs[at] = cost + step;
                open.push(Reverse((cost + step, next.y, next.x)));
            }
        }
    }
    field
}
