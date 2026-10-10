use super::*;
use crate::{
    DecorationFamily, HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation,
    MapChunkGenerator, NativeExposure, NativeHeightBand,
};

pub(super) fn fixture(count: usize) -> LandscapeChunk {
    let mut terrain = MapChunkGenerator::new([7; 32], 11, 64)
        .tile_at(TileCoord::new(0, 0))
        .expect("fixture tile");
    terrain.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::River,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    terrain.modern_land_cover_class = Some(40);
    LandscapeChunk {
        x: 0,
        y: 0,
        tiles: (0..count)
            .map(|index| LandscapeTile {
                tile: TileCoord::new((index % 32) as i32, (index / 32) as i32),
                terrain,
                appearance: LandscapeAppearance {
                    canopy_strength: 900,
                    floor_strength: 900,
                    palette: EcologicalPalette::Temperate,
                    exposure: NativeExposure::Open,
                    height_band: NativeHeightBand::Lowland,
                },
            })
            .collect(),
        resources: Vec::new(),
        decorations: Vec::new(),
    }
}

pub(super) fn tree(index: usize, family: ResourceVisualFamily) -> LandscapeResource {
    LandscapeResource {
        node: ResourceNode {
            id: index as u64,
            tile: TileCoord::new((index % 32) as i32, (index / 32) as i32),
            kind: ResourceKind::Wood,
            object: ObjectKind::Tree,
            initial_amount: 100,
            visual_variant: index as u8,
        },
        visual_family: family,
    }
}

pub(super) fn decoration(index: usize, family: DecorationFamily) -> LandscapeDecoration {
    LandscapeDecoration {
        tile: TileCoord::new((index % 32) as i32, (index / 32) as i32),
        family,
        variant: index as u8,
        orientation: (index % 8) as u8,
    }
}

pub(super) fn malformed(bytes: &[u8]) -> CompactChunk {
    CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(bytes),
    }
}

pub(super) fn bytes(chunk: &LandscapeChunk) -> Vec<u8> {
    decode_hex(&CompactChunk::encode(chunk).expect("fixture").payload_hex).expect("hex")
}
