use super::*;
use crate::land_use::HistoricalLandUse;
use crate::{
    EnvironmentPage, EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid, HistoricalCoverage,
    HistoricalLandUsePage, PreparedEnvironment, PyramidLevel, Ratio,
};
use std::sync::Arc;

fn policy() -> LandscapePolicy {
    LandscapePolicy {
        region: Region::Heavy,
        support_per_thousand: 1000,
    }
}
fn page(crop: u8, coverage: Vec<HistoricalCoverage>) -> HistoricalLandUsePage {
    HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        crop_percent: vec![crop],
        grazing_percent: vec![0],
        population_pressure_per_square_kilometer: vec![if coverage
            .first()
            .is_some_and(|cell| cell.valid_land_percent == 0)
        {
            0
        } else {
            12
        }],
        coverage,
    }
}
fn dense(page: HistoricalLandUsePage) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([17; 32], 1, 256).with_elevation_sampling_recipe(8);
    generator.historical_land_use =
        Some(Arc::new(HistoricalLandUse::new(1, [((0, 0), page)].into())));
    generator
}

#[derive(Debug)]
struct Source(Result<Arc<EnvironmentPage>, EnvironmentPageError>);
impl EnvironmentPageProvider for Source {
    fn page(
        &self,
        _key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0.clone()
    }
}
fn provider(result: Result<Arc<EnvironmentPage>, EnvironmentPageError>) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([17; 32], 1, 256).with_elevation_sampling_recipe(8);
    // Independent history axis, no source-elevation field in this fixture.
    let environment = PreparedEnvironment {
        samples_per_axis: 0,
        historical_land_use: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [1; 32],
            }],
        }),
        ..PreparedEnvironment::default()
    };
    generator.provider_environment = Some(Arc::new(environment));
    generator.provider_compression = Some(Ratio::new(1, 1).expect("ratio"));
    generator.provider = Some(Arc::new(Source(result)));
    generator
}

#[test]
fn raw_history_status_survives_candidate_evaluation_on_dense_and_provider_inputs() {
    let valid = HistoricalCoverage {
        land_percent: 100,
        valid_land_percent: 100,
        ..HistoricalCoverage::default()
    };
    for (crop, coverage, expected) in [
        (100, vec![valid], LandUse::Crop),
        (0, vec![valid], LandUse::Uncleared),
        (
            0,
            vec![HistoricalCoverage {
                nodata_percent: 100,
                ..HistoricalCoverage::default()
            }],
            LandUse::Unobserved,
        ),
        (
            0,
            vec![HistoricalCoverage {
                land_percent: 100,
                ..HistoricalCoverage::default()
            }],
            LandUse::Unobserved,
        ),
        (
            0,
            vec![HistoricalCoverage {
                ocean_percent: 100,
                ..HistoricalCoverage::default()
            }],
            LandUse::Nonland,
        ),
        (0, Vec::new(), LandUse::Uncleared),
    ] {
        let page = page(crop, coverage);
        page.validate().expect("fixture");
        for generator in [
            dense(page.clone()),
            provider(Ok(Arc::new(EnvironmentPage::HistoricalLandUse(
                page.clone(),
            )))),
        ] {
            let sample = generator
                .evaluate_landscape_with_cancel(
                    TileCoord::new(50, 50),
                    policy(),
                    &|_| Reservations::default(),
                    &|| false,
                )
                .expect("sources")
                .expect("bounded");
            assert_eq!(sample.historical_land_use, expected);
            assert_eq!(
                sample.historical_observation.expect("observation").coverage,
                page.coverage.first().copied()
            );
            if matches!(expected, LandUse::Crop | LandUse::Nonland) {
                assert!(!sample.density.tree);
                assert_eq!(sample.density.forest_floor_per_thousand, 0);
            }
        }
    }
}

#[test]
fn candidate_queries_do_not_modify_published_tile_or_resource_results() {
    let generator = dense(page(50, Vec::new()));
    let before = generator.chunk(1, 1).expect("published chunk");
    let mut trees = 0;
    for y in 0..128 {
        for x in 0..128 {
            let sample = generator
                .evaluate_landscape_with_cancel(
                    TileCoord::new(x, y),
                    policy(),
                    &|position| Reservations {
                        route: position.x % 32 < 8,
                        ..Reservations::default()
                    },
                    &|| false,
                )
                .expect("source")
                .expect("bounded");
            if x % 32 < 8 || sample.historical_land_use == LandUse::Crop {
                assert!(!sample.density.tree);
                assert_eq!(sample.density.forest_floor_per_thousand, 0);
            } else {
                trees += usize::from(sample.density.tree);
            }
        }
    }
    assert!(trees > 0, "some unreserved forest must remain");
    assert_eq!(generator.chunk(1, 1).expect("published chunk"), before);
}

#[test]
fn bounded_queries_forward_source_failures_and_cancellation() {
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Cancelled,
    ] {
        let generator = provider(Err(error));
        assert_eq!(
            generator.evaluate_landscape_with_cancel(
                TileCoord::new(50, 50),
                policy(),
                &|_| Reservations::default(),
                &|| false
            ),
            Err(error)
        );
    }
    let generator = dense(page(0, Vec::new()));
    assert_eq!(
        generator.evaluate_landscape_with_cancel(
            TileCoord::new(50, 50),
            policy(),
            &|_| Reservations::default(),
            &|| true
        ),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(
        generator.evaluate_landscape_with_cancel(
            TileCoord::new(-1, 50),
            policy(),
            &|_| Reservations::default(),
            &|| false
        ),
        Ok(None)
    );
    assert_eq!(
        generator.evaluate_landscape_with_cancel(
            TileCoord::new(50, 256),
            policy(),
            &|_| Reservations::default(),
            &|| false
        ),
        Ok(None)
    );
    assert_eq!(
        generator.evaluate_landscape_with_cancel(
            TileCoord::new(50, 50),
            LandscapePolicy {
                support_per_thousand: 1001,
                ..policy()
            },
            &|_| Reservations::default(),
            &|| false
        ),
        Err(EnvironmentPageError::Invalid)
    );
}
