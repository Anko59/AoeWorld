use super::*;
use crate::HistoricalLandUsePage;

fn page(coverage: Vec<HistoricalCoverage>) -> HistoricalLandUsePage {
    HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        crop_percent: vec![0; 4],
        grazing_percent: vec![0; 4],
        population_pressure_per_square_kilometer: vec![0; 4],
        coverage,
    }
}

#[test]
fn raw_coverage_preserves_valid_zero_nodata_and_unobserved_land() {
    let coverage = vec![
        HistoricalCoverage {
            land_percent: 100,
            valid_land_percent: 100,
            ..HistoricalCoverage::default()
        },
        HistoricalCoverage {
            nodata_percent: 100,
            ..HistoricalCoverage::default()
        },
        HistoricalCoverage {
            land_percent: 100,
            ..HistoricalCoverage::default()
        },
        HistoricalCoverage {
            lake_percent: 100,
            ..HistoricalCoverage::default()
        },
    ];
    let source = page(coverage.clone());
    source.validate().unwrap();
    let history = HistoricalLandUse::new(2, [((0, 0), source)].into());
    for (index, tile) in [
        TileCoord::new(0, 0),
        TileCoord::new(2, 0),
        TileCoord::new(0, 2),
        TileCoord::new(2, 2),
    ]
    .into_iter()
    .enumerate()
    {
        let observation = history.at_observation(tile, 3).unwrap();
        assert_eq!(observation.coverage, Some(coverage[index]));
        assert_eq!(observation.crop_percent, 0);
        assert_eq!(observation.grazing_percent, 0);
        assert_eq!(observation.population_pressure, 0);
        assert_eq!(history.at(tile, 3).is_some(), index == 0);
    }
}

#[test]
fn raw_legacy_quantities_and_coordinates_match_legacy_wrapper() {
    let mut source = page(Vec::new());
    source.crop_percent = vec![3, 7, 13, 19];
    source.grazing_percent = vec![5, 11, 17, 23];
    source.population_pressure_per_square_kilometer = vec![29, 31, 37, 41];
    source.validate().unwrap();
    let history = HistoricalLandUse::new(2, [((0, 0), source)].into());
    for y in 0..=4 {
        for x in 0..=4 {
            let tile = TileCoord::new(x, y);
            let observation = history.at_observation(tile, 5).unwrap();
            let legacy = history.at(tile, 5).unwrap();
            assert_eq!(observation.coverage, None);
            assert_eq!(observation.crop_percent, legacy.crop_percent);
            assert_eq!(observation.grazing_percent, legacy.grazing_percent);
            assert_eq!(
                observation.population_pressure,
                legacy.population_pressure_per_square_kilometer
            );
        }
    }
    assert!(history.at_observation(TileCoord::new(-1, 0), 5).is_none());
    assert!(history.at_observation(TileCoord::new(20, 0), 5).is_none());
    assert!(history.at_observation(TileCoord::new(0, 0), 0).is_none());
    let missing = HistoricalLandUse::new(2, Default::default());
    assert!(missing.at_observation(TileCoord::new(0, 0), 5).is_none());
}
