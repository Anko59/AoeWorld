use super::*;
use crate::{
    EnvironmentPageProvider, HistoricalCoverage, HistoricalLandUsePage, land_use::HistoricalLandUse,
};
use std::{
    cell::Cell,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Debug)]
struct Source {
    result: Result<Arc<EnvironmentPage>, EnvironmentPageError>,
    calls: AtomicUsize,
}
impl EnvironmentPageProvider for Source {
    fn page(
        &self,
        key: EnvironmentPageKey,
        _cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        assert_eq!(key.layer, PageLayer::HistoricalLandUse);
        assert_eq!((key.level, key.x, key.y), (0, 0, 0));
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.result.clone()
    }
}

fn generator(
    result: Result<Arc<EnvironmentPage>, EnvironmentPageError>,
    recipe: u16,
) -> (MapChunkGenerator, Arc<Source>) {
    let source = Arc::new(Source {
        result,
        calls: AtomicUsize::new(0),
    });
    let mut generator =
        MapChunkGenerator::new([17; 32], 1, 5).with_elevation_sampling_recipe(recipe);
    generator.provider = Some(source.clone());
    (generator, source)
}

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
fn raw_provider_matches_dense_coverage_and_legacy_filter_for_recipes_three_to_eight() {
    let cases = [
        Vec::new(),
        vec![
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
        ],
    ];
    for coverage in cases {
        let mut page = page(coverage);
        if page.coverage.is_empty() {
            page.crop_percent = vec![3, 7, 13, 19];
            page.grazing_percent = vec![5, 11, 17, 23];
            page.population_pressure_per_square_kilometer = vec![29, 31, 37, 41];
        }
        page.validate().unwrap();
        let dense = HistoricalLandUse::new(2, [((0, 0), page.clone())].into());
        for recipe in 3..=8 {
            let (generator, _) = generator(
                Ok(Arc::new(EnvironmentPage::HistoricalLandUse(page.clone()))),
                recipe,
            );
            for y in 0..5 {
                for x in 0..5 {
                    let tile = TileCoord::new(x, y);
                    let raw = sample_land_use_observation(&generator, 2, tile, &|| false).unwrap();
                    assert_eq!(raw, dense.at_observation(tile, 5));
                    let legacy =
                        super::super::sample_land_use(&generator, 2, tile, &|| false).unwrap();
                    assert_eq!(
                        legacy,
                        dense.at(tile, 5).map(|sample| (
                            sample.crop_percent,
                            sample.grazing_percent,
                            sample.population_pressure_per_square_kilometer,
                        ))
                    );
                }
            }
            assert_eq!(
                sample_land_use_observation(&generator, 2, TileCoord::new(-5, 9), &|| false)
                    .unwrap(),
                dense.at_observation(TileCoord::new(0, 4), 5)
            );
        }
    }
}

#[test]
fn raw_provider_and_wrapper_preserve_errors_and_cancellation_order() {
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Unavailable,
        EnvironmentPageError::Cancelled,
        EnvironmentPageError::Invalid,
    ] {
        let (generator, source) = generator(Err(error), 8);
        let checks = Cell::new(0);
        let cancelled = || {
            checks.set(checks.get() + 1);
            false
        };
        assert_eq!(
            sample_land_use_observation(&generator, 2, TileCoord::new(0, 0), &cancelled),
            Err(error)
        );
        assert_eq!(checks.get(), 1);
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            super::super::sample_land_use(&generator, 2, TileCoord::new(0, 0), &cancelled),
            Err(error)
        );
        assert_eq!(checks.get(), 2);
        assert_eq!(source.calls.load(Ordering::Relaxed), 2);
        assert_eq!(
            sample_land_use_observation(&generator, 2, TileCoord::new(0, 0), &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 2);
        let checks = Cell::new(0);
        assert_eq!(
            sample_land_use_observation(&generator, 0, TileCoord::new(0, 0), &|| {
                checks.set(checks.get() + 1);
                true
            }),
            Err(EnvironmentPageError::Invalid)
        );
        assert_eq!(checks.get(), 0);
        assert_eq!(source.calls.load(Ordering::Relaxed), 2);
    }
}

#[test]
fn raw_provider_preserves_page_key_and_bounds_failures() {
    let mut page = page(Vec::new());
    page.x = 1;
    let (wrong_key, _) = generator(Ok(Arc::new(EnvironmentPage::HistoricalLandUse(page))), 8);
    assert_eq!(
        sample_land_use_observation(&wrong_key, 2, TileCoord::new(0, 0), &|| false),
        Err(EnvironmentPageError::Corrupt)
    );
    let mut page = self::page(Vec::new());
    page.width = 1;
    page.height = 1;
    let (small_page, _) = generator(Ok(Arc::new(EnvironmentPage::HistoricalLandUse(page))), 8);
    assert_eq!(
        sample_land_use_observation(&small_page, 2, TileCoord::new(4, 4), &|| false),
        Err(EnvironmentPageError::Corrupt)
    );
}
