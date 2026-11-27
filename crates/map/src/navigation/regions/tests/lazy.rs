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
fn walkers_and_contacts_in_the_same_regions_reuse_a_directed_proof() {
    let mut grid = walled_base();
    grid.open_gates.insert(7);
    let mut regions = Regions::new(120, 110, LandBucket::Small);
    let (from, to) = (TileCoord::new(45, 80), TileCoord::new(45, 45));
    assert!(regions.reachable(&grid, 2, from, to));
    let searched = regions.stats().searched;
    assert!(searched > 0, "the ray crosses the wall, beside the gate");
    let contacts = (42..47).map(|x| TileCoord::new(x, 45)).collect::<Vec<_>>();
    for x in 42..47 {
        let start = TileCoord::new(x, 80);
        assert_eq!(
            regions.reachable_each(&grid, 2, start, &contacts),
            vec![true; 5]
        );
    }
    assert_eq!(regions.stats().searched, searched);

    grid.open_gates.remove(&7);
    regions.invalidate(TileCoord::new(48, 59), TileCoord::new(52, 60));
    assert!(!regions.reachable(&grid, 2, from, to));
    assert!(regions.reachable(&grid, 1, from, to));
    let searched = regions.stats().searched;
    assert_eq!(
        regions.reachable_each(&grid, 2, from, &contacts),
        vec![false; 5]
    );
    assert_eq!(regions.stats().searched, searched);

    grid.open_gates.insert(7);
    regions.invalidate(TileCoord::new(48, 59), TileCoord::new(52, 60));
    assert!(regions.reachable(&grid, 2, from, to));
}

#[test]
fn a_cached_proof_can_cross_a_one_way_edge_only_in_its_direction() {
    let mut grid = Grid::new(64, 64);
    grid.set(TileCoord::new(20, 10), RegionTile::Blocked);
    for y in 0..64 {
        for dy in -1..=1 {
            grid.walls
                .insert((TileCoord::new(32, y), TileCoord::new(31, y + dy)));
        }
    }
    let mut regions = Regions::new(64, 64, LandBucket::Small);
    let (left, right) = (TileCoord::new(10, 10), TileCoord::new(45, 10));
    for _ in 0..2 {
        assert!(regions.reachable(&grid, 0, left, right));
        assert!(!regions.reachable(&grid, 0, right, left));
    }
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

#[test]
fn an_unreachable_landmass_is_explored_once_per_frontier() {
    // A wall splits the map: the target lies deep in the far half, so every
    // far chunk nearer to it than the wall is tried before the near side.
    let mut grid = Grid::new(640, 64);
    for y in 0..64 {
        grid.set(TileCoord::new(320, y), RegionTile::Blocked);
    }
    let mut regions = Regions::new(640, 64, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(630, 10));
    assert_eq!(
        regions.frontier(&grid, 0, from, to),
        Some(TileCoord::new(319, 10))
    );
    // Each half is 20 chunk nodes: one search per half, not one per chunk.
    let searched = regions.stats().searched;
    assert!(
        searched <= 100,
        "the far half was re-explored: {searched} nodes"
    );
}

struct Counted<'a> {
    grid: &'a Grid,
    chunks: Cell<u32>,
}
impl RegionGround for Counted<'_> {
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
fn a_known_reach_answers_a_far_frontier_without_reading_the_far_side() {
    // The start is walled into a 64-tile corner box of a 640-tile map; the
    // target is in the far corner, 400 chunks of unreachable land away.
    let mut grid = Grid::new(640, 640);
    for i in 0..=64 {
        grid.set(TileCoord::new(64, i), RegionTile::Blocked);
        grid.set(TileCoord::new(i, 64), RegionTile::Blocked);
    }
    let ground = Counted {
        grid: &grid,
        chunks: Cell::new(0),
    };
    let mut regions = Regions::new(640, 640, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(630, 630));
    assert_eq!(
        regions.frontier(&ground, 0, from, to),
        Some(TileCoord::new(63, 63))
    );
    let read = ground.chunks.get();
    assert!(
        read <= 40,
        "the frontier read {read} chunks of the far side"
    );
}

#[test]
fn a_known_unreachable_side_is_crossed_without_scanning_its_tiles() {
    // The start's side is the larger: the search's goal side runs dry first,
    // so the whole far side is known unreachable before the ring crosses it.
    let mut grid = Grid::new(640, 320);
    for y in 0..320 {
        grid.set(TileCoord::new(400, y), RegionTile::Blocked);
    }
    let mut regions = Regions::new(640, 320, LandBucket::Small);
    let (from, to) = (TileCoord::new(10, 10), TileCoord::new(630, 300));
    assert_eq!(
        regions.frontier(&grid, 0, from, to),
        Some(TileCoord::new(399, 300))
    );
    let cells = regions.stats().frontier_cells;
    assert!(
        cells <= 16 * 1024,
        "the far side's tiles were scanned: {cells}"
    );
}
