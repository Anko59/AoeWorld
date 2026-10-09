use super::*;

#[tokio::test]
async fn composed_chunks_use_format_three_and_same_authoritative_resources() {
    let request = MapRequest {
        requested_side_meters: 328,
        compression: aoe_map::Ratio::new(2, 1).expect("ratio"),
        detail_profile: aoe_map::DetailProfile::LandscapeV2,
        ..MapRequest::default()
    };
    let package =
        MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("landscape package");
    let standard = MapPackage::new(
        MAP_SCHEMA_VERSION,
        MapRequest {
            detail_profile: aoe_map::DetailProfile::StandardV1,
            ..package.request
        },
        Vec::new(),
    )
    .expect("standard package");
    assert_ne!(package.content_hash, standard.content_hash);
    assert_eq!(
        crate::gameplay_map::map_metadata(&package).terrain_schema_version,
        aoe_map::LANDSCAPE_MAP_SCHEMA_VERSION
    );
    assert_eq!(
        crate::gameplay_map::map_metadata(&standard).terrain_schema_version,
        standard.schema_version
    );
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = crate::AppState::new(&config, "landscape-chunk-contract").expect("state");
    for value in [&package, &standard] {
        state
            .map_packages
            .write()
            .await
            .insert(value.content_hash_hex(), value.clone());
    }
    let hash = package.content_hash_hex();
    let generated = chunk(Path((hash.clone(), 2, 2)), State(state.clone()))
        .await
        .expect("chunk")
        .0;
    assert!(generated.payload_hex.starts_with("03"));
    assert!(
        generated.decode().is_err(),
        "legacy reader must not erase metadata"
    );
    let scene = generated.decode_landscape().expect("composed decode");
    assert_eq!(scene.tiles.len(), 18 * 18);
    assert!(scene.tiles.iter().all(|tile| tile.appearance.is_some()));
    let generator = package.generator();
    for resource in &scene.resources {
        assert_eq!(
            generator
                .object_at_with_cancel(resource.node.tile, &|| false)
                .expect("object"),
            Some(resource.node)
        );
    }
    let cached = chunk(Path((hash, 2, 2)), State(state.clone()))
        .await
        .expect("cached")
        .0;
    assert_eq!(cached, generated);
    let published = chunk(
        Path((standard.content_hash_hex(), 2, 2)),
        State(state.clone()),
    )
    .await
    .expect("standard chunk")
    .0;
    assert!(published.payload_hex.starts_with("02"));
    let legacy = standard
        .generator()
        .chunk(2, 2)
        .expect("legacy generated chunk");
    assert_eq!(published.decode().expect("legacy decode"), legacy);
    for (tile, base) in scene.tiles.iter().zip(&legacy.tiles) {
        assert_eq!(
            tile.terrain.geographic_height_centimeters,
            base.geographic_height_centimeters
        );
        assert_eq!(tile.terrain.surface, base.surface);
        assert_eq!(tile.terrain.elevation_provenance, base.elevation_provenance);
        assert_eq!(tile.terrain.water_provenance, base.water_provenance);
        assert_eq!(
            tile.terrain.vegetation_provenance,
            base.vegetation_provenance
        );
        assert_eq!(
            tile.terrain.hydrology_observation,
            base.hydrology_observation
        );
        assert_eq!(
            tile.terrain.modern_land_cover_class,
            base.modern_land_cover_class
        );
        assert_eq!(tile.terrain.passable, base.passable);
    }
    assert_eq!(
        chunk(Path((package.content_hash_hex(), 3, 0)), State(state))
            .await
            .err(),
        Some(StatusCode::NOT_FOUND)
    );
}
