use super::*;

#[test]
fn file_geodatabase_lake_cursor_is_rewound_after_iterator_count() {
    use gdal::vector::{Feature, Geometry, LayerOptions, OGRFieldType, OGRwkbGeometryType};
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "aoe-lake-cursor-{}-{nonce}.gdb",
        std::process::id()
    ));
    let driver = gdal::DriverManager::get_driver_by_name("OpenFileGDB").unwrap();
    let mut database = driver.create_vector_only(&path).unwrap();
    let mut reference = SpatialRef::from_epsg(4326).unwrap();
    reference.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    {
        let layer = database
            .create_layer(LayerOptions {
                name: "lakes",
                srs: Some(&reference),
                ty: OGRwkbGeometryType::wkbPolygon,
                ..Default::default()
            })
            .unwrap();
        layer
            .create_defn_fields(&[("Lake_type", OGRFieldType::OFTInteger)])
            .unwrap();
        for (west, kind) in [(25.0, 1), (27.0, 2)] {
            let east = west + 1.0;
            let mut feature = Feature::new(layer.defn()).unwrap();
            feature.set_field_integer(0, kind).unwrap();
            feature
                .set_geometry(
                    Geometry::from_wkt(&format!(
                        "POLYGON (({west} 61, {east} 61, {east} 62, {west} 62, {west} 61))"
                    ))
                    .unwrap(),
                )
                .unwrap();
            feature.create(&layer).unwrap();
        }
    }
    drop(database);
    let mut database = Dataset::open_ex(
        &path,
        DatasetOptions {
            open_flags: GdalOpenFlags::GDAL_OF_VECTOR,
            ..Default::default()
        },
    )
    .unwrap();
    for (longitude, expected) in [
        (25.5, HydrologyKind::Lake),
        (27.5, HydrologyKind::Reservoir),
        (25.5, HydrologyKind::Lake),
    ] {
        let definition =
            crate::local_aeqd_definition(615_000_000, (longitude * 10_000_000.0) as i32);
        let features = vector_features(
            &mut database,
            (longitude - 0.1, 61.4, longitude + 0.1, 61.6),
            &definition,
            false,
        )
        .unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].kind, expected);
        assert!(
            features[0]
                .geometry
                .contains(&Geometry::from_wkt("POINT (0 0)").unwrap())
        );
    }
    drop(database);
    fs::remove_dir_all(path).unwrap();
}

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
    assert!(flatten_ocean_pages(2, &[page.clone(), page.clone()]).is_err());
    assert!(flatten_ocean_pages(2, &[]).is_err());

    let zero_height = aoe_map::WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 0,
        ocean_coverage_percent: Vec::new(),
        inland_coverage_percent: Vec::new(),
    };
    assert!(matches!(
        flatten_ocean_pages(2, std::slice::from_ref(&zero_height)),
        Err(GeodataError::Preparation("ocean page has zero dimensions"))
    ));
    let malformed = aoe_map::WaterPage {
        ocean_coverage_percent: vec![0; 3],
        ..page.clone()
    };
    assert!(matches!(
        flatten_ocean_pages(2, std::slice::from_ref(&malformed)),
        Err(GeodataError::Preparation("ocean page shape is invalid"))
    ));
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
    assert!(matches!(
        resample_ocean_coverage(1, 2, &[]),
        Err(GeodataError::Preparation(
            "ocean resampling axes are outside supported bounds"
        ))
    ));
}

#[test]
fn vector_geometry_budget_counts_retained_buffer_and_transient_source() {
    assert_eq!(
        checked_geometry_bytes(10, 20, 30).expect("within budget"),
        60
    );
    assert!(checked_geometry_bytes(1, MAX_PAGE_GEOMETRY_BYTES, 1).is_err());
}

#[test]
fn hydrology_page_bounds_pad_the_query_without_accepting_empty_or_mismatched_shapes() {
    assert_eq!(
        page_bounds(&[2.0, 3.0], &[48.0, 49.0]).expect("bounds"),
        (1.99, 47.99, 3.01, 49.01)
    );
    assert!(matches!(
        page_bounds(&[], &[]),
        Err(GeodataError::Preparation(
            "hydrology page has no coordinates"
        ))
    ));
    assert!(page_bounds(&[2.0], &[48.0, 49.0]).is_err());
}

#[test]
fn vector_source_and_local_transform_fail_closed_without_a_real_archive() {
    assert!(
        open_vector_source(
            std::path::Path::new("/tmp/definitely-missing-hydrology.zip"),
            "missing.shp"
        )
        .is_err()
    );
    assert!(local_transform("not a projection definition").is_err());
    assert!(local_transform(&crate::local_aeqd_definition(48_850_000, 2_350_000)).is_ok());
}

fn worldcover_tile(
    path: &std::path::Path,
    latitude: i32,
    longitude: i32,
    transform: [f64; 6],
    value: u8,
    epsg: i32,
) -> OpenTile {
    let driver = gdal::DriverManager::get_driver_by_name("GTiff").expect("GTiff driver");
    let mut dataset = driver
        .create_with_band_type::<u8, _>(path, 3, 3, 1)
        .expect("worldcover raster");
    dataset.set_geo_transform(&transform).expect("transform");
    dataset
        .set_spatial_ref(
            &gdal::spatial_ref::SpatialRef::from_epsg(u32::try_from(epsg).expect("EPSG"))
                .expect("spatial ref"),
        )
        .expect("spatial reference");
    let mut band = dataset.rasterband(1).expect("band");
    let mut values = gdal::raster::Buffer::new((3, 3), vec![value; 9]);
    band.write((0, 0), (3, 3), &mut values).expect("values");
    dataset.flush_cache().expect("flush");
    OpenTile {
        latitude,
        longitude,
        dataset,
    }
}

#[test]
fn worldcover_sampling_validates_shape_crs_transform_coverage_and_class() {
    let directory =
        std::env::temp_dir().join(format!("aoe-worldcover-sampling-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("WorldCover fixture");
    let north_up = [2.0, 1.0, 0.0, 50.0, 0.0, -1.0];
    let first = directory.join("first.tif");
    let tile = worldcover_tile(&first, 47, 2, north_up, 40, 4326);
    assert!(matches!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[2.1], &[48.1, 48.2]),
        Err(GeodataError::Preparation(
            "WorldCover coordinate shape is invalid"
        ))
    ));
    assert_eq!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[2.1, 2.2], &[48.1, 48.2])
            .expect("covered samples"),
        vec![40, 40]
    );
    assert!(matches!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[6.0], &[48.0]),
        Err(GeodataError::Preparation(
            "selected WorldCover tiles do not cover every requested coordinate"
        ))
    ));

    let bad_crs = directory.join("bad-crs.tif");
    let tile = worldcover_tile(&bad_crs, 47, 2, north_up, 40, 3857);
    assert!(matches!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[2.5], &[48.5]),
        Err(GeodataError::Preparation(
            "WorldCover raster CRS is not EPSG:4326"
        ))
    ));

    let rotated = directory.join("rotated.tif");
    let tile = worldcover_tile(&rotated, 47, 2, [2.0, 1.0, 0.1, 50.0, 0.0, -1.0], 40, 4326);
    assert!(matches!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[2.5], &[48.5]),
        Err(GeodataError::Preparation(
            "WorldCover geotransform is not north-up"
        ))
    ));

    let unknown = directory.join("unknown.tif");
    let tile = worldcover_tile(&unknown, 47, 2, north_up, 55, 4326);
    assert!(matches!(
        sample_worldcover_page(std::slice::from_ref(&tile), &[2.5], &[48.5]),
        Err(GeodataError::Preparation(
            "WorldCover raster contains an unknown class value"
        ))
    ));

    let disagreement = directory.join("disagreement.tif");
    let second = worldcover_tile(&disagreement, 47, 2, north_up, 30, 4326);
    assert!(matches!(
        sample_worldcover_page(&[tile, second], &[2.5], &[48.5]),
        Err(GeodataError::Preparation(
            "overlapping WorldCover tiles disagree at a sample"
        ))
    ));
    std::fs::remove_dir_all(directory).expect("remove WorldCover fixture");
}

#[test]
fn river_line_projection_returns_stable_distance_and_station() {
    let line = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
    let (distance, station) = super::line_position(&line, 7.0, 3.0).expect("line projection");
    assert_eq!(distance, 3.0);
    assert_eq!(station, 7.0);

    let (distance, station) = super::line_position(&line, 10.0, 8.0).expect("line projection");
    assert_eq!(distance, 0.0);
    assert_eq!(station, 18.0);
    assert!(super::line_position(&line, f64::NAN, 0.0).is_none());
}
