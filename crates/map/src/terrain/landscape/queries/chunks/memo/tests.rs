use super::*;
use crate::{
    ElevationPage, EnvironmentPage, EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid,
    PreparedEnvironment, PyramidLevel, Ratio,
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
    calls: AtomicUsize,
    error: Option<EnvironmentPageError>,
}
impl EnvironmentPageProvider for Source {
    fn page(
        &self,
        key: EnvironmentPageKey,
        _: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = self.error {
            return Err(error);
        }
        assert_eq!(key.layer, crate::PageLayer::Elevation);
        Ok(Arc::new(EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        })))
    }
}
fn generator(error: Option<EnvironmentPageError>) -> (MapChunkGenerator, Arc<Source>) {
    let source = Arc::new(Source {
        calls: AtomicUsize::new(0),
        error,
    });
    let mut generator = MapChunkGenerator::new([17; 32], 1, 128)
        .with_elevation_sampling_recipe(crate::LANDSCAPE_GENERATION_RECIPE_VERSION);
    generator.provider = Some(source.clone());
    generator.provider_compression = Some(Ratio::new(1, 1).unwrap());
    generator.provider_environment = Some(Arc::new(PreparedEnvironment {
        samples_per_axis: 1,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: [1; 32],
            }],
        },
        ..PreparedEnvironment::default()
    }));
    (generator, source)
}
#[test]
fn point_halo_is_lazy_bounded_and_observes_cancellation_even_on_a_hit() {
    let (generator, source) = generator(None);
    let cancelled = Cell::new(false);
    let callback = || cancelled.get();
    let memo = PointMemo::new(&generator, TileCoord::new(10, 10), &callback);
    assert_eq!(
        source.calls.load(Ordering::Relaxed),
        0,
        "must not prefetch all25"
    );
    let first = memo.base(TileCoord::new(10, 10)).unwrap();
    let calls = source.calls.load(Ordering::Relaxed);
    assert!(calls > 0);
    assert_eq!(memo.base(TileCoord::new(10, 10)).unwrap(), first);
    assert_eq!(source.calls.load(Ordering::Relaxed), calls);
    cancelled.set(true);
    assert_eq!(
        memo.base(TileCoord::new(10, 10)),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(source.calls.load(Ordering::Relaxed), calls);
    cancelled.set(false);
    for y in 8..=12 {
        for x in 8..=12 {
            memo.base(TileCoord::new(x, y)).unwrap();
        }
    }
    assert_eq!(
        memo.entries
            .borrow()
            .iter()
            .filter(|entry| entry.base.is_some())
            .count(),
        25
    );
    let after = source.calls.load(Ordering::Relaxed);
    for y in 8..=12 {
        for x in 8..=12 {
            memo.base(TileCoord::new(x, y)).unwrap();
        }
    }
    assert_eq!(source.calls.load(Ordering::Relaxed), after);
}
#[test]
fn source_errors_are_not_cached_and_outside_bounds_never_fetches() {
    for error in [EnvironmentPageError::Missing, EnvironmentPageError::Corrupt] {
        let (generator, source) = generator(Some(error));
        let memo = PointMemo::new(&generator, TileCoord::new(10, 10), &|| false);
        assert_eq!(memo.base(TileCoord::new(10, 10)), Err(error));
        let first = source.calls.load(Ordering::Relaxed);
        assert_eq!(memo.base(TileCoord::new(10, 10)), Err(error));
        assert!(source.calls.load(Ordering::Relaxed) > first);
        let before = source.calls.load(Ordering::Relaxed);
        assert_eq!(memo.base(TileCoord::new(-1, 10)), Ok(None));
        assert_eq!(source.calls.load(Ordering::Relaxed), before);
    }
}
