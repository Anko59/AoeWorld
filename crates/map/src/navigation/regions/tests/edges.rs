//! Changes to edge geometry must invalidate connectivity even when tile classes stay fixed.
use super::*;

#[test]
fn an_edge_only_wall_splits_and_reopening_rejoins_regions() {
    let mut grid = Grid::new(65, 40);
    let mut regions = Regions::new(65, 40);
    let (a, b) = (TileCoord::new(5, 20), TileCoord::new(60, 20));
    assert!(regions.reachable(&grid, 1, a, b));
    // Both an intra-chunk cliff and a cliff on a chunk border.
    for x in [17, 32] {
        for y in 0..40 {
            for dy in -1..=1 {
                let left = TileCoord::new(x - 1, y);
                let right = TileCoord::new(x, y + dy);
                grid.walls.insert((left, right));
                grid.walls.insert((right, left));
            }
        }
        regions.invalidate(TileCoord::new(x - 1, 0), TileCoord::new(x + 1, 40));
        assert!(!regions.reachable(&grid, 1, a, b), "wall at {x}");
        assert_eq!(
            components(&mut regions, &grid),
            components(&mut Regions::new(65, 40), &grid)
        );
        grid.walls.clear();
        regions.invalidate(TileCoord::new(x - 1, 0), TileCoord::new(x + 1, 40));
        assert!(regions.reachable(&grid, 1, a, b));
    }
}

#[test]
fn more_than_255_pockets_keep_distinct_u16_labels() {
    let mut grid = Grid::new(32, 32);
    for y in 0..32 {
        for x in 0..32 {
            if x % 2 != 0 || y % 2 != 0 {
                grid.set(TileCoord::new(x, y), RegionTile::Blocked);
            }
        }
    }
    let mut regions = Regions::new(32, 32);
    let roots: BTreeSet<_> = components(&mut regions, &grid)
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(roots.len(), 256);
    assert!(!regions.reachable(&grid, 1, TileCoord::new(0, 0), TileCoord::new(30, 30)));
}

#[test]
fn one_way_drops_never_prove_the_reverse_trip_reachable() {
    for x in [17, 32] {
        let mut grid = Grid::new(65, 40);
        // Eastward crossing is possible, westward crossing is blocked.
        for y in 0..40 {
            for dy in -1..=1 {
                grid.walls
                    .insert((TileCoord::new(x, y), TileCoord::new(x - 1, y + dy)));
            }
        }
        let mut regions = Regions::new(65, 40);
        let (west, east) = (TileCoord::new(5, 20), TileCoord::new(60, 20));
        assert!(regions.reachable(&grid, 1, west, east));
        assert!(!regions.reachable(&grid, 1, east, west));
        assert_eq!(
            regions.frontier(&grid, 1, east, west),
            Some(TileCoord::new(x, 20))
        );
    }
}

#[test]
fn directed_connectivity_and_frontiers_match_a_tile_flood() {
    let mut grid = random_grid(71, 67, 39, 27);
    let mut rng = Lcg(17);
    for _ in 0..500 {
        let a = TileCoord::new((rng.next() % 67) as i32, (rng.next() % 39) as i32);
        grid.walls.insert((a, TileCoord::new(a.x + 1, a.y)));
    }
    let mut regions = Regions::new(67, 39);
    for _ in 0..30 {
        let a = TileCoord::new((rng.next() % 67) as i32, (rng.next() % 39) as i32);
        let b = TileCoord::new((rng.next() % 67) as i32, (rng.next() % 39) as i32);
        if grid.get(a) == RegionTile::Blocked {
            continue;
        }
        let reached = reference(&grid, a);
        assert_eq!(regions.reachable(&grid, 1, a, b), reached.contains(&b));
        let square =
            |p: TileCoord, q: TileCoord| i64::from(p.x - q.x).pow(2) + i64::from(p.y - q.y).pow(2);
        let expected = reached
            .iter()
            .min_by_key(|&&p| (square(p, b), square(p, a), p.y, p.x))
            .copied();
        assert_eq!(regions.frontier(&grid, 1, a, b), expected, "{a:?} -> {b:?}");
    }
}

#[test]
fn changing_a_gate_owner_updates_permission_without_relabelling() {
    let mut grid = walled_base();
    let mut regions = Regions::new(120, 110);
    let (a, b) = (TileCoord::new(50, 80), TileCoord::new(50, 45));
    assert!(regions.reachable(&grid, 1, a, b));
    assert!(!regions.reachable(&grid, 2, a, b));
    let before = regions.stats();
    grid.owners.insert(7, 2);
    regions.invalidate(TileCoord::new(48, 59), TileCoord::new(52, 60));
    assert!(!regions.reachable(&grid, 1, a, b));
    assert!(regions.reachable(&grid, 2, a, b));
    assert_eq!(regions.stats().relabels, before.relabels);
}

#[test]
fn a_diagonal_deck_connects_land_over_blocked_corners() {
    for from in [TileCoord::new(0, 0), TileCoord::new(31, 31)] {
        let to = TileCoord::new(from.x + 1, from.y + 1);
        let mut grid = Grid::new(64, 64);
        grid.cells.fill(RegionTile::Blocked);
        grid.set(from, RegionTile::Open);
        grid.set(to, RegionTile::Open);
        let mut regions = Regions::new(64, 64);
        assert!(!regions.reachable(&grid, 1, from, to));
        grid.decks.extend([(from, to), (to, from)]);
        regions.invalidate(from, TileCoord::new(to.x + 1, to.y + 1));
        assert!(regions.reachable(&grid, 1, from, to));
        assert!(regions.reachable(&grid, 1, to, from));
        grid.decks.clear();
        regions.invalidate(from, TileCoord::new(to.x + 1, to.y + 1));
        assert!(!regions.reachable(&grid, 1, from, to));
    }
}

#[test]
fn a_clear_ray_answers_without_labelling_unrelated_chunks() {
    let grid = Grid::new(130, 90);
    let mut regions = Regions::new(130, 90);
    let (from, to) = (TileCoord::new(2, 3), TileCoord::new(125, 83));
    assert!(regions.reachable(&grid, 1, from, to));
    assert_eq!(regions.frontier(&grid, 1, from, to), Some(to));
    assert_eq!(
        regions.stats().builds,
        0,
        "the explicit path itself proves reachability"
    );
}
