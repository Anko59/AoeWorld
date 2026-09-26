use super::*;
use aoe_map::{
    EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
    EnvironmentalProvenance, FieldPyramid, MapRequest, PageLayer, ProjectionMetadata, PyramidLevel,
    ordered_page_root,
};

fn pyramid() -> FieldPyramid {
    FieldPyramid {
        levels: vec![
            PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: [1; 32],
            },
            PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [2; 32],
            },
        ],
    }
}

fn prepared_ocean_package() -> MapPackage {
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: pyramid(),
        water: Some(pyramid()),
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("package")
}

fn ocean_root(coverage: u8) -> WaterPage {
    WaterPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![coverage],
        inland_coverage_percent: vec![0],
    }
}

#[derive(Debug)]
struct ElevationProvider {
    level_zero: Arc<EnvironmentPage>,
    overview: Arc<EnvironmentPage>,
}

impl EnvironmentPageProvider for ElevationProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if key.layer != PageLayer::Elevation {
            return Err(EnvironmentPageError::Missing);
        }
        match key.level {
            0 if (key.x, key.y) == (0, 0) => Ok(self.level_zero.clone()),
            1 if (key.x, key.y) == (0, 0) => Ok(self.overview.clone()),
            _ => Err(EnvironmentPageError::Missing),
        }
    }
}

fn provider_package() -> MapPackage {
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: pyramid(),
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("provider package")
}

fn flat_prepared_package() -> (MapPackage, Vec<ElevationPage>) {
    let level_zero = ElevationPage {
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
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&level_zero))
                        .expect("level zero root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                        .expect("overview root"),
                },
            ],
        },
        water: None,
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
    .expect("flat prepared package");
    (package, vec![level_zero, overview])
}

fn elevation_provider() -> ElevationProvider {
    ElevationProvider {
        level_zero: Arc::new(EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![0; 4],
        })),
        overview: Arc::new(EnvironmentPage::Elevation(ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        })),
    }
}

#[test]
fn all_ocean_root_skips_virtual_start_search() {
    let environment = PreparedEnvironment {
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [1; 32],
            }],
        }),
        ..PreparedEnvironment::default()
    };
    let page = WaterPage {
        level: 0,
        ..ocean_root(100)
    };
    assert!(all_ocean(&environment, &[page]));
}

#[test]
fn mixed_ocean_root_requires_regular_start_validation() {
    let environment = PreparedEnvironment {
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [1; 32],
            }],
        }),
        ..PreparedEnvironment::default()
    };
    let page = WaterPage {
        level: 0,
        ..ocean_root(99)
    };
    assert!(!all_ocean(&environment, &[page]));
}

#[test]
fn all_ocean_package_is_preview_only() {
    let service = GameplayService::from_prepared_map(
        prepared_ocean_package(),
        Vec::new(),
        vec![ocean_root(100)],
        Vec::new(),
        Vec::new(),
    )
    .expect("preview-only result");
    assert!(service.is_none());
}

#[test]
fn package_and_prepared_constructors_activate_a_playable_map() {
    assert!(matches!(
        GameplayService::from_map(
            MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package")
        ),
        Err(GameWorldError::StartSearchLimit)
    ));

    let (prepared, elevation_pages) = flat_prepared_package();
    let from_pages = GameplayService::from_prepared_map(
        prepared,
        elevation_pages,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect("prepared world");
    assert!(from_pages.is_some());
}

#[test]
fn page_provider_constructor_queries_bounded_environment_pages_before_activation() {
    let service =
        GameplayService::from_prepared_provider(provider_package(), Arc::new(elevation_provider()))
            .expect("provider world");
    assert!(service.is_some());

    assert!(matches!(
        GameplayService::from_prepared_provider_with_cancel(
            provider_package(),
            Arc::new(elevation_provider()),
            &|| true,
        ),
        Err(GameWorldError::StartSearchLimit)
    ));
}

#[path = "tests/modeled.rs"]
mod modeled;
