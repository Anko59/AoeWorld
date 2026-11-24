//! The directed edge test of a leg search, for callers that walk routes of
//! their own and must take only steps a search could take (#978 slice 6e).
use super::Terminal;
use super::search::neighbor_clear;
use crate::{EnvironmentPageError, ResourceOverlay, RouteTerrain};
use aoe_core::TileCoord;

/// The directed edge an ordinary leg search reads from `from` by `offset`.
pub fn route_neighbor_clear(
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    from: TileCoord,
    offset: (i32, i32),
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, EnvironmentPageError> {
    match neighbor_clear(terrain, overlay, from, offset, cancelled) {
        Ok(clear) => Ok(clear),
        Err(Terminal::Environment(error)) => Err(error),
        Err(_) => Ok(false),
    }
}
