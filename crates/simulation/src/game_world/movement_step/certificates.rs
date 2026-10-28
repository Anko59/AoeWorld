//! A cached clearance value proves an empty square of complete static tiles.
use aoe_core::{TileCoord, WorldPosition};

#[derive(Clone, Copy, Debug)]
pub(in crate::game_world) struct StaticArea {
    min_x: i64,
    min_y: i64,
    max_x: i64,
    max_y: i64,
}
impl StaticArea {
    pub(in crate::game_world) fn new(tile: TileCoord, clearance: u8) -> Option<Self> {
        if clearance == 0 {
            return None;
        }
        // c=2*d-1: every tile at Chebyshev distance <d is walkable.
        // A saturated 15 is a lower bound, so it still proves seven cells.
        let radius = i64::from((clearance - 1) / 2);
        Some(Self {
            min_x: (i64::from(tile.x) - radius) * 1024,
            min_y: (i64::from(tile.y) - radius) * 1024,
            max_x: (i64::from(tile.x) + radius + 1) * 1024,
            max_y: (i64::from(tile.y) + radius + 1) * 1024,
        })
    }
    pub(in crate::game_world) fn covers(self, from: WorldPosition, radius: i32, step: i32) -> bool {
        let reach = i64::from(radius) + i64::from(step);
        i64::from(from.x) - reach >= self.min_x
            && i64::from(from.y) - reach >= self.min_y
            && i64::from(from.x) + reach <= self.max_x
            && i64::from(from.y) + reach <= self.max_y
    }
}
