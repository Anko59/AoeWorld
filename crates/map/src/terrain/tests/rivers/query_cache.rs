use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug)]
struct Counting {
    inner: Arc<dyn EnvironmentPageProvider>,
    reads: AtomicUsize,
    fail: AtomicBool,
}
impl EnvironmentPageProvider for Counting {
    fn page(
        &self,
        key: crate::EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if key.layer == crate::PageLayer::HydrologyEvidence {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if self.fail.load(Ordering::Relaxed) {
                return Err(EnvironmentPageError::Missing);
            }
        }
        self.inner.page(key, cancelled)
    }
}
fn counted() -> (MapChunkGenerator, Arc<Counting>) {
    let mut generator = crossing();
    let provider = Arc::new(Counting {
        inner: generator.provider.take().unwrap(),
        reads: AtomicUsize::new(0),
        fail: AtomicBool::new(false),
    });
    generator.provider = Some(provider.clone());
    (generator, provider)
}
#[test]
fn repeated_channel_queries_share_verified_page_work_and_observe_cancellation() {
    let (generator, provider) = counted();
    for y in 0..256 {
        assert!(
            generator
                .river_within_with_cancel(TileCoord::new(133, y), 4, &|| false)
                .unwrap()
        );
    }
    assert_eq!(
        provider.reads.load(Ordering::Relaxed),
        1,
        "one immutable page index, not a provider lookup per physical/footprint sample"
    );
    assert_eq!(
        generator.river_within_with_cancel(TileCoord::new(133, 31), 4, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    assert_eq!(provider.reads.load(Ordering::Relaxed), 1);
}
#[test]
fn failed_channel_page_loads_are_retried_and_rebinding_drops_cached_geometry() {
    let (generator, provider) = counted();
    let tile = TileCoord::new(133, 31);
    provider.fail.store(true, Ordering::Relaxed);
    assert_eq!(
        generator.river_within_with_cancel(tile, 4, &|| false),
        Err(EnvironmentPageError::Missing)
    );
    provider.fail.store(false, Ordering::Relaxed);
    assert!(
        generator
            .river_within_with_cancel(tile, 4, &|| false)
            .unwrap()
    );
    assert_eq!(provider.reads.load(Ordering::Relaxed), 2);
    provider.fail.store(true, Ordering::Relaxed);
    let environment = generator
        .provider_environment
        .as_ref()
        .unwrap()
        .as_ref()
        .clone();
    let compression = generator.provider_compression.unwrap();
    let rebound = generator
        .with_page_provider(compression, environment, provider)
        .unwrap();
    assert_eq!(
        rebound.river_within_with_cancel(tile, 4, &|| false),
        Err(EnvironmentPageError::Missing)
    );
}
#[test]
fn channel_footprints_match_exact_lines_for_small_and_page_sized_radii() {
    let generator = crossing();
    let page = generator
        .provider
        .as_ref()
        .unwrap()
        .page(
            crate::EnvironmentPageKey {
                layer: crate::PageLayer::HydrologyEvidence,
                level: 0,
                x: 0,
                y: 0,
            },
            &|| false,
        )
        .unwrap();
    let EnvironmentPage::HydrologyEvidence(page) = page.as_ref() else {
        panic!("hydrology");
    };
    for radius in [0, 1, 3, 23, 257] {
        for y in (0..WIDTH).step_by(11) {
            for x in (0..WIDTH).step_by(7) {
                let point = [i64::from(x) * 256 + 128, i64::from(y) * 256 + 128];
                let expected = page
                    .river_segments
                    .iter()
                    .any(|segment| segment.sample(point, i64::from(radius) * 256).is_some());
                assert_eq!(
                    generator
                        .river_within_with_cancel(TileCoord::new(x, y), radius, &|| false)
                        .unwrap(),
                    expected,
                    "tile ({x},{y}), footprint {radius}"
                );
            }
        }
    }
}

#[test]
fn repeated_physical_water_queries_reuse_exact_tile_results() {
    let (generator, provider) = counted();
    let tile = TileCoord::new(133, 31);
    for _ in 0..64 {
        assert_eq!(
            generator.base_water_with_cancel(tile, &|| false).unwrap(),
            Some(WaterKind::River)
        );
    }
    assert!(
        provider.reads.load(Ordering::Relaxed) <= 2,
        "overlapping minimap halos must not reload the same physical water sample"
    );
    assert_eq!(
        generator.base_water_with_cancel(tile, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
}
