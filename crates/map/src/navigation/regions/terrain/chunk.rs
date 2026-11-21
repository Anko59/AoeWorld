//! A one-tile halo amortizes terrain cache lookups over every outgoing step.
use super::*;
use crate::navigation::regions::{CHUNK, label};

const SIDE: usize = (CHUNK + 2) as usize;

pub(super) fn read(
    ground: &MapRegionGround<'_>,
    key: (i32, i32),
    size: (i32, i32),
    admits: impl Fn(TileCoord, WalkCell) -> bool,
) -> [Option<u8>; super::super::CELLS] {
    let at = label::origin(key);
    let mut cells = vec![None; SIDE * SIDE];
    let mut open = [false; SIDE * SIDE];
    let mut bridge = [false; SIDE * SIDE];
    let Some(interior) = ground.chunk_cells(key) else {
        return [None; super::super::CELLS];
    };
    for y in 0..SIDE {
        for x in 0..SIDE {
            let tile = TileCoord::new(at.x + x as i32 - 1, at.y + y as i32 - 1);
            if tile.x < 0 || tile.y < 0 || tile.x >= size.0 || tile.y >= size.1 {
                continue;
            }
            let cell = if x == 0 || y == 0 || x + 1 == SIDE || y + 1 == SIDE {
                ground.cell(tile)
            } else {
                interior[(y - 1) * CHUNK as usize + x - 1]
            };
            let index = y * SIDE + x;
            cells[index] = cell;
            bridge[index] = ground.overlay.bridge_connections(tile).is_some();
            open[index] =
                cell.is_some_and(|cell| (cell.passable || bridge[index]) && admits(tile, cell));
        }
    }
    let edge = |a: usize, b: usize, from: TileCoord, to: TileCoord| {
        if (bridge[a] || bridge[b])
            && let Some(clear) = ground.overlay.bridge_edge(from, to)
        {
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
    let mut steps = [None; super::super::CELLS];
    for y in 1..=CHUNK as usize {
        for x in 1..=CHUNK as usize {
            let index = y * SIDE + x;
            if !open[index] {
                continue;
            }
            let from = TileCoord::new(at.x + x as i32 - 1, at.y + y as i32 - 1);
            let mut mask = 0_u8;
            for (direction, (dx, dy)) in label::NEIGHBOURS.into_iter().enumerate() {
                let next = ((y as i32 + dy) * SIDE as i32 + x as i32 + dx) as usize;
                if !open[next] {
                    continue;
                }
                let to = TileCoord::new(from.x + dx, from.y + dy);
                let allowed = if dx != 0 && dy != 0 {
                    if bridge[index] || bridge[next] {
                        ground.overlay.bridge_edge(from, to).unwrap_or(false)
                    } else {
                        let h = (y * SIDE) as i32 + x as i32 + dx;
                        let v = (y as i32 + dy) * SIDE as i32 + x as i32;
                        open[h as usize]
                            && open[v as usize]
                            && edge(index, h as usize, from, TileCoord::new(to.x, from.y))
                            && edge(index, v as usize, from, TileCoord::new(from.x, to.y))
                            && edge(index, next, from, to)
                    }
                } else {
                    edge(index, next, from, to)
                };
                if allowed {
                    mask |= 1 << direction;
                }
            }
            steps[(y - 1) * CHUNK as usize + x - 1] = Some(mask);
        }
    }
    steps
}
