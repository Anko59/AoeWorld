#[cfg(test)]
use super::{
    ElevationRange, MAX_PACKAGE_PAGE_BYTES, MAX_PACKAGE_PAGES, WaterEvidence, copy_tree,
    qualify_case, read_elevation_range, read_water_evidence, valid_hash,
    validate_chunk_cache_bound, validate_page_budget, validate_source_locked_package,
};
#[cfg(test)]
use std::fs;

#[test]
fn qualification_requires_the_region_specific_water_and_relief() {
    let no_water = WaterEvidence {
        sample_count: 10,
        ocean_nonzero_samples: 0,
        inland_nonzero_samples: 0,
        ocean_coverage_percent_sum: 0,
        inland_coverage_percent_sum: 0,
    };
    let relief = ElevationRange {
        minimum_centimeters: 0,
        maximum_centimeters: 150_000,
        level_zero_samples: 4,
    };
    assert!(qualify_case("river_lake", "inland", Some(&no_water), &relief).is_err());
    assert!(qualify_case("nile_delta_coast", "ocean", Some(&no_water), &relief).is_err());
    assert!(qualify_case("alpine_relief", "relief", None, &relief).is_ok());
    assert!(
        qualify_case(
            "alpine_relief",
            "relief",
            None,
            &ElevationRange {
                maximum_centimeters: 149_999,
                ..relief
            }
        )
        .is_err()
    );
    let inland = WaterEvidence {
        inland_nonzero_samples: 1,
        inland_coverage_percent_sum: 12,
        ..no_water.clone()
    };
    let ocean = WaterEvidence {
        ocean_nonzero_samples: 1,
        ocean_coverage_percent_sum: 37,
        ..no_water
    };
    assert!(qualify_case("river_lake", "inland", Some(&inland), &relief).is_ok());
    assert!(qualify_case("nile_delta_coast", "ocean", Some(&ocean), &relief).is_ok());
}

#[test]
fn only_lowercase_sha256_content_hashes_are_accepted() {
    assert!(valid_hash(&"a".repeat(64)));
    assert!(!valid_hash(&"A".repeat(64)));
    assert!(!valid_hash(&"a".repeat(63)));
}

#[test]
fn source_pages_stage_under_a_new_nested_package_directory() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let source = temporary.path().join("source");
    fs::create_dir_all(source.join("elevation")).expect("source page directory");
    fs::write(source.join("elevation/0-0-0.json"), b"page").expect("source page");
    let target = temporary.path().join("staging/pages/hash/pages/hash");

    copy_tree(&source, &target).expect("copy nested page tree");

    assert_eq!(
        fs::read(target.join("elevation/0-0-0.json")).expect("staged page"),
        b"page"
    );
}

#[test]
fn source_package_identity_and_fixed_budgets_enforce_the_production_bounds() {
    let request = aoe_map::MapRequest::default();
    let source_lock = aoe_map::SourceLock {
        id: "offline-fixture".to_owned(),
        provider: "test-only".to_owned(),
        release: "fixture".to_owned(),
        url: "https://example.invalid/fixture".to_owned(),
        sha256: [7; 32],
        acquired_at: "2026-09-26T00:00:00Z".to_owned(),
        native_resolution: "test".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "test".to_owned(),
        license: "test-only".to_owned(),
        preprocessing_version: "fixture-v1".to_owned(),
    };
    let package = aoe_map::MapPackage::new(aoe_map::MAP_SCHEMA_VERSION, request, vec![source_lock])
        .expect("valid package fixture");
    package.validate().expect("package fixture validation");
    let hash = package.content_hash_hex();
    validate_source_locked_package("fixture", &package, &hash, &request)
        .expect("matching locked package identity");

    let other_request = aoe_map::MapRequest {
        center_longitude_e7: request.center_longitude_e7 + 1,
        ..request
    };
    assert!(
        validate_source_locked_package("fixture", &package, &"0".repeat(64), &request).is_err()
    );
    assert!(validate_source_locked_package("fixture", &package, &hash, &other_request).is_err());
    let unlocked = aoe_map::MapPackage::new(aoe_map::MAP_SCHEMA_VERSION, request, Vec::new())
        .expect("valid unlocked package fixture");
    assert!(
        validate_source_locked_package(
            "fixture",
            &unlocked,
            &unlocked.content_hash_hex(),
            &request
        )
        .is_err()
    );

    validate_page_budget("fixture", MAX_PACKAGE_PAGES, MAX_PACKAGE_PAGE_BYTES)
        .expect("exact page caps are accepted");
    validate_chunk_cache_bound("fixture", 512).expect("exact chunk cache bound is accepted");
    assert!(validate_page_budget("fixture", MAX_PACKAGE_PAGES + 1, 0).is_err());
    assert!(validate_page_budget("fixture", 1, MAX_PACKAGE_PAGE_BYTES + 1).is_err());
    assert!(validate_chunk_cache_bound("fixture", 513).is_err());
}

#[test]
fn water_and_elevation_readers_count_level_zero_samples_and_reject_malformed_pages() {
    let temporary = tempfile::tempdir().expect("temporary package tree");
    let hash = "fixture-hash";
    let water_directory = temporary.path().join("pages").join(hash).join("water");
    fs::create_dir_all(&water_directory).expect("water pages");
    let water = aoe_map::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 1,
        ocean_coverage_percent: vec![20, 0],
        inland_coverage_percent: vec![0, 35],
    };
    fs::write(
        water_directory.join("0-0-0.json"),
        serde_json::to_vec(&water).expect("encode water fixture"),
    )
    .expect("write water fixture");
    let ignored_water = aoe_map::WaterPage {
        level: 1,
        ..water.clone()
    };
    fs::write(
        water_directory.join("1-0-0.json"),
        serde_json::to_vec(&ignored_water).expect("encode parent water fixture"),
    )
    .expect("write parent water fixture");
    let evidence = read_water_evidence(temporary.path(), hash).expect("water evidence");
    assert_eq!(evidence.sample_count, 2);
    assert_eq!(evidence.ocean_nonzero_samples, 1);
    assert_eq!(evidence.inland_nonzero_samples, 1);
    assert_eq!(evidence.ocean_coverage_percent_sum, 20);
    assert_eq!(evidence.inland_coverage_percent_sum, 35);

    let elevation_directory = temporary.path().join("pages").join(hash).join("elevation");
    fs::create_dir_all(&elevation_directory).expect("elevation pages");
    let elevation = aoe_map::ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![-20, 30, 100, 50],
    };
    fs::write(
        elevation_directory.join("0-0-0.json"),
        serde_json::to_vec(&elevation).expect("encode elevation fixture"),
    )
    .expect("write elevation fixture");
    let ignored_elevation = aoe_map::ElevationPage {
        level: 1,
        ..elevation
    };
    fs::write(
        elevation_directory.join("1-0-0.json"),
        serde_json::to_vec(&ignored_elevation).expect("encode parent elevation fixture"),
    )
    .expect("write parent elevation fixture");
    let range = read_elevation_range(temporary.path(), hash).expect("elevation evidence");
    assert_eq!(range.minimum_centimeters, -20);
    assert_eq!(range.maximum_centimeters, 100);
    assert_eq!(range.level_zero_samples, 4);

    fs::write(
        water_directory.join("0-1-0.json"),
        br#"{"level":0,"x":0,"y":0,"width":2,"height":1,"ocean_coverage_percent":[0,0],"inland_coverage_percent":[0]}"#,
    )
    .expect("write malformed water page");
    assert!(read_water_evidence(temporary.path(), hash).is_err());
}
