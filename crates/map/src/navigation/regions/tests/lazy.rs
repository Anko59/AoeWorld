use super::*;
use std::cell::Cell;

/// A single tree on a large map must cost local chunks, not map area.
struct LocalGround {
    chunks: Cell<usize>,
    sealed: bool,
}
impl RegionGround for LocalGround {
    fn chunk(&self, _: (i32, i32), _: (i32, i32)) -> Option<RegionChunk> {
        let count = self.chunks.get() + 1;
        self.chunks.set(count);
        assert!(
            count <= if self.sealed { 16 } else { 9 },
            "local detour read {count} chunks of a cold map"
        );
        None
    }
    fn tile(&self, tile: TileCoord) -> RegionTile {
        if tile == TileCoord::new(20, 20)
            || (self.sealed
                && ((40..60).contains(&tile.x) && [40, 59].contains(&tile.y)
                    || (40..60).contains(&tile.y) && [40, 59].contains(&tile.x)))
        {
            RegionTile::Blocked
        } else {
            RegionTile::Open
        }
    }
    fn edge_clear(&self, _: TileCoord, _: TileCoord) -> bool {
        true
    }
    fn gate_owner(&self, _: u32) -> u32 {
        0
    }
}

#[test]
fn a_cold_local_detour_does_not_label_the_whole_map() {
    let ground = LocalGround {
        chunks: Cell::new(0),
        sealed: false,
    };
    let mut regions = Regions::new(16_384, 16_384, LandBucket::Large);
    let (from, to) = (TileCoord::new(10, 20), TileCoord::new(30, 20));
    assert!(regions.reachable(&ground, 0, from, to));
    assert_eq!(regions.frontier(&ground, 0, from, to), Some(to));
    assert!(ground.chunks.get() <= 9);
}

#[test]
fn a_cold_sealed_goal_does_not_flood_its_continent() {
    let ground = LocalGround {
        chunks: Cell::new(0),
        sealed: true,
    };
    let mut regions = Regions::new(16_384, 16_384, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 49), TileCoord::new(50, 49));
    assert!(!regions.reachable(&ground, 0, from, to));
    assert_eq!(
        regions.frontier(&ground, 0, from, to),
        Some(TileCoord::new(50, 39))
    );
    assert!(ground.chunks.get() <= 16);
}

#[test]
fn demand_labels_equal_the_complete_region_graph() {
    let grid = random_grid(14, 96, 80, 35);
    let mut lazy = Regions::new(96, 80, LandBucket::Small);
    let mut complete = Regions::new(96, 80, LandBucket::Small);
    complete.flush(&grid);
    let mut rng = Lcg(312);
    for _ in 0..30 {
        let from = TileCoord::new((rng.next() % 96) as i32, (rng.next() % 80) as i32);
        let to = TileCoord::new((rng.next() % 96) as i32, (rng.next() % 80) as i32);
        assert_eq!(
            lazy.reachable(&grid, 0, from, to),
            complete.reachable(&grid, 0, from, to)
        );
        assert_eq!(
            lazy.frontier(&grid, 0, from, to),
            complete.frontier(&grid, 0, from, to)
        );
    }
}

#[test]
fn demand_labels_keep_gate_toggles_out_of_relabelling() {
    let mut grid = walled_base();
    let mut regions = Regions::new(120, 110, LandBucket::Small);
    let (from, to) = (TileCoord::new(50, 10), TileCoord::new(50, 45));
    assert!(!regions.reachable(&grid, 2, from, to));
    let before = regions.stats();
    for n in 0..1000 {
        if n % 2 == 0 {
            grid.open_gates.insert(7);
        } else {
            grid.open_gates.remove(&7);
        }
        regions.invalidate(TileCoord::new(48, 59), TileCoord::new(52, 60));
        assert_eq!(regions.reachable(&grid, 2, from, to), n % 2 == 0);
    }
    assert_eq!(regions.stats().relabels, before.relabels);
}

#[test]
fn a_foreign_gate_start_can_leave_across_a_chunk_boundary() {
    let mut grid = Grid::new(64, 64);
    grid.cells.fill(RegionTile::Blocked);
    grid.owners.insert(7, 1);
    for y in 30..34 {
        for x in 30..34 {
            grid.set(TileCoord::new(x, y), RegionTile::Gate(7));
        }
    }
    grid.set(TileCoord::new(32, 31), RegionTile::Blocked);
    for tile in [(34, 30), (35, 30), (35, 31)] {
        grid.set(TileCoord::new(tile.0, tile.1), RegionTile::Open);
    }
    let mut lazy = Regions::new(64, 64, LandBucket::Small);
    let mut complete = Regions::new(64, 64, LandBucket::Small);
    complete.flush(&grid);
    let (from, to) = (TileCoord::new(31, 31), TileCoord::new(35, 31));
    assert!(complete.reachable(&grid, 2, from, to));
    assert!(lazy.reachable(&grid, 2, from, to));
}

#[test]
fn border_cells_match_a_full_scan_in_order() {
    let scanned: Vec<usize> = (0..CELLS)
        .filter(|&cell| {
            let (x, y) = (cell as i32 % CHUNK, cell as i32 / CHUNK);
            x == 0 || y == 0 || x == CHUNK - 1 || y == CHUNK - 1
        })
        .collect();
    let direct: Vec<usize> = crate::navigation::regions::lazy::border_cells().collect();
    assert_eq!(direct, scanned);
    assert_eq!(direct.len(), 124);
}
