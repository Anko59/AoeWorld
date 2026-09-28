use super::*;
use crate::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS;

#[test]
fn decimal_source_capacity_accepts_binary_roundoff_but_rejects_real_excess() {
    let mut cell = HydeSourceAreaCell {
        valid_land_area_square_kilometers: Some(57.9623),
        ..source(
            rectangle(9.0, 47.5, 9.1, 47.6),
            HydeAreaState::Land,
            Some(57.4671039166),
            Some(0.4951960834),
            Some(0.0),
        )
    };
    let (crop, grazing, _, denominator) = source_quantities(&cell, 80_000_000.0)
        .expect("production decimal sum differs by only one ULP");
    assert!(crop + grazing > denominator);
    assert_eq!(crop, 57.4671039166);
    cell.grazing_area_square_kilometers = Some(0.49519609);
    assert!(source_quantities(&cell, 80_000_000.0).is_err());
    cell.valid_land_area_square_kilometers = Some(0.0);
    cell.crop_area_square_kilometers = Some(0.00005);
    cell.grazing_area_square_kilometers = Some(0.0);
    assert!(source_quantities(&cell, 80_000_000.0).is_err());
}

#[test]
fn spherical_denominator_does_not_make_coastal_valid_coverage_exceed_land() {
    let allocation = HydeAreaAllocation {
        land_area_square_meters: 49_490_000.0,
        valid_land_area_square_meters: 49_610_000.0,
        ocean_area_square_meters: 50_510_000.0,
        ..HydeAreaAllocation::default()
    };
    let coverage = allocation.to_coverage();
    assert_eq!(coverage.land_percent, 49);
    assert_eq!(coverage.valid_land_percent, 49);
    assert_eq!(allocation.valid_land_area_square_meters, 49_610_000.0);
}

#[test]
fn spherical_source_denominator_is_conserved_in_wgs84_allocation() {
    // Production maxln_cr.asc: row 792, full land cell, 6371-km sphere.
    // Its 78.4654 km² exceeds this WGS84 cell's approximately 78.2876 km².
    let polygon = rectangle(10.0, 24.0 - 1.0 / 12.0, 10.0 + 1.0 / 12.0, 24.0);
    let cell = HydeSourceAreaCell {
        valid_land_area_square_kilometers: Some(78.4654),
        ..source(
            polygon.clone(),
            HydeAreaState::Land,
            Some(40.0),
            Some(38.4654),
            Some(7846.54),
        )
    };
    let allocated =
        allocate_hyde_area_window(&[cell], &[target(polygon)], 240_000_000, 100_000_000)
            .expect("source areas use their own Earth model");
    let value = allocated[0];
    assert!(value.valid_land_area_square_meters > value.land_area_square_meters);
    close(value.valid_land_area_square_meters, 78_465_400.0);
    close(value.crop_area_square_kilometers, 40.0);
    close(value.grazing_area_square_kilometers, 38.4654);
    close(value.population, 7846.54);
    assert_eq!(
        value.to_land_use().population_pressure_per_square_kilometer,
        100
    );
}

#[test]
fn valid_land_area_remains_unrounded_and_is_the_pyramid_denominator() {
    let source_cell = source(
        rectangle(-0.2, -0.1, 0.2, 0.1),
        HydeAreaState::Land,
        Some(6.0),
        Some(2.0),
        Some(80.0),
    );
    let source_cell = HydeSourceAreaCell {
        valid_land_area_square_kilometers: Some(10.0),
        ..source_cell
    };
    let allocations = allocate_hyde_area_window(
        &[source_cell],
        &[
            target(rectangle(-0.2, -0.1, 0.0, 0.1)),
            target(rectangle(0.0, -0.1, 0.2, 0.1)),
        ],
        0,
        0,
    )
    .expect("allocated valid land denominator");

    close(
        allocations
            .iter()
            .map(|cell| cell.valid_land_area_square_meters)
            .sum(),
        10_000_000.0,
    );
    close(
        allocations
            .iter()
            .map(|cell| cell.crop_area_square_kilometers)
            .sum(),
        6.0,
    );
    assert!(
        allocations
            .iter()
            .all(|cell| cell.land_area_square_meters > 0.0)
    );
}

#[test]
fn historical_weighted_pyramid_reaches_its_independent_1024_axis_limit() {
    let prepared = prepare_hyde_area_pyramid(
        MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS,
        vec![
            HydeAreaAllocation::default();
            usize::from(MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).pow(2)
        ],
    )
    .expect("prepared 1024-sample historical field");
    assert_eq!(
        prepared.field.levels[0].samples_per_axis,
        MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS
    );
}
