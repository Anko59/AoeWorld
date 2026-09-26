use super::*;
use aoe_map::Ratio;

#[test]
fn fixed_fiji_page_unwraps_target_cells_around_the_request_center() {
    let request = MapRequest {
        center_latitude_e7: -178_000_000,
        center_longitude_e7: 1_798_000_000,
        requested_side_meters: 80_000,
        compression: Ratio::new(80, 1).expect("valid compression"),
        ..MapRequest::default()
    };
    let transform = area_reader::target_to_wgs84(request).expect("projection transform");
    let side = request
        .estimate()
        .expect("valid Fiji footprint")
        .effective_side_meters;
    let (targets, _) = area_reader::target_page(
        &transform,
        side,
        80,
        area_reader::PageBounds {
            x: 0,
            y: 0,
            width: 64,
            height: 64,
        },
        179.8,
    )
    .expect("transformed first target page");

    assert!(area_reader::validate_target_geography(&targets).is_ok());
    assert!(
        targets
            .iter()
            .flat_map(|target| &target.polygon)
            .any(|point| point.longitude_degrees > 180.0)
    );
}

#[test]
fn dateline_cells_are_unwrapped_around_the_request_center_and_conserved() {
    let polygon = vec![
        HydeGeographicPoint {
            longitude_degrees: 179.9,
            latitude_degrees: -0.1,
        },
        HydeGeographicPoint {
            longitude_degrees: -179.9,
            latitude_degrees: -0.1,
        },
        HydeGeographicPoint {
            longitude_degrees: -179.9,
            latitude_degrees: 0.1,
        },
        HydeGeographicPoint {
            longitude_degrees: 179.9,
            latitude_degrees: 0.1,
        },
    ];
    let source = HydeSourceAreaCell {
        polygon: polygon.clone(),
        state: HydeAreaState::Land,
        crop_area_square_kilometers: Some(0.0),
        grazing_area_square_kilometers: Some(0.0),
        population: Some(0.0),
        valid_land_area_square_kilometers: None,
    };
    let allocations = allocate_hyde_area_window(
        &[source],
        &[HydeTargetAreaCell { polygon }],
        -178_000_000,
        1_798_000_000,
    )
    .expect("dateline cells are local in the request-centered projection");

    assert!(allocations[0].land_area_square_meters > 0.0);
    assert!(allocations[0].outside_area_square_meters < 1.0e-4);
}

#[test]
fn polar_archive_request_fails_before_opening_sources() {
    let request = MapRequest {
        center_latitude_e7: 899_000_000,
        requested_side_meters: 80_000,
        ..MapRequest::default()
    };

    assert!(matches!(
        prepare_hyde_area_600(
            std::path::Path::new("missing-baseline.zip"),
            std::path::Path::new("missing-mask.zip"),
            request,
            80
        ),
        Err(GeodataError::Preparation(
            "HYDE target footprint touches or crosses a pole"
        ))
    ));
}

#[test]
fn archive_reader_rejects_1024_until_incremental_reduction_is_available() {
    assert!(matches!(
        prepare_hyde_area_600(
            std::path::Path::new("missing-baseline.zip"),
            std::path::Path::new("missing-mask.zip"),
            MapRequest::default(),
            1_024
        ),
        Err(GeodataError::Preparation(
            "area-aware HYDE grid is outside overview bounds"
        ))
    ));
}
