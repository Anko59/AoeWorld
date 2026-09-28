use super::*;
use aoe_map::{
    ElevationPage, EnvironmentalProvenance, FieldPyramid, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyWaterPolicy,
    MapPackage, MapRequest, ModernLandCoverPage, PreparedEnvironment, ProjectionMetadata,
    PyramidLevel, SourceLock, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
    ordered_page_root,
};
use std::path::PathBuf;

fn test_directory() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "aoe-directory-test-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir(&root).expect("root");
    root
}

fn fallback_map() -> GeneratedMap {
    GeneratedMap {
        package: MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package"),
        elevation_pages: Vec::new(),
        water_pages: Vec::new(),
        vegetation_pages: Vec::new(),
        historical_land_use_pages: Vec::new(),
        hydrology_evidence_pages: Vec::new(),
        modern_land_cover_pages: Vec::new(),
    }
}

fn prepared_map() -> GeneratedMap {
    let page = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![123],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_page_root(std::slice::from_ref(&page)).expect("root"),
            }],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("package");
    GeneratedMap {
        package,
        elevation_pages: vec![page],
        water_pages: Vec::new(),
        vegetation_pages: Vec::new(),
        historical_land_use_pages: Vec::new(),
        hydrology_evidence_pages: Vec::new(),
        modern_land_cover_pages: Vec::new(),
    }
}

#[test]
fn schema_eight_generated_map_keeps_vector_page_validation() {
    let mut legacy = prepared_map();
    legacy.package = MapPackage::with_prepared_environment(
        8,
        legacy.package.request,
        legacy.package.source_locks.clone(),
        legacy.package.projection.clone(),
        legacy.package.provenance.clone(),
        legacy.package.environment.clone(),
    )
    .expect("legacy generator package");
    legacy.package.schema_version = aoe_map::LEGACY_MAP_SCHEMA_VERSION;
    let identity = legacy.package.content_hash;
    legacy.validate().expect("legacy vectors validate");
    assert_eq!(legacy.package.content_hash, identity);

    let mut malformed = legacy;
    malformed.elevation_pages[0].geographic_height_centimeters[0] += 1;
    assert!(malformed.validate().is_err());
}

fn fallback_with_acquisition_time(acquired_at: &str) -> GeneratedMap {
    let source = SourceLock {
        id: "test-source".to_owned(),
        provider: "test-provider".to_owned(),
        release: "test-release".to_owned(),
        url: "https://example.invalid/source".to_owned(),
        sha256: [7; 32],
        acquired_at: acquired_at.to_owned(),
        native_resolution: "test".to_owned(),
        crs: "test".to_owned(),
        vertical_datum: "test".to_owned(),
        license: "test".to_owned(),
        preprocessing_version: "test".to_owned(),
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        vec![source],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment::default(),
    )
    .expect("package");
    GeneratedMap {
        package,
        elevation_pages: Vec::new(),
        water_pages: Vec::new(),
        vegetation_pages: Vec::new(),
        historical_land_use_pages: Vec::new(),
        hydrology_evidence_pages: Vec::new(),
        modern_land_cover_pages: Vec::new(),
    }
}

#[test]
fn fallback_directory_round_trips_and_publishes_a_manifest() {
    let root = test_directory();
    let map = fallback_map();
    let hash = map.package.content_hash_hex();
    map.write_directory(&root).expect("write");
    GeneratedMap::verify_directory(&root, &hash).expect("verify");
    assert_eq!(
        GeneratedMap::read_directory(&root, &hash).expect("read"),
        map
    );
    assert!(root.join(format!("{hash}.json")).is_file());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn noncanonical_package_hash_is_rejected() {
    let root = test_directory();
    let map = fallback_map();
    let hash = map.package.content_hash_hex();
    map.write_directory(&root).expect("write");
    assert!(GeneratedMap::verify_directory(&root, "../escape").is_err());
    assert!(GeneratedMap::verify_directory(&root, &hash).is_ok());
    fs::remove_dir_all(root).expect("cleanup");
}

#[cfg(unix)]
#[test]
fn redirected_parent_is_rejected_before_creating_directories() {
    let root = test_directory();
    let target = root.join("target");
    fs::create_dir_all(&target).expect("target");
    let link = root.join("link");
    std::os::unix::fs::symlink(&target, &link).expect("link");
    assert!(
        fallback_map()
            .write_directory(&link.join("new-map"))
            .is_err()
    );
    assert!(!target.join("new-map").exists());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn relative_output_directory_is_supported() {
    let absolute = test_directory();
    let name = absolute.file_name().expect("unique name");
    let root = PathBuf::from("target").join(name);
    fallback_map()
        .write_directory(&root)
        .expect("relative write");
    GeneratedMap::verify_directory(&root, &fallback_map().package.content_hash_hex())
        .expect("relative verify");
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn page_missing_corrupt_or_misindexed_is_rejected() {
    for mode in ["missing", "corrupt", "coordinates", "shape"] {
        let root = test_directory();
        let map = prepared_map();
        let hash = map.package.content_hash_hex();
        map.write_directory(&root).expect("write");
        let path = root.join(page_file(
            &hash,
            PageKey {
                layer: DirectoryLayer::Elevation,
                level: 0,
                x: 0,
                y: 0,
            },
        ));
        match mode {
            "missing" => fs::remove_file(&path).expect("remove"),
            "corrupt" => fs::write(&path, b"not-json").expect("corrupt"),
            "coordinates" => {
                let mut page = map.elevation_pages[0].clone();
                page.x = 1;
                fs::write(&path, serde_json::to_vec(&page).expect("encode")).expect("tamper");
            }
            "shape" => {
                let mut page = map.elevation_pages[0].clone();
                page.width = 2;
                page.geographic_height_centimeters.push(124);
                fs::write(&path, serde_json::to_vec(&page).expect("encode")).expect("tamper");
            }
            _ => unreachable!(),
        }
        assert!(
            GeneratedMap::verify_directory(&root, &hash).is_err(),
            "{mode}"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}

#[test]
fn repeated_publication_reuses_the_canonical_manifest_identity() {
    let root = test_directory();
    let first = fallback_with_acquisition_time("2026-09-20T00:00:00Z");
    let second = fallback_with_acquisition_time("2026-09-21T00:00:00Z");
    assert_eq!(first.package.content_hash, second.package.content_hash);
    first.write_directory(&root).expect("first write");
    second.write_directory(&root).expect("idempotent write");
    let hash = first.package.content_hash_hex();
    let loaded = GeneratedMap::read_directory(&root, &hash).expect("read");
    assert_eq!(loaded.package, first.package);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn rehashed_wrong_page_shape_is_rejected_before_root_acceptance() {
    let root = test_directory();
    let mut map = prepared_map();
    let mut page = map.elevation_pages[0].clone();
    page.width = 2;
    page.geographic_height_centimeters.push(124);
    map.elevation_pages[0] = page.clone();
    let mut environment = map.package.environment.clone();
    environment.elevation.levels[0].ordered_page_root =
        ordered_page_root(std::slice::from_ref(&page)).expect("root");
    map.package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("package");
    let hash = map.package.content_hash_hex();
    fs::create_dir_all(&root).expect("root");
    write_directory_contents(&map, &root).expect("pages");
    fs::write(
        root.join(format!("{hash}.json")),
        serde_json::to_vec(&map.package).expect("manifest"),
    )
    .expect("manifest");
    assert!(GeneratedMap::verify_directory(&root, &hash).is_err());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn typed_evidence_pages_round_trip_and_stream_verify_on_independent_axis() {
    let root = test_directory();
    let base = prepared_map();
    let hydrology = vec![HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![
            HydrologyKind::River as u8,
            HydrologyKind::RegulatedLake as u8,
            HydrologyKind::River as u8,
            HydrologyKind::NoEvidence as u8,
        ],
        method: vec![
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8,
            HydrologyEvidenceMethod::HydroLakesExtent as u8,
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8,
            HydrologyEvidenceMethod::None as u8,
        ],
        water_model: None,
    }];
    let land_cover = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![10, 40, 80, 0],
    }];
    let mut environment = base.package.environment.clone();
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 2,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(&hydrology).expect("hydrology root"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(&land_cover)
            .expect("land-cover root"),
        water_model: None,
    });
    let package = MapPackage::with_prepared_environment(
        base.package.generator_version,
        base.package.request,
        base.package.source_locks.clone(),
        base.package.projection.clone(),
        base.package.provenance.clone(),
        environment,
    )
    .expect("typed package");
    let map = GeneratedMap {
        package,
        hydrology_evidence_pages: hydrology,
        modern_land_cover_pages: land_cover,
        ..base
    };
    let package_before_validation = map.package.clone();
    map.validate().expect("typed generated map validates");
    assert_eq!(map.package, package_before_validation);

    let mut missing_page = map.clone();
    missing_page.hydrology_evidence_pages.clear();
    assert!(missing_page.validate().is_err());
    let mut malformed_page = map.clone();
    malformed_page.modern_land_cover_pages[0].worldcover_class[0] = 11;
    assert!(malformed_page.validate().is_err());

    let hash = map.package.content_hash_hex();
    map.write_directory(&root).expect("write typed package");
    GeneratedMap::verify_directory(&root, &hash).expect("stream verify typed pages");
    assert_eq!(
        GeneratedMap::read_directory(&root, &hash).expect("read typed pages"),
        map
    );
    let mut changed = map.hydrology_evidence_pages[0].clone();
    changed.kind[0] = HydrologyKind::Land as u8;
    changed.method[0] = HydrologyEvidenceMethod::WorldCoverClass as u8;
    fs::write(
        root.join("pages")
            .join(&hash)
            .join("hydrology-evidence/0-0-0.json"),
        serde_json::to_vec(&changed).expect("changed page JSON"),
    )
    .expect("replace typed evidence page");
    assert!(GeneratedMap::verify_directory(&root, &hash).is_err());
    fs::remove_dir_all(root).expect("cleanup");
}
