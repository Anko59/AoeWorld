//! Where the units of a staged army stand, and in what order: a fill of the open ground around
//! an anchor, and a seeded shuffle of which unit takes which place.
use super::super::choke::Tile;
use std::collections::{BTreeSet, VecDeque};

/// The first `count` walkable tiles around `anchor`, nearest first (a breadth-first fill through
/// the four neighbours, so every tile is joined to the anchor by ground, and ties go in a fixed
/// order). Fewer if the ground runs out within `limit` tiles of the anchor.
pub(super) fn fill(
    walkable: &dyn Fn(Tile) -> bool,
    anchor: Tile,
    count: usize,
    limit: i32,
) -> Vec<Tile> {
    let mut places = Vec::with_capacity(count);
    let mut seen = BTreeSet::from([anchor]);
    let mut queue = VecDeque::from([anchor]);
    while let Some(tile) = queue.pop_front() {
        if places.len() == count {
            break;
        }
        if !walkable(tile) {
            continue;
        }
        places.push(tile);
        for (dx, dy) in [(0, -1), (-1, 0), (1, 0), (0, 1)] {
            let next = (tile.0 + dx, tile.1 + dy);
            let near = (next.0 - anchor.0).abs().max((next.1 - anchor.1).abs()) <= limit;
            if near && seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    places
}

/// SplitMix64: a small generator with a fixed sequence for a seed, so a seed is a layout.
pub(super) struct Mix(u64);

impl Mix {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// `0..len` in the order a seed gives (Fisher-Yates).
pub(super) fn shuffled(len: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut mix = Mix::new(seed);
    for at in (1..len).rev() {
        let other = (mix.draw() % (at as u64 + 1)) as usize;
        order.swap(at, other);
    }
    order
}
