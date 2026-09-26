use super::*;
use crate::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS;

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
