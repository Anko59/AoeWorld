use super::*;
use crate::{HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation, MapChunkGenerator};

#[test]
fn compact_chunk_round_trips_typed_terrain_evidence() {
    let mut original = MapChunkGenerator::new([7; 32], 11, 64)
        .chunk(1, 1)
        .expect("fixture chunk");
    original.tiles[0].hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::River,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    original.tiles[0].modern_land_cover_class = Some(40);
    let compact = CompactChunk::encode(&original).expect("encodes a map chunk");
    assert_eq!(
        compact.decoded_len().expect("decoded length"),
        5 + 20 * original.tiles.len() + 21 * original.resources.len()
    );
    assert_eq!(compact.decode().expect("decodes a map chunk"), original);
}

#[test]
fn maximum_version_two_chunk_uses_twenty_bytes_per_tile_and_stays_bounded() {
    let generator = MapChunkGenerator::new([3; 32], 5, CHUNK_TILES);
    let mut tile = generator.tile_at(TileCoord::new(0, 0)).expect("tile");
    tile.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::River,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    tile.modern_land_cover_class = Some(40);
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
    assert_eq!(
        compact.decoded_len().expect("decoded length"),
        5 + 41 * MAX_CHUNK_TILES
    );
    assert!(compact.decoded_len().expect("decoded length") <= MAX_DECODED_CHUNK_BYTES);
    assert_eq!(compact.decode().expect("decodes maximum chunk"), chunk);
}

#[test]
fn version_one_fixture_defaults_modern_evidence_to_none() {
    // Captured from the pre-change CompactChunk::encode implementation.
    const VERSION_ONE_FIXTURE: [u8; 23] = [
        1, 1, 0, 0, 0, 2, 0xde, 0xff, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9, 0xff, 0xa9,
        0xff, 0x65, 0xc3, 0x16, 0x00,
    ];
    let compact = CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(&VERSION_ONE_FIXTURE),
    };
    let decoded = compact.decode().expect("legacy chunk remains readable");
    assert_eq!(decoded.tiles.len(), 1);
    assert_eq!(decoded.tiles[0].hydrology_observation, None);
    assert_eq!(decoded.tiles[0].modern_land_cover_class, None);
}

#[test]
fn invalid_kind_method_pairs_and_worldcover_codes_are_rejected() {
    let mut tile = MapChunkGenerator::new([1; 32], 2, 32)
        .tile_at(TileCoord::new(0, 0))
        .expect("tile");
    tile.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::Lake,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    let chunk = Chunk {
        x: 0,
        y: 0,
        tiles: vec![tile],
        resources: Vec::new(),
    };
    assert_eq!(
        CompactChunk::encode(&chunk),
        Err(CompactChunkError::InvalidEnum)
    );

    let mut bytes = vec![0; HEADER_BYTES + TILE_BYTES];
    bytes[0] = FORMAT_VERSION;
    bytes[1] = 1;
    bytes[HEADER_BYTES + LEGACY_TILE_BYTES] = 13;
    let malformed = CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(&bytes),
    };
    assert_eq!(malformed.decode(), Err(CompactChunkError::InvalidEnum));
}

#[test]
fn reserved_observation_bits_and_unknown_kinds_are_rejected() {
    for (low, high) in [(0x10, 0), (0, 0x01), (0, 0xf0)] {
        let mut bytes = vec![0; HEADER_BYTES + TILE_BYTES];
        bytes[0] = FORMAT_VERSION;
        bytes[1] = 1;
        bytes[HEADER_BYTES + LEGACY_TILE_BYTES] = low;
        bytes[HEADER_BYTES + LEGACY_TILE_BYTES + 1] = high;
        let malformed = CompactChunk {
            x: 0,
            y: 0,
            payload_hex: encode_hex(&bytes),
        };
        assert_eq!(malformed.decode(), Err(CompactChunkError::InvalidEnum));
    }
}

#[test]
fn worldcover_evidence_codes_round_trip_every_documented_class() {
    let mut tile = MapChunkGenerator::new([7; 32], 11, 64)
        .tile_at(TileCoord::new(0, 0))
        .expect("fixture tile");
    for class in [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 100] {
        tile.hydrology_observation = None;
        tile.modern_land_cover_class = Some(class);
        let properties =
            pack_observation_properties(Tile { ..tile }).expect("valid WorldCover class");
        assert_eq!(
            unpack_observation_properties(properties).expect("decoded class"),
            (None, Some(class))
        );
    }
    tile.hydrology_observation = None;
    tile.modern_land_cover_class = None;
    assert_eq!(pack_observation_properties(tile), Ok(0));
    tile.modern_land_cover_class = Some(55);
    assert_eq!(
        pack_observation_properties(tile),
        Err(CompactChunkError::InvalidEnum)
    );
}

#[test]
fn compact_chunk_rejects_truncated_and_trailing_payload_bytes() {
    for length in [HEADER_BYTES + TILE_BYTES - 1, HEADER_BYTES + TILE_BYTES + 1] {
        let mut bytes = vec![0; length];
        bytes[0] = FORMAT_VERSION;
        bytes[1] = 1;
        let malformed = CompactChunk {
            x: 0,
            y: 0,
            payload_hex: encode_hex(&bytes),
        };
        assert_eq!(malformed.decode(), Err(CompactChunkError::InvalidLength));
    }
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

#[test]
fn encode_and_decode_reject_impossible_tile_resource_counts_before_allocating() {
    let tile = MapChunkGenerator::new([1; 32], 2, 32)
        .tile_at(TileCoord::new(0, 0))
        .expect("tile");
    let too_many_tiles = Chunk {
        x: 0,
        y: 0,
        tiles: vec![tile; MAX_CHUNK_TILES + 1],
        resources: Vec::new(),
    };
    assert_eq!(
        CompactChunk::encode(&too_many_tiles),
        Err(CompactChunkError::TooManyTiles)
    );

    let resource = ResourceNode {
        id: 1,
        tile: TileCoord::new(0, 0),
        kind: ResourceKind::Wood,
        object: ObjectKind::Tree,
        initial_amount: 10,
        visual_variant: 0,
    };
    let too_many_resources = Chunk {
        x: 0,
        y: 0,
        tiles: vec![tile],
        resources: vec![resource, resource],
    };
    assert_eq!(
        CompactChunk::encode(&too_many_resources),
        Err(CompactChunkError::TooManyResources)
    );

    let unsupported = CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(&[3, 0, 0, 0, 0]),
    };
    assert_eq!(
        unsupported.decode(),
        Err(CompactChunkError::UnsupportedVersion)
    );

    let too_many_decoded_tiles = CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(&[2, 1, 4, 0, 0]),
    };
    assert_eq!(
        too_many_decoded_tiles.decode(),
        Err(CompactChunkError::TooManyTiles)
    );

    let too_many_decoded_resources = CompactChunk {
        x: 0,
        y: 0,
        payload_hex: encode_hex(&[2, 1, 0, 2, 0]),
    };
    assert_eq!(
        too_many_decoded_resources.decode(),
        Err(CompactChunkError::TooManyResources)
    );
}

#[test]
fn every_documented_terrain_enum_maps_and_every_out_of_range_value_fails() {
    for value in 0..=11 {
        assert!(ground_material(value).is_ok(), "material {value}");
    }
    for value in 0..=9 {
        assert!(biome(value).is_ok(), "biome {value}");
    }
    for value in 0..=4 {
        assert!(provenance(value).is_ok(), "provenance {value}");
        assert!(water_kind(value).is_ok(), "water {value}");
    }
    for value in 0..=2 {
        assert!(surface_kind(value).is_ok(), "surface {value}");
    }
    for value in 0..=1 {
        assert!(diagonal(value).is_ok(), "diagonal {value}");
    }
    for value in 0..=3 {
        assert!(resource_kind(value).is_ok(), "resource {value}");
    }
    for value in 0..=4 {
        assert!(object_kind(value).is_ok(), "object {value}");
    }

    assert_eq!(ground_material(12), Err(CompactChunkError::InvalidEnum));
    assert_eq!(biome(10), Err(CompactChunkError::InvalidEnum));
    assert_eq!(provenance(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(water_kind(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(surface_kind(3), Err(CompactChunkError::InvalidEnum));
    assert_eq!(diagonal(2), Err(CompactChunkError::InvalidEnum));
    assert_eq!(resource_kind(4), Err(CompactChunkError::InvalidEnum));
    assert_eq!(object_kind(5), Err(CompactChunkError::InvalidEnum));
}
