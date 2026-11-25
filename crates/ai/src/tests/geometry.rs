//! Ported from `ai/tests/test_geometry.py`.
use crate::geometry::{centroid, clamp_tile, dist, dist2, near_any, nearest, ring_points};
use crate::view::Tile;

struct Point {
    tile: Tile,
}

/// A fixed pseudo-random sequence in place of Python's `random.Random(3)`.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: i32) -> i32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n as u64) as i32
    }
}

#[test]
fn ring_points_lie_on_the_circle_and_get_denser_with_radius() {
    let small = ring_points((100, 100), 12, 11);
    let large = ring_points((100, 100), 60, 11);
    assert!(large.len() > small.len());
    assert!(small.len() >= 8);
    assert!(
        large
            .iter()
            .all(|p| (dist(*p, (100, 100)) - 60.0).abs() < 1.5)
    );
}

#[test]
fn ring_points_count_is_the_circumference_over_the_spacing_and_at_least_eight() {
    assert_eq!(ring_points((100, 100), 12, 11).len(), 8);
    assert_eq!(ring_points((100, 100), 60, 11).len(), 34);
    assert_eq!(ring_points((100, 100), 60, 11)[0], (160, 100));
}

#[test]
fn nearest_centroid_and_clamp() {
    let points = [
        Point { tile: (10, 10) },
        Point { tile: (3, 3) },
        Point { tile: (20, 1) },
    ];
    let best = nearest(&points, (0, 0), |p| p.tile);
    assert_eq!(best.map(|p| p.tile), Some((3, 3)));
    let none: [Point; 0] = [];
    assert!(nearest(&none, (0, 0), |p| p.tile).is_none());
    assert_eq!(centroid(&[(0, 0), (10, 20)]), Some((5, 10)));
    assert_eq!(centroid(&[]), None);
    assert_eq!(clamp_tile((-5, 400), (300, 300), 3), (3, 296));
}

#[test]
fn nearest_keeps_the_first_of_two_equally_near_items() {
    let points = [Point { tile: (4, 0) }, Point { tile: (0, 4) }];
    assert_eq!(
        nearest(&points, (0, 0), |p| p.tile).map(|p| p.tile),
        Some((4, 0))
    );
}

#[test]
fn centroid_floors_like_python_integer_division() {
    assert_eq!(centroid(&[(-1, -1), (0, 0)]), Some((-1, -1)));
}

#[test]
fn distances_are_euclidean() {
    assert_eq!(dist2((0, 0), (3, 4)), 25);
    assert_eq!(dist((0, 0), (3, 4)), 5.0);
}

#[test]
fn near_any_answers_like_a_scan_of_every_tile() {
    let mut rng = Lcg(3);
    let army: Vec<Tile> = (0..60).map(|_| (rng.below(400), rng.below(400))).collect();
    for radius in [0.0, 1.0, 7.5, 30.0] {
        let near = near_any(&army, radius);
        for _ in 0..500 {
            let tile = (rng.below(480) - 40, rng.below(480) - 40);
            assert_eq!(near(tile), army.iter().any(|u| dist(tile, *u) <= radius));
        }
    }
    assert!(!near_any(&[], 30.0)((5, 5)));
}
