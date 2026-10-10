use super::*;

const APPEARANCE: usize = HEADER_BYTES + TERRAIN_BYTES + 8;

#[test]
fn appearance_strengths_must_be_coherent_and_bounded() {
    let bytes = bytes(&fixture(1));
    for (canopy, floor) in [(1001_u16, 1001_u16), (0, 1), (1000, 1001)] {
        let mut bad = bytes.clone();
        bad[APPEARANCE..APPEARANCE + 2].copy_from_slice(&canopy.to_le_bytes());
        bad[APPEARANCE + 2..APPEARANCE + 4].copy_from_slice(&floor.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
        let mut chunk = fixture(1);
        chunk.tiles[0].appearance.canopy_strength = canopy;
        chunk.tiles[0].appearance.floor_strength = floor;
        assert_eq!(
            CompactChunk::encode(&chunk),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
    for (offset, value) in [(4, 6), (5, 3), (6, 5)] {
        let mut bad = bytes.clone();
        bad[APPEARANCE + offset] = value;
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidEnum)
        );
    }
}

#[test]
fn savanna_and_treeless_palettes_forbid_positive_forest_strengths() {
    let original = fixture(1);
    let encoded = CompactChunk::encode(&original).expect("positive forest palette");
    assert_eq!(encoded.decode().expect("forest decode"), original);
    let bytes = bytes(&original);
    for palette in [EcologicalPalette::Savanna, EcologicalPalette::Treeless] {
        let mut chunk = original.clone();
        chunk.tiles[0].appearance.palette = palette;
        assert_eq!(
            CompactChunk::encode(&chunk),
            Err(CompactChunkError::InvalidLandscape)
        );
        let mut bad = bytes.clone();
        bad[APPEARANCE + 4] = palette as u8;
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
        chunk.tiles[0].appearance.canopy_strength = 0;
        chunk.tiles[0].appearance.floor_strength = 0;
        let encoded = CompactChunk::encode(&chunk).expect("zero forest strength");
        assert_eq!(encoded.decode().expect("open decode"), chunk);
    }
}

#[test]
fn trees_require_a_family_and_other_resources_are_generic() {
    let mut chunk = fixture(1);
    chunk
        .resources
        .push(tree(0, ResourceVisualFamily::Broadleaf));
    chunk
        .decorations
        .push(decoration(0, DecorationFamily::Shrub));
    let bytes = bytes(&chunk);
    let resource = HEADER_BYTES + TILE_BYTES;
    let decoration = resource + RESOURCE_BYTES;
    for (offset, value) in [(resource + 21, 0), (decoration + 10, 8)] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
    for (offset, value) in [
        (resource + 16, 4),
        (resource + 17, 5),
        (resource + 21, 5),
        (decoration + 8, 4),
    ] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidEnum)
        );
    }
    let mut generic_tree = chunk.clone();
    generic_tree.resources[0].visual_family = ResourceVisualFamily::Generic;
    assert_eq!(
        CompactChunk::encode(&generic_tree),
        Err(CompactChunkError::InvalidLandscape)
    );
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
            CompactChunk::encode(&bad),
            Err(CompactChunkError::InvalidLandscape)
        );
        let mut bad_bytes = bytes.clone();
        bad_bytes[resource + 16] = kind as u8;
        bad_bytes[resource + 17] = object as u8;
        assert_eq!(
            malformed(&bad_bytes).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
        bad.resources[0].visual_family = ResourceVisualFamily::Generic;
        let encoded = CompactChunk::encode(&bad).expect("generic resource");
        assert_eq!(encoded.decode().expect("generic decode"), bad);
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
        .push(tree(49, ResourceVisualFamily::Conifer));
    chunk
        .decorations
        .push(decoration(49, DecorationFamily::Deadwood));
    let encoded = CompactChunk::encode(&chunk).expect("18-wide two rows");
    assert_eq!(encoded.decode().expect("partial edge"), chunk);
    let mut bad = chunk.clone();
    bad.resources[0].node.tile = TileCoord::new(18, 0);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
    bad = chunk.clone();
    bad.decorations[0].tile = TileCoord::new(18, 0);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
    let bytes = decode_hex(&encoded.payload_hex).expect("hex");
    let resources = HEADER_BYTES + 36 * TILE_BYTES;
    for offset in [resources + 8, resources + RESOURCE_BYTES] {
        let mut bad = bytes.clone();
        bad[offset..offset + 4].copy_from_slice(&18_i32.to_le_bytes());
        bad[offset + 4..offset + 8].copy_from_slice(&0_i32.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
}

#[test]
fn explicit_tile_coords_reject_duplicates_unsorted_and_out_of_domain() {
    let chunk = fixture(2);
    let bytes = bytes(&chunk);
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
            CompactChunk::encode(&bad),
            Err(CompactChunkError::InvalidLandscape)
        );
        let mut bad = bytes.clone();
        let offset = HEADER_BYTES + TILE_BYTES + TERRAIN_BYTES;
        bad[offset..offset + 4].copy_from_slice(&coord.x.to_le_bytes());
        bad[offset + 4..offset + 8].copy_from_slice(&coord.y.to_le_bytes());
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
    let mut bad = chunk;
    bad.tiles.swap(0, 1);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
    let first = HEADER_BYTES..HEADER_BYTES + TILE_BYTES;
    let second = HEADER_BYTES + TILE_BYTES..HEADER_BYTES + 2 * TILE_BYTES;
    let mut bad = bytes.clone();
    bad[first.clone()].copy_from_slice(&bytes[second.clone()]);
    bad[second].copy_from_slice(&bytes[first]);
    assert_eq!(
        malformed(&bad).decode(),
        Err(CompactChunkError::InvalidLandscape)
    );
}

#[test]
fn negative_chunks_membership_duplicates_and_orientation_are_strict() {
    let mut chunk = fixture(33);
    chunk
        .resources
        .push(tree(32, ResourceVisualFamily::Tropical));
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
    let encoded = CompactChunk::encode(&chunk).expect("negative chunk coordinates");
    assert_eq!(encoded.decode().expect("negative decode"), chunk);
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
            CompactChunk::encode(&bad),
            Err(CompactChunkError::InvalidLandscape)
        );
        bad = chunk.clone();
        bad.decorations[0].tile = tile;
        assert_eq!(
            CompactChunk::encode(&bad),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
    for (x, y) in [(i32::MAX, 0), (0, i32::MIN)] {
        let mut bad = chunk.clone();
        bad.x = x;
        bad.y = y;
        assert_eq!(
            CompactChunk::encode(&bad),
            Err(CompactChunkError::InvalidLandscape)
        );
        let bad = CompactChunk {
            x,
            y,
            ..encoded.clone()
        };
        assert_eq!(bad.decode(), Err(CompactChunkError::InvalidLandscape));
    }
    let mut bad = chunk.clone();
    bad.resources.push(bad.resources[0]);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
    bad = chunk.clone();
    bad.decorations.push(bad.decorations[0]);
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
    bad = chunk;
    bad.decorations[0].orientation = 8;
    assert_eq!(
        CompactChunk::encode(&bad),
        Err(CompactChunkError::InvalidLandscape)
    );
}

#[test]
fn decoder_rejects_duplicate_resource_and_decoration_cells() {
    let mut chunk = fixture(2);
    chunk.resources = vec![
        tree(0, ResourceVisualFamily::Broadleaf),
        tree(1, ResourceVisualFamily::Broadleaf),
    ];
    chunk.decorations = vec![
        decoration(0, DecorationFamily::Grass),
        decoration(1, DecorationFamily::Grass),
    ];
    let bytes = bytes(&chunk);
    let resources = HEADER_BYTES + 2 * TILE_BYTES;
    let decorations = resources + 2 * RESOURCE_BYTES;
    for (first, second) in [
        (resources + 8, resources + RESOURCE_BYTES + 8),
        (decorations, decorations + DECORATION_BYTES),
    ] {
        let mut bad = bytes.clone();
        bad[second..second + 8].copy_from_slice(&bytes[first..first + 8]);
        assert_eq!(
            malformed(&bad).decode(),
            Err(CompactChunkError::InvalidLandscape)
        );
    }
}
