//! Chunk seeds are ASCII hex: `map_chunk` feeds fuzz bytes into `payload_hex`.
use super::{Result, Seed, write};
use aoe_core::TileCoord;
use aoe_map::{
    CompactChunk, HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation, LandscapeChunk,
    MapChunkGenerator,
};
use std::path::Path;

pub(super) fn prepare(root: &Path) -> Result<Vec<Seed>> {
    let generator = MapChunkGenerator::new([3; 32], 7, 32);
    let full = generator.landscape_chunk_with_cancel(0, 0, &|| false)?;
    let mut typed = LandscapeChunk {
        tiles: full.tiles[..1].to_vec(),
        resources: Vec::new(),
        decorations: Vec::new(),
        ..full.clone()
    };
    typed.tiles[0].tile = TileCoord::new(0, 0);
    typed.tiles[0].terrain.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::Land,
        method: HydrologyEvidenceMethod::WorldCoverClass,
    });
    typed.tiles[0].terrain.modern_land_cover_class = Some(10);
    let empty = LandscapeChunk {
        tiles: Vec::new(),
        resources: Vec::new(),
        decorations: Vec::new(),
        ..full.clone()
    };
    let mut seeds = Vec::new();
    for (name, chunk) in [
        ("full-chunk-hex", &full),
        ("typed-tile-hex", &typed),
        ("empty-hex", &empty),
    ] {
        let encoded = CompactChunk::encode(chunk)?;
        seeds.push(write(
            root,
            "map_chunk",
            name,
            encoded.payload_hex.as_bytes(),
        )?);
    }
    seeds.push(write(root, "map_chunk", "invalid-hex", b"not-hex-payload")?);
    Ok(seeds)
}
