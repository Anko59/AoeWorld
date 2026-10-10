use super::*;
use crate::{
    DecorationFamily, HydrologyEvidenceMethod, HydrologyKind, HydrologyObservation,
    MapChunkGenerator, NativeExposure, NativeHeightBand,
};
mod helpers;
mod landscape;
use helpers::{bytes, decoration, fixture, malformed, tree};

const OBSERVATION: usize = HEADER_BYTES + TERRAIN_BYTES - 2;

#[test]
fn generated_chunks_round_trip_with_typed_terrain_evidence() {
    let mut original = MapChunkGenerator::new([7; 32], 11, 64)
        .landscape_chunk_with_cancel(1, 1, &|| false)
        .expect("fixture chunk");
    original.tiles[0].terrain.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::River,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    original.tiles[0].terrain.modern_land_cover_class = Some(40);
    let compact = CompactChunk::encode(&original).expect("encodes a map chunk");
    assert!(compact.payload_hex.starts_with("04"));
    assert_eq!(
        compact.decoded_len().expect("decoded length"),
        HEADER_BYTES
            + TILE_BYTES * original.tiles.len()
            + RESOURCE_BYTES * original.resources.len()
            + DECORATION_BYTES * original.decorations.len()
    );
    assert_eq!(compact.decode().expect("decodes a map chunk"), original);
}

#[test]
fn every_typed_field_round_trips_and_the_maximum_chunk_stays_bounded() {
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
        tile.appearance = LandscapeAppearance {
            canopy_strength: strength,
            floor_strength: strength,
            palette,
            exposure: exposures[index % exposures.len()],
            height_band: heights[index % heights.len()],
        };
    }
    chunk.resources = [
        ResourceVisualFamily::Broadleaf,
        ResourceVisualFamily::Conifer,
        ResourceVisualFamily::DryScrub,
        ResourceVisualFamily::Tropical,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, family)| tree(index, family))
    .collect();
    let mut gold = tree(4, ResourceVisualFamily::Generic);
    gold.node.kind = ResourceKind::Gold;
    gold.node.object = ObjectKind::GoldDeposit;
    chunk.resources.push(gold);
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
    let encoded = CompactChunk::encode(&chunk).expect("encode");
    assert_eq!(encoded.decode().expect("decode"), chunk);
    assert_eq!(
        encoded.decoded_len().expect("size"),
        HEADER_BYTES + 8 * TILE_BYTES + 5 * RESOURCE_BYTES + 4 * DECORATION_BYTES
    );

    let mut maximum = fixture(MAX_CHUNK_TILES);
    maximum.resources = (0..MAX_CHUNK_TILES)
        .map(|index| tree(index, ResourceVisualFamily::Conifer))
        .collect();
    maximum.decorations = (0..MAX_CHUNK_TILES)
        .map(|index| decoration(index, DecorationFamily::Grass))
        .collect();
    let encoded = CompactChunk::encode(&maximum).expect("encode maximum");
    assert_eq!(encoded.decoded_len().expect("size"), 69_639);
    assert!(encoded.decoded_len().expect("size") < MAX_DECODED_CHUNK_BYTES);
    assert_eq!(encoded.decode().expect("maximum decode"), maximum);
}

#[test]
fn only_the_current_version_byte_is_accepted() {
    let valid = bytes(&fixture(1));
    assert_eq!(valid[0], CHUNK_FORMAT_VERSION);
    for version in [0, 1, 2, 3, 5, u8::MAX] {
        let mut other = valid.clone();
        other[0] = version;
        assert_eq!(
            malformed(&other).decode(),
            Err(CompactChunkError::UnsupportedVersion)
        );
    }
    let empty = fixture(0);
    let encoded = CompactChunk::encode(&empty).expect("empty");
    assert_eq!(encoded.payload_hex, "04000000000000");
    assert_eq!(encoded.decode().expect("empty decode"), empty);
}

#[test]
fn rejects_bad_header_counts_lengths_hex_and_oversized_payloads() {
    let valid = CompactChunk::encode(&fixture(1)).expect("fixture");
    let bytes = decode_hex(&valid.payload_hex).expect("hex");
    for length in 0..bytes.len() {
        assert!(malformed(&bytes[..length]).decode().is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        malformed(&trailing).decode(),
        Err(CompactChunkError::InvalidLength)
    );
    for (offset, count, expected) in [
        (1, 1025_u16, CompactChunkError::TooManyTiles),
        (3, 2, CompactChunkError::TooManyResources),
        (5, 2, CompactChunkError::TooManyDecorations),
        (5, 1025, CompactChunkError::TooManyDecorations),
    ] {
        let mut bad = bytes.clone();
        bad[offset..offset + 2].copy_from_slice(&count.to_le_bytes());
        assert_eq!(malformed(&bad).decode(), Err(expected));
    }
    for payload_hex in ["0".to_owned(), "zz".to_owned()] {
        let bad = CompactChunk {
            payload_hex,
            ..valid.clone()
        };
        assert_eq!(bad.decode(), Err(CompactChunkError::InvalidHex));
    }
    let bad = CompactChunk {
        payload_hex: "00".repeat(MAX_DECODED_CHUNK_BYTES + 1),
        ..valid
    };
    assert_eq!(bad.decode(), Err(CompactChunkError::PayloadTooLarge));
}

#[test]
fn encoder_rejects_count_overflow() {
    let bad = fixture(MAX_CHUNK_TILES + 1);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::TooManyTiles)
    );
    let mut bad = fixture(0);
    bad.resources.push(tree(0, ResourceVisualFamily::Broadleaf));
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::TooManyResources)
    );
    bad.resources.clear();
    bad.decorations.push(decoration(0, DecorationFamily::Grass));
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::TooManyDecorations)
    );
}

#[test]
fn invalid_kind_method_pairs_reserved_bits_and_worldcover_codes_are_rejected() {
    let mut chunk = fixture(1);
    chunk.tiles[0].terrain.hydrology_observation = Some(HydrologyObservation {
        kind: HydrologyKind::Lake,
        method: HydrologyEvidenceMethod::HydroRiversBufferedCorridor,
    });
    assert_eq!(
        CompactChunk::encode(&chunk),
        Err(CompactChunkError::InvalidEnum)
    );
    let valid = bytes(&fixture(1));
    for (low, high) in [(13, 0), (0x10, 0), (0, 0x01), (0, 0xf0)] {
        let mut bad = valid.clone();
        bad[OBSERVATION] = low;
        bad[OBSERVATION + 1] = high;
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidEnum)
        );
    }
    let mut bad = valid;
    bad[HEADER_BYTES + 17] |= 0x80;
    assert_eq!(
        malformed(&bad).decode(),
        Err(CompactChunkError::InvalidEnum)
    );
}

#[test]
fn worldcover_evidence_codes_round_trip_every_documented_class() {
    let mut tile = fixture(1).tiles[0].terrain;
    for class in [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 100] {
        tile.hydrology_observation = None;
        tile.modern_land_cover_class = Some(class);
        let properties = pack_observation_properties(tile).expect("valid WorldCover class");
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
        assert!(resource_family_from(value).is_ok(), "family {value}");
        assert!(height_from(value).is_ok(), "height band {value}");
        assert!(object_kind(value).is_ok(), "object {value}");
    }
    for value in 0..=2 {
        assert!(surface_kind(value).is_ok(), "surface {value}");
        assert!(exposure_from(value).is_ok(), "exposure {value}");
    }
    for value in 0..=1 {
        assert!(diagonal(value).is_ok(), "diagonal {value}");
    }
    for value in 0..=3 {
        assert!(resource_kind(value).is_ok(), "resource {value}");
        assert!(decoration_from(value).is_ok(), "decoration {value}");
    }
    for value in 0..=5 {
        assert!(palette_from(value).is_ok(), "palette {value}");
    }
    assert_eq!(ground_material(12), Err(CompactChunkError::InvalidEnum));
    assert_eq!(biome(10), Err(CompactChunkError::InvalidEnum));
    assert_eq!(provenance(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(water_kind(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(surface_kind(3), Err(CompactChunkError::InvalidEnum));
    assert_eq!(diagonal(2), Err(CompactChunkError::InvalidEnum));
    assert_eq!(resource_kind(4), Err(CompactChunkError::InvalidEnum));
    assert_eq!(object_kind(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(palette_from(6), Err(CompactChunkError::InvalidEnum));
    assert_eq!(exposure_from(3), Err(CompactChunkError::InvalidEnum));
    assert_eq!(height_from(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(resource_family_from(5), Err(CompactChunkError::InvalidEnum));
    assert_eq!(decoration_from(4), Err(CompactChunkError::InvalidEnum));
}
