//! The parallel proposal phase must reproduce the sequential engine bit for
//! bit, for any number of worker threads. Pins were regenerated on the merged
//! playtest tree: fair route scheduling, destination spreading and spacing
//! intentionally change the upstream replay. The original merge pins used a
//! sequential reference. Re-pinned for the review fixes: player-first budget
//! sharing changes route timing, and resumable reverse frontiers/cursors are
//! hashed. The same cases still compare complete 1/2/6-thread replays.
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

const PINNED_FINAL_HASH: &str = "df903c9aeb25056efaa7c2ed4aed3385e7fa11797c6056faacedeeaf31986d6b";
const PINNED_CHANGE_DIGEST: &str =
    "bda7b77ed4d674bfcb27ed3bd96d995b68aff0ab0f5e6d19dd485edcb6a20a0a";

#[test]
fn parallel_movement_with_buildings_matches_the_pinned_sequential_replay() {
    let (hashes, digest) = run(3, true);
    let last = hashes.last().cloned().unwrap_or_default();
    assert_eq!(
        (last.as_str(), digest.as_str()),
        (PINNED_BUILDINGS_HASH, PINNED_BUILDINGS_DIGEST)
    );
}

// Current player-first/resumable-proof replay, including emitted changes.
// The tests above retain complete 1/2/6-thread equality checks.
const PINNED_BUILDINGS_HASH: &str =
    "4687dab12fd12ce97bcfe33c8dff0134c2cc0a557c65fa0f334d2682c23c2f56";
const PINNED_BUILDINGS_DIGEST: &str =
    "fa9ed781a90d816c14618a2a8b9732c622c2a3c75c48142ea1ee5c5707d30fc6";

fn waiting_fixture() -> (GameWorld, EntityId) {
    let config = WorldConfig::new(64, 64, Seed(0)).expect("config");
    let mut world = GameWorld::new(config).expect("world");
    // Uniform ground with a building forces world route navigation.
    world.economy.blocked_tiles.insert(TileCoord::new(50, 50));
    let position = WorldPosition::from_tile_center(TileCoord::new(10, 10)).unwrap();
    let destination = WorldPosition::from_tile_center(TileCoord::new(30, 10)).unwrap();
    let id = world.spawn_unit(PlayerId(0), position).unwrap();
    let index = world.lookup[&id];
    world.units[index].state.moving = true;
    world.units[index].order = Some(MovementOrder {
        origin: position,
        waypoint: position,
        destination,
        target_tile: destination.tile_floor(),
        segment_length: 1,
        travelled: 1,
        speed_carry: 0,
    });
    world.active_movers.push(id);
    world.tick = Tick(17);
    (world, id)
}

#[test]
fn proposal_defers_search_to_the_fair_pass_even_with_budget_and_slots() {
    let (mut world, id) = waiting_fixture();
    assert!(world.planning_budget > 0);
    let proposal = world.propose_steps(&[id], 0).pop().unwrap().unwrap();
    assert!(world.apply_step(proposal, &mut Vec::new(), &mut Vec::new()));
    assert!(world.unit(id).unwrap().planning);
    assert_eq!(world.units[world.lookup[&id]].planning_since, 17);
    assert_eq!(world.active_route_searches, 0);
}

#[test]
fn proposed_wait_records_the_tick_it_started() {
    let (mut world, id) = waiting_fixture();
    world.planning_budget = 0;
    let proposal = world.propose_steps(&[id], 0).pop().unwrap().unwrap();
    assert!(world.apply_step(proposal, &mut Vec::new(), &mut Vec::new()));
    assert_eq!(world.units[world.lookup[&id]].planning_since, 17);
}

#[test]
fn proposal_rechecks_following_after_an_earlier_mover_changes_spacing() {
    let (mut world, follower) = waiting_fixture();
    world.economy.blocked_tiles.clear();
    let position = world.unit(follower).unwrap().position;
    let leader = world
        .spawn_unit(
            PlayerId(0),
            WorldPosition::new(position.x + 760, position.y),
        )
        .unwrap();
    for id in [follower, leader] {
        world
            .issue_task_move(id, WorldPosition::new(position.x + 20_000, position.y))
            .unwrap();
    }
    let index = world.lookup[&follower];
    assert!(world.following_unit(index));
    let proposal = world.propose_steps(&[follower], 0).pop().unwrap().unwrap();
    // Applying the leader first moves it beyond the follower spacing radius.
    let leader_step = world.propose_steps(&[leader], 0).pop().unwrap().unwrap();
    assert!(world.apply_step(leader_step, &mut Vec::new(), &mut Vec::new()));
    assert!(!world.following_unit(index));
    assert!(!world.apply_step(proposal, &mut Vec::new(), &mut Vec::new()));
    assert_eq!(world.unit(follower).unwrap().position, position);
}
