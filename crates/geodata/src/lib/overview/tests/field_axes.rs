use super::*;

#[test]
fn offline_field_axes_prepare_independent_canonical_pyramids() {
    let fixture = OverviewFixture::new();
    let request = MapRequest::default();
    for axes in [
        OverviewFieldAxes {
            elevation: 128,
            vegetation: 32,
            water: 64,
            historical: 16,
        },
        OverviewFieldAxes::LANDSCAPE,
    ] {
        let historical = GeographicHistoricalCorrectionDocument::empty(
            request,
            axes.historical,
            hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
        )
        .expect("history binding");
        let vegetation =
            VegetationPatchDocument::empty(request, axes.vegetation).expect("PNV binding");
        let prepared = prepare_overview_fields_from_verified_sources(
            request,
            axes,
            true,
            &historical,
            &vegetation,
            fixture.sources.clone(),
        )
        .expect("offline GDAL overview");
        let environment = &prepared.environment;
        environment.validate().expect("independent axes");
        assert_eq!(environment.samples_per_axis, axes.elevation);
        assert_eq!(
            environment.vegetation_samples_per_axis(),
            Some(axes.vegetation)
        );
        assert_eq!(environment.water_samples_per_axis(), Some(axes.water));
        assert_eq!(
            environment.historical_samples_per_axis(),
            Some(axes.historical)
        );
        assert!(
            prepared
                .source_lock
                .preprocessing_version
                .contains(";axes=")
        );
        let generated = GeneratedMap::from_prepared(request, prepared).expect("package");
        generated.validate().expect("field-local published pages");
    }
}

#[test]
fn explicit_coupled_defaults_preserve_all_field_roots() {
    let fixture = OverviewFixture::new();
    let request = MapRequest::default();
    let axes = OverviewFieldAxes::coupled(128, 128);
    let history = GeographicHistoricalCorrectionDocument::empty(
        request,
        128,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )
    .unwrap();
    let vegetation = VegetationPatchDocument::empty(request, 128).unwrap();
    let coupled = prepare_overview_from_verified_sources(
        request,
        128,
        128,
        &history,
        &vegetation,
        fixture.sources.clone(),
    )
    .unwrap();
    let explicit = prepare_overview_fields_from_verified_sources(
        request,
        axes,
        true,
        &history,
        &vegetation,
        fixture.sources.clone(),
    )
    .unwrap();
    assert_eq!(coupled.environment, explicit.environment);
    assert_eq!(coupled.pages, explicit.pages);
    assert_eq!(coupled.water_pages, explicit.water_pages);
    assert_eq!(coupled.vegetation_pages, explicit.vegetation_pages);
    assert_eq!(
        coupled.historical_land_use_pages,
        explicit.historical_land_use_pages
    );
    assert_eq!(
        coupled.source_lock.preprocessing_version,
        "etopo-overview-gdal-0.19"
    );
}

#[test]
fn worker_rejects_inconsistent_elevation_axis_before_acquisition() {
    let fixture = OverviewFixture::new();
    let cache = fixture.root.join("uncreated-cache");
    let output = fixture.root.join("uncreated-output");
    let request = MapRequest::default();
    let operation: WorkerRequest = serde_json::from_value(serde_json::json!({
        "operation": "prepare_overview_directory", "cache_root": cache,
        "output_directory": output, "request": request,
        "samples_per_axis": 128, "field_axes": OverviewFieldAxes::LANDSCAPE,
    }))
    .unwrap();
    assert!(matches!(
        execute(operation),
        Err(GeodataError::Preparation(
            "overview elevation axis does not match requested samples"
        ))
    ));
    assert!(!cache.exists());
    assert!(!output.exists());
}

#[test]
fn correction_axis_and_year_fail_before_acquisition() {
    let fixture = OverviewFixture::new();
    let request = MapRequest::default();
    let axes = OverviewFieldAxes {
        elevation: 128,
        vegetation: 32,
        water: 64,
        historical: 16,
    };
    let cache = fixture.root.join("uncreated-cache");
    let history = GeographicHistoricalCorrectionDocument::empty(
        request,
        32,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )
    .unwrap();
    assert!(
        prepare_overview_with_field_axes(cache.clone(), request, axes, Some(&history), None)
            .is_err()
    );
    let mut vegetation = VegetationPatchDocument::empty(request, 32).unwrap();
    vegetation.target_year_ce = 2000;
    assert!(
        prepare_overview_with_field_axes(cache.clone(), request, axes, None, Some(&vegetation))
            .is_err()
    );
    vegetation.target_year_ce = 600;
    vegetation.binding.samples_per_axis = 128;
    assert!(
        prepare_overview_with_field_axes(cache.clone(), request, axes, None, Some(&vegetation))
            .is_err()
    );
    assert!(!cache.exists());
}

#[test]
fn explicit_preprocessing_identity_spells_axes_stably() {
    let axes = OverviewFieldAxes::LANDSCAPE;
    assert_eq!(
        crate::overview::preprocessing_identity("base".to_owned(), axes, false),
        "base"
    );
    assert_eq!(
        crate::overview::preprocessing_identity("base".to_owned(), axes, true),
        "base;axes=1024/128/128/1024"
    );
}
