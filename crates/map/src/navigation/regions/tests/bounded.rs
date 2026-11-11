use super::*;
use std::cell::Cell;

struct Observed<'a> {
    grid: &'a Grid,
    chunks: Cell<u32>,
}
impl RegionGround for Observed<'_> {
    fn tile(&self, tile: TileCoord) -> RegionTile {
        self.grid.tile(tile)
    }
    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        self.grid.edge_clear(from, to)
    }
    fn gate_owner(&self, gate: u32) -> u32 {
        self.grid.gate_owner(gate)
    }
    fn chunk(&self, _: (i32, i32), _: (i32, i32)) -> Option<RegionChunk> {
        self.chunks.set(self.chunks.get() + 1);
        None
    }
}

#[test]
fn exhausted_region_lookup_is_unknown_and_independent_of_cache_warming() {
    let mut grid = Grid::new(640, 64);
    for y in 0..64 {
        grid.set(TileCoord::new(320, y), RegionTile::Blocked);
    }
    let ground = Observed {
        grid: &grid,
        chunks: Cell::new(0),
    };
    let mut regions = Regions::new(640, 64, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(630, 10));
    assert_eq!(
        regions.reachable_bounded(&ground, 1, from, to, 0),
        RegionReachability::Unknown
    );
    assert_eq!(ground.chunks.get(), 0);
    assert_eq!(
        regions.reachable_bounded(&ground, 1, from, to, 4),
        RegionReachability::Unknown
    );
    assert!(
        ground.chunks.get() <= 2 + 8 * 4,
        "region lookup exceeded its node bound"
    );
    assert!(!regions.reachable(&ground, 1, from, to));
    assert_eq!(
        regions.reachable_bounded(&ground, 1, from, to, 4),
        RegionReachability::Unknown
    );
    assert!(regions.component(&ground, from).is_some());
    assert_eq!(
        regions.reachable_bounded(&ground, 1, from, to, 4),
        RegionReachability::Unknown
    );
}

#[test]
fn bounded_region_lookup_can_certify_a_detour_or_a_small_sealed_endpoint() {
    let mut grid = Grid::new(96, 64);
    for y in 0..56 {
        grid.set(TileCoord::new(48, y), RegionTile::Blocked);
    }
    let mut regions = Regions::new(96, 64, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(80, 10));
    assert_eq!(
        regions.reachable_bounded(&grid, 1, from, to, 32),
        RegionReachability::Connected
    );
    for tile in [
        TileCoord::new(79, 10),
        TileCoord::new(81, 10),
        TileCoord::new(80, 9),
        TileCoord::new(80, 11),
    ] {
        grid.set(tile, RegionTile::Blocked);
    }
    regions.invalidate(TileCoord::new(78, 8), TileCoord::new(83, 13));
    assert_eq!(
        regions.reachable_bounded(&grid, 1, from, to, 32),
        RegionReachability::Unreachable
    );
}

#[test]
fn clear_witnesses_are_distinct_from_graph_only_connectivity() {
    let grid = Grid::new(96, 64);
    let mut regions = Regions::new(96, 64, LandBucket::Small);
    assert_eq!(
        regions.reachable_bounded(&grid, 1, TileCoord::new(10, 10), TileCoord::new(80, 10), 32),
        RegionReachability::Straight
    );
}

#[test]
fn bounded_queries_preserve_dirty_labels_of_a_materialized_region_graph() {
    let mut grid = Grid::new(96, 64);
    let mut regions = Regions::new(96, 64, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(80, 10));
    assert!(regions.component(&grid, from).is_some());
    for y in 0..64 {
        grid.set(TileCoord::new(48, y), RegionTile::Blocked);
    }
    regions.invalidate(TileCoord::new(48, 0), TileCoord::new(49, 64));
    let _ = regions.reachable_bounded(&grid, 1, from, to, 64);
    assert!(
        !regions.reachable(&grid, 1, from, to),
        "an optional lookup must not consume the full graph's dirty markers"
    );
}
