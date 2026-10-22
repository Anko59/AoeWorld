//! The parallel proposal phase must reproduce the sequential engine bit for
//! bit, for any number of worker threads. The two pins were produced by the
//! sequential engine before the proposal phase existed.
use super::*;
use aoe_map::{MapPackage, MapRequest};

const UNITS: usize = 1_800;
const TICKS: usize = 160;

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

fn fixture() -> GameWorld {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let mut world = GameWorld::from_map(package).expect("map world");
    let centre = world.config.width_tiles / 2;
    let mut seed = 0x5eed_u64;
    let mut ids = Vec::new();
    for n in 0..UNITS {
        let at = |seed: &mut u64| {
            let dx = (lcg(seed) % 120) as i32 - 60;
            let dy = (lcg(seed) % 120) as i32 - 60;
            WorldPosition::from_tile_center(TileCoord::new(centre + dx, centre + dy))
        };
        let player = PlayerId(1 + (n % 3) as u16);
        let spawned = at(&mut seed).ok().and_then(|position| {
            if n % 2 == 0 {
                world.spawn_unit(player, position).ok()
            } else {
                world.spawn_villager(player, position).ok()
            }
        });
        ids.extend(spawned);
    }
    for id in ids {
        if let Ok(destination) = WorldPosition::from_tile_center(TileCoord::new(
            centre + (lcg(&mut seed) % 160) as i32 - 80,
            centre + (lcg(&mut seed) % 160) as i32 - 80,
        )) {
            let _ = world.issue_move(id, destination);
        }
    }
    world
}

/// Canonical hashes every 20 ticks plus a digest of every emitted change set.
fn run(threads: usize) -> (Vec<String>, String) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("pool");
    pool.install(|| {
        let mut world = fixture();
        assert!(
            world.active_movers.len() > 2 * 512,
            "enough movers to shard"
        );
        let mut hashes = Vec::new();
        let mut digest = blake3::Hasher::new();
        for tick in 1..=TICKS {
            for unit in world.advance() {
                digest.update(format!("{tick}:{unit:?}").as_bytes());
            }
            if tick % 20 == 0 {
                hashes.push(world.canonical_hash_hex());
            }
        }
        (hashes, digest.finalize().to_hex().to_string())
    })
}

#[test]
fn thread_count_never_changes_the_replay() {
    let one = run(1);
    assert_eq!(run(2), one);
    assert_eq!(run(6), one);
}

#[test]
fn parallel_movement_matches_the_pinned_sequential_replay() {
    let (hashes, digest) = run(3);
    let last = hashes.last().cloned().unwrap_or_default();
    assert_eq!(
        (last.as_str(), digest.as_str()),
        (PINNED_FINAL_HASH, PINNED_CHANGE_DIGEST)
    );
}

const PINNED_FINAL_HASH: &str = "4f7bbc587a1cdfcabcb2148a70e23d66981997142ee880a68d1e55185570c338";
const PINNED_CHANGE_DIGEST: &str =
    "3eb0deb65c0c6dc4e1d974c18a7cbeb8809133c8760741b3baeef76445073f52";
