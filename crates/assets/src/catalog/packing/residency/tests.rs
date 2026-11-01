use super::*;

#[test]
fn native_frame_cache_preserves_pixels_and_aligns_colour_mask_pages() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    let placement = cache
        .reserve(7, [66, 62], true, true)
        .unwrap()
        .placement
        .unwrap();
    assert_eq!((placement.page, placement.x, placement.y), (8, 2, 2));
    assert_eq!((placement.width, placement.height), (66, 62));
    assert_eq!(cache.layer_count(), 10);
    assert_eq!(cache.lookup(7), Some(placement));
    assert_eq!(
        cache.reserve(7, [66, 62], true, true).unwrap().placement,
        Some(placement)
    );
    assert!(cache.reserve(7, [67, 62], true, true).is_err());
    assert_eq!(cache.lookup(7), Some(placement));
}

#[test]
fn a_full_native_cache_evicts_old_frames_and_protects_visible_frames() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    for key in 0..1024 {
        assert!(
            cache
                .reserve(key, [60, 60], false, true)
                .unwrap()
                .placement
                .is_some()
        );
    }
    let first = cache.lookup(0).unwrap();
    let victim = cache.lookup(1).unwrap();
    cache.begin_frame(&[0]);
    let next = cache.reserve(1024, [60, 60], true, true).unwrap();
    assert_eq!(next.evicted, vec![1]);
    assert_eq!(next.placement, Some(victim));
    assert_eq!(cache.lookup(0), Some(first));
    assert_eq!(cache.lookup(1), None);
    assert_eq!(cache.len(), 1024);
}

#[test]
fn cache_pressure_defers_new_cells_without_evicting_current_pixels() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    let full = cache
        .reserve(1, [2044, 2044], true, true)
        .unwrap()
        .placement
        .unwrap();
    let deferred = cache.reserve(2, [1, 1], true, true).unwrap();
    assert_eq!(deferred.placement, None);
    assert!(deferred.evicted.is_empty());
    assert_eq!(cache.lookup(1), Some(full));
    assert_eq!(cache.lookup(2), None);
    for _ in 0..3 {
        cache.begin_frame(&[]);
    }
    let next = cache.reserve(2, [1, 1], true, true).unwrap();
    assert_eq!(next.evicted, vec![1]);
    assert!(next.placement.is_some());
    assert_eq!(cache.lookup(1), None);
}

#[test]
fn visible_cache_cells_survive_fragmentation_and_partial_evictions_are_reported() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    cache.reserve(1, [60, 60], false, true).unwrap();
    cache.reserve(2, [60, 60], false, true).unwrap();
    cache.begin_frame(&[1]);
    let deferred = cache.reserve(3, [2044, 2044], true, true).unwrap();
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
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    for size in [[0, 1], [1, 0], [2045, 1], [1, 2045]] {
        assert!(cache.reserve(1, size, true, true).is_err());
        assert_eq!(cache.len(), 0);
    }
}

#[test]
fn frame_inventory_can_cycle_without_growing_the_resident_working_set() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    for key in 0..4096 {
        cache.begin_frame(&[]);
        let result = cache.reserve(key, [60, 60], true, true).unwrap();
        assert!(result.placement.is_some());
        assert!(cache.len() <= 1024);
        assert_eq!(cache.layer_count(), 10);
    }
    assert_eq!(cache.len(), 1024);
    assert_eq!(cache.lookup(0), None);
    assert!(cache.lookup(4095).is_some());
}

#[test]
fn many_kind_visible_working_set_grows_past_four_pairs_without_evicting_pixels() {
    // 64 native poses, 16 facings and one independently resident current cell.
    // 188px cells need 3x3 upload tiles; the retired four pairs hold only 400.
    let mut cache = FrameCache::new(8, 4, 32).unwrap();
    let keys: Vec<_> = (1..=1024).collect();
    cache.begin_frame(&keys);
    let mut placed = Vec::new();
    for &key in &keys {
        let result = cache.reserve(key, [188; 2], true, true).unwrap();
        assert!(result.evicted.is_empty(), "visible cell evicted for {key}");
        placed.push(
            result
                .placement
                .expect("visible set must grow within its GPU budget"),
        );
    }
    assert!(cache.layer_count() > 16);
    assert!(u64::from(cache.layer_count()) * 2048 * 2048 * 4 <= 512 * 1024 * 1024);
    assert_eq!(cache.len(), 1024);
    cache.begin_frame(&keys);
    for (&key, &placement) in keys.iter().zip(&placed) {
        assert_eq!(cache.lookup(key), Some(placement));
    }
}

#[test]
fn previous_three_frames_remain_pinned_at_the_physical_limit() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    let old = cache
        .reserve(1, [2044; 2], true, true)
        .unwrap()
        .placement
        .unwrap();
    for _ in 0..2 {
        cache.begin_frame(&[]);
        let result = cache.reserve(2, [1; 2], true, true).unwrap();
        assert!(result.evicted.is_empty());
        assert!(result.placement.is_none());
        assert_eq!(cache.lookup(1), Some(old));
    }
    cache.begin_frame(&[]);
    let result = cache.reserve(2, [1; 2], true, true).unwrap();
    assert_eq!(result.evicted, [1]);
    assert!(result.placement.is_some());
}

#[test]
fn atlas_growth_waits_for_renderer_permission_and_prefetch_never_expands_it() {
    let mut cache = FrameCache::new(8, 1, 14).unwrap();
    let old = cache
        .reserve(1, [2044; 2], true, true)
        .unwrap()
        .placement
        .unwrap();
    let blocked = cache.reserve(2, [1; 2], true, false).unwrap();
    assert!(blocked.placement.is_none());
    assert!(blocked.evicted.is_empty());
    assert_eq!(cache.layer_count(), 10);
    let prefetch = cache.reserve(2, [1; 2], false, true).unwrap();
    assert!(prefetch.placement.is_none());
    assert_eq!(cache.layer_count(), 10);
    let admitted = cache.reserve(2, [1; 2], true, true).unwrap();
    assert_eq!(admitted.placement.unwrap().page, 10);
    assert_eq!(cache.layer_count(), 12);
    assert_eq!(cache.lookup(1), Some(old));
}

#[test]
fn sparse_out_of_order_native_keys_keep_lru_ties_and_existing_placements() {
    let mut cache = FrameCache::new(8, 1, 10).unwrap();
    for key in [u64::MAX, 3, 1000, 71] {
        assert!(
            cache
                .reserve(key, [1020; 2], false, false)
                .unwrap()
                .placement
                .is_some()
        );
    }
    let high = cache.lookup(u64::MAX).unwrap();
    cache.begin_frame(&[]);
    assert_eq!(
        cache
            .reserve(u64::MAX, [1020; 2], false, false)
            .unwrap()
            .placement,
        Some(high)
    );
    assert_eq!(
        cache.reserve(5, [1020; 2], false, false).unwrap().evicted,
        [3]
    );
    assert_eq!(
        cache.reserve(6, [1020; 2], false, false).unwrap().evicted,
        [71]
    );
    assert_eq!(cache.lookup(u64::MAX), Some(high));
    assert!(cache.lookup(3).is_none() && cache.lookup(71).is_none());
    assert_eq!(cache.len(), 4);
}
