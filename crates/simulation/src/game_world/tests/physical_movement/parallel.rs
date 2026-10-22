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

/// One town centre per player among the movers: their sight crosses the
/// foreign buildings, so the batched building-watch path (`Watch::Gain` and
/// `Watch::Loss`) runs.
const TOWN_CENTRES: [(u16, i32, i32); 3] = [(1, -30, 0), (2, 30, -20), (3, 0, 35)];

fn fixture(buildings: bool) -> GameWorld {
    let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let mut world = GameWorld::from_map(package).expect("map world");
    let centre = world.config.width_tiles / 2;
    if buildings {
        for (player, dx, dy) in TOWN_CENTRES {
            world
                .place_town_center(PlayerId(player), TileCoord::new(centre + dx, centre + dy))
                .expect("town centre");
        }
    }
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
fn run(threads: usize, buildings: bool) -> (Vec<String>, String) {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("pool");
    pool.install(|| {
        let mut world = fixture(buildings);
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
            if buildings {
                // What each player remembers of foreign buildings follows the
                // watch events, so it belongs in the digest.
                for player in 1..=3 {
                    digest.update(
                        format!("{tick}:{player}:{:?}", known_foreign(&world, player)).as_bytes(),
                    );
                }
            }
        }
        if buildings {
            let foreign = (1..=3)
                .map(|p| known_foreign(&world, p).len())
                .sum::<usize>();
            assert!(foreign > 0, "movers must have seen foreign buildings");
        }
        (hashes, digest.finalize().to_hex().to_string())
    })
}

fn known_foreign(world: &GameWorld, player: u16) -> Vec<EntityId> {
    world
        .known_buildings(PlayerId(player))
        .iter()
        .filter(|building| building.player != PlayerId(player))
        .map(|building| building.id)
        .collect()
}

#[test]
fn thread_count_never_changes_the_replay() {
    let one = run(1, false);
    assert_eq!(run(2, false), one);
    assert_eq!(run(6, false), one);
}

#[test]
fn thread_count_never_changes_the_replay_with_buildings_in_sight() {
    let one = run(1, true);
    assert_eq!(run(2, true), one);
    assert_eq!(run(6, true), one);
}

#[test]
fn parallel_movement_matches_the_pinned_sequential_replay() {
    let (hashes, digest) = run(3, false);
    let last = hashes.last().cloned().unwrap_or_default();
    assert_eq!(
        (last.as_str(), digest.as_str()),
        (PINNED_FINAL_HASH, PINNED_CHANGE_DIGEST)
    );
}

const PINNED_FINAL_HASH: &str = "4f7bbc587a1cdfcabcb2148a70e23d66981997142ee880a68d1e55185570c338";
const PINNED_CHANGE_DIGEST: &str =
    "3eb0deb65c0c6dc4e1d974c18a7cbeb8809133c8760741b3baeef76445073f52";

#[test]
fn parallel_movement_with_buildings_matches_the_pinned_sequential_replay() {
    let (hashes, digest) = run(3, true);
    let last = hashes.last().cloned().unwrap_or_default();
    assert_eq!(
        (last.as_str(), digest.as_str()),
        (PINNED_BUILDINGS_HASH, PINNED_BUILDINGS_DIGEST)
    );
}

// Produced with the proposal phase disabled (every mover on the sequential
// path), then checked against the parallel engine.
const PINNED_BUILDINGS_HASH: &str =
    "8f941cf20d2d3a8e3fca808ceb138238d035e5344231642eaca52caf428d95bd";
const PINNED_BUILDINGS_DIGEST: &str =
    "c03f67524dde25d46acc9558af0c11e7310bacaa26e1f44049288fbcd7d9c909";
