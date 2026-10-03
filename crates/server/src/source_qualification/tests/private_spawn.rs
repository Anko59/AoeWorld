//! Opt-in private source evidence, never a substitute for the ordinary offline gate.
use super::super::*;
use aoe_core::{PlayerId, TileRect, WorldPosition};
use aoe_protocol::GameplayServerMessage;
use aoe_simulation::Terrain;
use std::collections::{BTreeMap, VecDeque};

const EXIT_DISTANCE: u32 = 64;
const MAX_EXIT_VISITS: usize = 32_768;
const MAX_EXIT_TICKS: u64 = 20_000;

#[tokio::test]
#[ignore = "requires explicit verified private France recipe-8 package and hash"]
async fn private_source_spawn_cavalry_exits_certified_glade() {
    let directory = PathBuf::from(
        std::env::var("AOE_PRIVATE_SPAWN_PACKAGE_DIRECTORY")
            .expect("AOE_PRIVATE_SPAWN_PACKAGE_DIRECTORY is required; no synthetic fallback"),
    );
    let hash = std::env::var("AOE_PRIVATE_SPAWN_CONTENT_HASH")
        .expect("AOE_PRIVATE_SPAWN_CONTENT_HASH is required; no synthetic fallback");
    let packages = load_map_packages(Some(&directory)).expect("verify stored source package/pages");
    let package = packages
        .get(&hash)
        .expect("exact requested content hash must exist")
        .clone();
    assert_eq!(package.content_hash_hex(), hash);
    assert_eq!(package.generation_recipe_version, 8);
    assert_eq!(package.estimate.tiles_per_side, 70_000);
    assert_eq!(package.request.compression.numerator, 10);
    assert_eq!(package.request.compression.denominator, 1);
    assert!(!package.source_locks.is_empty());
    assert!(package.environment.samples_per_axis > 0);
    assert_eq!(package.provenance.elevation, LayerProvenance::SourceDerived);
    let provider =
        PageResidency::open(&directory, &package, &|| false).expect("verified source pages");
    let replay_provider =
        PageResidency::open(&directory, &package, &|| false).expect("independent replay pages");
    let mut world = GameWorld::from_page_provider(package.clone(), provider.clone()).unwrap();
    let mut replay =
        GameWorld::from_page_provider(package.clone(), replay_provider.clone()).unwrap();
    let config = world.config();
    assert_eq!(config.tick_hz, 20);
    assert_eq!(config.move_speed_subunits_per_tick, 384);
    assert_eq!(config.move_speed_subunits_per_tick_denominator, 5);
    let start = match world
        .terrain()
        .search_start_for_recipe(config, 8, 64, || false)
        .unwrap()
    {
        StartSearchResult::Found(tile) => tile,
        result => panic!("ordinary certified recipe-8 start failed: {result:?}"),
    };
    assert_eq!(
        replay
            .terrain()
            .search_start_for_recipe(config, 8, 64, || false)
            .unwrap(),
        StartSearchResult::Found(start)
    );
    // Exercise the production ordinary factory as well, without live-server or overlay writes.
    let factory =
        GameplayService::from_stored_map(package.clone(), Some(provider.clone()), None, &|| false)
            .unwrap()
            .expect("ordinary source activation must succeed");
    let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
    let (session, welcome) = factory.register(None, sender).await;
    let GameplayServerMessage::Welcome {
        primary_unit_id, ..
    } = welcome
    else {
        panic!("factory welcome")
    };
    factory
        .subscribe(
            session,
            1,
            TileRect::from_xywh(start.x - 128, start.y - 128, 256, 256),
        )
        .await;
    let GameplayServerMessage::Snapshot { units, .. } =
        receiver.try_recv().expect("factory snapshot")
    else {
        panic!("factory must snapshot ordinary spawn")
    };
    let position = WorldPosition::from_tile_center(start).unwrap();
    assert_eq!(
        units
            .iter()
            .find(|unit| unit.id == primary_unit_id)
            .unwrap()
            .position,
        position
    );
    let (goal, connected_route, visits) = find_exit(world.terrain(), config, start);
    let destination = WorldPosition::from_tile_center(goal).unwrap();
    let unit = world.spawn_unit(PlayerId(0), position).unwrap();
    let replay_unit = replay.spawn_unit(PlayerId(0), position).unwrap();
    let order = world.issue_move(unit, destination);
    println!(
        "source_spawn_exit hash={hash} ordinary_start={start:?} goal={goal:?} connected_route={connected_route:?} bfs_visits={visits} order={order:?}"
    );
    assert_eq!(order, Ok(true), "ordinary endpoint order must be accepted");
    assert_eq!(replay.issue_move(replay_unit, destination), order);
    let mut arrived = None;
    for tick in 1..=MAX_EXIT_TICKS {
        world.advance();
        replay.advance();
        assert_eq!(
            world.canonical_hash(),
            replay.canonical_hash(),
            "exact replay at tick {tick}"
        );
        assert_eq!(
            world.movement_failure(unit),
            None,
            "route rejection at tick {tick}"
        );
        let state = world.unit(unit).unwrap();
        if state.position == destination && !state.moving && !state.planning {
            arrived = Some(tick);
            break;
        }
    }
    let ticks = arrived.expect("source cavalry must reach the outside target within 20,000 ticks");
    let final_tile = world.unit(unit).unwrap().position.tile_floor();
    let displacement = start
        .x
        .abs_diff(final_tile.x)
        .max(start.y.abs_diff(final_tile.y));
    assert!(displacement >= EXIT_DISTANCE);
    assert!(provider.resident_pages() <= 128);
    assert!(replay_provider.resident_pages() <= 128);
    for simulation in [&world, &replay] {
        let usage = simulation.navigation_cache_usage();
        assert_eq!(usage.limit_bytes, 128 * 1024 * 1024);
        assert!(usage.retained_bytes <= usage.limit_bytes);
    }
    println!(
        "source_spawn_exit PASS hash={hash} start={start:?} end={final_tile:?} displacement_tiles={displacement} ticks={ticks} simulation_seconds={} speed_meters_per_second=3 route_rejections=0 exact_replay_hash={} source_pages={} replay_pages={} navigation_usage={:?} LIMIT=one native cavalry/local exit; no big-army or dedicated-hardware qualification",
        ticks as f64 / f64::from(config.tick_hz),
        world.canonical_hash_hex(),
        provider.resident_pages(),
        replay_provider.resident_pages(),
        world.navigation_cache_usage()
    );
}

fn find_exit(
    terrain: &Terrain,
    config: aoe_core::WorldConfig,
    start: TileCoord,
) -> (TileCoord, Vec<TileCoord>, usize) {
    let mut parents = BTreeMap::from([(start, start)]);
    let mut queue = VecDeque::from([start]);
    while let Some(tile) = queue.pop_front() {
        if start.x.abs_diff(tile.x).max(start.y.abs_diff(tile.y)) >= EXIT_DISTANCE {
            let mut route = vec![tile];
            while *route.last().unwrap() != start {
                route.push(parents[route.last().unwrap()]);
            }
            route.reverse();
            return (tile, route, parents.len());
        }
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if parents.contains_key(&next)
                || next.x < 0
                || next.y < 0
                || next.x >= config.width_tiles
                || next.y >= config.height_tiles
                || start.x.abs_diff(next.x).max(start.y.abs_diff(next.y)) > EXIT_DISTANCE
            {
                continue;
            }
            if terrain
                .crossable_with_cancel(tile, next, config, &|| false)
                .expect("verified source collision query")
            {
                assert!(
                    parents.len() < MAX_EXIT_VISITS,
                    "bounded source exit BFS exceeded {MAX_EXIT_VISITS} tiles"
                );
                parents.insert(next, tile);
                queue.push_back(next);
            }
        }
    }
    panic!("ordinary certified component has no reachable {EXIT_DISTANCE}-tile exit");
}
