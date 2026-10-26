//! Idle tile membership and its eight deterministic neighbours.
use aoe_core::{EntityId, TileCoord};
pub(super) const OFFSETS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];
#[derive(Debug, Default)]
pub(super) struct Cell {
    pub(super) units: Vec<EntityId>,
    pub(super) adjacent: u8,
}
pub(super) fn key(tile: TileCoord) -> u64 {
    u64::from(tile.x as u32) | (u64::from(tile.y as u32) << 32)
}
pub(super) fn adjacent(tile: TileCoord) -> impl Iterator<Item = TileCoord> {
    OFFSETS
        .into_iter()
        .map(move |(x, y)| TileCoord::new(tile.x + x, tile.y + y))
}
