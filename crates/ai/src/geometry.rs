//! Small tile-geometry helpers shared by the managers (`ai/aoe_ai/geometry.py`).
use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::view::Tile;

pub fn dist2(a: Tile, b: Tile) -> i64 {
    let dx = i64::from(a.0) - i64::from(b.0);
    let dy = i64::from(a.1) - i64::from(b.1);
    dx * dx + dy * dy
}

pub fn dist(a: Tile, b: Tile) -> f64 {
    (dist2(a, b) as f64).sqrt()
}

/// The item nearest to `origin`; the first of equally near ones.
pub fn nearest<T>(items: &[T], origin: Tile, key: impl Fn(&T) -> Tile) -> Option<&T> {
    let mut best: Option<(&T, i64)> = None;
    for item in items {
        let d = dist2(key(item), origin);
        if best.is_none_or(|(_, best_d)| d < best_d) {
            best = Some((item, d));
        }
    }
    best.map(|(item, _)| item)
}

/// The mean tile, rounded down like Python's integer division.
pub fn centroid(tiles: &[Tile]) -> Option<Tile> {
    if tiles.is_empty() {
        return None;
    }
    let n = tiles.len() as i32;
    Some((
        tiles.iter().map(|t| t.0).sum::<i32>().div_euclid(n),
        tiles.iter().map(|t| t.1).sum::<i32>().div_euclid(n),
    ))
}

/// Points on a circle, about `spacing` tiles apart (at least eight).
pub fn ring_points(center: Tile, radius: i32, spacing: i32) -> Vec<Tile> {
    let r = f64::from(radius);
    let count = ((2.0 * PI * r / f64::from(spacing)) as usize).max(8);
    (0..count)
        .map(|i| {
            let angle = 2.0 * PI * i as f64 / count as f64;
            (
                center.0 + (r * angle.cos()).round_ties_even() as i32,
                center.1 + (r * angle.sin()).round_ties_even() as i32,
            )
        })
        .collect()
}

pub fn clamp_tile(tile: Tile, size: (i32, i32), margin: i32) -> Tile {
    (
        tile.0.max(margin).min(size.0 - 1 - margin),
        tile.1.max(margin).min(size.1 - 1 - margin),
    )
}

/// A test of whether a tile lies within `radius` of any of `tiles`, the same
/// answer as scanning every tile, from a grid of `radius`-sized cells: only
/// the 3x3 cells around a tile can hold one.
pub fn near_any(tiles: &[Tile], radius: f64) -> impl Fn(Tile) -> bool + use<> {
    let size = (radius.ceil() as i32).max(1);
    let mut cells: BTreeMap<Tile, Vec<Tile>> = BTreeMap::new();
    for &u in tiles {
        cells
            .entry((u.0.div_euclid(size), u.1.div_euclid(size)))
            .or_default()
            .push(u);
    }
    move |tile: Tile| {
        let (cx, cy) = (tile.0.div_euclid(size), tile.1.div_euclid(size));
        (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                cells
                    .get(&(cx + dx, cy + dy))
                    .is_some_and(|us| us.iter().any(|&u| dist(tile, u) <= radius))
            })
        })
    }
}
