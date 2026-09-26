use super::*;
use crate::hyde::prepare_hyde_area_pyramid;

fn allocation(index: usize) -> HydeAreaAllocation {
    HydeAreaAllocation {
        land_area_square_meters: 1_000_000.0,
        valid_land_area_square_meters: 1_000_000.0,
        crop_area_square_kilometers: if index.is_multiple_of(3) { 0.5 } else { 0.25 },
        grazing_area_square_kilometers: 0.125,
        population: (index % 5) as f64,
        ..HydeAreaAllocation::default()
    }
}

fn pages(axis: u16, values: &[HydeAreaAllocation]) -> Vec<(u16, u16, Vec<HydeAreaAllocation>)> {
    let mut pages = Vec::new();
    for y in (0..axis).step_by(usize::from(PAGE)) {
        for x in (0..axis).step_by(usize::from(PAGE)) {
            let width = (axis - x).min(PAGE);
            let height = (axis - y).min(PAGE);
            let mut page = Vec::new();
            for row in y..y + height {
                for column in x..x + width {
                    page.push(values[usize::from(row) * usize::from(axis) + usize::from(column)]);
                }
            }
            pages.push((x / PAGE, y / PAGE, page));
        }
    }
    pages
}

#[test]
fn reverse_page_order_matches_full_grid_for_odd_axes() {
    for axis in [65, 127] {
        let values = (0..usize::from(axis).pow(2))
            .map(allocation)
            .collect::<Vec<_>>();
        let expected = prepare_hyde_area_pyramid(axis, values.clone()).unwrap();
        let mut stream = HistoricalPageStream::new(axis).unwrap();
        for (x, y, page) in pages(axis, &values).into_iter().rev() {
            stream.push(x, y, page).unwrap();
        }
        let actual = stream.finish().unwrap();
        assert_eq!(actual.field, expected.field);
        assert_eq!(actual.pages, expected.pages);
    }
}

#[test]
fn maximum_historical_axis_reduces_without_full_grid_retention() {
    let axis = super::super::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS;
    let values = vec![allocation(0); usize::from(PAGE).pow(2)];
    let mut stream = HistoricalPageStream::new(axis).unwrap();
    for y in (0..axis / PAGE).rev() {
        for x in (0..axis / PAGE).rev() {
            stream.push(x, y, values.clone()).unwrap();
            let pending_cells: usize = stream
                .levels
                .iter()
                .flat_map(|level| level.pending.values())
                .map(|page| page.values.len())
                .sum();
            assert!(pending_cells < usize::from(axis).pow(2) / 2);
        }
    }
    let output = stream.finish().unwrap();
    assert_eq!(output.field.levels.first().unwrap().samples_per_axis, 1_024);
    assert_eq!(output.field.levels.last().unwrap().samples_per_axis, 1);
    assert_eq!(output.pages.len(), 347);
}

#[test]
fn incomplete_or_duplicate_pages_fail_closed() {
    let mut stream = HistoricalPageStream::new(65).unwrap();
    let page = vec![allocation(0); usize::from(PAGE).pow(2)];
    stream.push(0, 0, page.clone()).unwrap();
    assert!(stream.push(0, 0, page).is_err());
    assert!(stream.finish().is_err());
}

#[test]
fn publication_preserves_unknown_and_valid_zero_coverage() {
    let mut values = vec![allocation(0); 4];
    values[0] = HydeAreaAllocation {
        nodata_area_square_meters: 1_000_000.0,
        ..HydeAreaAllocation::default()
    };
    values[1] = HydeAreaAllocation {
        land_area_square_meters: 1_000_000.0,
        valid_land_area_square_meters: 1_000_000.0,
        ..HydeAreaAllocation::default()
    };
    values[2] = HydeAreaAllocation {
        lake_area_square_meters: 1_000_000.0,
        ..HydeAreaAllocation::default()
    };
    values[3] = HydeAreaAllocation {
        ocean_area_square_meters: 1_000_000.0,
        ..HydeAreaAllocation::default()
    };
    let mut stream = HistoricalPageStream::new(2).unwrap();
    stream.push(0, 0, values).unwrap();
    let page = &stream.finish().unwrap().pages[0];
    assert_eq!(page.coverage[0].nodata_percent, 100);
    assert_eq!(page.coverage[1].land_percent, 100);
    assert_eq!(page.coverage[1].valid_land_percent, 100);
    assert_eq!(page.crop_percent[1], 0);
    assert_eq!(page.coverage[2].lake_percent, 100);
    assert_eq!(page.coverage[3].ocean_percent, 100);
}
