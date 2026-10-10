//! Opt-in codec 3 precursor. Published encode/decode and bindings stay unchanged.
use super::*;
use crate::terrain::{
    DecorationFamily, EcologicalPalette, LandscapeAppearance, LandscapeChunk, LandscapeDecoration,
    LandscapeResource, LandscapeTile, NativeExposure, NativeHeightBand, ResourceVisualFamily,
};

const VERSION: u8 = 3;
const HEADER: usize = 7;
// Full terrain base20 + explicit world coordinates8 + bounded metadata9.
const TILE: usize = 37;
const RESOURCE: usize = 22;
const DECORATION: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LandscapeChunkError {
    #[error(transparent)]
    Terrain(#[from] CompactChunkError),
    #[error("invalid landscape metadata, coordinates or duplicate cell")]
    InvalidLandscape,
    #[error("chunk has more than one decoration per tile")]
    TooManyDecorations,
}

impl CompactChunk {
    /// Explicit codec 3 encoding; this does not activate a map profile or recipe.
    pub fn encode_landscape(chunk: &LandscapeChunk) -> Result<Self, LandscapeChunkError> {
        let size = payload_size(
            chunk.tiles.len(),
            chunk.resources.len(),
            chunk.decorations.len(),
        )?;
        validate(chunk)?;
        let mut bytes = Vec::with_capacity(size);
        bytes.push(VERSION);
        push_u16(&mut bytes, chunk.tiles.len() as u16);
        push_u16(&mut bytes, chunk.resources.len() as u16);
        push_u16(&mut bytes, chunk.decorations.len() as u16);
        for sample in &chunk.tiles {
            let tile = sample.terrain;
            push_i32(&mut bytes, tile.geographic_height_centimeters);
            push_i16(&mut bytes, tile.game_height_level);
            for height in tile.surface.corner_game_height_levels {
                push_i16(&mut bytes, height);
            }
            push_u32(&mut bytes, pack_tile_properties(tile));
            push_u16(&mut bytes, pack_observation_properties(tile)?);
            push_i32(&mut bytes, sample.tile.x);
            push_i32(&mut bytes, sample.tile.y);
            if let Some(appearance) = sample.appearance {
                bytes.push(1);
                push_u16(&mut bytes, appearance.canopy_strength);
                push_u16(&mut bytes, appearance.floor_strength);
                bytes.push(appearance.palette as u8);
                bytes.push(appearance.exposure as u8);
                bytes.push(appearance.height_band as u8);
                bytes.push(0);
            } else {
                bytes.extend_from_slice(&[0; 9]);
            }
        }
        for resource in &chunk.resources {
            let node = resource.node;
            push_u64(&mut bytes, node.id);
            push_i32(&mut bytes, node.tile.x);
            push_i32(&mut bytes, node.tile.y);
            bytes.push(node.kind as u8);
            bytes.push(node.object as u8);
            push_u16(&mut bytes, node.initial_amount);
            bytes.push(node.visual_variant);
            bytes.push(resource.visual_family as u8);
        }
        for decoration in &chunk.decorations {
            push_i32(&mut bytes, decoration.tile.x);
            push_i32(&mut bytes, decoration.tile.y);
            bytes.push(decoration.family as u8);
            bytes.push(decoration.variant);
            bytes.push(decoration.orientation);
            bytes.push(0);
        }
        Ok(Self {
            x: chunk.x,
            y: chunk.y,
            payload_hex: encode_hex(&bytes),
        })
    }

    /// Versions 1/2 retain the exact existing decoder semantics, then project
    /// absent appearance, Legacy resource families and no decorations.
    pub fn decode_landscape(&self) -> Result<LandscapeChunk, LandscapeChunkError> {
        if self.payload_hex.starts_with("01") || self.payload_hex.starts_with("02") {
            let chunk = self.decode()?;
            return Ok(LandscapeChunk {
                x: chunk.x,
                y: chunk.y,
                // Preserve old acceptance even for overflowing legacy coordinates.
                // These implied coordinates do not repair old partial-edge geometry.
                tiles: chunk
                    .tiles
                    .into_iter()
                    .enumerate()
                    .map(|(index, terrain)| LandscapeTile {
                        tile: TileCoord::new(
                            chunk
                                .x
                                .wrapping_mul(CHUNK_TILES)
                                .wrapping_add((index % 32) as i32),
                            chunk
                                .y
                                .wrapping_mul(CHUNK_TILES)
                                .wrapping_add((index / 32) as i32),
                        ),
                        terrain,
                        appearance: None,
                    })
                    .collect(),
                resources: chunk
                    .resources
                    .into_iter()
                    .map(|node| LandscapeResource {
                        node,
                        visual_family: ResourceVisualFamily::Legacy,
                    })
                    .collect(),
                decorations: Vec::new(),
            });
        }
        self.decode_landscape_v3()
    }

    /// Strict schema-10 reader. Separate entry point keeps legacy projection
    /// allocations/code out of clients that retain their original legacy cache.
    pub fn decode_landscape_v3(&self) -> Result<LandscapeChunk, LandscapeChunkError> {
        let bytes = decode_hex(&self.payload_hex)?;
        let mut cursor = 0;
        let version = read_u8(&bytes, &mut cursor)?;
        if version != VERSION {
            return Err(CompactChunkError::UnsupportedVersion.into());
        }
        let tiles = usize::from(read_u16(&bytes, &mut cursor)?);
        let resources = usize::from(read_u16(&bytes, &mut cursor)?);
        let decorations = usize::from(read_u16(&bytes, &mut cursor)?);
        if payload_size(tiles, resources, decorations)? != bytes.len() {
            return Err(CompactChunkError::InvalidLength.into());
        }
        // Validate the complete coordinate domain before terrain allocations.
        origin(self.x, self.y)?;
        let mut chunk = LandscapeChunk {
            x: self.x,
            y: self.y,
            tiles: Vec::with_capacity(tiles),
            resources: Vec::with_capacity(resources),
            decorations: Vec::with_capacity(decorations),
        };
        for _ in 0..tiles {
            let terrain = read_base_tile(&bytes, &mut cursor, true)?;
            let tile = TileCoord::new(
                read_i32(&bytes, &mut cursor)?,
                read_i32(&bytes, &mut cursor)?,
            );
            let present = read_u8(&bytes, &mut cursor)?;
            let canopy_strength = read_u16(&bytes, &mut cursor)?;
            let floor_strength = read_u16(&bytes, &mut cursor)?;
            let palette = read_u8(&bytes, &mut cursor)?;
            let exposure = read_u8(&bytes, &mut cursor)?;
            let height_band = read_u8(&bytes, &mut cursor)?;
            let reserved = read_u8(&bytes, &mut cursor)?;
            let appearance = match present {
                0 if canopy_strength == 0
                    && floor_strength == 0
                    && palette == 0
                    && exposure == 0
                    && height_band == 0
                    && reserved == 0 =>
                {
                    None
                }
                1 if reserved == 0 => Some(LandscapeAppearance {
                    canopy_strength,
                    floor_strength,
                    palette: palette_from(palette)?,
                    exposure: exposure_from(exposure)?,
                    height_band: height_from(height_band)?,
                }),
                _ => return Err(LandscapeChunkError::InvalidLandscape),
            };
            chunk.tiles.push(LandscapeTile {
                tile,
                terrain,
                appearance,
            });
        }
        for _ in 0..resources {
            let node = read_resource_node(&bytes, &mut cursor)?;
            let visual_family = resource_from(read_u8(&bytes, &mut cursor)?)?;
            chunk.resources.push(LandscapeResource {
                node,
                visual_family,
            });
        }
        for _ in 0..decorations {
            let decoration = LandscapeDecoration {
                tile: TileCoord::new(
                    read_i32(&bytes, &mut cursor)?,
                    read_i32(&bytes, &mut cursor)?,
                ),
                family: decoration_from(read_u8(&bytes, &mut cursor)?)?,
                variant: read_u8(&bytes, &mut cursor)?,
                orientation: read_u8(&bytes, &mut cursor)?,
            };
            if read_u8(&bytes, &mut cursor)? != 0 {
                return Err(LandscapeChunkError::InvalidLandscape);
            }
            chunk.decorations.push(decoration);
        }
        validate(&chunk)?;
        Ok(chunk)
    }
}

fn payload_size(
    tiles: usize,
    resources: usize,
    decorations: usize,
) -> Result<usize, LandscapeChunkError> {
    if tiles > MAX_CHUNK_TILES {
        return Err(CompactChunkError::TooManyTiles.into());
    }
    if resources > tiles {
        return Err(CompactChunkError::TooManyResources.into());
    }
    if decorations > tiles || decorations > MAX_CHUNK_TILES {
        return Err(LandscapeChunkError::TooManyDecorations);
    }
    let size = HEADER + tiles * TILE + resources * RESOURCE + decorations * DECORATION;
    if size > MAX_DECODED_CHUNK_BYTES {
        return Err(CompactChunkError::PayloadTooLarge.into());
    }
    Ok(size)
}

fn origin(x: i32, y: i32) -> Result<TileCoord, LandscapeChunkError> {
    let x = x
        .checked_mul(CHUNK_TILES)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    let y = y
        .checked_mul(CHUNK_TILES)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    x.checked_add(CHUNK_TILES - 1)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    y.checked_add(CHUNK_TILES - 1)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    Ok(TileCoord::new(x, y))
}

fn cell(tile: TileCoord, origin: TileCoord) -> Result<usize, LandscapeChunkError> {
    let x = tile
        .x
        .checked_sub(origin.x)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    let y = tile
        .y
        .checked_sub(origin.y)
        .ok_or(LandscapeChunkError::InvalidLandscape)?;
    if !(0..CHUNK_TILES).contains(&x) || !(0..CHUNK_TILES).contains(&y) {
        return Err(LandscapeChunkError::InvalidLandscape);
    }
    Ok((y * CHUNK_TILES + x) as usize)
}

fn validate(chunk: &LandscapeChunk) -> Result<(), LandscapeChunkError> {
    let origin = origin(chunk.x, chunk.y)?;
    let mut tile_cells = [false; MAX_CHUNK_TILES];
    let mut previous = None;
    for tile in &chunk.tiles {
        let index = cell(tile.tile, origin)?;
        if previous.is_some_and(|last| index <= last) {
            return Err(LandscapeChunkError::InvalidLandscape);
        }
        previous = Some(index);
        tile_cells[index] = true;
        if let Some(a) = tile.appearance
            && (a.canopy_strength > 1000
                || a.floor_strength != a.canopy_strength
                || (a.canopy_strength > 0
                    && matches!(
                        a.palette,
                        EcologicalPalette::Savanna | EcologicalPalette::Treeless
                    )))
        {
            return Err(LandscapeChunkError::InvalidLandscape);
        }
    }
    let mut resource_cells = [false; MAX_CHUNK_TILES];
    for resource in &chunk.resources {
        let index = cell(resource.node.tile, origin)?;
        if !tile_cells[index] || resource_cells[index] {
            return Err(LandscapeChunkError::InvalidLandscape);
        }
        resource_cells[index] = true;
        let node = resource.node;
        if resource.visual_family != ResourceVisualFamily::Legacy
            && (node.kind != ResourceKind::Wood || node.object != ObjectKind::Tree)
        {
            return Err(LandscapeChunkError::InvalidLandscape);
        }
    }
    let mut decoration_cells = [false; MAX_CHUNK_TILES];
    for decoration in &chunk.decorations {
        let index = cell(decoration.tile, origin)?;
        if !tile_cells[index] || decoration_cells[index] || decoration.orientation > 7 {
            return Err(LandscapeChunkError::InvalidLandscape);
        }
        decoration_cells[index] = true;
    }
    Ok(())
}

fn palette_from(value: u8) -> Result<EcologicalPalette, LandscapeChunkError> {
    match value {
        0 => Ok(EcologicalPalette::Temperate),
        1 => Ok(EcologicalPalette::Boreal),
        2 => Ok(EcologicalPalette::Tropical),
        3 => Ok(EcologicalPalette::DryScrub),
        4 => Ok(EcologicalPalette::Savanna),
        5 => Ok(EcologicalPalette::Treeless),
        _ => Err(LandscapeChunkError::InvalidLandscape),
    }
}
fn exposure_from(value: u8) -> Result<NativeExposure, LandscapeChunkError> {
    match value {
        0 => Ok(NativeExposure::Sheltered),
        1 => Ok(NativeExposure::Open),
        2 => Ok(NativeExposure::Exposed),
        _ => Err(LandscapeChunkError::InvalidLandscape),
    }
}
fn height_from(value: u8) -> Result<NativeHeightBand, LandscapeChunkError> {
    match value {
        0 => Ok(NativeHeightBand::Lowland),
        1 => Ok(NativeHeightBand::Montane),
        2 => Ok(NativeHeightBand::Subalpine),
        3 => Ok(NativeHeightBand::Alpine),
        4 => Ok(NativeHeightBand::Nival),
        _ => Err(LandscapeChunkError::InvalidLandscape),
    }
}
fn resource_from(value: u8) -> Result<ResourceVisualFamily, LandscapeChunkError> {
    match value {
        0 => Ok(ResourceVisualFamily::Legacy),
        1 => Ok(ResourceVisualFamily::Broadleaf),
        2 => Ok(ResourceVisualFamily::Conifer),
        3 => Ok(ResourceVisualFamily::DryScrub),
        4 => Ok(ResourceVisualFamily::Tropical),
        _ => Err(LandscapeChunkError::InvalidLandscape),
    }
}
fn decoration_from(value: u8) -> Result<DecorationFamily, LandscapeChunkError> {
    match value {
        0 => Ok(DecorationFamily::Shrub),
        1 => Ok(DecorationFamily::Grass),
        2 => Ok(DecorationFamily::Stone),
        3 => Ok(DecorationFamily::Deadwood),
        _ => Err(LandscapeChunkError::InvalidLandscape),
    }
}

#[cfg(test)]
mod tests;
