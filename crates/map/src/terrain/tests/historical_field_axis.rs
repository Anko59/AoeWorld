use super::super::*;
use crate::{FieldPyramid, PyramidLevel, ordered_land_use_page_root};

#[test]
fn history_lookup_uses_historical_axis_below_elevation_axis() {
    let first = HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        crop_percent: vec![0, 10, 20, 30],
        grazing_percent: vec![0; 4],
        population_pressure_per_square_kilometer: vec![0; 4],
        coverage: Vec::new(),
    };
    let last = HistoricalLandUsePage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        crop_percent: vec![15],
        grazing_percent: vec![0],
        population_pressure_per_square_kilometer: vec![0],
        coverage: Vec::new(),
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 4,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: [4, 2, 1]
                .into_iter()
                .map(|axis| PyramidLevel {
                    samples_per_axis: axis,
                    ordered_page_root: [axis as u8; 32],
                })
                .collect(),
        },
        water: None,
        vegetation: None,
        historical_land_use: Some(FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_land_use_page_root(std::slice::from_ref(&first))
                        .unwrap(),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_land_use_page_root(std::slice::from_ref(&last))
                        .unwrap(),
                },
            ],
        }),
        hydrology_evidence: None,
    };
    environment.validate().unwrap();
    let generator = MapChunkGenerator::new([1; 32], 1, 100)
        .with_historical_land_use(&environment, vec![first, last])
        .unwrap();
    let history = generator.historical_land_use.as_ref().unwrap();
    assert_eq!(
        history.at(TileCoord::new(0, 0), 100).unwrap().crop_percent,
        0
    );
    assert_eq!(
        history
            .at(TileCoord::new(99, 99), 100)
            .unwrap()
            .crop_percent,
        30
    );
}
