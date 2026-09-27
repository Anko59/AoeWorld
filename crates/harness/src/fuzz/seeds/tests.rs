use super::*;
#[test]
fn seeds_reach_valid_parsers_and_preserve_discovered_inputs() {
    let root = tempfile::tempdir().unwrap();
    let inventory = prepare(root.path()).unwrap();
    let corpus = root.path().join("fuzz/corpus");
    let package: MapPackage =
        serde_json::from_slice(&fs::read(corpus.join("map_package/package")).unwrap()).unwrap();
    package.validate().unwrap();
    for name in ["typed-package", "schema8-default"] {
        let package: MapPackage =
            serde_json::from_slice(&fs::read(corpus.join("map_package").join(name)).unwrap())
                .unwrap();
        package.validate().unwrap();
        if name == "typed-package" {
            assert_eq!(package.schema_version, MAP_SCHEMA_VERSION);
            assert_eq!(package.schema_version, 9);
            assert!(package.environment.hydrology_evidence.is_some());
        }
    }
    for name in ["modeled-water-v1", "modeled-water-v2"] {
        let package: MapPackage =
            serde_json::from_slice(&fs::read(corpus.join("map_package").join(name)).unwrap())
                .unwrap();
        package.validate().unwrap();
    }
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
    check!("history-legacy-coverage", aoe_map::HistoricalLandUsePage);
    check!("hydrology-evidence", aoe_map::HydrologyEvidencePage);
    check!("schema9-typed-hydrology", aoe_map::HydrologyEvidencePage);
    check!("modeled-water-page-v1", aoe_map::HydrologyEvidencePage);
    check!("modeled-water-page-v2", aoe_map::HydrologyEvidencePage);
    check!("modern-land-cover", aoe_map::ModernLandCoverPage);
    check!("schema9-modern-land-cover", aoe_map::ModernLandCoverPage);
    let invalid = fs::read(corpus.join("map_chunk/invalid-hex")).unwrap();
    assert_eq!(
        CompactChunk {
            x: 0,
            y: 0,
            payload_hex: String::from_utf8_lossy(&invalid).into_owned(),
        }
        .decode(),
        Err(aoe_map::CompactChunkError::InvalidHex)
    );
    for path in [
        "fuzz/corpus/environment_page/schema9-typed-hydrology",
        "fuzz/corpus/environment_page/schema9-modern-land-cover",
        "fuzz/corpus/environment_page/modeled-water-page-v1",
        "fuzz/corpus/environment_page/modeled-water-page-v2",
        "fuzz/corpus/environment_page/history-compact-coverage",
        "fuzz/corpus/environment_page/history-legacy-coverage",
        "fuzz/corpus/map_package/modeled-water-v1",
        "fuzz/corpus/map_package/modeled-water-v2",
        "fuzz/corpus/map_chunk/one-tile-v1",
    ] {
        assert!(
            inventory
                .prepared_seeds
                .iter()
                .any(|seed| seed.path == path)
        );
    }
    for legacy in &LEGACY_SEEDS {
        let path = format!("fuzz/corpus/{}/{}", legacy.target, legacy.name);
        assert_eq!(fs::read(root.path().join(&path)).unwrap(), legacy.bytes);
        let seed = inventory
            .verified_legacy_seeds
            .iter()
            .find(|seed| seed.path == path)
            .expect("verified legacy seed");
        assert_eq!(
            seed.blake3_hex,
            blake3::hash(legacy.bytes).to_hex().to_string()
        );
    }
    let hydrology_alias = fs::read(corpus.join("environment_page/hydrology-evidence")).unwrap();
    let hydrology_schema9 =
        fs::read(corpus.join("environment_page/schema9-typed-hydrology")).unwrap();
    let land_cover_alias = fs::read(corpus.join("environment_page/modern-land-cover")).unwrap();
    let land_cover_schema9 =
        fs::read(corpus.join("environment_page/schema9-modern-land-cover")).unwrap();
    assert_eq!(hydrology_alias, hydrology_schema9);
    assert_eq!(land_cover_alias, land_cover_schema9);
    assert_ne!(hydrology_schema9, land_cover_schema9);
    let hydrology: aoe_map::HydrologyEvidencePage =
        serde_json::from_slice(&hydrology_schema9).unwrap();
    let land_cover: aoe_map::ModernLandCoverPage =
        serde_json::from_slice(&land_cover_schema9).unwrap();
    assert_ne!(
        hydrology.content_hash().unwrap(),
        land_cover.content_hash().unwrap()
    );
    for name in ["one-tile-v1", "typed-v2"] {
        let bytes = fs::read(corpus.join("map_chunk").join(name)).unwrap();
        assert_eq!(bytes[0], if name == "one-tile-v1" { 1 } else { 2 });
        let payload_hex = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let chunk = CompactChunk {
            x: 0,
            y: 0,
            payload_hex,
        }
        .decode()
        .unwrap();
        assert_eq!(chunk.tiles.len(), 1);
        assert_eq!(
            chunk.tiles[0].modern_land_cover_class,
            if name == "typed-v2" { Some(10) } else { None },
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
fn modeled_water_versions_and_historical_coverage_seeds_roundtrip_hashes() {
    let root = tempfile::tempdir().unwrap();
    let inventory = prepare(root.path()).unwrap();
    let corpus = root.path().join("fuzz/corpus");
    let package = |name: &str| {
        let bytes = fs::read(corpus.join("map_package").join(name)).unwrap();
        let parsed: MapPackage = serde_json::from_slice(&bytes).unwrap();
        parsed.validate().unwrap();
        let canonical = serde_json::to_vec(&parsed).unwrap();
        let decoded: MapPackage = serde_json::from_slice(&canonical).unwrap();
        assert_eq!(decoded, parsed);
        decoded.validate().unwrap();
        assert_eq!(decoded.content_hash, parsed.content_hash);
        parsed
    };
    let legacy_package = package("modeled-water-v1");
    let current_package = package("modeled-water-v2");
    assert_eq!(
        legacy_package.generation_recipe_version,
        aoe_map::WATER_MODEL_GENERATION_RECIPE_VERSION
    );
    assert_eq!(
        current_package.generation_recipe_version,
        aoe_map::WATER_MODEL_GENERATION_RECIPE_VERSION
    );
    let model_version = |package: &MapPackage| {
        package
            .environment
            .hydrology_evidence
            .as_ref()
            .and_then(|index| index.water_model.as_ref())
            .expect("modeled water index")
            .model_version
    };
    assert_eq!(model_version(&legacy_package), 1);
    assert_eq!(
        model_version(&current_package),
        aoe_map::HYDROLOGY_WATER_MODEL_VERSION
    );
    let legacy_model = legacy_package
        .environment
        .hydrology_evidence
        .as_ref()
        .and_then(|index| index.water_model.as_ref())
        .expect("legacy modeled water index");
    let serialized_model = serde_json::to_vec(legacy_model).unwrap();
    let decoded_model: aoe_map::HydrologyWaterModelIndex =
        serde_json::from_slice(&serialized_model).unwrap();
    assert_eq!(
        decoded_model.digest().unwrap(),
        legacy_model.digest().unwrap()
    );
    assert_ne!(legacy_package.content_hash, current_package.content_hash);

    let modeled_page = |name: &str| {
        let bytes = fs::read(corpus.join("environment_page").join(name)).unwrap();
        let page: aoe_map::HydrologyEvidencePage = serde_json::from_slice(&bytes).unwrap();
        page.validate().unwrap();
        let decoded: aoe_map::HydrologyEvidencePage =
            serde_json::from_slice(&serde_json::to_vec(&page).unwrap()).unwrap();
        assert_eq!(decoded, page);
        assert_eq!(
            decoded.content_hash().unwrap(),
            page.content_hash().unwrap()
        );
        page
    };
    assert_eq!(
        modeled_page("modeled-water-page-v1")
            .content_hash()
            .unwrap(),
        modeled_page("modeled-water-page-v2")
            .content_hash()
            .unwrap()
    );

    let compact_bytes = fs::read(corpus.join("environment_page/history-compact-coverage")).unwrap();
    let compact_json: serde_json::Value = serde_json::from_slice(&compact_bytes).unwrap();
    assert!(compact_json["coverage"].is_string());
    let compact: aoe_map::HistoricalLandUsePage = serde_json::from_slice(&compact_bytes).unwrap();
    let legacy: aoe_map::HistoricalLandUsePage = serde_json::from_slice(
        &fs::read(corpus.join("environment_page/history-legacy-coverage")).unwrap(),
    )
    .unwrap();
    compact.validate().unwrap();
    legacy.validate().unwrap();
    assert_eq!(compact, legacy);
    assert_eq!(
        compact.content_hash().unwrap(),
        legacy.content_hash().unwrap()
    );
    for seed in [
        "fuzz/corpus/map_package/modeled-water-v1",
        "fuzz/corpus/map_package/modeled-water-v2",
        "fuzz/corpus/environment_page/modeled-water-page-v1",
        "fuzz/corpus/environment_page/modeled-water-page-v2",
        "fuzz/corpus/environment_page/history-compact-coverage",
        "fuzz/corpus/environment_page/history-legacy-coverage",
    ] {
        assert!(
            inventory
                .prepared_seeds
                .iter()
                .any(|item| item.path == seed)
        );
    }
}

#[test]
fn fixed_seed_names_retain_existing_bytes_and_changed_names_stay_stable() {
    let root = tempfile::tempdir().unwrap();
    let first = prepare(root.path()).unwrap();
    let corpus = root.path().join("fuzz/corpus");
    let legacy = corpus.join("map_chunk/one-tile-v1");
    let generated = corpus.join("map_package/package");
    let captured_legacy = fs::read(&legacy).unwrap();
    assert_eq!(captured_legacy.first(), Some(&1));
    fs::write(&legacy, b"retained legacy seed").unwrap();
    fs::write(&generated, b"retained generated seed").unwrap();

    let second = prepare(root.path()).unwrap();
    assert_eq!(fs::read(&legacy).unwrap(), b"retained legacy seed");
    assert_eq!(fs::read(&generated).unwrap(), b"retained generated seed");
    for (name, preferred, inventory) in [
        ("one-tile-v1", legacy, &first),
        ("package", generated, &first),
    ] {
        let retained = second
            .prepared_seeds
            .iter()
            .find(|seed| {
                seed.target
                    == if name == "package" {
                        "map_package"
                    } else {
                        "map_chunk"
                    }
                    && seed.name.starts_with(&format!("{name}."))
            })
            .expect("content-addressed replacement seed");
        assert_ne!(preferred, root.path().join(&retained.path));
        assert!(
            inventory
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
fn changed_legacy_blob_is_preserved_and_blocks_the_fuzz_campaign() {
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
