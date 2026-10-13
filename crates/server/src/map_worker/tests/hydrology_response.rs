use super::*;

#[test]
fn valid_source_response_cannot_silently_downgrade_sample_detail() {
    let request = MapRequest::default();
    let source = aoe_map::SourceLock {
        id: "fixture".into(),
        provider: "fixture".into(),
        release: "v1".into(),
        url: "https://example.invalid/fixture".into(),
        sha256: [1; 32],
        acquired_at: "fixture".into(),
        native_resolution: "30 meters".into(),
        crs: "EPSG:4326".into(),
        vertical_datum: "EGM2008".into(),
        license: "fixture".into(),
        preprocessing_version: "v1".into(),
    };
    let environment = aoe_map::PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 15_000_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: aoe_map::FieldPyramid {
            levels: vec![
                aoe_map::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [1; 32],
                },
                aoe_map::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [2; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        aoe_map::MAP_SCHEMA_VERSION,
        request,
        vec![source],
        aoe_map::ProjectionMetadata::default(),
        aoe_map::EnvironmentalProvenance::default(),
        environment,
    )
    .expect("source fixture");
    let bytes = serde_json::to_vec(&serde_json::json!({
        "operation": "prepared_directory", "package": package,
    }))
    .expect("worker response");
    assert!(decode_prepared_output(&bytes, request, 2, None).is_ok());
    let landscape_request = request;
    let mut landscape_environment = package.environment.clone();
    landscape_environment.water = Some(landscape_environment.elevation.clone());
    landscape_environment.vegetation = Some(landscape_environment.elevation.clone());
    landscape_environment.historical_land_use = Some(landscape_environment.elevation.clone());
    let landscape = MapPackage::with_prepared_environment(
        package.generator_version,
        landscape_request,
        package.source_locks.clone(),
        package.projection.clone(),
        package.provenance.clone(),
        landscape_environment,
    )
    .expect("canonical metadata-only field fixture");
    let bytes_landscape = serde_json::to_vec(&serde_json::json!({
        "operation": "prepared_directory", "package": landscape,
    }))
    .expect("landscape response");
    let axes = crate::map_jobs::OverviewFieldAxes {
        elevation: 2,
        vegetation: 2,
        water: 2,
        historical: 2,
    };
    assert!(decode_prepared_output(&bytes_landscape, landscape_request, 2, Some(axes)).is_ok());
    for field in 0..4 {
        let mut changed = axes;
        match field {
            0 => changed.elevation = 3,
            1 => changed.vegetation = 3,
            2 => changed.water = 3,
            _ => changed.historical = 3,
        }
        assert!(
            decode_prepared_output(&bytes_landscape, landscape_request, 2, Some(changed))
                .expect_err("silent field downgrade")
                .contains("different field axes")
        );
    }
    assert!(
        decode_prepared_output(&bytes, request, 1024, None)
            .expect_err("wrong detail")
            .contains("different preparation detail")
    );
}

#[test]
fn directory_response_cannot_substitute_another_request_or_silent_fallback() {
    let request = MapRequest::default();
    let package =
        MapPackage::new(aoe_map::MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let output = serde_json::to_vec(&serde_json::json!({
        "operation": "prepared_directory", "package": package,
    }))
    .expect("response");
    let mut other = request;
    other.seed += 1;
    assert!(
        decode_prepared_output(&output, other, 128, None)
            .expect_err("different request")
            .contains("different request")
    );
    assert!(
        decode_prepared_output(&output, request, 128, None)
            .expect_err("fallback")
            .contains("no environmental sources")
    );
}

#[test]
fn automatic_worker_requires_rivers_or_the_explicit_outside_coverage_note() {
    for covered in [true, false] {
        let package = overview_response(covered);
        assert!(hydrology::verify(&package, crate::map_jobs::HydrologyMode::Automatic).is_ok());
        if covered {
            assert!(hydrology::verify(&package, crate::map_jobs::HydrologyMode::Vectors).is_ok());
        } else {
            assert!(hydrology::verify(&package, crate::map_jobs::HydrologyMode::Vectors).is_err());
            let silent = MapPackage::with_prepared_environment(
                1,
                package.request,
                package.source_locks.clone(),
                package.projection.clone(),
                package.provenance.clone(),
                package.environment.clone(),
            )
            .unwrap();
            assert!(hydrology::verify(&silent, crate::map_jobs::HydrologyMode::Automatic).is_err());
        }
    }
}

fn overview_response(covered: bool) -> MapPackage {
    use aoe_map::*;
    let request = MapRequest::default();
    let field = |mut axis| {
        let mut levels = Vec::new();
        loop {
            levels.push(PyramidLevel {
                samples_per_axis: axis,
                ordered_page_root: [3; 32],
            });
            if axis == 1 {
                break;
            }
            axis = axis.div_ceil(2);
        }
        FieldPyramid { levels }
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 1024,
        geographic_millimeters_per_sample: 1000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: field(1024),
        water: Some(field(128)),
        vegetation: Some(field(128)),
        historical_land_use: Some(field(1024)),
        hydrology_evidence: covered.then(|| HydrologyEvidenceIndex {
            samples_per_axis: 1024,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [4; 32],
            modern_land_cover_page_root: [5; 32],
            water_model: Some(HydrologyWaterModelIndex {
                model_version: HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: 1024,
                target_year_ce: 600,
                correction_document: WaterCorrectionDocument::empty(request, 1024).unwrap(),
            }),
        }),
    };
    let mut sources = (0..7)
        .map(|i| SourceLock {
            id: format!("fixture-{i}"),
            provider: "fixture".into(),
            release: "current".into(),
            url: "https://example.invalid/source".into(),
            sha256: [1; 32],
            acquired_at: "fixture".into(),
            native_resolution: "fixture".into(),
            crs: "EPSG:4326".into(),
            vertical_datum: "fixture".into(),
            license: "fixture".into(),
            preprocessing_version: "fixture".into(),
        })
        .collect::<Vec<_>>();
    if covered {
        for id in ["hydrolakes-v1.0-global-gdb", "hydrorivers-v1.0-eu-shp"] {
            let mut lock = sources[0].clone();
            lock.id = id.into();
            lock.preprocessing_version="modern-landcover=not-requested;modern-class=0-nodata;2021=classification-legend-only".into();
            sources.push(lock);
        }
    }
    let package = MapPackage::with_prepared_environment(
        1,
        request,
        sources,
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .unwrap();
    if covered {
        package
    } else {
        package
            .with_preparation_note(PreparationNote::OutsideRiverCoverage)
            .unwrap()
    }
}
