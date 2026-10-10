//! The strict reader accepts only the current schema and generation recipe.
use super::*;

fn modeled_environment(request: MapRequest) -> PreparedEnvironment {
    PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [3; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [4; 32],
                },
            ],
        },
        hydrology_evidence: Some(crate::HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
            policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [1; 32],
            modern_land_cover_page_root: [2; 32],
            water_model: Some(crate::HydrologyWaterModelIndex {
                model_version: crate::HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: 2,
                target_year_ce: crate::WATER_CORRECTION_TARGET_YEAR_CE,
                correction_document: crate::WaterCorrectionDocument::empty(request, 2).unwrap(),
            }),
        }),
        ..PreparedEnvironment::default()
    }
}

fn modeled_package(request: MapRequest) -> MapPackage {
    MapPackage::with_prepared_environment(
        1,
        request,
        vec![source("elevation")],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        modeled_environment(request),
    )
    .unwrap()
}

fn geography(package: &MapPackage) -> [u8; 32] {
    hash_package(
        package.generator_version,
        package.request,
        &package.source_locks,
        &package.projection,
        &package.provenance,
        &package.environment,
        HashMode::Geography,
    )
}

#[test]
fn unknown_semantics_are_rejected_at_every_package_boundary() {
    let package = modeled_package(MapRequest::default());
    for pointer in [
        "",
        "/request",
        "/request/compression",
        "/estimate",
        "/projection",
        "/provenance",
        "/source_locks/0",
        "/environment",
        "/environment/elevation",
        "/environment/elevation/levels/0",
        "/environment/hydrology_evidence",
        "/environment/hydrology_evidence/water_model",
        "/environment/hydrology_evidence/water_model/correction_document",
        "/environment/hydrology_evidence/water_model/correction_document/request",
        "/environment/hydrology_evidence/water_model/correction_document/request/compression",
    ] {
        let mut value = serde_json::to_value(&package).unwrap();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unrecognized_semantics".into(), serde_json::json!(true));
        assert!(
            serde_json::from_value::<MapPackage>(value).is_err(),
            "unknown semantics accepted at {pointer}"
        );
    }
}

#[test]
fn other_schemas_recipes_and_missing_identity_fields_are_rejected() {
    let package = modeled_package(MapRequest::default());
    let value = serde_json::to_value(&package).unwrap();
    assert_eq!(value["schema_version"], crate::MAP_SCHEMA_VERSION);
    assert_eq!(
        value["generation_recipe_version"],
        crate::GENERATION_RECIPE_VERSION
    );
    let decoded: MapPackage = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(decoded, package);
    decoded.validate().unwrap();
    for (key, other) in [
        ("schema_version", crate::MAP_SCHEMA_VERSION + 1),
        ("schema_version", 0),
        (
            "generation_recipe_version",
            crate::GENERATION_RECIPE_VERSION + 1,
        ),
        ("generation_recipe_version", 0),
    ] {
        let mut changed = value.clone();
        changed[key] = other.into();
        assert!(
            serde_json::from_value::<MapPackage>(changed).is_err(),
            "{key}={other} accepted"
        );
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<MapPackage>(missing).is_err());
    }
    let mut request = value;
    request["request"]["detail_profile"] = "landscape_v2".into();
    assert!(serde_json::from_value::<MapPackage>(request).is_err());
    assert_eq!(
        MapPackage::with_generation_recipe(
            1,
            crate::GENERATION_RECIPE_VERSION + 1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            PreparedEnvironment::default(),
        ),
        Err(MapPackageError::InvalidGenerationRecipeVersion)
    );
}

#[test]
fn duplicate_known_package_fields_never_collapse_before_deserialization() {
    let value = serde_json::to_string(&modeled_package(MapRequest::default())).unwrap();
    let duplicated = format!(
        "{{\"schema_version\":{},{}",
        crate::MAP_SCHEMA_VERSION,
        &value[1..]
    );
    assert!(serde_json::from_str::<MapPackage>(&duplicated).is_err());
}

#[test]
fn modeled_water_binds_the_actual_request_and_seed_keeps_geography() {
    let first = modeled_package(MapRequest::default());
    let other_request = MapRequest {
        seed: MapRequest::default().seed + 1,
        ..MapRequest::default()
    };
    let second = modeled_package(other_request);
    assert_ne!(first.content_hash, second.content_hash);
    assert_ne!(
        first.environment.hydrology_evidence,
        second.environment.hydrology_evidence
    );
    assert!(
        MapPackage::with_prepared_environment(
            1,
            other_request,
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            first.environment.clone(),
        )
        .is_err()
    );
    let plain = MapPackage::new(1, MapRequest::default(), Vec::new()).unwrap();
    let reseeded = MapPackage::new(1, other_request, Vec::new()).unwrap();
    assert_ne!(plain.content_hash, reseeded.content_hash);
    assert_eq!(geography(&plain), geography(&reseeded));
}
