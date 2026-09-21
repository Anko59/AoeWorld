use crate::{
    Biome, CHUNK_TILES, Chunk, GroundMaterial, ObjectKind, Provenance, ResourceKind, ResourceNode,
    SurfaceDiagonal, SurfaceKind, Tile, TileSurface, WaterKind,
};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u8 = 1;
const TILE_BYTES: usize = 18;
const RESOURCE_BYTES: usize = 21;
const HEADER_BYTES: usize = 5;
pub const MAX_DECODED_CHUNK_BYTES: usize = 128 * 1024;
const MAX_CHUNK_TILES: usize = (CHUNK_TILES * CHUNK_TILES) as usize;

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
}

impl CompactChunk {
    pub fn encode(chunk: &Chunk) -> Result<Self, CompactChunkError> {
        if chunk.tiles.len() > MAX_CHUNK_TILES || chunk.tiles.len() > u16::MAX as usize {
            return Err(CompactChunkError::TooManyTiles);
        }
        if chunk.resources.len() > chunk.tiles.len() || chunk.resources.len() > u16::MAX as usize {
            return Err(CompactChunkError::TooManyResources);
        }
        let byte_len = HEADER_BYTES
            .checked_add(chunk.tiles.len().saturating_mul(TILE_BYTES))
            .and_then(|value| {
                value.checked_add(chunk.resources.len().saturating_mul(RESOURCE_BYTES))
            })
            .ok_or(CompactChunkError::PayloadTooLarge)?;
        if byte_len > MAX_DECODED_CHUNK_BYTES {
            return Err(CompactChunkError::PayloadTooLarge);
        }
        let mut payload = Vec::with_capacity(byte_len);
        payload.push(FORMAT_VERSION);
        push_u16(&mut payload, chunk.tiles.len() as u16);
        push_u16(&mut payload, chunk.resources.len() as u16);
        for tile in &chunk.tiles {
            push_i32(&mut payload, tile.geographic_height_centimeters);
            push_i16(&mut payload, tile.game_height_level);
            for height in tile.surface.corner_game_height_levels {
                push_i16(&mut payload, height);
            }
            push_u32(&mut payload, pack_tile_properties(*tile));
        }
        for resource in &chunk.resources {
            push_u64(&mut payload, resource.id);
            push_i32(&mut payload, resource.tile.x);
            push_i32(&mut payload, resource.tile.y);
            payload.push(resource.kind as u8);
            payload.push(resource.object as u8);
            push_u16(&mut payload, resource.initial_amount);
            payload.push(resource.visual_variant);
        }
        Ok(Self {
            x: chunk.x,
            y: chunk.y,
            payload_hex: encode_hex(&payload),
        })
    }

    pub fn decode(&self) -> Result<Chunk, CompactChunkError> {
        let payload = decode_hex(&self.payload_hex)?;
        if payload.len() > MAX_DECODED_CHUNK_BYTES {
            return Err(CompactChunkError::PayloadTooLarge);
        }
        let mut cursor = 0;
        if read_u8(&payload, &mut cursor)? != FORMAT_VERSION {
            return Err(CompactChunkError::UnsupportedVersion);
        }
        let tile_count = read_u16(&payload, &mut cursor)? as usize;
        let resource_count = read_u16(&payload, &mut cursor)? as usize;
        if tile_count > MAX_CHUNK_TILES {
            return Err(CompactChunkError::TooManyTiles);
        }
        if resource_count > tile_count {
            return Err(CompactChunkError::TooManyResources);
        }
        let expected = HEADER_BYTES
            .checked_add(tile_count.saturating_mul(TILE_BYTES))
            .and_then(|value| value.checked_add(resource_count.saturating_mul(RESOURCE_BYTES)))
            .ok_or(CompactChunkError::PayloadTooLarge)?;
        if expected != payload.len() {
            return Err(CompactChunkError::InvalidLength);
        }
        let mut tiles = Vec::with_capacity(tile_count);
        for _ in 0..tile_count {
            let geographic_height_centimeters = read_i32(&payload, &mut cursor)?;
            let game_height_level = read_i16(&payload, &mut cursor)?;
            let corner_game_height_levels = [
                read_i16(&payload, &mut cursor)?,
                read_i16(&payload, &mut cursor)?,
                read_i16(&payload, &mut cursor)?,
                read_i16(&payload, &mut cursor)?,
            ];
            tiles.push(unpack_tile(
                geographic_height_centimeters,
                game_height_level,
                corner_game_height_levels,
                read_u32(&payload, &mut cursor)?,
            )?);
        }
        let mut resources = Vec::with_capacity(resource_count);
        for _ in 0..resource_count {
            resources.push(ResourceNode {
                id: read_u64(&payload, &mut cursor)?,
                tile: TileCoord::new(
                    read_i32(&payload, &mut cursor)?,
                    read_i32(&payload, &mut cursor)?,
                ),
                kind: resource_kind(read_u8(&payload, &mut cursor)?)?,
                object: object_kind(read_u8(&payload, &mut cursor)?)?,
                initial_amount: read_u16(&payload, &mut cursor)?,
                visual_variant: read_u8(&payload, &mut cursor)?,
            });
        }
        (cursor == payload.len())
            .then_some(Chunk {
                x: self.x,
                y: self.y,
                tiles,
                resources,
            })
            .ok_or(CompactChunkError::InvalidLength)
    }

    pub fn decoded_len(&self) -> Result<usize, CompactChunkError> {
        Ok(self.payload_hex.len() / 2)
    }
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
) -> Result<Tile, CompactChunkError> {
    if properties >> 24 != 0 {
        return Err(CompactChunkError::InvalidEnum);
    }
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
        passable: (properties & (1 << 20)) != 0,
    })
}

fn ground_material(value: u8) -> Result<GroundMaterial, CompactChunkError> {
    match value {
        0 => Ok(GroundMaterial::TemperateGrass),
        1 => Ok(GroundMaterial::DryGrass),
        2 => Ok(GroundMaterial::LushGrass),
        3 => Ok(GroundMaterial::ForestFloor),
        4 => Ok(GroundMaterial::Dirt),
        5 => Ok(GroundMaterial::Sand),
        6 => Ok(GroundMaterial::Rock),
        7 => Ok(GroundMaterial::Mud),
        8 => Ok(GroundMaterial::Snow),
        9 => Ok(GroundMaterial::Ice),
        10 => Ok(GroundMaterial::Shore),
        11 => Ok(GroundMaterial::Water),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn biome(value: u8) -> Result<Biome, CompactChunkError> {
    match value {
        0 => Ok(Biome::Temperate),
        1 => Ok(Biome::Boreal),
        2 => Ok(Biome::Tropical),
        3 => Ok(Biome::Woodland),
        4 => Ok(Biome::Savanna),
        5 => Ok(Biome::Steppe),
        6 => Ok(Biome::Desert),
        7 => Ok(Biome::Tundra),
        8 => Ok(Biome::Alpine),
        9 => Ok(Biome::Polar),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn provenance(value: u8) -> Result<Provenance, CompactChunkError> {
    match value {
        0 => Ok(Provenance::SourceDerived),
        1 => Ok(Provenance::ModelDerived),
        2 => Ok(Provenance::Procedural),
        3 => Ok(Provenance::Fallback),
        4 => Ok(Provenance::HistoricallyCorrected),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn water_kind(value: u8) -> Result<WaterKind, CompactChunkError> {
    match value {
        0 => Ok(WaterKind::None),
        1 => Ok(WaterKind::Shallow),
        2 => Ok(WaterKind::Lake),
        3 => Ok(WaterKind::River),
        4 => Ok(WaterKind::Ocean),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn surface_kind(value: u8) -> Result<SurfaceKind, CompactChunkError> {
    match value {
        0 => Ok(SurfaceKind::Plateau),
        1 => Ok(SurfaceKind::Ramp),
        2 => Ok(SurfaceKind::Cliff),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn diagonal(value: u8) -> Result<SurfaceDiagonal, CompactChunkError> {
    match value {
        0 => Ok(SurfaceDiagonal::NorthwestSoutheast),
        1 => Ok(SurfaceDiagonal::NortheastSouthwest),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn resource_kind(value: u8) -> Result<ResourceKind, CompactChunkError> {
    match value {
        0 => Ok(ResourceKind::Food),
        1 => Ok(ResourceKind::Wood),
        2 => Ok(ResourceKind::Gold),
        3 => Ok(ResourceKind::Stone),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn object_kind(value: u8) -> Result<ObjectKind, CompactChunkError> {
    match value {
        0 => Ok(ObjectKind::Tree),
        1 => Ok(ObjectKind::ForageBush),
        2 => Ok(ObjectKind::GoldDeposit),
        3 => Ok(ObjectKind::StoneDeposit),
        4 => Ok(ObjectKind::Decoration),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn push_i16(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, CompactChunkError> {
    let value = *bytes.get(*cursor).ok_or(CompactChunkError::InvalidLength)?;
    *cursor += 1;
    Ok(value)
}
fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, CompactChunkError> {
    Ok(u16::from_le_bytes(read_array(bytes, cursor)?))
}
fn read_i16(bytes: &[u8], cursor: &mut usize) -> Result<i16, CompactChunkError> {
    Ok(i16::from_le_bytes(read_array(bytes, cursor)?))
}
fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, CompactChunkError> {
    Ok(u32::from_le_bytes(read_array(bytes, cursor)?))
}
fn read_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, CompactChunkError> {
    Ok(i32::from_le_bytes(read_array(bytes, cursor)?))
}
fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, CompactChunkError> {
    Ok(u64::from_le_bytes(read_array(bytes, cursor)?))
}

fn read_array<const N: usize>(
    bytes: &[u8],
    cursor: &mut usize,
) -> Result<[u8; N], CompactChunkError> {
    let end = cursor
        .checked_add(N)
        .ok_or(CompactChunkError::InvalidLength)?;
    let source = bytes
        .get(*cursor..end)
        .ok_or(CompactChunkError::InvalidLength)?;
    *cursor = end;
    source
        .try_into()
        .map_err(|_| CompactChunkError::InvalidLength)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0xf) as usize] as char);
    }
    hex
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, CompactChunkError> {
    if hex.len() > MAX_DECODED_CHUNK_BYTES * 2 {
        return Err(CompactChunkError::PayloadTooLarge);
    }
    if !hex.len().is_multiple_of(2) {
        return Err(CompactChunkError::InvalidHex);
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        bytes.push(high << 4 | low);
    }
    Ok(bytes)
}

fn hex_value(value: u8) -> Result<u8, CompactChunkError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(CompactChunkError::InvalidHex),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MapChunkGenerator;

    #[test]
    fn compact_chunk_round_trips_every_terrain_field() {
        let original = MapChunkGenerator::new([7; 32], 11, 64)
            .chunk(1, 1)
            .expect("fixture chunk");
        let compact = CompactChunk::encode(&original).expect("encodes a map chunk");
        assert!(compact.decoded_len().expect("decoded length") <= MAX_DECODED_CHUNK_BYTES);
        assert_eq!(compact.decode().expect("decodes a map chunk"), original);
    }

    #[test]
    fn maximum_chunk_has_a_bounded_payload() {
        let generator = MapChunkGenerator::new([3; 32], 5, CHUNK_TILES);
        let tile = generator.tile_at(TileCoord::new(0, 0)).expect("tile");
        let resource = ResourceNode {
            id: 0,
            tile: TileCoord::new(0, 0),
            kind: ResourceKind::Wood,
            object: ObjectKind::Tree,
            initial_amount: 100,
            visual_variant: 0,
        };
        let chunk = Chunk {
            x: 0,
            y: 0,
            tiles: vec![tile; MAX_CHUNK_TILES],
            resources: vec![resource; MAX_CHUNK_TILES],
        };
        let compact = CompactChunk::encode(&chunk).expect("encodes maximum chunk");
        assert!(compact.decoded_len().expect("decoded length") <= MAX_DECODED_CHUNK_BYTES);
        assert_eq!(compact.decode().expect("decodes maximum chunk"), chunk);
    }

    #[test]
    fn malformed_payload_is_rejected_before_decode() {
        let compact = CompactChunk {
            x: 0,
            y: 0,
            payload_hex: "zz".to_owned(),
        };
        assert_eq!(compact.decode(), Err(CompactChunkError::InvalidHex));
    }

    #[test]
    fn oversized_hex_is_rejected_before_allocation() {
        let compact = CompactChunk {
            x: 0,
            y: 0,
            payload_hex: "00".repeat(MAX_DECODED_CHUNK_BYTES + 1),
        };
        assert_eq!(compact.decode(), Err(CompactChunkError::PayloadTooLarge));
    }
}
