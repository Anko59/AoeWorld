use super::*;

#[test]
fn ascii_scene_seeds_reach_both_readers_without_rewriting_existing_corpus() {
    let root = tempfile::tempdir().unwrap();
    let mut seeds = Vec::new();
    prepare(root.path(), &mut seeds).unwrap();
    assert_eq!(seeds.len(), 2);
    for seed in &seeds {
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
