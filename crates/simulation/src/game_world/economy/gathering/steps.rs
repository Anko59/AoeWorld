//! Exact short static edges, shared by work floods and bounded interaction rays.
use crate::game_world::movement_step::{CellHasher, tile_key};
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, TileRect};
use aoe_map::EnvironmentPageError;
use std::{collections::HashMap, hash::BuildHasherDefault};

#[derive(Debug, Default)]
struct Edges {
    known: u16,
    clear: u16,
}

#[derive(Debug, Default)]
pub(crate) struct Steps {
    cells: [HashMap<u64, Edges, BuildHasherDefault<CellHasher>>; 2],
}

impl Steps {
    pub(super) fn clear(&mut self) {
        for cells in &mut self.cells {
            cells.clear();
        }
    }

    pub(super) fn invalidate(&mut self, area: TileRect) {
        let min = TileCoord::new(area.min.x.saturating_sub(1), area.min.y.saturating_sub(1));
        let max = TileCoord::new(area.max.x.saturating_add(1), area.max.y.saturating_add(1));
        let count = (i64::from(max.x) - i64::from(min.x)) * (i64::from(max.y) - i64::from(min.y));
        for cells in &mut self.cells {
            if count <= cells.len() as i64 {
                for y in min.y..max.y {
                    for x in min.x..max.x {
                        cells.remove(&tile_key(TileCoord::new(x, y)));
                    }
                }
            } else {
                cells.retain(|&key, _| {
                    let tile = TileCoord::new(key as u32 as i32, (key >> 32) as u32 as i32);
                    tile.x < min.x || tile.y < min.y || tile.x >= max.x || tile.y >= max.y
                });
            }
        }
    }

    pub(crate) fn probe(
        &mut self,
        world: &GameWorld,
        kind: UnitKind,
        from: TileCoord,
        to: TileCoord,
    ) -> Result<bool, EnvironmentPageError> {
        let direction = match (
            i64::from(to.x) - i64::from(from.x),
            i64::from(to.y) - i64::from(from.y),
        ) {
            (0, 0) => 8,
            (0, -1) => 0,
            (-1, 0) => 1,
            (1, 0) => 2,
            (0, 1) => 3,
            (-1, -1) => 4,
            (1, -1) => 5,
            (-1, 1) => 6,
            (1, 1) => 7,
            _ => {
                return Ok(
                    world.building_step_clear(from, to) && world.unit_step_clear(kind, from, to)?
                );
            }
        };
        let bit = 1 << direction;
        let cells = &mut self.cells[usize::from(kind.is_naval())];
        let key = tile_key(from);
        if let Some(edge) = cells.get(&key)
            && edge.known & bit != 0
        {
            return Ok(edge.clear & bit != 0);
        }
        // Failed environment pages may recover without a ground event.
        let clear = world.building_step_clear(from, to) && world.unit_step_clear(kind, from, to)?;
        if cells.len() == 262_144 && !cells.contains_key(&key) {
            cells.clear();
        }
        let edge = cells.entry(key).or_default();
        edge.known |= bit;
        if clear {
            edge.clear |= bit;
        }
        Ok(clear)
    }
}
