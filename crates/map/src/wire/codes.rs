//! Bounded enum codes, little-endian primitives and hexadecimal transport.
use super::{CompactChunkError, MAX_DECODED_CHUNK_BYTES};
use crate::{
    Biome, GroundMaterial, ObjectKind, Provenance, ResourceKind, SurfaceDiagonal, SurfaceKind,
    WaterKind,
};
use crate::{
    DecorationFamily, EcologicalPalette, NativeExposure, NativeHeightBand, ResourceVisualFamily,
};

pub(super) fn ground_material(value: u8) -> Result<GroundMaterial, CompactChunkError> {
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

pub(super) fn biome(value: u8) -> Result<Biome, CompactChunkError> {
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

pub(super) fn provenance(value: u8) -> Result<Provenance, CompactChunkError> {
    match value {
        0 => Ok(Provenance::SourceDerived),
        1 => Ok(Provenance::ModelDerived),
        2 => Ok(Provenance::Procedural),
        3 => Ok(Provenance::Fallback),
        4 => Ok(Provenance::HistoricallyCorrected),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn water_kind(value: u8) -> Result<WaterKind, CompactChunkError> {
    match value {
        0 => Ok(WaterKind::None),
        1 => Ok(WaterKind::Shallow),
        2 => Ok(WaterKind::Lake),
        3 => Ok(WaterKind::River),
        4 => Ok(WaterKind::Ocean),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn surface_kind(value: u8) -> Result<SurfaceKind, CompactChunkError> {
    match value {
        0 => Ok(SurfaceKind::Plateau),
        1 => Ok(SurfaceKind::Ramp),
        2 => Ok(SurfaceKind::Cliff),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn diagonal(value: u8) -> Result<SurfaceDiagonal, CompactChunkError> {
    match value {
        0 => Ok(SurfaceDiagonal::NorthwestSoutheast),
        1 => Ok(SurfaceDiagonal::NortheastSouthwest),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn resource_kind(value: u8) -> Result<ResourceKind, CompactChunkError> {
    match value {
        0 => Ok(ResourceKind::Food),
        1 => Ok(ResourceKind::Wood),
        2 => Ok(ResourceKind::Gold),
        3 => Ok(ResourceKind::Stone),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn object_kind(value: u8) -> Result<ObjectKind, CompactChunkError> {
    match value {
        0 => Ok(ObjectKind::Tree),
        1 => Ok(ObjectKind::ForageBush),
        2 => Ok(ObjectKind::GoldDeposit),
        3 => Ok(ObjectKind::StoneDeposit),
        4 => Ok(ObjectKind::Decoration),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}

pub(super) fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn push_i16(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}
pub(super) fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, CompactChunkError> {
    let value = *bytes.get(*cursor).ok_or(CompactChunkError::InvalidLength)?;
    *cursor += 1;
    Ok(value)
}
pub(super) fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, CompactChunkError> {
    Ok(u16::from_le_bytes(read_array(bytes, cursor)?))
}
pub(super) fn read_i16(bytes: &[u8], cursor: &mut usize) -> Result<i16, CompactChunkError> {
    Ok(i16::from_le_bytes(read_array(bytes, cursor)?))
}
pub(super) fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, CompactChunkError> {
    Ok(u32::from_le_bytes(read_array(bytes, cursor)?))
}
pub(super) fn read_i32(bytes: &[u8], cursor: &mut usize) -> Result<i32, CompactChunkError> {
    Ok(i32::from_le_bytes(read_array(bytes, cursor)?))
}
pub(super) fn read_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, CompactChunkError> {
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

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push(HEX[(byte >> 4) as usize] as char);
        hex.push(HEX[(byte & 0xf) as usize] as char);
    }
    hex
}

pub(super) fn decode_hex(hex: &str) -> Result<Vec<u8>, CompactChunkError> {
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

pub(super) fn palette_from(value: u8) -> Result<EcologicalPalette, CompactChunkError> {
    match value {
        0 => Ok(EcologicalPalette::Temperate),
        1 => Ok(EcologicalPalette::Boreal),
        2 => Ok(EcologicalPalette::Tropical),
        3 => Ok(EcologicalPalette::DryScrub),
        4 => Ok(EcologicalPalette::Savanna),
        5 => Ok(EcologicalPalette::Treeless),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
pub(super) fn exposure_from(value: u8) -> Result<NativeExposure, CompactChunkError> {
    match value {
        0 => Ok(NativeExposure::Sheltered),
        1 => Ok(NativeExposure::Open),
        2 => Ok(NativeExposure::Exposed),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
pub(super) fn height_from(value: u8) -> Result<NativeHeightBand, CompactChunkError> {
    match value {
        0 => Ok(NativeHeightBand::Lowland),
        1 => Ok(NativeHeightBand::Montane),
        2 => Ok(NativeHeightBand::Subalpine),
        3 => Ok(NativeHeightBand::Alpine),
        4 => Ok(NativeHeightBand::Nival),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
pub(super) fn resource_family_from(value: u8) -> Result<ResourceVisualFamily, CompactChunkError> {
    match value {
        0 => Ok(ResourceVisualFamily::Generic),
        1 => Ok(ResourceVisualFamily::Broadleaf),
        2 => Ok(ResourceVisualFamily::Conifer),
        3 => Ok(ResourceVisualFamily::DryScrub),
        4 => Ok(ResourceVisualFamily::Tropical),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
pub(super) fn decoration_from(value: u8) -> Result<DecorationFamily, CompactChunkError> {
    match value {
        0 => Ok(DecorationFamily::Shrub),
        1 => Ok(DecorationFamily::Grass),
        2 => Ok(DecorationFamily::Stone),
        3 => Ok(DecorationFamily::Deadwood),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
