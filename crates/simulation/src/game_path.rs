use crate::GameWorldError;
use aoe_core::{TileCoord, WorldPosition};
use std::collections::VecDeque;

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
    let (x, y) = (dx.unsigned_abs(), dy.unsigned_abs());
    if x < 1 << 31 && y < 1 << 31 {
        // Map distances: the float root lands within one of the floor, and
        // integer steps make it exact.
        let squared = x * x + y * y;
        let mut floor = (squared as f64).sqrt() as u64;
        while floor * floor > squared {
            floor -= 1;
        }
        while (floor + 1) * (floor + 1) <= squared {
            floor += 1;
        }
        let ceil = floor + u64::from(floor * floor != squared);
        return u32::try_from(ceil).unwrap_or(u32::MAX).max(1);
    }
    let squared = (x as u128).pow(2) + (y as u128).pow(2);
    let floor = squared.isqrt();
    u32::try_from(if floor * floor == squared {
        floor
    } else {
        floor + 1
    })
    .unwrap_or(u32::MAX)
    .max(1)
}

pub(crate) fn route_waypoint(
    tiles: Vec<TileCoord>,
    origin: WorldPosition,
    destination: WorldPosition,
) -> Result<(WorldPosition, VecDeque<TileCoord>), GameWorldError> {
    let mut route = VecDeque::from(tiles);
    if route.pop_front() != Some(origin.tile_floor()) {
        return Err(GameWorldError::InvalidPosition);
    }
    let waypoint = route
        .pop_front()
        .and_then(|tile| WorldPosition::from_tile_center(tile).ok())
        .unwrap_or(destination);
    Ok((waypoint, route))
}

#[path = "game_path/tests.rs"]
#[cfg(test)]
mod tests;
