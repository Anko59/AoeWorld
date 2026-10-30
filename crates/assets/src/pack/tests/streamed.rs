use super::*;

fn mixed_pack(root: &Path) -> PathBuf {
    atlas::write_pack_streamed(
        root,
        "c".repeat(64),
        &[SpriteRef {
            id: "native".into(),
            hash: "a".repeat(64),
            frames: 3,
        }],
        Vec::new(),
        |_, frame| frame != 0,
        |_, frame| {
            let mut planes = atlas::Planes::blank(2, 1, -8, 30 + frame as i32);
            planes.color = vec![12, 34, 56, 200, 0, 0, 0, 0];
            planes.player = vec![7, 0, 0, 119, 0, 0, 0, 0];
            planes.shadow = vec![0, 0, 0, 77, 0, 0, 0, 0];
            Ok(planes)
        },
    )
    .unwrap()
}

#[test]
fn streamed_pack_keeps_full_pixels_and_distinct_hotspots_without_png_residency() {
    let root = tempfile::tempdir().unwrap();
    let path = mixed_pack(root.path());
    let manifest = verify(&path).unwrap();
    assert_eq!(manifest.pages.len(), 1);
    assert_eq!(manifest.chunks.len(), 1);
    assert_eq!(manifest.frames.len(), 3);
    assert!(manifest.frames[0].native.is_none());
    let first = &manifest.frames[1];
    let second = &manifest.frames[2];
    assert_eq!(first.native, second.native, "identical bytes share storage");
    assert_eq!((first.anchor_x, first.anchor_y), (-8, 31));
    assert_eq!(second.anchor_y, 32);
    assert_eq!((first.page, first.x, first.y), (0, 0, 0));
    let location = first.native.unwrap();
    let bytes = fs::read(path.join(&manifest.chunks[location.chunk as usize].file)).unwrap();
    let planes = compressed::decode_frame(location.slice(&bytes).unwrap(), [2, 1]).unwrap();
    assert_eq!(planes[0], [12, 34, 56, 200, 0, 0, 0, 0]);
    assert_eq!(planes[1][3], 119);
    assert_eq!(planes[2][3], 77);
    fs::write(path.join(&manifest.chunks[0].file), b"tampered").unwrap();
    assert!(verify(&path).is_err());
}

#[test]
fn streamed_manifest_rejects_unsafe_chunks_and_unbounded_or_ambiguous_references() {
    let root = tempfile::tempdir().unwrap();
    let path = mixed_pack(root.path());
    let manifest = verify(&path).unwrap();
    let original = serde_json::to_value(&manifest).unwrap();
    for (pointer, replacement) in [
        ("/chunks/0/file", serde_json::json!("../escape.bin")),
        ("/chunks/0/file", serde_json::json!("..")),
        ("/chunks/0/hash", serde_json::json!("bad")),
        ("/chunks/0/bytes", serde_json::json!(0)),
        ("/chunks/0/bytes", serde_json::json!(67_108_865)),
        ("/frames/1/native/chunk", serde_json::json!(1)),
        ("/frames/1/native/offset", serde_json::json!(u32::MAX)),
        ("/frames/1/native/length", serde_json::json!(0)),
        ("/frames/1/x", serde_json::json!(1)),
        ("/frames/1/width", serde_json::json!(2045)),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            parse_manifest(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{pointer}"
        );
    }
    let mut value = original;
    value["frames"][1]["width"] = 3.into();
    fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert!(
        verify(&path).is_err(),
        "payload dimensions must match metadata"
    );
}
