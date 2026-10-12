use super::*;

#[test]
fn lru_stays_bounded_under_a_large_valid_page_workload() {
    let mut cache = PageCache::default();
    // This exercises the cache capacity with valid page coordinates.  The
    // large, real-coordinate pyramid eviction fixture lives in
    // `tests::residency`; this unit test only checks the cache itself.
    for index in 0..16_384_u16 {
        let x = index % 256;
        let y = index / 256;
        let key = EnvironmentPageKey {
            layer: PageLayer::Elevation,
            level: 0,
            x,
            y,
        };
        let page = Arc::new(EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x,
            y,
            width: 64,
            height: 64,
            geographic_height_centimeters: vec![i32::from(index); 64 * 64],
        }));
        cache_insert(&mut cache, key, page).expect("bounded cache insertion");
    }
    assert_eq!(cache.pages.len(), MAX_RESIDENT_ENVIRONMENT_PAGES);
    assert_eq!(cache.order.len(), MAX_RESIDENT_ENVIRONMENT_PAGES);
}
