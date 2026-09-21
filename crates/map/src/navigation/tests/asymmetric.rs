use super::*;
use std::collections::{BTreeMap, VecDeque};

fn asymmetric_fixture_neighbors(tile: TileCoord) -> Vec<(TileCoord, u32)> {
    match tile {
        TileCoord { x: 1, y: 2 } => vec![(TileCoord::new(2, 2), ORTHOGONAL_COST)],
        TileCoord { x: 2, y: 2 } => vec![(TileCoord::new(3, 3), DIAGONAL_COST)],
        TileCoord { x: 3, y: 3 } => vec![(TileCoord::new(4, 3), ORTHOGONAL_COST)],
        TileCoord { x: 4, y: 3 } => vec![(TileCoord::new(5, 4), DIAGONAL_COST)],
        _ => Vec::new(),
    }
}

fn reference_fixture_path(origin: TileCoord, destination: TileCoord) -> Vec<TileCoord> {
    let mut frontier = VecDeque::from([origin]);
    let mut parents = BTreeMap::new();
    while let Some(tile) = frontier.pop_front() {
        if tile == destination {
            break;
        }
        for (next, _) in asymmetric_fixture_neighbors(tile) {
            if next != origin && !parents.contains_key(&next) {
                parents.insert(next, tile);
                frontier.push_back(next);
            }
        }
    }
    let mut path = vec![destination];
    let mut current = destination;
    while current != origin {
        current = *parents.get(&current).expect("fixture has a reference path");
        path.push(current);
    }
    path.reverse();
    path
}

#[test]
fn asymmetric_fixture_preserves_open_node_coordinates() {
    let origin = TileCoord::new(1, 2);
    let destination = TileCoord::new(5, 4);
    let expected = reference_fixture_path(origin, destination);
    let passable = |tile| expected.contains(&tile);
    let actual = search_path(
        origin,
        destination,
        32,
        passable,
        asymmetric_fixture_neighbors,
    );
    assert_eq!(
        actual,
        MovementOutcome::Path(Path {
            tiles: expected,
            cost: u64::from(ORTHOGONAL_COST)
                + u64::from(DIAGONAL_COST)
                + u64::from(ORTHOGONAL_COST)
                + u64::from(DIAGONAL_COST),
        })
    );
}
