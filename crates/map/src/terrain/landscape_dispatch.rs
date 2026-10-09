//! Recipe-nine authoritative dispatch. Reservations are procedural geometry,
//! not proof that source water/cliffs admit an exit; no bridges are synthesized.
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
    pub(super) fn uses_landscape_v2(&self) -> bool {
        self.generation_recipe_version() == crate::LANDSCAPE_GENERATION_RECIPE_VERSION
    }

    /// Recipe-nine physical terrain only. Composition changes material, not
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

    /// Terrain and immutable node from one composition for recipe nine.
    /// Legacy recipes retain the ordered tile query followed by object query.
    /// Failed queries are returned directly and never cached here.
    pub fn tile_and_node_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<(Tile, Option<ResourceNode>)>, EnvironmentPageError> {
        if self.uses_landscape_v2() {
            return self
                .landscape_point_with_cancel(tile, cancelled)
                .map(|point| {
                    point.map(|point| {
                        (
                            point.tile.terrain,
                            point.resource.map(|resource| resource.node),
                        )
                    })
                });
        }
        let Some(sample) = self.tile_at_with_cancel(tile, cancelled)? else {
            return Ok(None);
        };
        let node = self.object_at_with_cancel(tile, cancelled)?;
        Ok(Some((sample, node)))
    }

    /// One bounded shared source evaluation; trees/resources and terrain are the
    /// same objects returned by the typed chunk path. All page errors propagate.
    pub fn landscape_point_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<LandscapePoint>, EnvironmentPageError> {
        if !self.uses_landscape_v2() {
            return Err(EnvironmentPageError::Invalid);
        }
        self.evaluate_landscape_point_with_cancel(
            tile,
            &default_policy,
            &|position| self.landscape_reservations_at(position),
            cancelled,
        )
    }

    pub(super) fn landscape_tile_required(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Tile, EnvironmentPageError> {
        self.landscape_point_with_cancel(tile, cancelled)?
            .map(|point| point.tile.terrain)
            .ok_or(EnvironmentPageError::Invalid)
    }

    pub(super) fn landscape_node_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<ResourceNode>, EnvironmentPageError> {
        Ok(self
            .landscape_point_with_cancel(tile, cancelled)?
            .and_then(|point| point.resource.map(|resource| resource.node)))
    }

    pub(super) fn landscape_occupied_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        Ok(self
            .landscape_point_with_cancel(tile, cancelled)?
            .is_some_and(|point| !point.tile.terrain.passable || point.resource.is_some()))
    }

    /// Shared start/opening/route reservations. These clear canopy, floor, trees
    /// and dressing together before filtering. Resource approaches are added by
    /// the composer, from base terrain only. Actual physical exits remain subject
    /// to source passability and require geographic qualification.
    pub fn landscape_reservations_at(&self, tile: TileCoord) -> Reservations {
        if !self.uses_landscape_v2() {
            return Reservations::default();
        }
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
        if !self.uses_landscape_v2() {
            return Err(EnvironmentPageError::Invalid);
        }
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

    // Byte/semantic-compatible recipes 3..8 loop, moved without changing order,
    // bounds behavior, sampling or source-error propagation.
    pub(super) fn legacy_chunk_with_cancel(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Chunk, EnvironmentPageError> {
        let mut tiles = Vec::with_capacity((CHUNK_TILES * CHUNK_TILES) as usize);
        let mut resources = Vec::new();
        for local_y in 0..CHUNK_TILES {
            for local_x in 0..CHUNK_TILES {
                let tile = TileCoord::new(x * CHUNK_TILES + local_x, y * CHUNK_TILES + local_y);
                let Some(sample) = self.tile_at_with_cancel(tile, cancelled)? else {
                    continue;
                };
                tiles.push(sample);
                let node = if self.provider.is_some() {
                    provider::resource_at(self, tile, sample, cancelled)?
                } else {
                    self.resource_at(tile, sample)
                };
                if let Some(node) = node {
                    resources.push(node);
                }
            }
        }
        Ok(Chunk {
            x,
            y,
            tiles,
            resources,
        })
    }

    pub(super) fn landscape_legacy_chunk(
        &self,
        x: i32,
        y: i32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Chunk, EnvironmentPageError> {
        let scene = self.landscape_chunk_with_cancel(x, y, cancelled)?;
        Ok(Chunk {
            x,
            y,
            tiles: scene.tiles.into_iter().map(|tile| tile.terrain).collect(),
            resources: scene
                .resources
                .into_iter()
                .map(|resource| resource.node)
                .collect(),
        })
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
