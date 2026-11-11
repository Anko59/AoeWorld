//! The phase-3 cost gate: a batched flush under churn (trees felled,
//! buildings placed and razed every tick) stays within 2 ms.
use super::*;
use std::time::Instant;

const SIDE: i32 = 2_048;

fn forest_map(rng: &mut Lcg) -> Grid {
    let mut grid = Grid::new(SIDE, SIDE);
    // Clustered forests and lakes, plus scattered single trees.
    for _ in 0..1_500 {
        let (x, y) = (
            (rng.next() % SIDE as u32) as i32,
            (rng.next() % SIDE as u32) as i32,
        );
        let (w, h) = (4 + (rng.next() % 40) as i32, 4 + (rng.next() % 40) as i32);
        for ty in y..(y + h).min(SIDE) {
            for tx in x..(x + w).min(SIDE) {
                if rng.next() % 100 < 80 {
                    grid.set(TileCoord::new(tx, ty), RegionTile::Blocked);
                }
            }
        }
    }
    for _ in 0..200_000 {
        let tile = TileCoord::new(
            (rng.next() % SIDE as u32) as i32,
            (rng.next() % SIDE as u32) as i32,
        );
        grid.set(tile, RegionTile::Blocked);
    }
    grid
}

#[test]
fn churn_flush_stays_within_two_milliseconds() {
    let mut rng = Lcg(2026);
    let mut grid = forest_map(&mut rng);
    let mut regions = Regions::new(SIDE, SIDE, LandBucket::Small);
    let started = Instant::now();
    regions.flush(&grid);
    let build = started.elapsed();
    let mut flushes = Vec::new();
    let mut unions = Vec::new();
    for tick in 0..300 {
        // Four trees felled, and every third tick a 3 x 3 house placed or razed.
        for _ in 0..4 {
            let tile = TileCoord::new(
                (rng.next() % SIDE as u32) as i32,
                (rng.next() % SIDE as u32) as i32,
            );
            grid.set(tile, RegionTile::Open);
            regions.invalidate(tile, TileCoord::new(tile.x + 1, tile.y + 1));
        }
        if tick % 3 == 0 {
            let at = TileCoord::new(
                (rng.next() % (SIDE as u32 - 3)) as i32,
                (rng.next() % (SIDE as u32 - 3)) as i32,
            );
            let class = if tick % 2 == 0 {
                RegionTile::Blocked
            } else {
                RegionTile::Open
            };
            for y in at.y..at.y + 3 {
                for x in at.x..at.x + 3 {
                    grid.set(TileCoord::new(x, y), class);
                }
            }
            regions.invalidate(at, TileCoord::new(at.x + 3, at.y + 3));
        }
        let started = Instant::now();
        regions.flush(&grid);
        flushes.push(started.elapsed());
        // Isolate union-find work from tile classification and labelling.
        // Rebuild four spatially separate super-chunks, then the global level.
        let supers: BTreeSet<_> = (0..4)
            .map(|_| rng.next() % regions.supers.len() as u32)
            .collect();
        let started = Instant::now();
        for at in supers {
            regions.rebuild_super(at);
        }
        regions.rebuild_global();
        unions.push(started.elapsed());
    }
    flushes.sort_unstable();
    unions.sort_unstable();
    let median = flushes[flushes.len() / 2];
    let p99 = flushes[flushes.len() * 99 / 100];
    let stats = regions.stats();
    let report = format!(
        "regions churn {SIDE}x{SIDE}: build {build:?}, flush median {median:?}, p99 {p99:?}, \
         max {:?}; union-find median {:?}, p99 {:?}, max {:?}; {stats:?}\n",
        flushes[flushes.len() - 1],
        unions[unions.len() / 2],
        unions[unions.len() * 99 / 100],
        unions[unions.len() - 1],
    );
    print!("{report}");
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/regions");
    std::fs::create_dir_all(&evidence).unwrap();
    std::fs::write(evidence.join("churn.txt"), report).unwrap();
    assert!(unions[unions.len() / 2].as_micros() <= 2_000);
    assert!(median.as_micros() <= 2_000, "median flush {median:?}");
    // Spot-check the incremental answer against a fresh build.
    let mut fresh = Regions::new(SIDE, SIDE, LandBucket::Small);
    for _ in 0..200 {
        let a = TileCoord::new(
            (rng.next() % SIDE as u32) as i32,
            (rng.next() % SIDE as u32) as i32,
        );
        assert_eq!(regions.component(&grid, a), fresh.component(&grid, a));
    }
}
