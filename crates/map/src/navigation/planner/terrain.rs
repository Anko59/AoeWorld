//! Terrain queries shared by the incremental planner's map and uniform worlds.
use crate::{
    EdgePassability, EnvironmentPageError, GroundMaterial, MapChunkGenerator, ResourceOverlay,
};
use aoe_core::TileCoord;

pub trait RouteTerrain {
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
    fn route_walkable(
        &self,
        overlay: &ResourceOverlay,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        let Some((sample, object)) = self.tile_and_node_with_cancel(tile, cancelled)? else {
            return Ok(false);
        };
        Ok(!overlay.blocks_tile(tile)
            && sample.passable
            && object.is_none_or(|node| !overlay.blocks_node(node)))
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
        let multiplier = if matches!(
            self.tile_at_with_cancel(to, cancelled)?
                .map(|sample| sample.material),
            Some(GroundMaterial::Mud)
        ) {
            3
        } else {
            2
        };
        Ok(base * multiplier / 2)
    }
}
