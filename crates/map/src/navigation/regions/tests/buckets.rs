use super::super::bucket::{BucketEdges, LandBucket};
use super::*;

#[test]
fn five_land_radii_select_three_buckets() {
    for (radius, expected) in [
        (205, LandBucket::Small),
        (256, LandBucket::Small),
        (308, LandBucket::Medium),
        (410, LandBucket::Medium),
        (461, LandBucket::Large),
    ] {
        assert_eq!(LandBucket::for_radius(radius), Some(expected));
    }
    assert_eq!(LandBucket::for_radius(462), None);
}

#[test]
fn eight_edge_clearances_fit_in_two_bits_each() {
    assert_eq!(std::mem::size_of::<BucketEdges>(), 2);
    let mut edges = BucketEdges::default();
    let limits = [
        None,
        Some(LandBucket::Small),
        Some(LandBucket::Medium),
        Some(LandBucket::Large),
    ];
    for direction in 0..8 {
        edges.set(direction, limits[direction % 4]);
    }
    for direction in 0..8 {
        assert_eq!(edges.clearance(direction), limits[direction % 4]);
        for bucket in LandBucket::ALL {
            assert_eq!(
                edges.allows(direction, bucket),
                limits[direction % 4].is_some_and(|limit| bucket <= limit)
            );
        }
    }
}

struct Narrow<'a> {
    grid: &'a Grid,
    limit: LandBucket,
}
impl RegionGround for Narrow<'_> {
    fn tile(&self, tile: TileCoord) -> RegionTile {
        self.grid.tile(tile)
    }
    fn edge_clear(&self, from: TileCoord, to: TileCoord) -> bool {
        self.grid.edge_clear(from, to)
    }
    fn edge_bucket(&self, from: TileCoord, to: TileCoord) -> Option<LandBucket> {
        self.edge_clear(from, to)
            .then_some(if (from.x < 32) != (to.x < 32) {
                self.limit
            } else {
                LandBucket::Large
            })
    }
    fn gate_owner(&self, gate: u32) -> u32 {
        self.grid.gate_owner(gate)
    }
}

#[test]
fn bucket_labels_frontiers_and_edge_only_changes_agree() {
    let grid = Grid::new(64, 32);
    let mut ground = Narrow {
        grid: &grid,
        limit: LandBucket::Small,
    };
    let (from, to) = (TileCoord::new(10, 16), TileCoord::new(50, 16));
    for bucket in LandBucket::ALL {
        let mut regions = Regions::new(64, 32, bucket);
        assert_eq!(
            regions.reachable(&ground, 1, from, to),
            bucket == LandBucket::Small
        );
        assert_eq!(
            regions.frontier(&ground, 1, from, to),
            Some(if bucket == LandBucket::Small {
                to
            } else {
                TileCoord::new(31, 16)
            })
        );
        ground.limit = LandBucket::Large;
        regions.invalidate(TileCoord::new(31, 0), TileCoord::new(33, 32));
        assert!(regions.reachable(&ground, 1, from, to));
        assert_eq!(regions.frontier(&ground, 1, from, to), Some(to));
        ground.limit = LandBucket::Small;
    }
}
