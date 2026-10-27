//! Shared integer footprint geometry.
use crate::GameWorldError;
use aoe_core::{TileCoord, TileRect, WorldPosition};

pub(crate) fn position(tile: TileCoord) -> Result<WorldPosition, GameWorldError> {
    WorldPosition::from_tile_center(tile).map_err(|_| GameWorldError::InvalidPosition)
}
pub(crate) fn distance_squared(a: TileCoord, b: TileCoord) -> u64 {
    let x = u64::from(a.x.abs_diff(b.x));
    let y = u64::from(a.y.abs_diff(b.y));
    x.saturating_mul(x).saturating_add(y.saturating_mul(y))
}
pub(crate) fn perimeter(rect: TileRect) -> Vec<TileCoord> {
    let mut tiles = Vec::new();
    for y in rect.min.y - 1..=rect.max.y {
        for x in rect.min.x - 1..=rect.max.x {
            let tile = TileCoord::new(x, y);
            if !rect.contains(tile) {
                tiles.push(tile);
            }
        }
    }
    tiles
}

pub(crate) fn rects_overlap(a: TileRect, b: TileRect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}
/// Is `tile` in the one-tile ring around `rect`, corners included?
pub(crate) fn adjacent(rect: TileRect, tile: TileCoord) -> bool {
    tile.x >= rect.min.x - 1
        && tile.x <= rect.max.x
        && tile.y >= rect.min.y - 1
        && tile.y <= rect.max.y
        && !rect.contains(tile)
}
