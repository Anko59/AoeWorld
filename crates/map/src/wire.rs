use crate::{
    CHUNK_TILES, EcologicalPalette, LandscapeAppearance, LandscapeChunk, LandscapeDecoration,
    LandscapeResource, LandscapeTile, ObjectKind, ResourceKind, ResourceNode, ResourceVisualFamily,
    Tile, TileSurface,
};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};

/// The only chunk payload version. Any other leading byte is rejected; cached
/// payloads from earlier development formats are discarded and regenerated.
pub const CHUNK_FORMAT_VERSION: u8 = 4;
pub const MAX_DECODED_CHUNK_BYTES: usize = 128 * 1024;
const HEADER_BYTES: usize = 7;
/// Physical terrain record before explicit coordinates and appearance.
const TERRAIN_BYTES: usize = 20;
/// Terrain 20 + world coordinates 8 + appearance 7.
const TILE_BYTES: usize = TERRAIN_BYTES + 8 + 7;
const RESOURCE_BYTES: usize = 22;
const DECORATION_BYTES: usize = 11;
const MAX_CHUNK_TILES: usize = (CHUNK_TILES * CHUNK_TILES) as usize;

mod codes;
mod evidence;
use codes::*;
use evidence::{pack_observation_properties, unpack_observation_properties};

/// Compact, immutable transport representation for one 32 by 32 map chunk.
///
/// The hex payload is a versioned, little-endian byte stream rather than an
/// unbounded JSON object per tile. It is deliberately decoded through this
/// type so clients and servers reject malformed enum values and overlarge
/// chunks before allocating terrain state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompactChunk {
    pub x: i32,
    pub y: i32,
    pub payload_hex: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CompactChunkError {
    #[error("chunk exceeds the 32 by 32 tile limit")]
    TooManyTiles,
    #[error("chunk has more than one object per tile")]
    TooManyResources,
    #[error("chunk has more than one decoration per tile")]
    TooManyDecorations,
    #[error("chunk payload exceeds the decoded size limit")]
    PayloadTooLarge,
    #[error("chunk payload is not valid hexadecimal")]
    InvalidHex,
    #[error("chunk payload has an unsupported format version")]
    UnsupportedVersion,
    #[error("chunk payload is truncated or has trailing bytes")]
    InvalidLength,
    #[error("chunk payload contains an invalid terrain enum")]
    InvalidEnum,
    #[error("invalid landscape metadata, coordinates or duplicate cell")]
    InvalidLandscape,
}

impl CompactChunk {
    pub fn encode(chunk: &LandscapeChunk) -> Result<Self, CompactChunkError> {
        let size = payload_size(
            chunk.tiles.len(),
            chunk.resources.len(),
            chunk.decorations.len(),
        )?;
        validate(chunk)?;
        let mut bytes = Vec::with_capacity(size);
        bytes.push(CHUNK_FORMAT_VERSION);
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
            let appearance = sample.appearance;
            push_u16(&mut bytes, appearance.canopy_strength);
            push_u16(&mut bytes, appearance.floor_strength);
            bytes.push(appearance.palette as u8);
            bytes.push(appearance.exposure as u8);
            bytes.push(appearance.height_band as u8);
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
        }
        Ok(Self {
            x: chunk.x,
            y: chunk.y,
            payload_hex: encode_hex(&bytes),
        })
    }

    pub fn decode(&self) -> Result<LandscapeChunk, CompactChunkError> {
        let bytes = decode_hex(&self.payload_hex)?;
        let mut cursor = 0;
        if read_u8(&bytes, &mut cursor)? != CHUNK_FORMAT_VERSION {
            return Err(CompactChunkError::UnsupportedVersion);
        }
        let tiles = usize::from(read_u16(&bytes, &mut cursor)?);
        let resources = usize::from(read_u16(&bytes, &mut cursor)?);
        let decorations = usize::from(read_u16(&bytes, &mut cursor)?);
        if payload_size(tiles, resources, decorations)? != bytes.len() {
            return Err(CompactChunkError::InvalidLength);
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
            let terrain = read_terrain(&bytes, &mut cursor)?;
            let tile = TileCoord::new(
                read_i32(&bytes, &mut cursor)?,
                read_i32(&bytes, &mut cursor)?,
            );
            let appearance = LandscapeAppearance {
                canopy_strength: read_u16(&bytes, &mut cursor)?,
                floor_strength: read_u16(&bytes, &mut cursor)?,
                palette: palette_from(read_u8(&bytes, &mut cursor)?)?,
                exposure: exposure_from(read_u8(&bytes, &mut cursor)?)?,
                height_band: height_from(read_u8(&bytes, &mut cursor)?)?,
            };
            chunk.tiles.push(LandscapeTile {
                tile,
                terrain,
                appearance,
            });
        }
        for _ in 0..resources {
            let node = read_resource_node(&bytes, &mut cursor)?;
            let visual_family = resource_family_from(read_u8(&bytes, &mut cursor)?)?;
            chunk.resources.push(LandscapeResource {
                node,
                visual_family,
            });
        }
        for _ in 0..decorations {
            chunk.decorations.push(LandscapeDecoration {
                tile: TileCoord::new(
                    read_i32(&bytes, &mut cursor)?,
                    read_i32(&bytes, &mut cursor)?,
                ),
                family: decoration_from(read_u8(&bytes, &mut cursor)?)?,
                variant: read_u8(&bytes, &mut cursor)?,
                orientation: read_u8(&bytes, &mut cursor)?,
            });
        }
        validate(&chunk)?;
        Ok(chunk)
    }

    pub fn decoded_len(&self) -> Result<usize, CompactChunkError> {
        Ok(self.payload_hex.len() / 2)
    }
}

fn payload_size(
    tiles: usize,
    resources: usize,
    decorations: usize,
) -> Result<usize, CompactChunkError> {
    if tiles > MAX_CHUNK_TILES {
        return Err(CompactChunkError::TooManyTiles);
    }
    if resources > tiles {
        return Err(CompactChunkError::TooManyResources);
    }
    if decorations > tiles {
        return Err(CompactChunkError::TooManyDecorations);
    }
    let size = HEADER_BYTES
        + tiles * TILE_BYTES
        + resources * RESOURCE_BYTES
        + decorations * DECORATION_BYTES;
    if size > MAX_DECODED_CHUNK_BYTES {
        return Err(CompactChunkError::PayloadTooLarge);
    }
    Ok(size)
}

fn read_terrain(bytes: &[u8], cursor: &mut usize) -> Result<Tile, CompactChunkError> {
    let height = read_i32(bytes, cursor)?;
    let level = read_i16(bytes, cursor)?;
    let corners = [
        read_i16(bytes, cursor)?,
        read_i16(bytes, cursor)?,
        read_i16(bytes, cursor)?,
        read_i16(bytes, cursor)?,
    ];
    let properties = read_u32(bytes, cursor)?;
    let observation = read_u16(bytes, cursor)?;
    unpack_tile(height, level, corners, properties, observation)
}

fn read_resource_node(bytes: &[u8], cursor: &mut usize) -> Result<ResourceNode, CompactChunkError> {
    Ok(ResourceNode {
        id: read_u64(bytes, cursor)?,
        tile: TileCoord::new(read_i32(bytes, cursor)?, read_i32(bytes, cursor)?),
        kind: resource_kind(read_u8(bytes, cursor)?)?,
        object: object_kind(read_u8(bytes, cursor)?)?,
        initial_amount: read_u16(bytes, cursor)?,
        visual_variant: read_u8(bytes, cursor)?,
    })
}

fn pack_tile_properties(tile: Tile) -> u32 {
    u32::from(tile.material as u8)
        | (u32::from(tile.biome as u8) << 4)
        | (u32::from(tile.vegetation_provenance as u8) << 8)
        | (u32::from(tile.water as u8) << 11)
        | (u32::from(tile.elevation_provenance as u8) << 14)
        | (u32::from(tile.water_provenance as u8) << 17)
        | (u32::from(tile.passable) << 20)
        | (u32::from(tile.surface.kind as u8) << 21)
        | (u32::from(tile.surface.triangulation as u8) << 23)
}

fn unpack_tile(
    geographic_height_centimeters: i32,
    game_height_level: i16,
    corner_game_height_levels: [i16; 4],
    properties: u32,
    observation_properties: u16,
) -> Result<Tile, CompactChunkError> {
    if properties >> 24 != 0 {
        return Err(CompactChunkError::InvalidEnum);
    }
    let (hydrology_observation, modern_land_cover_class) =
        unpack_observation_properties(observation_properties)?;
    Ok(Tile {
        geographic_height_centimeters,
        game_height_level,
        surface: TileSurface {
            corner_game_height_levels,
            kind: surface_kind(((properties >> 21) & 0x3) as u8)?,
            triangulation: diagonal(((properties >> 23) & 0x1) as u8)?,
        },
        material: ground_material((properties & 0xf) as u8)?,
        biome: biome(((properties >> 4) & 0xf) as u8)?,
        vegetation_provenance: provenance(((properties >> 8) & 0x7) as u8)?,
        water: water_kind(((properties >> 11) & 0x7) as u8)?,
        elevation_provenance: provenance(((properties >> 14) & 0x7) as u8)?,
        water_provenance: provenance(((properties >> 17) & 0x7) as u8)?,
        hydrology_observation,
        modern_land_cover_class,
        passable: (properties & (1 << 20)) != 0,
    })
}

fn origin(x: i32, y: i32) -> Result<TileCoord, CompactChunkError> {
    let x = x
        .checked_mul(CHUNK_TILES)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    let y = y
        .checked_mul(CHUNK_TILES)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    x.checked_add(CHUNK_TILES - 1)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    y.checked_add(CHUNK_TILES - 1)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    Ok(TileCoord::new(x, y))
}

fn cell(tile: TileCoord, origin: TileCoord) -> Result<usize, CompactChunkError> {
    let x = tile
        .x
        .checked_sub(origin.x)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    let y = tile
        .y
        .checked_sub(origin.y)
        .ok_or(CompactChunkError::InvalidLandscape)?;
    if !(0..CHUNK_TILES).contains(&x) || !(0..CHUNK_TILES).contains(&y) {
        return Err(CompactChunkError::InvalidLandscape);
    }
    Ok((y * CHUNK_TILES + x) as usize)
}

/// Explicit coordinates represent partial/sparse chunks in strict row-major
/// order; at most one resource and one decoration per present tile cell.
fn validate(chunk: &LandscapeChunk) -> Result<(), CompactChunkError> {
    let origin = origin(chunk.x, chunk.y)?;
    let mut tile_cells = [false; MAX_CHUNK_TILES];
    let mut previous = None;
    for tile in &chunk.tiles {
        let index = cell(tile.tile, origin)?;
        if previous.is_some_and(|last| index <= last) {
            return Err(CompactChunkError::InvalidLandscape);
        }
        previous = Some(index);
        tile_cells[index] = true;
        let a = tile.appearance;
        if a.canopy_strength > 1000
            || a.floor_strength != a.canopy_strength
            || (a.canopy_strength > 0
                && matches!(
                    a.palette,
                    EcologicalPalette::Savanna | EcologicalPalette::Treeless
                ))
        {
            return Err(CompactChunkError::InvalidLandscape);
        }
    }
    let mut resource_cells = [false; MAX_CHUNK_TILES];
    for resource in &chunk.resources {
        let index = cell(resource.node.tile, origin)?;
        if !tile_cells[index] || resource_cells[index] {
            return Err(CompactChunkError::InvalidLandscape);
        }
        resource_cells[index] = true;
        let node = resource.node;
        let tree = node.kind == ResourceKind::Wood && node.object == ObjectKind::Tree;
        if tree == (resource.visual_family == ResourceVisualFamily::Generic) {
            return Err(CompactChunkError::InvalidLandscape);
        }
    }
    let mut decoration_cells = [false; MAX_CHUNK_TILES];
    for decoration in &chunk.decorations {
        let index = cell(decoration.tile, origin)?;
        if !tile_cells[index] || decoration_cells[index] || decoration.orientation > 7 {
            return Err(CompactChunkError::InvalidLandscape);
        }
        decoration_cells[index] = true;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
