use super::*;
use crate::{HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation, MapChunkGenerator};

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
                appearance: Some(LandscapeAppearance {
                    canopy_strength: 900,
                    floor_strength: 900,
                    palette: EcologicalPalette::Temperate,
                    exposure: NativeExposure::Open,
                    height_band: NativeHeightBand::Lowland,
                }),
            })
            .collect(),
        resources: Vec::new(),
        decorations: Vec::new(),
    }
}

pub(super) fn resource(index: usize, family: ResourceVisualFamily) -> LandscapeResource {
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

#[test]
fn savanna_and_treeless_palettes_forbid_positive_forest_strengths() {
    let original = fixture(1);
    let encoded = CompactChunk::encode_landscape(&original).expect("positive forest palette");
    assert_eq!(encoded.decode_landscape().expect("forest decode"), original);
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    for palette in [EcologicalPalette::Savanna, EcologicalPalette::Treeless] {
        let mut chunk = original.clone();
        chunk.tiles[0]
            .appearance
            .as_mut()
            .expect("appearance")
            .palette = palette;
        assert_eq!(
            CompactChunk::encode_landscape(&chunk),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let mut bad = bytes.clone();
        bad[HEADER + TILE_BYTES + 8 + 5] = palette as u8;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let appearance = chunk.tiles[0].appearance.as_mut().expect("appearance");
        appearance.canopy_strength = 0;
        appearance.floor_strength = 0;
        let encoded = CompactChunk::encode_landscape(&chunk).expect("zero forest strength");
        assert_eq!(encoded.decode_landscape().expect("open decode"), chunk);
    }
}

#[test]
fn explicit_sparse_tiles_support_partial_right_edges_and_require_membership() {
    let mut chunk = fixture(36);
    for (index, sample) in chunk.tiles.iter_mut().enumerate() {
        sample.tile = TileCoord::new((index % 18) as i32, (index / 18) as i32);
    }
    chunk
        .resources
        .push(resource(49, ResourceVisualFamily::Conifer));
    chunk
        .decorations
        .push(decoration(49, DecorationFamily::Deadwood));
    let encoded = CompactChunk::encode_landscape(&chunk).expect("18-wide two rows");
    assert_eq!(encoded.decode_landscape().expect("partial edge"), chunk);
    let mut bad = chunk.clone();
    bad.resources[0].node.tile = TileCoord::new(18, 0);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
    bad = chunk.clone();
    bad.decorations[0].tile = TileCoord::new(18, 0);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    for offset in [HEADER + 36 * TILE + 8, HEADER + 36 * TILE + RESOURCE] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&18_i32.to_le_bytes());
        bad[offset + 4..offset + 8].copy_from_slice(&0_i32.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
}

#[test]
fn explicit_tile_coords_reject_duplicates_unsorted_and_out_of_domain() {
    let chunk = fixture(2);
    let bytes = decode_hex(
        &CompactChunk::encode_landscape(&chunk)
            .expect("fixture")
            .payload_hex,
    )
    .expect("hex");
    for coord in [
        TileCoord::new(0, 0),
        TileCoord::new(-1, 0),
        TileCoord::new(32, 0),
        TileCoord::new(0, 32),
        TileCoord::new(i32::MIN, i32::MAX),
    ] {
        let mut bad = chunk.clone();
        bad.tiles[1].tile = coord;
        assert_eq!(
            CompactChunk::encode_landscape(&bad),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let mut bad = bytes.clone();
        let offset = HEADER + TILE + TILE_BYTES;
        bad[offset..offset + 4].copy_from_slice(&coord.x.to_le_bytes());
        bad[offset + 4..offset + 8].copy_from_slice(&coord.y.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    let mut bad = chunk;
    bad.tiles.swap(0, 1);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
    let mut bad = bytes.clone();
    bad[HEADER..HEADER + TILE].copy_from_slice(&bytes[HEADER + TILE..HEADER + 2 * TILE]);
    bad[HEADER + TILE..HEADER + 2 * TILE].copy_from_slice(&bytes[HEADER..HEADER + TILE]);
    assert_eq!(
        malformed(&bad).decode_landscape(),
        Err(LandscapeChunkError::InvalidLandscape)
    );
}
