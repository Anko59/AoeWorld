use super::*;

fn point(longitude: f64, latitude: f64) -> HydeGeographicPoint {
    HydeGeographicPoint {
        longitude_degrees: longitude,
        latitude_degrees: latitude,
    }
}

fn rectangle(west: f64, south: f64, east: f64, north: f64) -> Vec<HydeGeographicPoint> {
    vec![
        point(west, south),
        point(east, south),
        point(east, north),
        point(west, north),
    ]
}

fn source(
    polygon: Vec<HydeGeographicPoint>,
    state: HydeAreaState,
    crop: Option<f64>,
    grazing: Option<f64>,
    population: Option<f64>,
) -> HydeSourceAreaCell {
    HydeSourceAreaCell {
        polygon,
        state,
        crop_area_square_kilometers: crop,
        grazing_area_square_kilometers: grazing,
        population,
    }
}

fn target(polygon: Vec<HydeGeographicPoint>) -> HydeTargetAreaCell {
    HydeTargetAreaCell { polygon }
}

fn land_source(
    polygon: Vec<HydeGeographicPoint>,
    crop: f64,
    grazing: f64,
    population: f64,
) -> HydeSourceAreaCell {
    source(
        polygon,
        HydeAreaState::Land,
        Some(crop),
        Some(grazing),
        Some(population),
    )
}

fn projected_area_square_kilometers(polygon: &[HydeGeographicPoint]) -> f64 {
    project_polygon(
        polygon,
        &equal_area_transform(0, 0).expect("local equal-area transform"),
    )
    .expect("projected polygon")
    .area
        / 1_000_000.0
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs().max(1.0) * 1.0e-8,
        "actual {actual} differs from expected {expected}"
    );
}

fn allocations_for_three_by_three() -> Vec<HydeAreaAllocation> {
    (1..=9)
        .map(|quantity| HydeAreaAllocation {
            land_area_square_meters: if quantity == 9 {
                900_000_000.0
            } else {
                100_000_000.0
            },
            crop_area_square_kilometers: f64::from(quantity) + 0.125,
            grazing_area_square_kilometers: f64::from(quantity) / 2.0 + 0.125,
            population: f64::from(quantity * 100) + 0.125,
            ..HydeAreaAllocation::default()
        })
        .collect()
}

#[test]
fn one_source_cell_split_among_targets_conserves_all_quantities() {
    let source_polygon = rectangle(-0.2, -0.1, 0.2, 0.1);
    let left = rectangle(-0.2, -0.1, 0.0, 0.1);
    let right = rectangle(0.0, -0.1, 0.2, 0.1);
    let allocations = allocate_hyde_area_window(
        &[source(
            source_polygon,
            HydeAreaState::Land,
            Some(12.0),
            Some(8.0),
            Some(900.0),
        )],
        &[target(left), target(right)],
        0,
        0,
    )
    .expect("allocated split cell");

    close(
        allocations
            .iter()
            .map(|cell| cell.crop_area_square_kilometers)
            .sum(),
        12.0,
    );
    close(
        allocations
            .iter()
            .map(|cell| cell.grazing_area_square_kilometers)
            .sum(),
        8.0,
    );
    close(allocations.iter().map(|cell| cell.population).sum(), 900.0);
    close(
        allocations
            .iter()
            .map(|cell| cell.land_area_square_meters)
            .sum(),
        project_polygon(
            &rectangle(-0.2, -0.1, 0.2, 0.1),
            &equal_area_transform(0, 0).expect("local equal-area transform"),
        )
        .expect("projected source polygon")
        .area,
    );
}

#[test]
fn two_source_cells_contribute_to_one_target_without_averaging() {
    let allocations = allocate_hyde_area_window(
        &[
            source(
                rectangle(-0.2, -0.1, 0.0, 0.1),
                HydeAreaState::Land,
                Some(10.0),
                Some(2.0),
                Some(100.0),
            ),
            source(
                rectangle(0.2, -0.1, 0.4, 0.1),
                HydeAreaState::Land,
                Some(20.0),
                Some(4.0),
                Some(300.0),
            ),
        ],
        &[target(rectangle(-0.15, -0.1, 0.3, 0.1))],
        0,
        0,
    )
    .expect("allocated partial overlaps from adjacent source cells");

    close(allocations[0].crop_area_square_kilometers, 17.5);
    close(allocations[0].grazing_area_square_kilometers, 3.5);
    close(allocations[0].population, 225.0);
    assert!(allocations[0].land_area_square_meters > 0.0);
    assert!(allocations[0].outside_area_square_meters > 0.0);
}

#[test]
fn partial_selection_gets_only_its_proportional_quantity() {
    let allocations = allocate_hyde_area_window(
        &[source(
            rectangle(-0.2, -0.1, 0.2, 0.1),
            HydeAreaState::Land,
            Some(20.0),
            Some(4.0),
            Some(800.0),
        )],
        &[target(rectangle(-0.2, -0.1, 0.0, 0.1))],
        0,
        0,
    )
    .expect("allocated partial source cell");

    close(allocations[0].crop_area_square_kilometers, 10.0);
    close(allocations[0].grazing_area_square_kilometers, 2.0);
    close(allocations[0].population, 400.0);
    close(allocations[0].outside_area_square_meters, 0.0);
}

#[test]
fn odd_three_to_two_pyramid_reduction_sums_extensive_values() {
    let input = allocations_for_three_by_three();
    let reduced = reduce_area_grid(3, &input).expect("reduced odd axis");

    assert_eq!(reduced.len(), 4);
    close(reduced[0].crop_area_square_kilometers, 12.5);
    close(reduced[1].crop_area_square_kilometers, 9.25);
    close(reduced[2].crop_area_square_kilometers, 15.25);
    close(reduced[3].crop_area_square_kilometers, 9.125);
    close(
        reduced
            .iter()
            .map(|cell| cell.crop_area_square_kilometers)
            .sum(),
        46.125,
    );
    close(
        reduced
            .iter()
            .map(|cell| cell.grazing_area_square_kilometers)
            .sum(),
        23.625,
    );
    close(reduced.iter().map(|cell| cell.population).sum(), 4_501.125);
    close(
        reduced
            .iter()
            .map(|cell| cell.land_area_square_meters)
            .sum(),
        1_700_000_000.0,
    );

    let prepared = prepare_hyde_area_pyramid(3, input).expect("built weighted pyramid");
    let top = prepared.pages.last().expect("top page");
    assert_eq!(top.crop_percent, [3]);
    assert_eq!(top.population_pressure_per_square_kilometer, [3]);
}

#[test]
fn zero_land_values_are_distinct_from_nodata_and_water_states() {
    let whole_target = vec![
        point(-0.25, -0.05),
        point(-0.15, -0.05),
        point(-0.05, -0.05),
        point(0.05, -0.05),
        point(0.15, -0.05),
        point(0.25, -0.05),
        point(0.25, 0.05),
        point(0.15, 0.05),
        point(0.05, 0.05),
        point(-0.05, 0.05),
        point(-0.15, 0.05),
        point(-0.25, 0.05),
    ];
    let states = [
        HydeAreaState::Land,
        HydeAreaState::Lake,
        HydeAreaState::Ocean,
        HydeAreaState::NoData,
        HydeAreaState::OutsideCoverage,
    ];
    let sources = states
        .into_iter()
        .enumerate()
        .map(|(index, state)| {
            let west = -0.25 + index as f64 * 0.1;
            let east = west + 0.1;
            if state == HydeAreaState::Land {
                source(
                    rectangle(west, -0.05, east, 0.05),
                    state,
                    Some(0.0),
                    Some(0.0),
                    Some(0.0),
                )
            } else {
                source(rectangle(west, -0.05, east, 0.05), state, None, None, None)
            }
        })
        .collect::<Vec<_>>();
    let allocation = allocate_hyde_area_window(&sources, &[target(whole_target)], 0, 0)
        .expect("allocated mixed states")[0];

    assert!(allocation.land_area_square_meters > 0.0);
    assert!(allocation.lake_area_square_meters > 0.0);
    assert!(allocation.ocean_area_square_meters > 0.0);
    assert!(allocation.nodata_area_square_meters > 0.0);
    assert!(allocation.outside_area_square_meters > 0.0);
    assert_eq!(allocation.crop_area_square_kilometers, 0.0);
    assert_eq!(allocation.population, 0.0);
}

#[test]
fn source_and_target_query_order_do_not_change_allocations() {
    let sources = vec![
        source(
            rectangle(-0.2, -0.1, 0.0, 0.1),
            HydeAreaState::Land,
            Some(11.0),
            Some(1.0),
            Some(101.0),
        ),
        source(
            rectangle(0.0, -0.1, 0.2, 0.1),
            HydeAreaState::Land,
            Some(19.0),
            Some(3.0),
            Some(307.0),
        ),
    ];
    let targets = vec![
        target(rectangle(0.0, -0.1, 0.2, 0.1)),
        target(rectangle(-0.2, -0.1, 0.0, 0.1)),
    ];
    let forward =
        allocate_hyde_area_window(&sources, &targets, 0, 0).expect("forward allocation order");
    let mut reverse_sources = sources;
    reverse_sources.reverse();
    let mut reverse_targets = targets;
    reverse_targets.reverse();
    let reverse = allocate_hyde_area_window(&reverse_sources, &reverse_targets, 0, 0)
        .expect("reverse allocation order");

    assert_eq!(forward[0], reverse[1]);
    assert_eq!(forward[1], reverse[0]);
}

#[test]
fn target_page_enumeration_order_does_not_change_page_allocations() {
    let source_cell = source(
        rectangle(-0.2, -0.1, 0.4, 0.1),
        HydeAreaState::Land,
        Some(36.0),
        Some(6.0),
        Some(720.0),
    );
    let pages = vec![
        vec![
            target(rectangle(-0.2, -0.1, 0.0, 0.1)),
            target(rectangle(0.0, -0.1, 0.2, 0.1)),
        ],
        vec![target(rectangle(0.2, -0.1, 0.4, 0.1))],
    ];
    let forward = pages
        .iter()
        .map(|targets| {
            allocate_hyde_area_window(std::slice::from_ref(&source_cell), targets, 0, 0)
                .expect("allocated forward page")
        })
        .collect::<Vec<_>>();
    let mut reversed_pages = pages;
    reversed_pages.reverse();
    let mut reversed = reversed_pages
        .iter()
        .map(|targets| {
            let mut sources = vec![source_cell.clone()];
            sources.reverse();
            allocate_hyde_area_window(&sources, targets, 0, 0).expect("allocated reversed page")
        })
        .collect::<Vec<_>>();
    reversed.reverse();

    assert_eq!(forward, reversed);
}

#[test]
fn required_land_values_cannot_be_silently_replaced_with_zero() {
    let missing = source(
        rectangle(-0.1, -0.1, 0.1, 0.1),
        HydeAreaState::Land,
        None,
        Some(0.0),
        Some(0.0),
    );
    let result =
        allocate_hyde_area_window(&[missing], &[target(rectangle(-0.1, -0.1, 0.1, 0.1))], 0, 0);
    assert!(matches!(result, Err(GeodataError::Preparation(_))));
}

#[test]
fn antimeridian_edges_fail_closed_for_sources_and_targets() {
    let crossing = rectangle(179.5, -0.1, -179.5, 0.1);
    let ordinary = rectangle(-0.1, -0.1, 0.1, 0.1);
    let cases = [
        (
            vec![land_source(crossing.clone(), 0.0, 0.0, 0.0)],
            vec![target(ordinary.clone())],
        ),
        (
            vec![land_source(ordinary.clone(), 0.0, 0.0, 0.0)],
            vec![target(crossing)],
        ),
    ];

    for (sources, targets) in cases {
        let result = allocate_hyde_area_window(&sources, &targets, 0, 0);
        assert!(
            matches!(
                result,
                Err(GeodataError::Preparation(reason)) if reason.contains("antimeridian")
            ),
            "expected antimeridian rejection"
        );
    }
}

#[test]
fn pole_boundary_edges_fail_closed_for_sources_and_targets() {
    let touching = rectangle(-0.5, 89.9, 0.5, 90.0);
    let ordinary = rectangle(-0.5, -0.1, 0.5, 0.1);
    let cases = [
        (
            vec![land_source(touching.clone(), 0.0, 0.0, 0.0)],
            vec![target(ordinary.clone())],
        ),
        (
            vec![land_source(ordinary, 0.0, 0.0, 0.0)],
            vec![target(touching)],
        ),
    ];

    for (sources, targets) in cases {
        let result = allocate_hyde_area_window(&sources, &targets, 0, 0);
        assert!(
            matches!(
                result,
                Err(GeodataError::Preparation(reason)) if reason.contains("pole")
            ),
            "expected pole rejection"
        );
    }
}

#[test]
fn edges_strictly_inside_geographic_boundaries_are_accepted() {
    let antimeridian_interior = rectangle(179.5, -0.1, 179.9, 0.1);
    let polar_interior = rectangle(-0.5, 89.0, 0.5, 89.9);
    let centers = [(0, 1_797_000_000), (89_500_000, 0)];

    for (polygon, (latitude, longitude)) in [antimeridian_interior, polar_interior]
        .into_iter()
        .zip(centers)
    {
        allocate_hyde_area_window(
            &[land_source(polygon.clone(), 0.0, 0.0, 0.0)],
            &[target(polygon)],
            latitude,
            longitude,
        )
        .expect("strictly interior geographic edges should be accepted");
    }
}

#[test]
fn source_land_quantities_at_combined_capacity_are_accepted() {
    let polygon = rectangle(-0.2, -0.1, 0.2, 0.1);
    let land_area = projected_area_square_kilometers(&polygon);
    let half = land_area / 2.0;
    let allocations = allocate_hyde_area_window(
        &[land_source(polygon.clone(), half, half, 100.0)],
        &[target(polygon)],
        0,
        0,
    )
    .expect("capacity boundary should be accepted");

    close(
        allocations[0].crop_area_square_kilometers + allocations[0].grazing_area_square_kilometers,
        allocations[0].land_area_square_meters / 1_000_000.0,
    );
}

#[test]
fn source_land_quantities_above_area_or_combined_capacity_are_rejected() {
    let polygon = rectangle(-0.2, -0.1, 0.2, 0.1);
    let land_area = projected_area_square_kilometers(&polygon);
    let half = land_area / 2.0;
    let cases = [
        (land_area + 1.0, 0.0, "land area"),
        (0.0, land_area + 1.0, "land area"),
        (half, half + 1.0, "capacity"),
    ];

    for (crop, grazing, expected_reason) in cases {
        let result = allocate_hyde_area_window(
            &[land_source(polygon.clone(), crop, grazing, 0.0)],
            &[target(polygon.clone())],
            0,
            0,
        );
        assert!(
            matches!(
                result,
                Err(GeodataError::Preparation(reason)) if reason.contains(expected_reason)
            ),
            "expected rejection containing {expected_reason:?}"
        );
    }
}
