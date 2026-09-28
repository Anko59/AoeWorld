use super::CompactChunkError;
use crate::{HydrologyObservation, Tile};

pub(super) fn pack_observation_properties(tile: Tile) -> Result<u16, CompactChunkError> {
    let kind = if let Some(observation) = tile.hydrology_observation {
        observation
            .validate()
            .map_err(|_| CompactChunkError::InvalidEnum)?;
        observation.kind as u8 + 1
    } else {
        0
    };
    let method = tile
        .hydrology_observation
        .map(|observation| observation.method as u8)
        .unwrap_or(0);
    let cover = world_cover_code(tile.modern_land_cover_class)?;
    Ok((u16::from(kind) << 12) | (u16::from(method) << 9) | u16::from(cover))
}

pub(super) fn unpack_observation_properties(
    properties: u16,
) -> Result<(Option<HydrologyObservation>, Option<u8>), CompactChunkError> {
    if properties & 0x01f0 != 0 {
        return Err(CompactChunkError::InvalidEnum);
    }
    let kind = ((properties >> 12) & 0xf) as u8;
    let method = ((properties >> 9) & 0x7) as u8;
    let cover = world_cover_class((properties & 0xf) as u8)?;
    let observation = if kind == 0 {
        if method != 0 {
            return Err(CompactChunkError::InvalidEnum);
        }
        None
    } else {
        let observation = HydrologyObservation {
            kind: (kind - 1)
                .try_into()
                .map_err(|_| CompactChunkError::InvalidEnum)?,
            method: method
                .try_into()
                .map_err(|_| CompactChunkError::InvalidEnum)?,
        };
        observation
            .validate()
            .map_err(|_| CompactChunkError::InvalidEnum)?;
        Some(observation)
    };
    Ok((observation, cover))
}

fn world_cover_code(class: Option<u8>) -> Result<u8, CompactChunkError> {
    match class {
        None => Ok(0),
        Some(0) => Ok(1),
        Some(10) => Ok(2),
        Some(20) => Ok(3),
        Some(30) => Ok(4),
        Some(40) => Ok(5),
        Some(50) => Ok(6),
        Some(60) => Ok(7),
        Some(70) => Ok(8),
        Some(80) => Ok(9),
        Some(90) => Ok(10),
        Some(95) => Ok(11),
        Some(100) => Ok(12),
        Some(_) => Err(CompactChunkError::InvalidEnum),
    }
}

fn world_cover_class(code: u8) -> Result<Option<u8>, CompactChunkError> {
    match code {
        0 => Ok(None),
        1 => Ok(Some(0)),
        2 => Ok(Some(10)),
        3 => Ok(Some(20)),
        4 => Ok(Some(30)),
        5 => Ok(Some(40)),
        6 => Ok(Some(50)),
        7 => Ok(Some(60)),
        8 => Ok(Some(70)),
        9 => Ok(Some(80)),
        10 => Ok(Some(90)),
        11 => Ok(Some(95)),
        12 => Ok(Some(100)),
        _ => Err(CompactChunkError::InvalidEnum),
    }
}
