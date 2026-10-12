use super::*;

fn page() -> ElevationPage {
    ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![1, 2, 3, 4],
    }
}

#[test]
fn elevation_pages_are_bounded_and_content_addressed() {
    let original = page().content_hash().expect("original hash");
    let mut changed = page();
    changed.geographic_height_centimeters[3] = 5;
    assert_ne!(original, changed.content_hash().expect("changed hash"));
    changed.geographic_height_centimeters.pop();
    assert_eq!(changed.content_hash(), Err(EnvironmentError::InvalidPage));
}

#[test]
fn page_roots_are_canonical_and_reject_duplicate_locations() {
    let first = page();
    let mut second = page();
    second.x = 1;
    assert_eq!(
        ordered_page_root(&[first.clone(), second.clone()]),
        ordered_page_root(&[second, first.clone()])
    );
    assert_eq!(
        ordered_page_root(&[first.clone(), first]),
        Err(EnvironmentError::InvalidPyramid)
    );
}

#[test]
fn prepared_field_requires_a_complete_bounded_pyramid() {
    let field = PreparedEnvironment {
        samples_per_axis: 4,
        geographic_millimeters_per_sample: 30_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 4,
                    ordered_page_root: [1; 32],
                },
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [2; 32],
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [3; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    assert!(field.validate().is_ok());
    let mut invalid = field;
    invalid.elevation.levels[1].samples_per_axis = 3;
    assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidPyramid));
}

#[test]
fn historical_field_uses_its_own_declared_grid_axis() {
    let mut environment = PreparedEnvironment {
        samples_per_axis: 4,
        geographic_millimeters_per_sample: 30_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
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
            levels: [2, 1]
                .into_iter()
                .map(|axis| PyramidLevel {
                    samples_per_axis: axis,
                    ordered_page_root: [axis as u8; 32],
                })
                .collect(),
        }),
        hydrology_evidence: None,
    };
    assert_eq!(environment.historical_samples_per_axis(), Some(2));
    assert_eq!(environment.validate(), Ok(()));
    environment.historical_land_use.as_mut().unwrap().levels[0].samples_per_axis = 5_000;
    assert_eq!(
        environment.validate(),
        Err(EnvironmentError::InvalidPyramid)
    );
}

#[test]
fn complete_prepared_pages_override_fallback_relief_and_apply_compression() {
    let first = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![-100, 200, 300, 400],
    };
    let second = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![250],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 125_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&first))
                        .expect("root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&second))
                        .expect("root"),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,

        hydrology_evidence: None,
    };
    let request = crate::MapRequest {
        requested_side_meters: 250,
        compression: crate::Ratio::new(1, 1).expect("compression"),
        ..crate::MapRequest::default()
    };
    let package = crate::MapPackage::with_prepared_environment(
        1,
        request,
        Vec::new(),
        crate::ProjectionMetadata::default(),
        crate::EnvironmentalProvenance::default(),
        environment,
    )
    .expect("package");
    let terrain = package
        .generator_with_elevation(vec![first, second])
        .expect("prepared terrain");
    let low_elevation = terrain
        .tile_at(aoe_core::TileCoord::new(0, 0))
        .expect("low elevation");
    assert_eq!(low_elevation.geographic_height_centimeters, -100);
    assert_eq!(
        low_elevation.elevation_provenance,
        crate::Provenance::SourceDerived
    );
    assert_eq!(low_elevation.water_provenance, crate::Provenance::Fallback);
    let tile = terrain
        .tile_at(aoe_core::TileCoord::new(124, 124))
        .expect("tile");
    assert_eq!(tile.geographic_height_centimeters, 400);
    assert_eq!(tile.game_height_level, 4);
    assert_eq!(tile.elevation_provenance, crate::Provenance::SourceDerived);
}
