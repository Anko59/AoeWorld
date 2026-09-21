use super::*;
use aoe_map::Ratio;
use std::collections::BTreeSet;

#[test]
fn public_tile_names_match_the_aws_one_degree_catalog() {
    assert_eq!(
        tile_prefix(48, 2, "30"),
        "Copernicus_DSM_COG_30_N48_00_E002_00_DEM"
    );
    assert_eq!(
        tile_prefix(-1, -7, "10"),
        "Copernicus_DSM_COG_10_S01_00_W007_00_DEM"
    );
}

#[test]
fn detailed_sample_cap_is_rejected_before_any_source_access() {
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-test-cache"),
        std::env::temp_dir().join("aoe-detailed-test-output"),
        MapRequest::default(),
        MAX_DETAILED_SAMPLES_PER_AXIS + 1,
        DemResolution::Glo90,
    );
    assert!(matches!(result, Err(GeodataError::Preparation(_))));
}

#[test]
fn oversized_tile_extent_is_rejected_before_overview_acquisition() {
    let request = MapRequest {
        requested_side_meters: 2_000_000,
        compression: Ratio::new(10_000, 1).expect("valid ratio"),
        ..MapRequest::default()
    };
    let cache = std::env::temp_dir().join("aoe-detailed-overlarge-cache");
    let result = prepare_detailed_directory(
        cache,
        std::env::temp_dir().join("aoe-detailed-overlarge-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("64 tile")));
}

#[test]
fn footprint_containing_the_pole_is_rejected_before_source_access() {
    let request = MapRequest {
        center_latitude_e7: 875_000_000,
        requested_side_meters: 500_000,
        compression: Ratio::new(1, 1).expect("valid ratio"),
        ..MapRequest::default()
    };
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-polar-cache"),
        std::env::temp_dir().join("aoe-detailed-polar-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(
        matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("geographic boundary"))
    );
}

#[test]
fn antimeridian_footprint_is_rejected_without_coordinate_overflow() {
    let request = MapRequest {
        center_longitude_e7: 1_799_900_000,
        ..MapRequest::default()
    };
    let result = prepare_detailed_directory(
        std::env::temp_dir().join("aoe-detailed-antimeridian-cache"),
        std::env::temp_dir().join("aoe-detailed-antimeridian-output"),
        request,
        16,
        DemResolution::Glo90,
    );
    assert!(
        matches!(result, Err(GeodataError::Preparation(reason)) if reason.contains("geographic boundary"))
    );
}

#[test]
fn nearest_resampling_uses_the_center_of_each_coarse_cell() {
    assert_eq!(super::pyramid::coarse_coordinate(256, 0), 0);
    assert_eq!(super::pyramid::coarse_coordinate(256, 1), 0);
    assert_eq!(super::pyramid::coarse_coordinate(256, 127), 63);
    assert_eq!(super::pyramid::coarse_coordinate(256, 128), 64);
    assert_eq!(super::pyramid::coarse_coordinate(256, 255), 127);
    assert_eq!(super::pyramid::coarse_coordinate(64, 0), 1);
    assert_eq!(super::pyramid::coarse_coordinate(64, 63), 127);
}

#[test]
fn tile_candidates_include_both_sides_of_exact_geocell_boundaries() {
    let below = super::sampler::tile_candidate_keys(48.999_999_999, 1.999_999_999);
    let above = super::sampler::tile_candidate_keys(49.0, 2.0);
    assert!(below.contains(&(48, 1)) && below.contains(&(49, 2)));
    assert!(above.contains(&(48, 1)) && above.contains(&(49, 2)));
}

#[test]
fn only_authoritative_missing_tiles_can_supply_ocean_zero() {
    let bounds = super::Bounds {
        min_latitude: 47,
        max_latitude: 49,
        min_longitude: 1,
        max_longitude: 3,
    };
    let absent = BTreeSet::from([(48, 2)]);
    assert_eq!(
        super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, Some(100))
            .expect("404 ocean"),
        0
    );
    assert!(super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, Some(0)).is_err());
    assert!(super::sampler::missing_tile_value(bounds, &absent, (48, 2), None, None).is_err());
    assert!(super::sampler::missing_tile_value(bounds, &absent, (50, 2), None, Some(100)).is_err());
}
