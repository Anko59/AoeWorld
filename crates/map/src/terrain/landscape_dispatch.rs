//! Authoritative composed-landscape dispatch. Reservations are procedural
//! geometry, not proof that source water/cliffs admit an exit; no bridges are
//! synthesized.
use super::*;
use crate::landscape_ecology::Reservations;
use crate::landscape_patches::Region;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LandscapePoint {
    pub tile: LandscapeTile,
    pub resource: Option<LandscapeResource>,
    pub decoration: Option<LandscapeDecoration>,
}

impl MapChunkGenerator {
    /// Physical terrain only. Composition changes material, not
    /// passability, surface or height; overlays remain a caller concern.
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

    /// Shared start/opening/route reservations. These clear canopy, floor, trees
    /// and dressing together before filtering. Resource approaches are added by
    /// the composer, from base terrain only. Actual physical exits remain subject
    /// to source passability and require geographic qualification.
    pub fn landscape_reservations_at(&self, tile: TileCoord) -> Reservations {
        Reservations {
            route: landscape::procedural_trail_contains(self, tile),
            start: landscape::opening_contains(self, tile),
            resource_approach: false,
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
        let ox = x
            .checked_mul(CHUNK_TILES)
            .ok_or(EnvironmentPageError::Invalid)?;
        let oy = y
            .checked_mul(CHUNK_TILES)
            .ok_or(EnvironmentPageError::Invalid)?;
        let mut chunk = LandscapeChunk {
            x,
            y,
            tiles: Vec::with_capacity(1024),
            resources: Vec::new(),
            decorations: Vec::new(),
        };
        for ly in 0..CHUNK_TILES {
            for lx in 0..CHUNK_TILES {
                let tile = TileCoord::new(
                    ox.checked_add(lx).ok_or(EnvironmentPageError::Invalid)?,
                    oy.checked_add(ly).ok_or(EnvironmentPageError::Invalid)?,
                );
                if let Some(point) = self.landscape_point_with_cancel(tile, cancelled)? {
                    chunk.tiles.push(point.tile);
                    if let Some(resource) = point.resource {
                        chunk.resources.push(resource);
                    }
                    if let Some(decoration) = point.decoration {
                        chunk.decorations.push(decoration);
                    }
                }
            }
        }
        Ok(chunk)
    }
}

fn default_policy(_: TileCoord, base: Tile) -> LandscapePolicy {
    let region = match base.biome {
        Biome::Temperate => Region::Moderate,
        Biome::Boreal => Region::Heavy,
        Biome::Tropical => Region::Exceptional,
        _ => Region::Sparse,
    };
    LandscapePolicy {
        region,
        support_per_thousand: 1000,
    }
}

#[cfg(test)]
mod tests;
