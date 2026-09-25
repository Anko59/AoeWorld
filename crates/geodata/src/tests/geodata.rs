use super::*;
use aoe_map::PreparedEnvironment;
use std::{path::Path, sync::atomic::AtomicBool};

#[test]
fn local_projection_places_its_center_at_the_origin() {
    let definition = local_aeqd_definition(488_500_000, 23_500_000);
    let (east, north) = project_wgs84(&definition, 2.35, 48.85).expect("projection");
    assert!(east.abs() < 0.01);
    assert!(north.abs() < 0.01);
}

#[test]
fn worker_projects_the_requested_center_to_zero_meters() {
    let response = execute(WorkerRequest::ProjectPoint {
        center_latitude_e7: 488_500_000,
        center_longitude_e7: 23_500_000,
        longitude: 2.35,
        latitude: 48.85,
    })
    .expect("projected point");
    assert_eq!(
        response,
        WorkerResponse::ProjectedPoint {
            east_meters: 0,
            north_meters: 0,
        }
    );
}

#[test]
fn worker_returns_a_bounded_densified_geographic_footprint() {
    let response = execute(WorkerRequest::ProjectFootprint {
        request: MapRequest::default(),
        samples_per_edge: 8,
    })
    .expect("footprint");
    let WorkerResponse::GeographicFootprint { points, distortion } = response else {
        panic!("footprint response");
    };
    assert_eq!(points.len(), 33);
    assert_eq!(points.first(), points.last());
    assert!(distortion.min_scale_error_ppm <= distortion.max_scale_error_ppm);
}

#[test]
fn worker_lists_only_the_allowlisted_global_overview() {
    let response = execute(WorkerRequest::ListOverviewSources).expect("sources");
    let WorkerResponse::KnownSources { sources } = response else {
        panic!("source response");
    };
    assert_eq!(sources, vec![etopo_2022_60s_surface()]);
}

#[test]
fn overview_response_keeps_its_bounded_protocol_shape_when_boxed() {
    let response = WorkerResponse::PreparedOverview(Box::new(PreparedOverview {
        source_lock: source_lock(
            "overview",
            7,
            "https://example.invalid/overview.tif",
            "1 arc-minute",
        ),
        water_source_lock: source_lock("coastline", 8, "https://example.invalid/land.zip", "1:10m"),
        vegetation_source_lock: source_lock(
            "vegetation",
            9,
            "https://example.invalid/vegetation.tif",
            "250m",
        ),
        vegetation_classes_source_lock: source_lock(
            "vegetation-classes",
            10,
            "https://example.invalid/vegetation.csv",
            "table",
        ),
        hyde_baseline_source_lock: source_lock(
            "hyde-baseline",
            11,
            "https://example.invalid/baseline.zip",
            "5 arc-minutes",
        ),
        hyde_supplementary_source_lock: source_lock(
            "hyde-supplementary",
            12,
            "https://example.invalid/supplementary.zip",
            "5 arc-minutes",
        ),
        hyde_readme_source_lock: source_lock(
            "hyde-readme",
            13,
            "https://example.invalid/readme.txt",
            "text",
        ),
        projection: ProjectionMetadata::default(),
        provenance: EnvironmentalProvenance::default(),
        environment: PreparedEnvironment::default(),
        pages: Vec::new(),
        water_pages: Vec::new(),
        vegetation_pages: Vec::new(),
        historical_land_use_pages: Vec::new(),
    }));
    let encoded = serde_json::to_value(response).expect("serializes");
    assert_eq!(encoded["operation"], "prepared_overview");
    assert_eq!(encoded["source_lock"]["id"], "overview");
    assert_eq!(encoded["water_source_lock"]["id"], "coastline");
    assert_eq!(encoded["vegetation_source_lock"]["id"], "vegetation");
    assert!(encoded.get("value").is_none());
}

#[test]
fn overview_preflight_and_helpers_fail_closed_without_cached_catalogs() {
    let directory = std::env::temp_dir().join(format!(
        "aoe-geodata-preflight-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    std::fs::create_dir(&directory).expect("cache directory");
    let cache = SourceCache::new(
        directory.clone(),
        DownloadPolicy {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: MAX_OVERVIEW_INPUT_BYTES,
        },
    )
    .expect("source cache");
    let potential = potential_biome_sources().expect("potential sources");
    let hyde = hyde_sources().expect("hyde sources");
    preflight_overview_acquisition(&cache, Some(&potential), Some(&hyde)).expect("preflight");
    assert!(matches!(
        preflight_overview_acquisition(&cache, None, None),
        Err(GeodataError::Preparation(_))
    ));

    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        acquire_or_cached(&cache, None, "missing", &cancelled),
        Err(GeodataError::Preparation(_))
    ));
    assert_eq!(round_meters(1.4).expect("round"), 1);
    assert!(matches!(
        round_meters(f64::NAN),
        Err(GeodataError::Coordinate)
    ));
    assert!(matches!(
        raster_dimensions(Path::new("missing")),
        Err(GeodataError::Gdal(_))
    ));
    assert!(acquisition_marker().starts_with("unix-seconds-"));
    std::fs::remove_dir_all(directory).expect("cleanup");
}

fn source_lock(id: &str, byte: u8, url: &str, resolution: &str) -> aoe_map::SourceLock {
    aoe_map::SourceLock {
        id: id.to_owned(),
        provider: "provider".to_owned(),
        release: "release".to_owned(),
        url: url.to_owned(),
        sha256: [byte; 32],
        acquired_at: "2026-09-18".to_owned(),
        native_resolution: resolution.to_owned(),
        crs: "not applicable".to_owned(),
        vertical_datum: "not applicable".to_owned(),
        license: "test".to_owned(),
        preprocessing_version: "test".to_owned(),
    }
}
