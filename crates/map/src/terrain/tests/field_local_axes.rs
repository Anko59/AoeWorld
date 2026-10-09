use super::super::*;
use crate::{
    DetailProfile, EnvironmentPage, EnvironmentPageKey, FieldPyramid, MapPackage, MapPackageError,
    MapRequest, PageLayer, PyramidLevel,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
struct Pages(BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);

impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

fn field(axis: u16, layer: PageLayer, pages: &mut Pages) -> FieldPyramid {
    let mut levels = Vec::new();
    let mut samples = axis;
    loop {
        let level = levels.len() as u8;
        let mut hashes = Vec::new();
        for y in 0..samples.div_ceil(64) {
            for x in 0..samples.div_ceil(64) {
                let width = (samples - x * 64).min(64) as u8;
                let height = (samples - y * 64).min(64) as u8;
                let count = usize::from(width) * usize::from(height);
                let values = (0..count)
                    .map(|index| {
                        let sx = x * 64 + index as u16 % u16::from(width);
                        let sy = y * 64 + index as u16 / u16::from(width);
                        (sx + sy) % 3
                    })
                    .collect::<Vec<_>>();
                let page = match layer {
                    PageLayer::Elevation => EnvironmentPage::Elevation(ElevationPage {
                        level,
                        x,
                        y,
                        width,
                        height,
                        geographic_height_centimeters: vec![100; count],
                    }),
                    PageLayer::Water => EnvironmentPage::Water(WaterPage {
                        level,
                        x,
                        y,
                        width,
                        height,
                        ocean_coverage_percent: vec![0; count],
                        inland_coverage_percent: values.iter().map(|v| (*v * 50) as u8).collect(),
                    }),
                    PageLayer::Vegetation => EnvironmentPage::Vegetation(PotentialBiomePage {
                        level,
                        x,
                        y,
                        width,
                        height,
                        potential_biome_class: values.iter().map(|v| (*v * 4) as u8).collect(),
                    }),
                    PageLayer::HistoricalLandUse => {
                        EnvironmentPage::HistoricalLandUse(HistoricalLandUsePage {
                            level,
                            x,
                            y,
                            width,
                            height,
                            crop_percent: values.iter().map(|v| (*v * 10) as u8).collect(),
                            grazing_percent: vec![0; count],
                            population_pressure_per_square_kilometer: vec![0; count],
                            coverage: Vec::new(),
                        })
                    }
                    _ => unreachable!(),
                };
                hashes.push(page.content_hash().unwrap());
                pages.0.insert(page.key(), Arc::new(page));
            }
        }
        let mut root = crate::PageRootBuilder::new(layer, hashes.len()).unwrap();
        for hash in hashes {
            root.push(hash).unwrap();
        }
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: root.finish().unwrap(),
        });
        if samples == 1 {
            break;
        }
        samples = samples.div_ceil(2);
    }
    FieldPyramid { levels }
}

fn fixture(axes: [u16; 4]) -> (MapPackage, Pages) {
    let mut pages = Pages::default();
    let environment = PreparedEnvironment {
        samples_per_axis: axes[0],
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: field(axes[0], PageLayer::Elevation, &mut pages),
        vegetation: Some(field(axes[1], PageLayer::Vegetation, &mut pages)),
        water: Some(field(axes[2], PageLayer::Water, &mut pages)),
        historical_land_use: Some(field(axes[3], PageLayer::HistoricalLandUse, &mut pages)),
        hydrology_evidence: None,
    };
    let package = package(environment, DetailProfile::LandscapeV2).unwrap();
    (package, pages)
}

fn package(
    environment: PreparedEnvironment,
    detail_profile: DetailProfile,
) -> Result<MapPackage, MapPackageError> {
    MapPackage::with_prepared_environment(
        1,
        MapRequest {
            requested_side_meters: 512,
            compression: Ratio::new(1, 1).unwrap(),
            detail_profile,
            ..MapRequest::default()
        },
        Vec::new(),
        crate::ProjectionMetadata::default(),
        crate::EnvironmentalProvenance::default(),
        environment,
    )
}

fn dense(package: &MapPackage, pages: &Pages) -> MapChunkGenerator {
    let mut elevation = Vec::new();
    let mut water = Vec::new();
    let mut biome = Vec::new();
    let mut history = Vec::new();
    for page in pages.0.values() {
        match page.as_ref() {
            EnvironmentPage::Elevation(page) => elevation.push(page.clone()),
            EnvironmentPage::Water(page) => water.push(page.clone()),
            EnvironmentPage::Vegetation(page) => biome.push(page.clone()),
            EnvironmentPage::HistoricalLandUse(page) => history.push(page.clone()),
            _ => unreachable!(),
        }
    }
    package
        .generator_with_environment(elevation, water, biome, history)
        .unwrap()
}

#[test]
fn schema_ten_field_axes_roundtrip_without_new_wire_fields_or_identity_changes() {
    for axes in [[128, 32, 64, 16], [1024, 128, 128, 1024]] {
        let (original, _) = fixture(axes);
        assert_eq!(original.schema_version, crate::LANDSCAPE_MAP_SCHEMA_VERSION);
        assert_eq!(
            original.environment.validate(),
            Err(EnvironmentError::InvalidPyramid)
        );
        original.validate().unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let restored: MapPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(original, restored);
        assert_eq!(original.content_hash, restored.content_hash);
        assert_eq!(original.environment, restored.environment);
        restored.validate().unwrap();
        assert_eq!(
            package(original.environment, DetailProfile::StandardV1),
            Err(MapPackageError::InvalidEnvironment)
        );
    }
}

#[test]
fn published_prepared_axis_ceiling_and_valid_legacy_packages_are_unchanged() {
    // The country acquisition plan is bounded separately: do not narrow the
    // existing prepared-package contract while adding independent field axes.
    assert_eq!(crate::MAX_ENVIRONMENT_SAMPLES_PER_AXIS, 16_384);
    let (small, _) = fixture([128, 32, 64, 16]);
    let mut environment = small.environment;
    let mut axis = crate::MAX_ENVIRONMENT_SAMPLES_PER_AXIS;
    let mut levels = Vec::new();
    loop {
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: [3; 32],
        });
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
    }
    environment.samples_per_axis = crate::MAX_ENVIRONMENT_SAMPLES_PER_AXIS;
    environment.elevation = FieldPyramid { levels };
    environment.water = Some(environment.elevation.clone());
    environment.vegetation = Some(environment.elevation.clone());
    environment.historical_land_use = Some(environment.elevation.clone());
    environment.validate().unwrap();
    for profile in [DetailProfile::StandardV1, DetailProfile::LandscapeV2] {
        let original = package(environment.clone(), profile).unwrap();
        let restored: MapPackage =
            serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
        assert_eq!(restored, original);
        restored.validate().unwrap();
    }
    environment.samples_per_axis += 1;
    assert_eq!(environment.validate(), Err(EnvironmentError::InvalidIndex));
    assert_eq!(
        environment.validate_for_profile(DetailProfile::LandscapeV2),
        Err(EnvironmentError::InvalidIndex)
    );
}

#[test]
fn schema_ten_validates_every_axis_and_complete_canonical_pyramids() {
    let (original, _) = fixture([128, 32, 64, 16]);
    for layer in 0..4 {
        for corruption in 0..5 {
            let mut environment = original.environment.clone();
            let field = match layer {
                0 => &mut environment.elevation,
                1 => environment.vegetation.as_mut().unwrap(),
                2 => environment.water.as_mut().unwrap(),
                _ => environment.historical_land_use.as_mut().unwrap(),
            };
            match corruption {
                0 => field.levels[0].samples_per_axis = crate::MAX_ENVIRONMENT_SAMPLES_PER_AXIS + 1,
                1 => {
                    field.levels.pop();
                }
                2 => field.levels.push(field.levels.last().unwrap().clone()),
                3 => field.levels[0].ordered_page_root = [0; 32],
                _ => field.levels[0].samples_per_axis = 0,
            }
            assert!(
                environment
                    .validate_for_profile(DetailProfile::LandscapeV2)
                    .is_err()
            );
            let mut malformed = original.clone();
            malformed.environment = environment.clone();
            let json = serde_json::to_string(&malformed).unwrap();
            assert!(serde_json::from_str::<MapPackage>(&json).is_err());
            assert_eq!(
                package(environment, DetailProfile::LandscapeV2),
                Err(MapPackageError::InvalidEnvironment)
            );
        }
    }
    let mut environment = original.environment;
    environment.page_samples = 32;
    assert_eq!(environment.validate(), Err(EnvironmentError::InvalidIndex));
    assert_eq!(
        environment.validate_for_profile(DetailProfile::LandscapeV2),
        Err(EnvironmentError::InvalidIndex)
    );
}

#[test]
fn field_local_dense_and_provider_base_samples_match_edges_and_boundaries() {
    for axes in [[128, 32, 64, 16], [129, 65, 67, 17], [1024, 128, 128, 1024]] {
        let (package, pages) = fixture(axes);
        let dense = dense(&package, &pages);
        let lazy = package
            .generator_with_page_provider(Arc::new(pages))
            .unwrap();
        let nodata = TileCoord::new(0, 0);
        assert_eq!(
            dense.sample_base_tile(nodata).vegetation_provenance,
            Provenance::Fallback
        );
        assert_eq!(dense.sample_base_tile(nodata).water, WaterKind::None);
        let edge = dense.width_tiles - 1;
        for y in [0, 1, 31, 32, 63, 64, edge] {
            for x in [0, 1, 31, 32, 63, 64, edge] {
                let tile = TileCoord::new(x, y);
                assert_eq!(
                    dense.sample_base_tile(tile),
                    provider::sample_base_tile(&lazy, tile, &|| false).unwrap(),
                    "axes {axes:?}, tile {tile:?}"
                );
                let history = dense
                    .historical_land_use
                    .as_ref()
                    .unwrap()
                    .at(tile, dense.width_tiles)
                    .unwrap();
                let observed =
                    provider::sample_land_use_observation(&lazy, axes[3], tile, &|| false)
                        .unwrap()
                        .unwrap();
                assert_eq!(history.crop_percent, observed.crop_percent);
            }
        }
        for tile in [
            TileCoord::new(-1, 0),
            TileCoord::new(0, -1),
            TileCoord::new(edge + 1, 0),
        ] {
            assert_eq!(dense.tile_at_with_cancel(tile, &|| false), Ok(None));
            assert_eq!(lazy.tile_at_with_cancel(tile, &|| false), Ok(None));
        }
        assert_eq!(
            provider::sample_base_tile(&lazy, TileCoord::new(0, 0), &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
    }
}

#[test]
fn legacy_water_and_vegetation_mismatches_and_error_precedence_remain_strict() {
    let (package, _) = fixture([128, 32, 64, 16]);
    for water in [false, true] {
        let mut environment = package.environment.clone();
        if water {
            environment.vegetation = None;
        } else {
            environment.water = None;
        }
        assert_eq!(
            environment.validate(),
            Err(EnvironmentError::InvalidPyramid)
        );
        environment.geographic_millimeters_per_sample = 0;
        assert_eq!(environment.validate(), Err(EnvironmentError::InvalidIndex));
    }
}

#[test]
fn eager_field_page_grids_still_reject_missing_duplicate_and_outside_pages() {
    let (package, pages) = fixture([128, 32, 64, 16]);
    let water = pages
        .0
        .values()
        .filter_map(|page| match page.as_ref() {
            EnvironmentPage::Water(page) => Some(page.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    for corruption in 0..3 {
        let mut invalid = water.clone();
        match corruption {
            0 => {
                invalid.pop();
            }
            1 => invalid.push(invalid[0].clone()),
            _ => invalid[0].x = 1,
        }
        assert!(
            package
                .generator()
                .with_prepared_water(&package.environment, invalid)
                .is_err()
        );
    }
}

#[test]
fn missing_field_pages_never_silently_fall_back() {
    let (package, pages) = fixture([128, 32, 64, 16]);
    for layer in [
        PageLayer::Elevation,
        PageLayer::Water,
        PageLayer::Vegetation,
        PageLayer::HistoricalLandUse,
    ] {
        let mut missing = pages.clone();
        missing.0.remove(&EnvironmentPageKey {
            layer,
            level: 0,
            x: 0,
            y: 0,
        });
        let lazy = package
            .generator_with_page_provider(Arc::new(missing))
            .unwrap();
        let tile = TileCoord::new(0, 0);
        let result = if layer == PageLayer::HistoricalLandUse {
            provider::sample_land_use_observation(&lazy, 16, tile, &|| false).map(|_| ())
        } else {
            provider::sample_base_tile(&lazy, tile, &|| false).map(|_| ())
        };
        assert_eq!(result, Err(EnvironmentPageError::Missing));
    }
}
