use super::*;
use std::cell::Cell;

/// Candidates of a 12 x 5 window in canonical order, nearest to `(0, 2)`.
fn window() -> Vec<(i64, i32, i32, WorldPosition)> {
    let mut tiles = Vec::new();
    for y in 0..5 {
        for x in 0..12 {
            let at =
                WorldPosition::from_tile_center(TileCoord::new(x + 100, y + 100)).expect("centre");
            tiles.push((i64::from(x * x + (y - 2) * (y - 2)), y + 100, x + 100, at));
        }
    }
    tiles.sort_unstable_by_key(|&(d, y, x, _)| (d, y, x));
    tiles
}

fn adjacent(a: TileCoord, b: TileCoord) -> bool {
    (a.x - b.x).abs() <= 1 && (a.y - b.y).abs() <= 1
}

#[test]
fn an_islet_with_many_nearer_tiles_costs_one_proof_and_still_finds_the_shore() {
    // The target's islet holds the 30 nearest tiles (x < 6); the reachable
    // shore starts at x >= 6. Before, 16 proofs of islet tiles rejected.
    let proofs = Cell::new(0);
    let found = pick_firing_tile(
        window(),
        true,
        |_| Ok(true),
        |a, b| Ok(adjacent(a, b) && (a.x < 106) == (b.x < 106)),
        |tile| {
            proofs.set(proofs.get() + 1);
            tile.x >= 106
        },
    )
    .expect("search")
    .expect("a reachable firing tile exists");
    assert!(found.tile_floor().x >= 106);
    assert_eq!(proofs.get(), 2, "one proof of the islet, one of the shore");
}

#[test]
fn a_pocket_whose_tiles_are_all_unreachable_is_still_rejected_cheaply() {
    let proofs = Cell::new(0);
    let found = pick_firing_tile(
        window(),
        true,
        |_| Ok(true),
        |a, b| Ok(adjacent(a, b)),
        |_| {
            proofs.set(proofs.get() + 1);
            false
        },
    )
    .expect("search");
    assert!(found.is_none());
    assert_eq!(proofs.get(), 1);
}

#[test]
fn proofs_stay_bounded_when_every_tile_is_a_separate_islet() {
    let proofs = Cell::new(0);
    let found = pick_firing_tile(
        window(),
        true,
        |_| Ok(true),
        |_, _| Ok(false),
        |_| {
            proofs.set(proofs.get() + 1);
            false
        },
    )
    .expect("search");
    assert_eq!(proofs.get(), MAX_HIERARCHY_PROOFS);
    assert!(found.is_some(), "past the cap the next tile is walked to");
}
