//! A one-tile halo amortizes terrain cache lookups over every outgoing step.
use super::*;
use crate::navigation::regions::{CHUNK, label};

const SIDE: usize = (CHUNK + 2) as usize;

pub(super) fn read(ground: &MapRegionGround<'_>, key: (i32, i32), size: (i32, i32)) -> RegionChunk {
    let at = label::origin(key);
    let mut cells = vec![None; SIDE * SIDE];
    let mut open = vec![false; SIDE * SIDE];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let tile = TileCoord::new(at.x + x as i32 - 1, at.y + y as i32 - 1);
            if tile.x < 0 || tile.y < 0 || tile.x >= size.0 || tile.y >= size.1 {
                continue;
            }
            let cell = ground.cell(tile);
            let index = y * SIDE + x;
            cells[index] = cell;
            open[index] = cell.is_some_and(|cell| {
                !ground.overlay.blocks_tile(tile)
                    && (cell.passable || ground.overlay.bridge_connections(tile).is_some())
                    && cell
                        .node
                        .is_none_or(|node| !ground.overlay.blocks_node(node))
            });
        }
    }
    let edge = |a: usize, b: usize, from: TileCoord, to: TileCoord| {
        if let Some(clear) = ground.overlay.bridge_edge(from, to) {
            return clear;
        }
        let (Some(a), Some(b)) = (cells[a], cells[b]) else {
            return false;
        };
        a.passable
            && b.passable
            && a.walkable_surface
            && b.walkable_surface
            && (i32::from(a.game_height_level) - i32::from(b.game_height_level)).abs() <= 1
    };
    let mut classes = Vec::with_capacity(super::super::CELLS);
    let mut steps = Vec::with_capacity(super::super::CELLS);
    for y in 1..=CHUNK as usize {
        for x in 1..=CHUNK as usize {
            let index = y * SIDE + x;
            classes.push(if open[index] {
                RegionTile::Open
            } else {
                RegionTile::Blocked
            });
            let from = TileCoord::new(at.x + x as i32 - 1, at.y + y as i32 - 1);
            let mut mask = 0_u8;
            if open[index] {
                for (direction, (dx, dy)) in label::NEIGHBOURS.into_iter().enumerate() {
                    let next = ((y as i32 + dy) * SIDE as i32 + x as i32 + dx) as usize;
                    if !open[next] {
                        continue;
                    }
                    let to = TileCoord::new(from.x + dx, from.y + dy);
                    let allowed = if dx != 0 && dy != 0 {
                        ground.overlay.bridge_edge(from, to).unwrap_or_else(|| {
                            let h = (y * SIDE) as i32 + x as i32 + dx;
                            let v = (y as i32 + dy) * SIDE as i32 + x as i32;
                            open[h as usize]
                                && open[v as usize]
                                && edge(index, h as usize, from, TileCoord::new(to.x, from.y))
                                && edge(index, v as usize, from, TileCoord::new(from.x, to.y))
                                && edge(index, next, from, to)
                        })
                    } else {
                        edge(index, next, from, to)
                    };
                    if allowed {
                        mask |= 1 << direction;
                    }
                }
            }
            steps.push(mask);
        }
    }
    RegionChunk::new(classes, steps)
}
