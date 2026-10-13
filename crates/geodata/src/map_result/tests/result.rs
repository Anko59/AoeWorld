use super::*;

fn package() -> MapPackage {
    MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment::default(),
    )
    .expect("package")
}

fn generated() -> GeneratedMap {
    GeneratedMap {
        package: package(),
        elevation_pages: Vec::new(),
        water_pages: Vec::new(),
        vegetation_pages: Vec::new(),
        historical_land_use_pages: Vec::new(),
        hydrology_evidence_pages: Vec::new(),
        modern_land_cover_pages: Vec::new(),
    }
}

#[test]
fn detailed_worker_request_without_staging_scope_is_accepted() {
    let request: WorkerRequest = serde_json::from_value(serde_json::json!({
        "operation": "prepare_detailed_directory",
        "cache_root": "/cache",
        "output_directory": "/maps",
        "request": MapRequest::default(),
        "samples_per_axis": 128,
        "resolution": "glo90"
    }))
    .expect("detailed request");
    assert!(matches!(
        request,
        WorkerRequest::PrepareDetailedDirectory {
            staging_root: None,
            ..
        }
    ));
}

#[test]
fn overview_worker_request_accepts_absent_historical_corrections() {
    let request: WorkerRequest = serde_json::from_value(serde_json::json!({
        "operation": "prepare_overview_directory",
        "cache_root": "/cache",
        "output_directory": "/maps",
        "request": MapRequest::default(),
        "samples_per_axis": 1024,
        "field_axes": OverviewFieldAxes::LANDSCAPE
    }))
    .unwrap();
    assert!(matches!(
        request,
        WorkerRequest::PrepareOverviewDirectory {
            historical_corrections: None,
            field_axes: OverviewFieldAxes::LANDSCAPE,
            hydrology_mode: OverviewHydrologyMode::None,
            water_corrections: None,
            ..
        }
    ));
}

#[test]
fn overview_worker_request_requires_field_axes() {
    assert!(
        serde_json::from_value::<WorkerRequest>(serde_json::json!({
            "operation": "prepare_overview_directory",
            "cache_root": "/cache",
            "output_directory": "/maps",
            "request": MapRequest::default(),
            "samples_per_axis": 1024
        }))
        .is_err()
    );
}

#[test]
fn complete_prepared_result_validates_without_provider_or_filesystem_access() {
    generated().validate().expect("complete empty prepared map");
}

#[test]
fn overview_worker_request_accepts_absent_vegetation_corrections() {
    let request: WorkerRequest = serde_json::from_value(serde_json::json!({
        "operation": "prepare_overview_directory",
        "cache_root": "/cache",
        "output_directory": "/maps",
        "request": MapRequest::default(),
        "samples_per_axis": 1024,
        "field_axes": OverviewFieldAxes::LANDSCAPE
    }))
    .expect("existing overview request");
    assert!(matches!(
        request,
        WorkerRequest::PrepareOverviewDirectory {
            vegetation_corrections: None,
            ..
        }
    ));
}

#[test]
fn typed_evidence_pages_require_a_matching_package_index() {
    let mut result = generated();
    result.hydrology_evidence_pages.push(HydrologyEvidencePage {
        river_segments: Vec::new(),
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        kind: vec![0],
        method: vec![0],
        water_model: None,
    });
    assert!(matches!(
        result.validate(),
        Err(GeodataError::Preparation(
            "typed evidence pages require a package evidence index"
        ))
    ));
}
