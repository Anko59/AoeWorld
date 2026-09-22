use super::*;

#[test]
fn worldcover_window_bound_rejects_large_allocation_before_read() {
    assert!(check_window_bound(2_048, 2_048).is_ok());
    assert!(check_window_bound(2_049, 2_048).is_err());
    assert!(check_window_bound(usize::MAX, 2).is_err());
}

#[test]
fn worldcover_tile_ids_parse_only_coordinate_tags() {
    let source = "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N48E003_Map.tif";
    assert_eq!(tile_latitude(source).expect("latitude"), 48);
    assert_eq!(tile_longitude(source).expect("longitude"), 3);
    let southern = "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_S03W006_Map.tif";
    assert_eq!(tile_latitude(southern).expect("latitude"), -3);
    assert_eq!(tile_longitude(southern).expect("longitude"), -6);
}

#[test]
fn worldcover_tile_id_parser_rejects_malformed_and_non_ascii_names() {
    for source in [
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N4",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N48E00_Map.tif",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N4ßE003_Map.tif",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_S00E003_Map.tif",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N48W000_Map.tif",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N99E003_Map.tif",
        "worldcover-2021-v200:ESA_WorldCover_10m_2021_v200_N48E181_Map.tif",
    ] {
        assert!(tile_latitude(source).is_err(), "accepted {source}");
        assert!(tile_longitude(source).is_err(), "accepted {source}");
    }
}

#[test]
fn ocean_page_flatten_rejects_missing_and_duplicate_pages() {
    let page = aoe_map::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![100, 0, 0, 100],
        inland_coverage_percent: vec![0; 4],
    };
    assert!(flatten_ocean_pages(2, std::slice::from_ref(&page)).is_ok());
    assert!(flatten_ocean_pages(3, std::slice::from_ref(&page)).is_err());
    assert!(flatten_ocean_pages(2, &[page.clone(), page]).is_err());
    assert!(flatten_ocean_pages(2, &[]).is_err());
}

#[test]
fn overview_ocean_coverage_resamples_by_target_cell_center() {
    let page = aoe_map::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![10, 20, 30, 40],
        inland_coverage_percent: vec![0; 4],
    };
    assert_eq!(
        resample_ocean_coverage(2, 4, std::slice::from_ref(&page)).expect("upsample"),
        vec![
            10, 10, 20, 20, 10, 10, 20, 20, 30, 30, 40, 40, 30, 30, 40, 40
        ]
    );
    let page = aoe_map::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        ocean_coverage_percent: (0..16).collect(),
        inland_coverage_percent: vec![0; 16],
    };
    assert_eq!(
        resample_ocean_coverage(4, 2, &[page]).expect("downsample"),
        vec![5, 7, 13, 15]
    );
}

#[test]
fn vector_geometry_budget_counts_retained_buffer_and_transient_source() {
    assert_eq!(
        checked_geometry_bytes(10, 20, 30).expect("within budget"),
        60
    );
    assert!(checked_geometry_bytes(1, MAX_PAGE_GEOMETRY_BYTES, 1).is_err());
}
