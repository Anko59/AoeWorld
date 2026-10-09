use super::*;
mod helpers;
use helpers::{decoration, fixture, malformed, resource};

#[test]
fn codec_three_roundtrips_all_typed_metadata_and_full_base_evidence() {
    let mut chunk = fixture(8);
    let palettes = [
        EcologicalPalette::Temperate,
        EcologicalPalette::Boreal,
        EcologicalPalette::Tropical,
        EcologicalPalette::DryScrub,
        EcologicalPalette::Savanna,
        EcologicalPalette::Treeless,
    ];
    let exposures = [
        NativeExposure::Sheltered,
        NativeExposure::Open,
        NativeExposure::Exposed,
    ];
    let heights = [
        NativeHeightBand::Lowland,
        NativeHeightBand::Montane,
        NativeHeightBand::Subalpine,
        NativeHeightBand::Alpine,
        NativeHeightBand::Nival,
    ];
    for (index, tile) in chunk.tiles.iter_mut().enumerate() {
        let palette = palettes[index % palettes.len()];
        let strength = if matches!(
            palette,
            EcologicalPalette::Savanna | EcologicalPalette::Treeless
        ) {
            0
        } else {
            1000
        };
        tile.appearance = Some(LandscapeAppearance {
            canopy_strength: strength,
            floor_strength: strength,
            palette,
            exposure: exposures[index % exposures.len()],
            height_band: heights[index % heights.len()],
        });
    }
    for tile in &chunk.tiles[..4] {
        assert_eq!(
            tile.appearance.expect("forest palette").canopy_strength,
            1000
        );
    }
    for tile in &chunk.tiles[4..6] {
        assert_eq!(tile.appearance.expect("open palette").canopy_strength, 0);
    }
    chunk.tiles[7].appearance = None;
    chunk.resources = [
        ResourceVisualFamily::Legacy,
        ResourceVisualFamily::Broadleaf,
        ResourceVisualFamily::Conifer,
        ResourceVisualFamily::DryScrub,
        ResourceVisualFamily::Tropical,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, family)| resource(index, family))
    .collect();
    chunk.decorations = [
        DecorationFamily::Shrub,
        DecorationFamily::Grass,
        DecorationFamily::Stone,
        DecorationFamily::Deadwood,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, family)| decoration(index, family))
    .collect();
    let encoded = CompactChunk::encode_landscape(&chunk).expect("encode");
    assert_eq!(encoded.decode_landscape().expect("decode"), chunk);
    assert_eq!(encoded.decode(), Err(CompactChunkError::UnsupportedVersion));
    assert_eq!(
        encoded.decoded_len().expect("size"),
        HEADER + 8 * TILE + 5 * RESOURCE + 4 * DECORATION
    );
}

#[test]
fn maximum_tree_and_decoration_counts_stay_well_below_payload_ceiling() {
    let mut chunk = fixture(MAX_CHUNK_TILES);
    chunk.resources = (0..MAX_CHUNK_TILES)
        .map(|index| resource(index, ResourceVisualFamily::Conifer))
        .collect();
    chunk.decorations = (0..MAX_CHUNK_TILES)
        .map(|index| decoration(index, DecorationFamily::Grass))
        .collect();
    let encoded = CompactChunk::encode_landscape(&chunk).expect("encode maximum");
    assert_eq!(encoded.decoded_len().expect("size"), 72_711);
    assert!(encoded.decoded_len().expect("size") < MAX_DECODED_CHUNK_BYTES);
    assert_eq!(encoded.decode_landscape().expect("maximum decode"), chunk);
}

#[test]
fn old_versions_project_without_inference_or_new_coordinate_validation() {
    let old = Chunk {
        x: i32::MAX,
        y: i32::MIN,
        tiles: fixture(1)
            .tiles
            .into_iter()
            .map(|sample| sample.terrain)
            .collect(),
        // Legacy accepts out-of-domain coordinates and unusual kind/object pairs.
        resources: vec![ResourceNode {
            id: 7,
            tile: TileCoord::new(-999, 999),
            kind: ResourceKind::Food,
            object: ObjectKind::Decoration,
            initial_amount: 10,
            visual_variant: 4,
        }],
    };
    let encoded = CompactChunk::encode(&old).expect("old encode");
    assert!(encoded.payload_hex.starts_with("02"));
    let projected = encoded.decode_landscape().expect("legacy projection");
    assert_eq!(projected.tiles[0].tile, TileCoord::new(-32, 0));
    assert_eq!(projected.tiles[0].terrain, old.tiles[0]);
    assert_eq!(projected.tiles[0].appearance, None);
    assert_eq!(projected.resources[0].node, old.resources[0]);
    assert_eq!(
        projected.resources[0].visual_family,
        ResourceVisualFamily::Legacy
    );
    assert!(projected.decorations.is_empty());
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    let mut version_one = bytes[..HEADER_BYTES + LEGACY_TILE_BYTES].to_vec();
    version_one[0] = LEGACY_FORMAT_VERSION;
    version_one.extend_from_slice(&bytes[HEADER_BYTES + TILE_BYTES..]);
    let legacy = CompactChunk {
        payload_hex: encode_hex(&version_one),
        ..encoded
    };
    let projected = legacy.decode_landscape().expect("version one projection");
    assert_eq!(projected.tiles[0].appearance, None);
    assert_eq!(projected.tiles[0].terrain.hydrology_observation, None);
    assert_eq!(projected.tiles[0].terrain.modern_land_cover_class, None);
    assert_eq!(
        projected.resources[0].visual_family,
        ResourceVisualFamily::Legacy
    );
}

#[test]
fn rejects_bad_header_counts_lengths_hex_and_version() {
    let valid = CompactChunk::encode_landscape(&fixture(1)).expect("fixture");
    let bytes = decode_hex(&valid.payload_hex).expect("hex");
    for length in 0..bytes.len() {
        assert!(malformed(&bytes[..length]).decode_landscape().is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        malformed(&trailing).decode_landscape(),
        Err(CompactChunkError::InvalidLength.into())
    );
    for (offset, count, expected) in [
        (1, 1025_u16, CompactChunkError::TooManyTiles.into()),
        (3, 2, CompactChunkError::TooManyResources.into()),
        (5, 2, LandscapeChunkError::TooManyDecorations),
        (5, 1025, LandscapeChunkError::TooManyDecorations),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
        assert_eq!(malformed(&bad).decode_landscape(), Err(expected));
    }
    assert_eq!(
        malformed(&[4]).decode_landscape(),
        Err(CompactChunkError::UnsupportedVersion.into())
    );
    for payload_hex in ["0".to_owned(), "zz".to_owned()] {
        let bad = CompactChunk {
            payload_hex,
            ..valid.clone()
        };
        assert_eq!(
            bad.decode_landscape(),
            Err(CompactChunkError::InvalidHex.into())
        );
    }
    let bad = CompactChunk {
        payload_hex: "00".repeat(MAX_DECODED_CHUNK_BYTES + 1),
        ..valid
    };
    assert_eq!(
        bad.decode_landscape(),
        Err(CompactChunkError::PayloadTooLarge.into())
    );
}

#[test]
fn rejects_appearance_enums_reserved_presence_and_incoherent_strengths() {
    let encoded = CompactChunk::encode_landscape(&fixture(1)).expect("fixture");
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    let metadata = HEADER + TILE_BYTES + 8;
    for (offset, value) in [(0, 2), (5, 6), (6, 3), (7, 5), (8, 1)] {
        let mut bad = bytes.clone();
        bad[metadata + offset] = value;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    for (canopy, floor) in [(1001_u16, 1001_u16), (0, 1), (1000, 1001)] {
        let mut bad = bytes.clone();
        bad[metadata + 1..metadata + 3].copy_from_slice(&canopy.to_le_bytes());
        bad[metadata + 3..metadata + 5].copy_from_slice(&floor.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let mut chunk = fixture(1);
        let appearance = chunk.tiles[0].appearance.as_mut().expect("appearance");
        appearance.canopy_strength = canopy;
        appearance.floor_strength = floor;
        assert_eq!(
            CompactChunk::encode_landscape(&chunk),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    let mut absent = fixture(1);
    absent.tiles[0].appearance = None;
    let bytes = decode_hex(
        &CompactChunk::encode_landscape(&absent)
            .expect("absent")
            .payload_hex,
    )
    .expect("hex");
    for offset in 1..9 {
        let mut bad = bytes.clone();
        bad[metadata + offset] = 1;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    for (offset, value) in [(HEADER + 17, 0x80), (HEADER + 19, 0xf0)] {
        let mut bad = bytes.clone();
        bad[offset] |= value;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(CompactChunkError::InvalidEnum.into())
        );
    }
}

#[test]
fn rejects_bad_resource_and_decoration_fields() {
    let mut chunk = fixture(1);
    chunk
        .resources
        .push(resource(0, ResourceVisualFamily::Broadleaf));
    chunk
        .decorations
        .push(decoration(0, DecorationFamily::Shrub));
    let bytes = decode_hex(
        &CompactChunk::encode_landscape(&chunk)
            .expect("fixture")
            .payload_hex,
    )
    .expect("hex");
    let resource = HEADER + TILE;
    let decoration = resource + RESOURCE;
    for (offset, value) in [
        (resource + 21, 5),
        (decoration + 8, 4),
        (decoration + 10, 8),
        (decoration + 11, 1),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    for (offset, value) in [(resource + 16, 4), (resource + 17, 5)] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(CompactChunkError::InvalidEnum.into())
        );
    }
    for (kind, object) in [
        (ResourceKind::Food, ObjectKind::ForageBush),
        (ResourceKind::Gold, ObjectKind::GoldDeposit),
        (ResourceKind::Stone, ObjectKind::StoneDeposit),
        (ResourceKind::Wood, ObjectKind::ForageBush),
        (ResourceKind::Food, ObjectKind::Tree),
    ] {
        let mut bad = chunk.clone();
        bad.resources[0].node.kind = kind;
        bad.resources[0].node.object = object;
        assert_eq!(
            CompactChunk::encode_landscape(&bad),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let mut bad_bytes = bytes.clone();
        bad_bytes[resource + 16] = kind as u8;
        bad_bytes[resource + 17] = object as u8;
        assert_eq!(
            malformed(&bad_bytes).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        bad.resources[0].visual_family = ResourceVisualFamily::Legacy;
        let encoded = CompactChunk::encode_landscape(&bad).expect("Legacy allowed");
        assert_eq!(encoded.decode_landscape().expect("Legacy decode"), bad);
    }
}

#[test]
fn coordinates_membership_duplicate_cells_and_orientation_are_strict() {
    let mut chunk = fixture(33);
    chunk
        .resources
        .push(resource(32, ResourceVisualFamily::Tropical));
    chunk
        .decorations
        .push(decoration(32, DecorationFamily::Stone));
    chunk.x = -1;
    chunk.y = -2;
    for sample in &mut chunk.tiles {
        sample.tile.x -= 32;
        sample.tile.y -= 64;
    }
    chunk.resources[0].node.tile = TileCoord::new(-32, -63);
    chunk.decorations[0].tile = TileCoord::new(-32, -63);
    let encoded = CompactChunk::encode_landscape(&chunk).expect("negative chunk coordinates");
    assert_eq!(encoded.decode_landscape().expect("negative decode"), chunk);
    for tile in [
        TileCoord::new(-31, -63),
        TileCoord::new(0, -64),
        TileCoord::new(-33, -64),
        TileCoord::new(-32, -65),
        TileCoord::new(i32::MAX, i32::MIN),
    ] {
        let mut bad = chunk.clone();
        bad.resources[0].node.tile = tile;
        assert_eq!(
            CompactChunk::encode_landscape(&bad),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        bad = chunk.clone();
        bad.decorations[0].tile = tile;
        assert_eq!(
            CompactChunk::encode_landscape(&bad),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let mut bytes = decode_hex(&encoded.payload_hex).expect("hex");
        let offset = HEADER + 33 * TILE + 8;
        bytes[offset..offset + 4].copy_from_slice(&tile.x.to_le_bytes());
        bytes[offset + 4..offset + 8].copy_from_slice(&tile.y.to_le_bytes());
        let malformed = CompactChunk {
            payload_hex: encode_hex(&bytes),
            ..encoded.clone()
        };
        assert_eq!(
            malformed.decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    for (x, y) in [(i32::MAX, 0), (0, i32::MIN)] {
        let mut bad = chunk.clone();
        bad.x = x;
        bad.y = y;
        assert_eq!(
            CompactChunk::encode_landscape(&bad),
            Err(LandscapeChunkError::InvalidLandscape)
        );
        let bad = CompactChunk {
            x,
            y,
            ..encoded.clone()
        };
        assert_eq!(
            bad.decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
    let mut bad = chunk.clone();
    bad.resources.push(bad.resources[0]);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
    bad = chunk.clone();
    bad.decorations.push(bad.decorations[0]);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
    bad = chunk;
    bad.decorations[0].orientation = 8;
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::InvalidLandscape)
    );
}

#[test]
fn decoder_rejects_duplicate_resource_and_decoration_cells() {
    let mut chunk = fixture(2);
    chunk.resources = vec![
        resource(0, ResourceVisualFamily::Legacy),
        resource(1, ResourceVisualFamily::Legacy),
    ];
    chunk.decorations = vec![
        decoration(0, DecorationFamily::Grass),
        decoration(1, DecorationFamily::Grass),
    ];
    let encoded = CompactChunk::encode_landscape(&chunk).expect("two cells");
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    let resources = HEADER + 2 * TILE;
    let decorations = resources + 2 * RESOURCE;
    for (first, second) in [
        (resources + 8, resources + RESOURCE + 8),
        (decorations, decorations + DECORATION),
    ] {
        let mut bad = bytes.clone();
        bad[second..second + 8].copy_from_slice(&bytes[first..first + 8]);
        assert_eq!(
            malformed(&bad).decode_landscape(),
            Err(LandscapeChunkError::InvalidLandscape)
        );
    }
}

#[test]
fn encoder_rejects_count_overflow_and_empty_chunks_remain_valid() {
    let empty = fixture(0);
    let encoded = CompactChunk::encode_landscape(&empty).expect("empty");
    assert_eq!(encoded.payload_hex, "03000000000000");
    assert_eq!(encoded.decode_landscape().expect("empty decode"), empty);
    let mut bad = fixture(MAX_CHUNK_TILES + 1);
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(CompactChunkError::TooManyTiles.into())
    );
    bad = fixture(0);
    bad.resources
        .push(resource(0, ResourceVisualFamily::Legacy));
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(CompactChunkError::TooManyResources.into())
    );
    bad.resources.clear();
    bad.decorations.push(decoration(0, DecorationFamily::Grass));
    assert_eq!(
        CompactChunk::encode_landscape(&bad),
        Err(LandscapeChunkError::TooManyDecorations)
    );
}
