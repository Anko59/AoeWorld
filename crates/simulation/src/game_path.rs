use aoe_core::{TileCoord, WorldPosition};

pub(crate) fn next_waypoint(
    origin: WorldPosition,
    target_tile: TileCoord,
    destination: WorldPosition,
) -> WorldPosition {
    let current_tile = origin.tile_floor();
    let next_tile = TileCoord::new(
        current_tile.x + (target_tile.x - current_tile.x).signum(),
        current_tile.y + (target_tile.y - current_tile.y).signum(),
    );
    WorldPosition::from_tile_center(next_tile).unwrap_or(destination)
}

pub(crate) fn segment_length(dx: i64, dy: i64) -> u32 {
    let squared = (dx.unsigned_abs() as u128).pow(2) + (dy.unsigned_abs() as u128).pow(2);
    let floor = squared.isqrt();
    u32::try_from(if floor * floor == squared {
        floor
    } else {
        floor + 1
    })
    .unwrap_or(u32::MAX)
    .max(1)
}
