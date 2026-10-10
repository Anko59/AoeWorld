use super::*;
use crate::{EnvironmentPageProvider, HistoricalCoverage, HistoricalLandUsePage};
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
) -> (MapChunkGenerator, Arc<Source>) {
    let source = Arc::new(Source {
        result,
        calls: AtomicUsize::new(0),
    });
    let mut generator = MapChunkGenerator::new([17; 32], 1, 5);
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
fn raw_provider_preserves_errors_and_cancellation_order() {
    for error in [
        EnvironmentPageError::Missing,
        EnvironmentPageError::Corrupt,
        EnvironmentPageError::Unavailable,
        EnvironmentPageError::Cancelled,
        EnvironmentPageError::Invalid,
    ] {
        let (generator, source) = generator(Err(error));
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
            sample_land_use_observation(&generator, 2, TileCoord::new(0, 0), &|| true),
            Err(EnvironmentPageError::Cancelled)
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
        let checks = Cell::new(0);
        assert_eq!(
            sample_land_use_observation(&generator, 0, TileCoord::new(0, 0), &|| {
                checks.set(checks.get() + 1);
                true
            }),
            Err(EnvironmentPageError::Invalid)
        );
        assert_eq!(checks.get(), 0);
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn raw_provider_preserves_page_key_and_bounds_failures() {
    let mut page = page(Vec::new());
    page.x = 1;
    let (wrong_key, _) = generator(Ok(Arc::new(EnvironmentPage::HistoricalLandUse(page))));
    assert_eq!(
        sample_land_use_observation(&wrong_key, 2, TileCoord::new(0, 0), &|| false),
        Err(EnvironmentPageError::Corrupt)
    );
    let mut page = self::page(Vec::new());
    page.width = 1;
    page.height = 1;
    let (small_page, _) = generator(Ok(Arc::new(EnvironmentPage::HistoricalLandUse(page))));
    assert_eq!(
        sample_land_use_observation(&small_page, 2, TileCoord::new(4, 4), &|| false),
        Err(EnvironmentPageError::Corrupt)
    );
}
