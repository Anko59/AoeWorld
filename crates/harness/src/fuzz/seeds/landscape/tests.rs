use super::*;

#[test]
fn ascii_scene_seeds_reach_both_readers_without_rewriting_existing_corpus() {
    let root = tempfile::tempdir().unwrap();
    let mut seeds = Vec::new();
    prepare(root.path(), &mut seeds).unwrap();
    assert_eq!(seeds.len(), 4);
    assert_eq!(
        seeds
            .iter()
            .filter(|seed| seed.target == "map_chunk")
            .count(),
        2
    );
    for seed in seeds.iter().filter(|seed| seed.target == "map_chunk") {
        let payload_hex = fs::read_to_string(root.path().join(&seed.path)).unwrap();
        let encoded = CompactChunk {
            x: 0,
            y: 0,
            payload_hex,
        };
        let scene = encoded.decode_landscape().unwrap();
        if seed.name == "landscape-v3-hex" {
            assert!(
                encoded.decode().is_err(),
                "legacy callers must not silently lose metadata"
            );
            assert!(scene.tiles[0].appearance.is_some());
            assert_eq!(
                CompactChunk::encode_landscape(&scene)
                    .unwrap()
                    .decode_landscape()
                    .unwrap(),
                scene
            );
        } else {
            assert!(encoded.decode().is_ok());
            assert!(scene.tiles[0].appearance.is_none());
        }
    }
    for seed in seeds.iter().filter(|seed| seed.target == "map_package") {
        let bytes = fs::read(root.path().join(&seed.path)).unwrap();
        if seed.name == "landscape-schema10-package" {
            let package: MapPackage = serde_json::from_slice(&bytes).unwrap();
            package.validate().unwrap();
            assert_eq!(package.schema_version, 10);
            assert_eq!(package.generation_recipe_version, 9);
            assert_eq!(package.generator_version, MAP_SCHEMA_VERSION);
            assert_eq!(
                package.request.detail_profile,
                aoe_map::DetailProfile::LandscapeV2
            );
            assert_eq!(
                serde_json::from_slice::<MapPackage>(&serde_json::to_vec(&package).unwrap())
                    .unwrap(),
                package
            );
        } else {
            assert_eq!(seed.name, "landscape-v2-request");
            let request: MapRequest = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request.detail_profile, aoe_map::DetailProfile::LandscapeV2);
            assert_eq!(request.schema_version, 1);
            assert_eq!(request.normalized().unwrap(), request);
            assert_eq!(
                serde_json::from_slice::<MapRequest>(&serde_json::to_vec(&request).unwrap())
                    .unwrap(),
                request
            );
        }
    }
    let original = root.path().join("fuzz/corpus/map_chunk/landscape-v3-hex");
    fs::write(&original, b"retained-old-corpus-byte-sequence").unwrap();
    let mut repeated = Vec::new();
    prepare(root.path(), &mut repeated).unwrap();
    assert_eq!(
        fs::read(original).unwrap(),
        b"retained-old-corpus-byte-sequence"
    );
    assert!(repeated[0].name.starts_with("landscape-v3-hex."));
    let hex = fs::read_to_string(root.path().join(&repeated[0].path)).unwrap();
    assert!(
        CompactChunk {
            x: 0,
            y: 0,
            payload_hex: hex
        }
        .decode_landscape()
        .is_ok()
    );
}
