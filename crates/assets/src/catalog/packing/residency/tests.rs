use super::*;

#[test]
fn native_frame_cache_preserves_pixels_and_aligns_colour_mask_pages() {
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    let placement = cache.reserve(7, [66, 62], true).unwrap().placement.unwrap();
    assert_eq!((placement.page, placement.x, placement.y), (8, 2, 2));
    assert_eq!((placement.width, placement.height), (66, 62));
    assert_eq!(cache.layer_count(), 10);
    assert_eq!(cache.lookup(7), Some(placement));
    assert_eq!(
        cache.reserve(7, [66, 62], true).unwrap().placement,
        Some(placement)
    );
    assert!(cache.reserve(7, [67, 62], true).is_err());
    assert_eq!(cache.lookup(7), Some(placement));
}

#[test]
fn a_full_native_cache_evicts_old_frames_and_protects_visible_frames() {
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    for key in 0..1024 {
        assert!(
            cache
                .reserve(key, [60, 60], false)
                .unwrap()
                .placement
                .is_some()
        );
    }
    let first = cache.lookup(0).unwrap();
    let victim = cache.lookup(1).unwrap();
    cache.begin_frame(&[0]);
    let next = cache.reserve(1024, [60, 60], true).unwrap();
    assert_eq!(next.evicted, vec![1]);
    assert_eq!(next.placement, Some(victim));
    assert_eq!(cache.lookup(0), Some(first));
    assert_eq!(cache.lookup(1), None);
    assert_eq!(cache.len(), 1024);
}

#[test]
fn cache_pressure_defers_new_cells_without_evicting_current_pixels() {
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    let full = cache
        .reserve(1, [2044, 2044], true)
        .unwrap()
        .placement
        .unwrap();
    let deferred = cache.reserve(2, [1, 1], true).unwrap();
    assert_eq!(deferred.placement, None);
    assert!(deferred.evicted.is_empty());
    assert_eq!(cache.lookup(1), Some(full));
    assert_eq!(cache.lookup(2), None);
    cache.begin_frame(&[]);
    let next = cache.reserve(2, [1, 1], true).unwrap();
    assert_eq!(next.evicted, vec![1]);
    assert!(next.placement.is_some());
    assert_eq!(cache.lookup(1), None);
}

#[test]
fn visible_cache_cells_survive_fragmentation_and_partial_evictions_are_reported() {
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    cache.reserve(1, [60, 60], false).unwrap();
    cache.reserve(2, [60, 60], false).unwrap();
    cache.begin_frame(&[1]);
    let deferred = cache.reserve(3, [2044, 2044], true).unwrap();
    assert_eq!(deferred.placement, None);
    assert_eq!(deferred.evicted, vec![2]);
    assert!(cache.lookup(1).is_some());
    assert_eq!(cache.lookup(2), None);
}

#[test]
fn native_frame_cache_rejects_invalid_budgets_and_sizes_before_mutation() {
    for (base, pairs, limit) in [(7, 1, 64), (9, 1, 64), (8, 0, 64), (62, 2, 64), (8, 1, 257)] {
        assert!(FrameCache::new(base, pairs, limit).is_err());
    }
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    for size in [[0, 1], [1, 0], [2045, 1], [1, 2045]] {
        assert!(cache.reserve(1, size, true).is_err());
        assert_eq!(cache.len(), 0);
    }
}

#[test]
fn frame_inventory_can_cycle_without_growing_the_resident_working_set() {
    let mut cache = FrameCache::new(8, 1, 64).unwrap();
    for key in 0..4096 {
        cache.begin_frame(&[]);
        let result = cache.reserve(key, [60, 60], true).unwrap();
        assert!(result.placement.is_some());
        assert!(cache.len() <= 1024);
        assert_eq!(cache.layer_count(), 10);
    }
    assert_eq!(cache.len(), 1024);
    assert_eq!(cache.lookup(0), None);
    assert!(cache.lookup(4095).is_some());
}
