use super::*;
use aoe_map::{FieldPyramid, PageLayer, PyramidLevel};
use std::sync::atomic::AtomicBool;

fn prepared_package(samples_per_axis: u16) -> MapPackage {
    let levels = if samples_per_axis == 2 {
        vec![
            PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: [1; 32],
            },
            PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [2; 32],
            },
        ]
    } else {
        vec![PyramidLevel {
            samples_per_axis,
            ordered_page_root: [3; 32],
        }]
    };
    let environment = PreparedEnvironment {
        samples_per_axis,
        geographic_millimeters_per_sample: 1_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid { levels },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("prepared package")
}

#[test]
fn resolution_metadata_distinguishes_native_and_fallback_sources() {
    assert_eq!(DemResolution::Glo90.suffix(), "30");
    assert_eq!(DemResolution::Glo30PreferGlo90.suffix(), "10");
    assert_eq!(DemResolution::Glo90.native_resolution(), "3 arc-seconds");
    assert_eq!(
        DemResolution::Glo30PreferGlo90.native_resolution(),
        "1 arc-second"
    );
    assert_eq!(DemResolution::Glo90.fallback(), None);
    assert_eq!(
        DemResolution::Glo30PreferGlo90.fallback(),
        Some(DemResolution::Glo90)
    );
}

#[test]
fn geographic_bounds_and_tile_budget_stay_within_supported_footprints() {
    let request = MapRequest::default();
    let side = request.estimate().expect("estimate").effective_side_meters;
    let bounds = geographic_bounds(request, side).expect("supported footprint");
    assert!(bounds.contains(48, 2));
    assert!(bounds.min_latitude >= -90 && bounds.max_latitude <= 89);
    assert!(bounds.min_longitude >= -180 && bounds.max_longitude <= 179);
    validate_tile_budget(bounds).expect("bounded tile grid");

    let too_wide = Bounds {
        min_latitude: 0,
        max_latitude: 8,
        min_longitude: -180,
        max_longitude: 179,
    };
    assert!(matches!(
        validate_tile_budget(too_wide),
        Err(GeodataError::Preparation(
            "detailed footprint exceeds the 64 tile limit"
        ))
    ));

    let polar = MapRequest {
        center_latitude_e7: 890_000_000,
        requested_side_meters: 500_000,
        compression: aoe_map::Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    assert!(matches!(
        geographic_bounds(polar, 500_000),
        Err(GeodataError::Preparation(
            "detailed footprint crosses an unsupported geographic boundary"
        ))
    ));
}

#[test]
fn cancelled_tile_acquisition_stops_before_provider_access() {
    let root =
        std::env::temp_dir().join(format!("aoe-copernicus-cancelled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let cache = SourceCache::new(root.clone(), DownloadPolicy::default()).expect("cache");
    let bounds = Bounds {
        min_latitude: 48,
        max_latitude: 48,
        min_longitude: 2,
        max_longitude: 2,
    };
    let cancelled = AtomicBool::new(true);
    assert!(matches!(
        acquire_tiles(&cache, bounds, DemResolution::Glo90, &cancelled),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn staged_publish_validates_axis_and_missing_layers_before_writing_manifest() {
    let root = std::env::temp_dir().join(format!(
        "aoe-copernicus-stage-publish-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("stage root");
    let output = root.join("output");
    let package = prepared_package(2);
    let stage = Stage::new(&root).expect("stage");

    assert!(matches!(
        publish_staged_pages(&stage, &output, &package, 3),
        Err(GeodataError::Preparation(
            "staged package axis does not match its manifest"
        ))
    ));

    stage
        .write(PageLayer::Elevation, 0, 0, 0, b"level-zero")
        .expect("level zero page");
    stage
        .write(PageLayer::Elevation, 1, 0, 0, b"overview")
        .expect("overview page");
    assert!(matches!(
        publish_staged_pages(&stage, &output, &package, 2),
        Err(GeodataError::Preparation("detailed water field is missing"))
    ));
    assert!(
        !output
            .join(format!("{}.json", package.content_hash_hex()))
            .exists()
    );
    drop(stage);
    let _ = std::fs::remove_dir_all(root);
}
