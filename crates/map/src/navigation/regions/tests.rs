use super::*;
use std::collections::VecDeque;

struct Grid {
    size: (i32, i32),
    cells: Vec<RegionTile>,
    owners: BTreeMap<u32, u32>,
    open_gates: BTreeSet<u32>,
    /// Blocked edges (height walls), stored both ways.
    walls: BTreeSet<(TileCoord, TileCoord)>,
    decks: BTreeSet<(TileCoord, TileCoord)>,
}
impl Grid {
    fn new(width: i32, height: i32) -> Self {
        Self {
            size: (width, height),
            cells: vec![RegionTile::Open; (width * height) as usize],
            owners: BTreeMap::new(),
            open_gates: BTreeSet::new(),
            walls: BTreeSet::new(),
            decks: BTreeSet::new(),
        }
    }
    fn set(&mut self, tile: TileCoord, class: RegionTile) {
        self.cells[(tile.y * self.size.0 + tile.x) as usize] = class;
    }
    fn get(&self, tile: TileCoord) -> RegionTile {
        if tile.x < 0 || tile.y < 0 || tile.x >= self.size.0 || tile.y >= self.size.1 {
            return RegionTile::Blocked;
        }
        self.cells[(tile.y * self.size.0 + tile.x) as usize]
    }
}
impl RegionGround for Grid {
    fn tile(&self, tile: TileCoord) -> RegionTile {
        self.get(tile)
    }
    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        !self.walls.contains(&(from, to))
    }
    fn diagonal_override(&self, from: TileCoord, to: TileCoord) -> Option<bool> {
        self.decks.contains(&(from, to)).then_some(true)
    }
    fn gate_owner(&self, gate: u32) -> u32 {
        self.owners[&gate]
    }
    fn gate_open(&self, gate: u32) -> bool {
        self.open_gates.contains(&gate)
    }
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u32
    }
}

fn random_grid(seed: u64, width: i32, height: i32, percent: u32) -> Grid {
    let mut rng = Lcg(seed);
    let mut grid = Grid::new(width, height);
    for y in 0..height {
        for x in 0..width {
            if rng.next() % 100 < percent {
                grid.set(TileCoord::new(x, y), RegionTile::Blocked);
            }
        }
    }
    for _ in 0..(width * height / 20) {
        let a = TileCoord::new(
            (rng.next() % width as u32) as i32,
            (rng.next() % height as u32) as i32,
        );
        let b = TileCoord::new(a.x + 1, a.y);
        grid.walls.insert((a, b));
        grid.walls.insert((b, a));
    }
    grid
}

/// Reference: a plain 8-neighbour flood with the same step rule, for a
/// player admitted through every gate.
fn reference(grid: &Grid, from: TileCoord) -> BTreeSet<TileCoord> {
    let class = |tile| grid.get(tile);
    let mut seen = BTreeSet::from([from]);
    let mut queue = VecDeque::from([from]);
    while let Some(tile) = queue.pop_front() {
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if grid.get(next) != RegionTile::Blocked
                && !seen.contains(&next)
                && label::step_allowed(grid, &class, tile, next)
            {
                seen.insert(next);
                queue.push_back(next);
            }
        }
    }
    seen
}

fn components(regions: &mut Regions, grid: &Grid) -> Vec<Option<u32>> {
    let mut out = Vec::new();
    for y in 0..grid.size.1 {
        for x in 0..grid.size.0 {
            out.push(regions.component(grid, TileCoord::new(x, y)));
        }
    }
    out
}

#[test]
fn components_match_a_reference_flood() {
    for seed in 1..4 {
        let grid = random_grid(seed, 100, 90, 38);
        let mut regions = Regions::new(100, 90, LandBucket::Small);
        let mut rng = Lcg(seed * 77);
        for _ in 0..40 {
            let a = TileCoord::new((rng.next() % 100) as i32, (rng.next() % 90) as i32);
            let b = TileCoord::new((rng.next() % 100) as i32, (rng.next() % 90) as i32);
            if grid.get(a) == RegionTile::Blocked || grid.get(b) == RegionTile::Blocked {
                assert!(!regions.reachable(&grid, 1, a, b));
                continue;
            }
            let expected = reference(&grid, a).contains(&b);
            assert_eq!(
                regions.reachable(&grid, 1, a, b),
                expected,
                "{a:?} -> {b:?}"
            );
        }
    }
}

#[test]
fn incremental_changes_equal_a_fresh_build_in_any_order() {
    let mut grid = random_grid(9, 130, 75, 30);
    let mut regions = Regions::new(130, 75, LandBucket::Small);
    components(&mut regions, &grid);
    let mut rng = Lcg(5);
    let mut changes = Vec::new();
    for _ in 0..300 {
        let tile = TileCoord::new((rng.next() % 130) as i32, (rng.next() % 75) as i32);
        let class = if grid.get(tile) == RegionTile::Blocked {
            RegionTile::Open
        } else {
            RegionTile::Blocked
        };
        grid.set(tile, class);
        changes.push(tile);
        regions.invalidate(tile, TileCoord::new(tile.x + 1, tile.y + 1));
        if rng.next().is_multiple_of(7) {
            regions.flush(&grid);
        }
    }
    let incremental = components(&mut regions, &grid);
    let mut fresh = Regions::new(130, 75, LandBucket::Small);
    assert_eq!(incremental, components(&mut fresh, &grid));
    // Invalidating in the reverse order gives the same canonical roots.
    let mut reversed = Regions::new(130, 75, LandBucket::Small);
    components(&mut reversed, &grid);
    for tile in changes.iter().rev() {
        reversed.invalidate(*tile, TileCoord::new(tile.x + 1, tile.y + 1));
    }
    assert_eq!(incremental, components(&mut reversed, &grid));
}

/// A 20 x 20 walled square at (40, 40) with a 4-tile gate in its south wall.
fn walled_base() -> Grid {
    let mut grid = Grid::new(120, 110);
    for i in 40..60 {
        for tile in [
            TileCoord::new(i, 40),
            TileCoord::new(i, 59),
            TileCoord::new(40, i),
            TileCoord::new(59, i),
        ] {
            grid.set(tile, RegionTile::Blocked);
        }
    }
    for x in 48..52 {
        grid.set(TileCoord::new(x, 59), RegionTile::Gate(7));
    }
    grid.owners.insert(7, 1);
    grid
}

#[test]
fn a_closed_gate_admits_its_owner_only_and_others_get_the_frontier() {
    let grid = walled_base();
    let mut regions = Regions::new(120, 110, LandBucket::Small);
    let (outside, inside) = (TileCoord::new(50, 80), TileCoord::new(50, 45));
    assert!(regions.reachable(&grid, 1, outside, inside));
    assert!(!regions.reachable(&grid, 2, outside, inside));
    assert_eq!(regions.frontier(&grid, 1, outside, inside), Some(inside));
    // The enemy's nearest reachable tile is just outside the north wall.
    assert_eq!(
        regions.frontier(&grid, 2, TileCoord::new(50, 10), TileCoord::new(50, 49)),
        Some(TileCoord::new(50, 39))
    );
    // (45, 45) is as near the north as the west wall: the side nearer the
    // army wins.
    let corner = TileCoord::new(45, 45);
    assert_eq!(
        regions.frontier(&grid, 2, TileCoord::new(50, 10), corner),
        Some(TileCoord::new(45, 39))
    );
    assert_eq!(
        regions.frontier(&grid, 2, outside, corner),
        Some(TileCoord::new(39, 45))
    );
    // An enemy that slipped into the open gateway can leave on either side.
    let gateway = TileCoord::new(49, 59);
    assert!(regions.reachable(&grid, 2, gateway, inside));
    assert!(regions.reachable(&grid, 2, gateway, outside));
}

#[test]
fn a_thousand_gate_toggles_relabel_nothing() {
    let mut grid = walled_base();
    let mut regions = Regions::new(120, 110, LandBucket::Small);
    regions.component(&grid, TileCoord::new(0, 0));
    let before = regions.stats();
    for n in 0..1_000 {
        let open = n % 2 == 0;
        if open {
            grid.open_gates.insert(7);
        } else {
            grid.open_gates.remove(&7);
        }
        // The simulation reports a toggled gate as changed ground.
        regions.invalidate(TileCoord::new(48, 59), TileCoord::new(52, 60));
        let (outside, inside) = (TileCoord::new(0, 0), TileCoord::new(45, 45));
        assert_eq!(regions.reachable(&grid, 2, outside, inside), open);
        assert_eq!(
            regions.player_component(&grid, 2, outside)
                == regions.player_component(&grid, 2, inside),
            open
        );
        assert_eq!(
            regions.frontier(&grid, 2, outside, inside) == Some(inside),
            open
        );
        assert!(regions.reachable(&grid, 1, outside, inside));
    }
    let after = regions.stats();
    assert_eq!(after.relabels, before.relabels);
    assert_eq!(after.global_rebuilds, before.global_rebuilds);
    assert!(after.unchanged >= 1_000);
}

#[test]
fn an_open_enemy_gate_admits_a_direct_ray_and_a_cached_detour_in_every_bucket() {
    let mut grid = walled_base();
    grid.open_gates.insert(7);
    let (outside, inside) = (TileCoord::new(50, 80), TileCoord::new(50, 45));
    for bucket in LandBucket::ALL {
        let mut regions = Regions::new(120, 110, bucket);
        assert!(regions.reachable(&grid, 2, outside, inside));
        assert_eq!(
            regions.stats().builds,
            0,
            "direct ray through the open gate"
        );
        assert_eq!(regions.frontier(&grid, 2, outside, inside), Some(inside));
        // From the north the path must detour to the south gate.
        let north = TileCoord::new(50, 10);
        assert!(regions.reachable(&grid, 2, north, inside));
        assert_eq!(regions.stats().builds, 1);
        assert_eq!(
            regions.player_component(&grid, 2, north),
            regions.player_component(&grid, 2, inside)
        );
        assert_eq!(regions.frontier(&grid, 2, north, inside), Some(inside));
    }
}

#[test]
fn closing_a_wall_splits_and_reopening_joins() {
    let mut grid = Grid::new(64, 64);
    let mut regions = Regions::new(64, 64, LandBucket::Small);
    let (a, b) = (TileCoord::new(5, 30), TileCoord::new(60, 30));
    assert!(regions.reachable(&grid, 1, a, b));
    for y in 0..64 {
        grid.set(TileCoord::new(33, y), RegionTile::Blocked);
    }
    regions.invalidate(TileCoord::new(33, 0), TileCoord::new(34, 64));
    assert!(!regions.reachable(&grid, 1, a, b));
    assert_eq!(
        regions.frontier(&grid, 1, a, b),
        Some(TileCoord::new(32, 30))
    );
    grid.set(TileCoord::new(33, 10), RegionTile::Open);
    regions.invalidate(TileCoord::new(33, 10), TileCoord::new(34, 11));
    assert!(regions.reachable(&grid, 1, a, b));
}

#[test]
fn blocked_starts_reach_nothing() {
    let mut grid = Grid::new(40, 40);
    grid.set(TileCoord::new(3, 3), RegionTile::Blocked);
    let mut regions = Regions::new(40, 40, LandBucket::Small);
    assert!(!regions.reachable(&grid, 1, TileCoord::new(3, 3), TileCoord::new(9, 9)));
    assert_eq!(
        regions.frontier(&grid, 1, TileCoord::new(3, 3), TileCoord::new(9, 9)),
        None
    );
}

#[path = "tests/churn.rs"]
mod churn;

#[path = "tests/edges.rs"]
mod edges;

#[path = "tests/terrain.rs"]
mod terrain;

#[path = "tests/buckets.rs"]
mod buckets;

#[path = "tests/lazy.rs"]
mod lazy;
