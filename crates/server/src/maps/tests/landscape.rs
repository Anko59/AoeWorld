use super::*;

#[tokio::test]
async fn composed_chunks_use_the_current_format_and_authoritative_resources() {
    let request = MapRequest {
        requested_side_meters: 328,
        compression: aoe_map::Ratio::new(2, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    assert_eq!(
        crate::gameplay_map::map_metadata(&package).terrain_schema_version,
        aoe_map::MAP_SCHEMA_VERSION
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
    state
        .map_packages
        .write()
        .await
        .insert(package.content_hash_hex(), package.clone());
    let hash = package.content_hash_hex();
    let generated = chunk(Path((hash.clone(), 2, 2)), State(state.clone()))
        .await
        .expect("chunk")
        .0;
    assert!(generated.payload_hex.starts_with("04"));
    let scene = generated.decode().expect("composed decode");
    assert_eq!(scene.tiles.len(), 18 * 18);
    let generator = package.generator();
    assert_eq!(
        scene,
        generator
            .landscape_chunk_with_cancel(2, 2, &|| false)
            .expect("generated scene")
    );
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
    assert_eq!(
        chunk(Path((package.content_hash_hex(), 3, 0)), State(state))
            .await
            .err(),
        Some(StatusCode::NOT_FOUND)
    );
}
