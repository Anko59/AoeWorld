use aoe_map::*;

/// Survey legal build sites without leaving an observer that could mask
/// construction sight. Economy fixtures still place through ordinary orders.
pub(crate) fn survey(
    world: &mut crate::GameWorld,
    player: aoe_core::PlayerId,
    center: aoe_core::TileCoord,
    radius: u8,
) {
    // Reserve an ordinary transient ID: observer storage is dense by the
    // sequential entity allocator, so a sentinel ID would allocate 64 GiB.
    let observer = world.allocate_entity_id().expect("survey observer id");
    world.update_observer(observer, Some((player, center, radius)));
    world.update_observer(observer, None);
}

pub(crate) fn flat_fixture(ocean: bool) -> (MapPackage, Vec<ElevationPage>, Vec<WaterPage>) {
    let zero = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let water_zero = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![100; 4],
        inland_coverage_percent: vec![0; 4],
    };
    let water_overview = WaterPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![100],
        inland_coverage_percent: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&zero)).unwrap(),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview)).unwrap(),
                },
            ],
        },
        water: ocean.then(|| FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water_zero))
                        .unwrap(),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_water_page_root(std::slice::from_ref(
                        &water_overview,
                    ))
                    .unwrap(),
                },
            ],
        }),
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .unwrap();
    (
        package,
        vec![zero, overview],
        if ocean {
            vec![water_zero, water_overview]
        } else {
            Vec::new()
        },
    )
}
