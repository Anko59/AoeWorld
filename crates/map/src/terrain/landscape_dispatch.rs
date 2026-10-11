//! Authoritative composed-landscape dispatch. Reservations are procedural
//! geometry, not proof that source water/cliffs admit an exit; no bridges are
//! synthesized.
use super::*;
use crate::landscape_ecology::Reservations;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LandscapePoint {
    pub tile: LandscapeTile,
    pub resource: Option<LandscapeResource>,
    pub decoration: Option<LandscapeDecoration>,
}

impl MapChunkGenerator {
    /// Physical terrain only. Composition changes material and marks the
    /// shallow rim, never passability, surface or height; overlays remain a
    /// caller concern.
    pub(super) fn base_physical_tile_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Tile>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width_tiles || tile.y >= self.width_tiles {
            return Ok(None);
        }
        if self.provider.is_some() {
            provider::sample_base_tile(self, tile, cancelled).map(Some)
        } else {
            Ok(Some(self.sample_base_tile(tile)))
        }
    }

    /// Shaped physical water kind without elevation, biome or composition;
    /// `None` outside the map.
    pub(super) fn base_water_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<WaterKind>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if tile.x < 0 || tile.y < 0 || tile.x >= self.width_tiles || tile.y >= self.width_tiles {
            return Ok(None);
        }
        if self.provider.is_some() {
            provider::water::water_kind(self, tile, cancelled).map(Some)
        } else {
            Ok(Some(self.dense_water(tile).0))
        }
    }

    /// Conservative reach test: `false` proves that no tile within `radius`
    /// tiles is water, without sampling those tiles.
    pub(super) fn water_possible_near(
        &self,
        tile: TileCoord,
        radius: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.provider.is_some() {
            provider::water::water_near(self, tile, radius, cancelled)
        } else {
            Ok(self.dense_water_near(tile, radius))
        }
    }

    /// Conservative depth test: `true` proves that every tile within
    /// `radius` tiles is lake or ocean water, without sampling those tiles.
    pub(super) fn open_water_certain_near(
        &self,
        tile: TileCoord,
        radius: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        let inside = |value: i32| value >= radius && value < self.width_tiles - radius;
        if !inside(tile.x) || !inside(tile.y) {
            return Ok(false);
        }
        if self.provider.is_some() {
            provider::water::open_water_near(self, tile, radius, cancelled)
        } else {
            Ok(self.dense_open_water_near(tile, radius))
        }
    }

    /// Terrain and immutable node from one composition. Failed queries are
    /// returned directly and never cached here.
    pub fn tile_and_node_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<(Tile, Option<ResourceNode>)>, EnvironmentPageError> {
        self.landscape_point_with_cancel(tile, cancelled)
            .map(|point| {
                point.map(|point| {
                    (
                        point.tile.terrain,
                        point.resource.map(|resource| resource.node),
                    )
                })
            })
    }

    /// One bounded shared source evaluation; trees/resources and terrain are the
    /// same objects returned by the typed chunk path. All page errors propagate.
    pub fn landscape_point_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapePoint>, EnvironmentPageError> {
        self.evaluate_landscape_point_with_cancel(
            tile,
            &default_policy,
            &|position| self.landscape_reservations_at(position),
            cancelled,
        )
    }

    /// Shared start reservations: the starting glade always clears canopy,
    /// floor, trees and dressing; its connectors are corridors cleared only
    /// inside large forests. Resource approaches are added by the composer,
    /// from base terrain only. Actual physical exits remain subject to source
    /// passability and require geographic qualification.
    pub fn landscape_reservations_at(&self, tile: TileCoord) -> Reservations {
        Reservations {
            route: landscape::starting_connector_contains(self, tile),
            start: landscape::starting_glade(self, tile),
            resource_approach: false,
            shore: false,
        }
    }

    pub fn landscape_chunk_with_cancel(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<LandscapeChunk, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        Ok(self
            .evaluate_landscape_chunk_with_cancel(
                x,
                y,
                &default_policy,
                &|position| self.landscape_reservations_at(position),
                cancelled,
            )?
            .unwrap_or_else(|| LandscapeChunk {
                x,
                y,
                tiles: Vec::new(),
                resources: Vec::new(),
                decorations: Vec::new(),
            }))
    }
}

/// Forest amount follows the source signal; support is full below the treeline.
fn default_policy(_: TileCoord, _: Tile) -> LandscapePolicy {
    LandscapePolicy {
        wildness_per_thousand: None,
        support_per_thousand: 1000,
    }
}

#[cfg(test)]
mod tests;
