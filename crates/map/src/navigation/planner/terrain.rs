//! Terrain queries shared by the incremental planner's map and uniform worlds.
use crate::{EdgePassability, EnvironmentPageError, MapChunkGenerator, ResourceOverlay};
use aoe_core::TileCoord;

/// Ground a route search reads. Answers must not change during one poll (a
/// poll memoizes them), and a step's cost may depend only on its target tile
/// and on whether the step is diagonal.
pub trait RouteTerrain {
    /// Static map guidance is optional for other movement domains.
    /// Reserve one logical chunk preparation, independently of physical cache hits.
    fn prepare_navigation_chunk(&self) -> bool {
        true
    }

    fn route_generator(&self) -> Option<&MapChunkGenerator> {
        None
    }

    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError>;

    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError>;

    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError>;
}

impl RouteTerrain for MapChunkGenerator {
    fn route_generator(&self) -> Option<&MapChunkGenerator> {
        Some(self)
    }

    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        let Some(cell) = self.walk_at_with_cancel(tile, cancelled)? else {
            return Ok(false);
        };
        Ok(!overlay.blocks_tile(tile)
            && cell.passable
            && cell.node.is_none_or(|node| !overlay.blocks_node(node)))
    }

    fn route_edge_clear(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(matches!(
            self.edge_between_with_cancel(from, to, cancelled)?,
            EdgePassability::Passable
        ))
    }

    fn route_step_cost(
        &self,
        from: TileCoord,
        to: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, EnvironmentPageError> {
        let base = if from.x == to.x || from.y == to.y {
            super::super::ORTHOGONAL_COST
        } else {
            super::super::DIAGONAL_COST
        };
        let multiplier = if self
            .walk_at_with_cancel(to, cancelled)?
            .is_some_and(|cell| cell.mud)
        {
            3
        } else {
            2
        };
        Ok(base * multiplier / 2)
    }
}
