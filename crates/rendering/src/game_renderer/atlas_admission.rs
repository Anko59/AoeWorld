//! Construction proof is separate from mutable art metadata and raw image uploads.
use crate::{AtlasAddress, GameArt, TerrainTopology};
use aoe_assets::catalog::{TerrainFrameTopology, packing::terrain_lookup::ConstructedTerrainAtlas};

/// Canvas pixels and admission cannot be separated through the public enum variant.
/// Raw construction never admits lookup, even when the bytes contain a valid row.
#[derive(Default)]
pub struct CanvasAtlas {
    pixels: Vec<u8>,
    pub(super) world: Option<u32>,
}
impl CanvasAtlas {
    pub fn raw(pixels: Vec<u8>) -> Self {
        Self {
            pixels,
            world: None,
        }
    }
}
impl std::ops::Deref for CanvasAtlas {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.pixels
    }
}

impl crate::Renderer {
    pub fn upload_terrain_atlas(
        &mut self,
        atlas: &ConstructedTerrainAtlas,
        art: &GameArt,
    ) -> Result<(), String> {
        let key = validate_art(atlas, art)?;
        self.upload_game_atlas(atlas.pixels())?;
        self.world_atlas = Some(key);
        Ok(())
    }
}
#[cfg(test)]
impl super::webgl::WebGlRenderer {
    pub(super) fn upload_terrain_atlas(
        &mut self,
        atlas: &ConstructedTerrainAtlas,
        art: &GameArt,
    ) -> Result<(), String> {
        let key = validate_art(atlas, art)?;
        self.upload(atlas.pixels())?;
        self.world_atlas = Some(key);
        Ok(())
    }
}
impl super::GameRenderer {
    /// Admit a fully constructed immutable layout and its current art snapshot.
    /// Art mutation requires readmission; this is not source-art authentication.
    pub fn upload_terrain_atlas(
        &mut self,
        atlas: &ConstructedTerrainAtlas,
        art: &GameArt,
    ) -> Result<(), String> {
        let key = validate_art(atlas, art)?;
        self.upload_game_atlas(atlas.pixels())?;
        match self {
            Self::WebGpu(renderer) => renderer.world_atlas = Some(key),
            Self::WebGl(renderer) => renderer.world_atlas = Some(key),
            Self::Canvas { source_atlas, .. } => source_atlas.world = Some(key),
        }
        Ok(())
    }
    pub(crate) fn world_atlas(&self) -> Option<u32> {
        match self {
            Self::WebGpu(renderer) => renderer.world_atlas,
            Self::WebGl(renderer) => renderer.world_atlas,
            Self::Canvas { source_atlas, .. } => source_atlas.world,
        }
    }
}

/// Validate the entire art/layout association once, before touching backend state.
/// The owned constructor already validated row bytes, bounds, gutters and overlap;
/// this immutable view does not link or run the untrusted-row decoder.
pub(crate) fn validate_art(atlas: &ConstructedTerrainAtlas, art: &GameArt) -> Result<u32, String> {
    let table = atlas.table();
    let metadata = table.metadata();
    let invalid = || "Terrain atlas and art layout do not match".to_owned();
    if art.terrain_world != Some(metadata.layout_checksum) {
        return Err(invalid());
    }
    for (slot, group) in metadata.groups.iter().enumerate() {
        let topology = group.topology.map(|topology| match topology {
            TerrainFrameTopology::PeriodicXMajorReversedY { columns, rows } => {
                TerrainTopology::PeriodicXMajorReversedY { columns, rows }
            }
            TerrainFrameTopology::CoordinateStableAccents => {
                TerrainTopology::CoordinateStableAccents
            }
        });
        if art.terrain_topology[slot] != topology
            || art.terrain[slot].len() != usize::from(group.count)
        {
            return Err(invalid());
        }
        for (index, frame) in art.terrain[slot].iter().enumerate() {
            let descriptor = table
                .descriptor(group.base + index as u16)
                .map_err(|_| invalid())?;
            let expected = AtlasAddress {
                page: u32::from(descriptor.page),
                uv: [
                    descriptor.x,
                    descriptor.y,
                    descriptor.width,
                    descriptor.height,
                ]
                .map(|value| f32::from(value) / crate::GAME_ATLAS_SIDE as f32),
            };
            if frame.atlas != expected {
                return Err(invalid());
            }
        }
    }
    Ok(metadata.layout_checksum)
}
