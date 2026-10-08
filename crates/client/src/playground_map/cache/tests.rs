use super::*;
use aoe_map::{
    DecorationFamily, EcologicalPalette, MapChunkGenerator, NativeExposure, NativeHeightBand,
    ObjectKind, ResourceKind, ResourceVisualFamily, WaterKind,
};
use wasm_bindgen_test::wasm_bindgen_test;

fn landscape() -> LandscapeChunk {
    let source = MapChunkGenerator::new([0; 32], 1, 50).chunk(1, 1).unwrap();
    let appearance = LandscapeAppearance {
        canopy_strength: 800,
        floor_strength: 800,
        palette: EcologicalPalette::Boreal,
        exposure: NativeExposure::Open,
        height_band: NativeHeightBand::Montane,
    };
    let mut tiles: Vec<_> = source
        .tiles
        .into_iter()
        .enumerate()
        .map(|(index, mut terrain)| {
            terrain.material = GroundMaterial::ForestFloor;
            terrain.water = WaterKind::None;
            terrain.game_height_level = 7;
            terrain.surface.corner_game_height_levels = [7; 4];
            LandscapeTile {
                tile: TileCoord::new(32 + (index % 18) as i32, 32 + (index / 18) as i32),
                terrain,
                appearance: (index != 0).then_some(appearance),
            }
        })
        .collect();
    tiles.remove(19); // Sparse row: no source cell at (33,33).
    LandscapeChunk {
        x: 1,
        y: 1,
        tiles,
        resources: vec![LandscapeResource {
            node: ResourceNode {
                id: 42,
                tile: TileCoord::new(32, 33),
                kind: ResourceKind::Wood,
                object: ObjectKind::Tree,
                initial_amount: 100,
                visual_variant: 6,
            },
            visual_family: ResourceVisualFamily::Conifer,
        }],
        decorations: vec![LandscapeDecoration {
            tile: TileCoord::new(32, 33),
            family: DecorationFamily::Deadwood,
            variant: 3,
            orientation: 2,
        }],
    }
}

#[wasm_bindgen_test]
fn legacy_partial_rows_preserve_twenty_column_geometry() {
    let source = MapChunkGenerator::new([0; 32], 1, 500)
        .chunk(15, 15)
        .unwrap();
    let cached = CachedChunk::decode(&CompactChunk::encode(&source).unwrap()).unwrap();
    assert!(matches!(cached, CachedChunk::Legacy(_)));
    let tiles: Vec<_> = cached.scene_tiles(500, 500).collect();
    assert_eq!(tiles.len(), 400);
    assert_eq!(tiles[20].0, TileCoord::new(480, 481));
    assert_eq!(tiles[399].0, TileCoord::new(499, 499));
    assert!(tiles.iter().all(|(_, _, appearance)| appearance.is_none()));
    assert_eq!(cached.tile_at(500, 500, 499, 499), source.tiles.get(399));
    assert!(cached.tile_at(500, 500, 500, 499).is_none());
    assert!(cached.resources().all(|(_, family)| family == 0));
    assert!(cached.decorations().is_empty());
}

#[wasm_bindgen_test]
fn explicit_sparse_rows_and_none_appearance_are_not_legacy_layout() {
    let source = landscape();
    let cached = CachedChunk::decode(&CompactChunk::encode_landscape(&source).unwrap()).unwrap();
    assert!(matches!(cached, CachedChunk::Landscape(_)));
    let tiles: Vec<_> = cached.scene_tiles(50, 50).collect();
    assert_eq!(tiles.len(), 323);
    assert_eq!(tiles[0].0, TileCoord::new(32, 32));
    assert!(tiles[0].2.is_none());
    assert_eq!(tiles[18].0, TileCoord::new(32, 33));
    assert_eq!(tiles[19].0, TileCoord::new(34, 33));
    for (position, tile, _) in &tiles {
        assert_eq!(cached.tile_at(50, 50, position.x, position.y), Some(*tile));
    }
    assert!(cached.tile_at(50, 50, 33, 33).is_none());
    assert!(cached.tile_at(50, 50, 50, 49).is_none());
    let sample = terrain_scene_sample(tiles[18].0, tiles[18].1, tiles[18].2);
    let appearance = sample.appearance.unwrap();
    assert_eq!(appearance.canopy_strength, 800);
    assert_eq!(appearance.floor_strength, 800);
    assert_eq!(appearance.palette, EcologicalPalette::Boreal as u8);
    assert_eq!(appearance.exposure, NativeExposure::Open as u8);
    assert_eq!(appearance.height_band, NativeHeightBand::Montane as u8);
}

#[wasm_bindgen_test]
fn explicit_scene_contacts_and_picking_use_source_coordinates_and_heights() {
    let cached = CachedChunk::Landscape(landscape());
    let terrain = cached
        .scene_tiles(50, 50)
        .map(|(position, tile, appearance)| terrain_scene_sample(position, tile, appearance))
        .collect();
    let camera = Camera {
        center: [32.5, 33.5],
        zoom: 4.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 7.0,
    };
    let scene = scene::PreparedScene::new(camera, terrain);
    let position = [32.5, 33.5];
    assert_eq!(scene.height(position), Some(7.0));
    let screen = camera.world_to_screen_at_height(position, 7.0);
    let picked = pick_surface_point(&scene.triangles, screen).unwrap();
    assert!((picked[0] - position[0]).abs() < 1e-6);
    assert!((picked[1] - position[1]).abs() < 1e-6);
    assert_eq!(
        cached
            .tile_at(50, 50, picked[0].floor() as i32, picked[1].floor() as i32)
            .unwrap()
            .game_height_level,
        7
    );
}

#[wasm_bindgen_test]
fn physical_metadata_and_dressing_capacity_are_accounted_without_resource_proxy() {
    let mut source = landscape();
    source.tiles.reserve(500);
    source.resources.reserve(23);
    source.decorations.reserve(71);
    let expected = size_of::<CachedChunk>()
        + source.tiles.capacity() * size_of::<LandscapeTile>()
        + source.resources.capacity() * size_of::<LandscapeResource>()
        + source.decorations.capacity() * size_of::<LandscapeDecoration>();
    let cached = CachedChunk::Landscape(source);
    assert_eq!(chunk_resident_bytes(&cached), expected);
    let resources: Vec<_> = cached.resources().collect();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].0.id, 42);
    assert_eq!(resources[0].0.initial_amount, 100);
    assert_eq!(resources[0].1, ResourceVisualFamily::Conifer as u8);
    assert_eq!(cached.decorations().len(), 1);
    assert_eq!(cached.decorations()[0].family, DecorationFamily::Deadwood);
    let dressing = decorations::sample(&cached.decorations()[0], 7.0);
    assert_eq!(dressing.position, [32.5, 33.5]);
    assert_eq!(dressing.family, DecorationFamily::Deadwood as u8);
    assert_eq!(dressing.visual_variant, 3);
    assert_eq!(dressing.orientation, 2);
    assert_eq!(dressing.elevation_meters, 7.0);
    assert_eq!(cached.resources().count(), 1);
    assert_eq!(heights::chunk_height_bounds(&cached), Some((7, 7)));
}

#[wasm_bindgen_test]
fn mixed_cache_eviction_applies_owned_byte_limit_and_releases_all_metadata() {
    let legacy = CachedChunk::Legacy(MapChunkGenerator::new([0; 32], 1, 50).chunk(0, 0).unwrap());
    let limit = chunk_resident_bytes(&legacy);
    let mut chunks = std::collections::BTreeMap::from([
        ((0, 0), legacy),
        ((1, 1), CachedChunk::Landscape(landscape())),
    ]);
    let config = aoe_core::WorldConfig::new(50, 50, aoe_core::Seed(1)).unwrap();
    let camera = Camera {
        center: [1.5, 1.5],
        zoom: 1.0,
        viewport: [800.0, 600.0],
        focus_elevation_meters: 0.0,
    };
    let (removed, _) = evict_distant_chunks_with_limits(
        &mut chunks,
        camera,
        config,
        MAX_CACHED_CHUNKS,
        limit,
        &[],
    );
    assert!(removed);
    assert_eq!(chunks.len(), 1);
    assert!(chunks.contains_key(&(0, 0)));
    assert!(!chunks.contains_key(&(1, 1)));
    assert_eq!(
        chunks.values().map(chunk_resident_bytes).sum::<usize>(),
        limit
    );
}

#[wasm_bindgen_test]
fn unsupported_and_malformed_payloads_do_not_enter_cache() {
    for payload in [
        "",
        "03",
        "03ff",
        "03zz",
        "04",
        "0100",
        "02ff",
        "0300000000000000",
    ] {
        let compact = CompactChunk {
            x: 1,
            y: 1,
            payload_hex: payload.to_owned(),
        };
        assert!(CachedChunk::decode(&compact).is_err(), "accepted {payload}");
    }
}
