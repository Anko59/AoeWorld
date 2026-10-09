use super::*;
use aoe_core::TileCoord;
use aoe_map::{
    EcologicalPalette, LandscapeAppearance, LandscapeChunk, LandscapeTile, NativeExposure,
    NativeHeightBand,
};

pub(super) fn prepare(root: &Path, seeds: &mut Vec<Seed>) -> Result<()> {
    let terrain = MapChunkGenerator::new([23; 32], 1, 128)
        .tile_at(TileCoord::new(0, 0))
        .ok_or("missing fixture tile")?;
    let scene = LandscapeChunk {
        x: 0,
        y: 0,
        tiles: vec![LandscapeTile {
            tile: TileCoord::new(0, 0),
            terrain,
            appearance: Some(LandscapeAppearance {
                canopy_strength: 0,
                floor_strength: 0,
                palette: EcologicalPalette::Temperate,
                exposure: NativeExposure::Open,
                height_band: NativeHeightBand::Lowland,
            }),
        }],
        resources: Vec::new(),
        decorations: Vec::new(),
    };
    let encoded = CompactChunk::encode_landscape(&scene)?;
    // map_chunk feeds ASCII directly into payload_hex: do not use binary seed
    // bytes here. Retain all older corpus bytes/names without rewriting them.
    seeds.push(write(
        root,
        "map_chunk",
        "landscape-v3-hex",
        encoded.payload_hex.as_bytes(),
    )?);
    let legacy = aoe_map::Chunk {
        x: 0,
        y: 0,
        tiles: vec![terrain],
        resources: Vec::new(),
    };
    seeds.push(write(
        root,
        "map_chunk",
        "legacy-v2-hex",
        CompactChunk::encode(&legacy)?.payload_hex.as_bytes(),
    )?);
    // Exercise the strict schema-10 branch with actual JSON, while all earlier
    // prepared/discovered corpus names and bytes remain retained unchanged.
    let request = MapRequest {
        detail_profile: aoe_map::DetailProfile::LandscapeV2,
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new())?;
    package.validate()?;
    seeds.push(write(
        root,
        "map_package",
        "landscape-schema10-package",
        &serde_json::to_vec(&package)?,
    )?);
    seeds.push(write(
        root,
        "map_package",
        "landscape-v2-request",
        &serde_json::to_vec(&request)?,
    )?);
    Ok(())
}

#[cfg(test)]
mod tests;
