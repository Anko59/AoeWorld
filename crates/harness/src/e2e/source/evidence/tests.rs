use super::*;
use aoe_map::{FieldPyramid, PreparedEnvironment, PyramidLevel};
use serde_json::{Value, json};

// Synthetic metadata contracts only. No source qualification report is written.
fn fixture(profile: &str) -> MapPackage {
    let axis: u16 = if profile == "detailed" { 1024 } else { 128 };
    let request = MapRequest::default();
    let mut samples = axis;
    let mut levels = Vec::new();
    loop {
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: [1; 32],
        });
        if samples == 1 {
            break;
        }
        samples = samples.div_ceil(2);
    }
    let mut environment = PreparedEnvironment {
        samples_per_axis: axis,
        geographic_millimeters_per_sample: 1000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid { levels },
        ..Default::default()
    };
    if profile == "detailed" {
        environment.hydrology_evidence = Some(aoe_map::HydrologyEvidenceIndex {
            samples_per_axis: axis,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
            policy: aoe_map::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [2; 32],
            modern_land_cover_page_root: [3; 32],
            water_model: Some(aoe_map::HydrologyWaterModelIndex {
                model_version: aoe_map::HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: axis,
                target_year_ce: 600,
                correction_document: aoe_map::WaterCorrectionDocument::empty(request, axis)
                    .expect("binding"),
            }),
        });
    }
    let lock = aoe_map::SourceLock {
        id: "offline-metadata-fixture".into(),
        provider: "test".into(),
        release: "test".into(),
        url: "https://example.invalid/test".into(),
        sha256: [9; 32],
        acquired_at: "2026-09-26".into(),
        native_resolution: "test".into(),
        crs: "EPSG:4326".into(),
        vertical_datum: "test".into(),
        license: "test-only".into(),
        preprocessing_version: "test-v1".into(),
    };
    MapPackage::with_prepared_environment(
        1,
        request,
        vec![lock],
        Default::default(),
        Default::default(),
        environment,
    )
    .expect("valid metadata fixture")
}

fn created(package: &MapPackage, profile: &str) -> Value {
    let hash = package.content_hash_hex();
    json!({"content_hash": hash, "preparation": {"mode": profile},
        "joined_world_chunks": [format!("/maps/{hash}/chunks/7/7")]})
}

#[test]
fn creator_contract_keeps_both_profiles_and_rejects_wrong_identity_or_grid() {
    for profile in ["overview", "detailed"] {
        let package = fixture(profile);
        let evidence = created(&package, profile);
        let record = created_record(profile, &package, &evidence, package.request)
            .expect("matching metadata");
        assert_eq!(record["profile"], profile);
        assert_eq!(record["modeled_water_source_model"], profile == "detailed");
        let mut wrong = evidence.clone();
        wrong["content_hash"] = json!("a".repeat(64));
        assert!(created_record(profile, &package, &wrong, package.request).is_err());
        let mut wrong = evidence.clone();
        wrong["preparation"]["mode"] = json!("other");
        assert!(created_record(profile, &package, &wrong, package.request).is_err());
        let mut request = package.request;
        request.center_longitude_e7 += 1;
        assert!(created_record(profile, &package, &evidence, request).is_err());
        assert!(created_record("unknown", &package, &evidence, package.request).is_err());
    }
    let package = fixture("overview");
    assert!(
        created_record(
            "detailed",
            &package,
            &created(&package, "detailed"),
            package.request
        )
        .is_err()
    );
}

#[test]
fn offline_reopen_requires_same_package_and_genuinely_separate_chunk() {
    let package = fixture("overview");
    let evidence = created(&package, "overview");
    let record = created_record("overview", &package, &evidence, package.request).expect("created");
    let hash = package.content_hash_hex();
    let reopened = json!({"content_hash": hash, "offline_unseen_chunk": true,
        "unseen_chunk": format!("/maps/{hash}/chunks/0/0"),
        "joined_world_chunks": [format!("/maps/{hash}/chunks/7/7")]});
    let report = reopen_record("overview", &record, &reopened).expect("offline evidence");
    assert_eq!(report["created"], evidence);
    assert_eq!(report["reopened"], reopened);
    for (field, value) in [
        ("content_hash", json!("b".repeat(64))),
        ("offline_unseen_chunk", json!(false)),
        ("joined_world_chunks", json!([])),
        ("unseen_chunk", json!("")),
        ("unseen_chunk", json!(format!("/maps/{hash}/chunks/7/7"))),
        ("unseen_chunk", json!(format!("/maps/{hash}/chunks/0/nope"))),
    ] {
        let mut wrong = reopened.clone();
        wrong[field] = value;
        assert!(
            reopen_record("overview", &record, &wrong).is_err(),
            "{field}"
        );
    }
    assert!(reopen_record("detailed", &record, &reopened).is_err());
    let mut unknown = record.clone();
    unknown["profile"] = json!("unknown");
    assert!(reopen_record("unknown", &unknown, &reopened).is_err());
    for path in [
        "",
        "/maps/a/chunks/0/0",
        &format!("/maps/{hash}/chunks/-1/0"),
        &format!("/maps/{hash}/chunks/0/0/1"),
    ] {
        assert!(!valid_chunk_path(path, &hash));
    }
}
