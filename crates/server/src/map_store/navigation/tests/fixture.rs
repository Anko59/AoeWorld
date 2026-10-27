use aoe_map::*;
use aoe_simulation::GameWorld;

pub(super) fn flat_world(width: u32) -> GameWorld {
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let biome = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![28],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest {
            requested_side_meters: u64::from(width) * 60,
            compression: Ratio::new(30, 1).unwrap(),
            ..Default::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 1,
            geographic_millimeters_per_sample: u64::from(width) * 60_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation)).unwrap(),
                }],
            },
            vegetation: Some(FieldPyramid {
                levels: vec![PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&biome))
                        .unwrap(),
                }],
            }),
            water: Some(FieldPyramid {
                levels: vec![PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                        .unwrap(),
                }],
            }),
            ..Default::default()
        },
    )
    .unwrap();
    GameWorld::from_prepared_map(
        package,
        vec![elevation],
        vec![water],
        vec![biome],
        Vec::new(),
    )
    .unwrap()
}
