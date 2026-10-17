//! A river is drawn by importance: the width follows discharge, from a narrow
//! stream at the smallest kept discharge to a wide river, on every map size.
use super::super::channels::segments;
use super::cutoff::river_features;
use super::*;

const BOUNDS: [f64; 4] = [-15_000.0, -15_000.0, 15_000.0, 15_000.0];
const DISCHARGES: [f64; 6] = [0.1, 1.0, 10.0, 100.0, 1_000.0, 10_000.0];

/// Width in tiles of one reach per discharge, in `DISCHARGES` order.
fn widths(tiles: u64, discharges: &[f64]) -> Vec<f64> {
    let directory = TestDirectory::new();
    let reaches = discharges
        .iter()
        .enumerate()
        .map(|(i, d)| (i as i64 + 1, *d, -2_000.0 + 600.0 * i as f64))
        .collect::<Vec<_>>();
    let features = river_features(&directory, &reaches);
    let kept = segments(&features, 30_000.0, tiles, BOUNDS).expect("segments");
    let mut by_northing = kept
        .iter()
        .map(|s| (s.start[1], f64::from(s.width_units) / 256.0))
        .collect::<Vec<_>>();
    // Segment y grows southward in page units, so the first reach is last.
    by_northing.sort_by_key(|(y, _)| std::cmp::Reverse(*y));
    by_northing.dedup_by_key(|(y, _)| *y);
    by_northing.into_iter().map(|(_, w)| w).collect()
}

#[test]
fn the_smallest_kept_stream_is_narrower_than_a_large_river() {
    let width = widths(500, &DISCHARGES);
    assert_eq!(width.len(), DISCHARGES.len());
    assert!(width[0] <= 1.5, "0.1 m3/s is a stream: {} tiles", width[0]);
    assert!(width[3] >= 5.0, "100 m3/s is a river: {} tiles", width[3]);
    assert!(width[0] * 4.0 <= width[3], "{width:?}");
}

#[test]
fn width_rises_with_every_discharge_decade_until_the_playability_ceiling() {
    let width = widths(500, &DISCHARGES);
    for pair in width.windows(2) {
        assert!(
            pair[1] > pair[0] || pair[1] == 10.0,
            "width must rise with discharge: {width:?}"
        );
    }
    assert!(width.iter().all(|w| (1.0..=10.0).contains(w)), "{width:?}");
}

#[test]
fn width_follows_discharge_not_the_metres_per_tile_of_the_map() {
    let discharges = [0.1, 100.0, 3_000.0];
    assert_eq!(widths(500, &discharges), widths(50, &discharges));
}
