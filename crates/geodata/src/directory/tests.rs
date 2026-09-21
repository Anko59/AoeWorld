use super::*;
use aoe_map::{
    ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage, MapRequest,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, SourceLock, ordered_page_root,
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
    }
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
