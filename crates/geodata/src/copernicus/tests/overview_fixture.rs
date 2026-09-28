use super::*;
use aoe_map::{FieldPyramid, PyramidLevel, ordered_land_use_page_root};

pub(super) fn overview_fixture() -> PreparedOverview {
    let water_pages = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| WaterPage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            ocean_coverage_percent: vec![100; 64 * 64],
            inland_coverage_percent: vec![0; 64 * 64],
        })
        .collect();
    let vegetation_pages = [(0, 0), (1, 0), (0, 1), (1, 1)]
        .into_iter()
        .map(|(x, y)| PotentialBiomePage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            potential_biome_class: vec![7; 64 * 64],
        })
        .collect();
    let mut historical_land_use_pages = Vec::new();
    let mut historical_levels = Vec::new();
    let mut axis = 128_u16;
    let mut level = 0_u8;
    loop {
        let mut level_pages = Vec::new();
        for y in (0..axis).step_by(64) {
            for x in (0..axis).step_by(64) {
                let width = (axis - x).min(64) as u8;
                let height = (axis - y).min(64) as u8;
                let samples = usize::from(width) * usize::from(height);
                level_pages.push(HistoricalLandUsePage {
                    level,
                    x: x / 64,
                    y: y / 64,
                    width,
                    height,
                    crop_percent: vec![1; samples],
                    grazing_percent: vec![2; samples],
                    population_pressure_per_square_kilometer: vec![3; samples],
                    coverage: Vec::new(),
                });
            }
        }
        historical_levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_land_use_page_root(&level_pages).unwrap(),
        });
        historical_land_use_pages.extend(level_pages);
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
        level += 1;
    }
    let elevation = FieldPyramid {
        levels: historical_levels
            .iter()
            .map(|level| PyramidLevel {
                samples_per_axis: level.samples_per_axis,
                ordered_page_root: [1; 32],
            })
            .collect(),
    };
    PreparedOverview {
        source_lock: fixture_source_lock("overview"),
        water_source_lock: fixture_source_lock("water"),
        vegetation_source_lock: fixture_source_lock("vegetation"),
        vegetation_classes_source_lock: fixture_source_lock("classes"),
        hyde_baseline_source_lock: fixture_source_lock("baseline"),
        hyde_supplementary_source_lock: fixture_source_lock("supplementary"),
        hyde_readme_source_lock: fixture_source_lock("readme"),
        projection: ProjectionMetadata::default(),
        provenance: EnvironmentalProvenance::default(),
        environment: PreparedEnvironment {
            samples_per_axis: 128,
            geographic_millimeters_per_sample: 1_000,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            elevation,
            historical_land_use: Some(FieldPyramid {
                levels: historical_levels,
            }),
            ..PreparedEnvironment::default()
        },
        pages: Vec::new(),
        water_pages,
        vegetation_pages,
        historical_land_use_pages,
    }
}
