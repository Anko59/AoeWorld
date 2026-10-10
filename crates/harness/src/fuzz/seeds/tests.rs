use super::*;
use aoe_map::CompactChunk;

#[test]
fn seeds_reach_valid_parsers_and_preserve_discovered_inputs() {
    let root = tempfile::tempdir().unwrap();
    let inventory = prepare(root.path()).unwrap();
    let corpus = root.path().join("fuzz/corpus");
    for name in ["package", "typed-package", "modeled-water"] {
        let bytes = fs::read(corpus.join("map_package").join(name)).unwrap();
        let package: MapPackage = serde_json::from_slice(&bytes).unwrap();
        package.validate().unwrap();
        assert_eq!(package.schema_version, MAP_SCHEMA_VERSION);
        assert_eq!(
            package.generation_recipe_version,
            aoe_map::GENERATION_RECIPE_VERSION
        );
        let canonical = serde_json::to_vec(&package).unwrap();
        let decoded: MapPackage = serde_json::from_slice(&canonical).unwrap();
        assert_eq!(decoded, package);
        if name != "package" {
            assert!(package.environment.hydrology_evidence.is_some());
        }
    }
    let request: MapRequest =
        serde_json::from_slice(&fs::read(corpus.join("map_package/request")).unwrap()).unwrap();
    assert_eq!(request.normalized().unwrap(), request);
    macro_rules! check {
        ($name:expr, $kind:ty) => {
            let page: $kind = serde_json::from_slice(
                &fs::read(corpus.join(concat!("environment_page/", $name))).unwrap(),
            )
            .unwrap();
            page.validate().unwrap();
        };
    }
    check!("elevation", aoe_map::ElevationPage);
    check!("water", aoe_map::WaterPage);
    check!("vegetation", aoe_map::PotentialBiomePage);
    check!("history", aoe_map::HistoricalLandUsePage);
    check!("history-compact-coverage", aoe_map::HistoricalLandUsePage);
    check!("hydrology-evidence", aoe_map::HydrologyEvidencePage);
    check!("modeled-water-page", aoe_map::HydrologyEvidencePage);
    check!("modern-land-cover", aoe_map::ModernLandCoverPage);
    let compact: serde_json::Value = serde_json::from_slice(
        &fs::read(corpus.join("environment_page/history-compact-coverage")).unwrap(),
    )
    .unwrap();
    assert!(compact["coverage"].is_string());
    let chunk = |name: &str| CompactChunk {
        x: 0,
        y: 0,
        payload_hex: fs::read_to_string(corpus.join("map_chunk").join(name)).unwrap(),
    };
    assert_eq!(
        chunk("invalid-hex").decode(),
        Err(aoe_map::CompactChunkError::InvalidHex)
    );
    for name in ["full-chunk-hex", "typed-tile-hex", "empty-hex"] {
        let encoded = chunk(name);
        assert!(encoded.payload_hex.starts_with("04"), "{name}");
        let scene = encoded.decode().unwrap();
        assert_eq!(CompactChunk::encode(&scene).unwrap(), encoded);
        match name {
            "full-chunk-hex" => assert_eq!(scene.tiles.len(), 1024),
            "typed-tile-hex" => {
                assert_eq!(scene.tiles.len(), 1);
                assert_eq!(scene.tiles[0].terrain.modern_land_cover_class, Some(10));
            }
            _ => assert!(scene.tiles.is_empty()),
        }
    }
    for path in [
        "fuzz/corpus/map_package/modeled-water",
        "fuzz/corpus/environment_page/modeled-water-page",
        "fuzz/corpus/environment_page/history-compact-coverage",
        "fuzz/corpus/map_chunk/full-chunk-hex",
    ] {
        assert!(
            inventory
                .prepared_seeds
                .iter()
                .any(|seed| seed.path == path),
            "{path}"
        );
    }
    for checked_in in &CHECKED_IN_SEEDS {
        let path = format!("fuzz/corpus/{}/{}", checked_in.target, checked_in.name);
        assert_eq!(fs::read(root.path().join(&path)).unwrap(), checked_in.bytes);
        let seed = inventory
            .verified_legacy_seeds
            .iter()
            .find(|seed| seed.path == path)
            .expect("verified asset-format seed");
        assert_eq!(
            seed.blake3_hex,
            blake3::hash(checked_in.bytes).to_hex().to_string()
        );
    }
    fs::write(corpus.join("map_chunk/discovered"), b"keep").unwrap();
    prepare(root.path()).unwrap();
    assert_eq!(
        fs::read(corpus.join("map_chunk/discovered")).unwrap(),
        b"keep"
    );
}

#[test]
fn fixed_seed_names_retain_existing_bytes_and_changed_names_stay_stable() {
    let root = tempfile::tempdir().unwrap();
    let first = prepare(root.path()).unwrap();
    let corpus = root.path().join("fuzz/corpus");
    let chunk = corpus.join("map_chunk/full-chunk-hex");
    let generated = corpus.join("map_package/package");
    fs::write(&chunk, b"retained discovered chunk").unwrap();
    fs::write(&generated, b"retained generated seed").unwrap();

    let second = prepare(root.path()).unwrap();
    assert_eq!(fs::read(&chunk).unwrap(), b"retained discovered chunk");
    assert_eq!(fs::read(&generated).unwrap(), b"retained generated seed");
    for (name, target, preferred) in [
        ("full-chunk-hex", "map_chunk", chunk),
        ("package", "map_package", generated),
    ] {
        let retained = second
            .prepared_seeds
            .iter()
            .find(|seed| seed.target == target && seed.name.starts_with(&format!("{name}.")))
            .expect("content-addressed replacement seed");
        assert_ne!(preferred, root.path().join(&retained.path));
        assert!(
            first
                .prepared_seeds
                .iter()
                .all(|seed| seed.path != retained.path)
        );
        assert_eq!(
            retained.bytes,
            fs::read(root.path().join(&retained.path)).unwrap().len()
        );
    }

    let count = fs::read_dir(corpus.join("map_chunk")).unwrap().count()
        + fs::read_dir(corpus.join("map_package")).unwrap().count();
    let third = prepare(root.path()).unwrap();
    assert_eq!(second, third);
    assert_eq!(
        count,
        fs::read_dir(corpus.join("map_chunk")).unwrap().count()
            + fs::read_dir(corpus.join("map_package")).unwrap().count()
    );
}

#[test]
fn changed_asset_format_blob_is_preserved_and_blocks_the_fuzz_campaign() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("fuzz/corpus/drs");
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join("one-entry.drs"), b"preserved older blob").unwrap();
    assert!(verify_legacy_seeds(root.path()).is_err());
    assert_eq!(
        fs::read(directory.join("one-entry.drs")).unwrap(),
        b"preserved older blob"
    );
}
