#[path = "../maps.rs"]
mod proof;

#[test]
fn building_proof_maps_isolates_each_requested_package() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first");
    let second = root.path().join("second");
    let output = root.path().join("prepared");
    let bookmarks = root.path().join("bookmarks.json");
    for (directory, name, pixels) in [
        (&first, "first.json", b"first"),
        (&second, "second.json", b"other"),
    ] {
        std::fs::create_dir_all(directory.join("pages")).unwrap();
        std::fs::write(directory.join("pages/shared.bin"), pixels).unwrap();
        std::fs::write(
            directory.join(name),
            br#"{"request":{},"estimate":{"tiles_per_side":64}}"#,
        )
        .unwrap();
    }
    std::fs::write(first.join("pages/stale.bin"), b"old").unwrap();
    proof::prepare(first.join("first.json"), &output, &bookmarks).unwrap();
    proof::prepare(second.join("second.json"), &output, &bookmarks).unwrap();
    assert!(
        !output.join("first.json").exists(),
        "previous package retained"
    );
    assert!(output.join("second.json").exists());
    assert!(
        !output.join("pages/stale.bin").exists(),
        "previous page retained"
    );
    assert_eq!(
        std::fs::read(output.join("pages/shared.bin")).unwrap(),
        b"other"
    );
    assert_eq!(
        std::fs::read(first.join("pages/shared.bin")).unwrap(),
        b"first"
    );
    assert!(proof::prepare(output.join("second.json"), &output, &bookmarks).is_err());
    assert!(
        output.join("second.json").exists(),
        "overlap must preserve source"
    );
    assert_eq!(
        std::fs::read(output.join("pages/shared.bin")).unwrap(),
        b"other"
    );
}

#[test]
fn building_proof_replaces_store_symlink_without_modifying_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("original");
    let output = root.path().join("prepared");
    std::fs::create_dir_all(source.join("pages")).unwrap();
    std::fs::write(source.join("pages/data.bin"), b"original").unwrap();
    std::fs::write(
        source.join("map.json"),
        br#"{"request":{},"estimate":{"tiles_per_side":64}}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&source, &output).unwrap();
    proof::prepare(
        source.join("map.json"),
        &output,
        &root.path().join("bookmarks.json"),
    )
    .unwrap();
    assert!(
        !std::fs::symlink_metadata(&output)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read(source.join("pages/data.bin")).unwrap(),
        b"original"
    );
    assert_eq!(
        std::fs::read(output.join("pages/data.bin")).unwrap(),
        b"original"
    );
}

#[test]
fn building_proof_documented_capture_request_has_required_revision() {
    let guide = include_str!("../../../../../docs/assets/game-data.md");
    let section = guide.split("For animated villages,").nth(1).unwrap();
    let example = section
        .split("```json\n")
        .nth(1)
        .expect("capture guide needs a directly usable request JSON example")
        .split("```")
        .next()
        .unwrap();
    let request: serde_json::Value = serde_json::from_str(example).unwrap();
    for key in ["url", "x", "y", "revision"] {
        assert!(!request[key].is_null(), "missing required {key}");
    }
    assert!(!request["revision"].as_str().unwrap().is_empty());
    let browser = include_str!("../../../../../browser/tests/proofs/building-village.spec.ts");
    assert!(browser.contains("expect(health.build).toBe(config.revision)"));
}
