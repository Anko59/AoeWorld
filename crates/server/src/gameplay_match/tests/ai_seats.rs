//! Rust AI seats played inside the server (`AOE_AI_RUNTIME=rust`).
//!
//! The interface these tests pin, for the implementer:
//! - `crate::config::AiRuntime { Python, Rust }` (`Debug`, `PartialEq`, `Copy`)
//!   and `Config::ai_runtime_from(Option<&str>) -> Result<AiRuntime, String>`,
//!   the parser behind `Config::ai_runtime_from_env`; unset means `Rust`.
//! - `crate::gameplay_match::play_ai_seats(&AppState, AiRuntime) -> usize`
//!   seats every non-human slot of the state's current match in process and
//!   returns how many; with `Python` it seats none. The current match then
//!   decides inside `GameplayService::tick` / `tick_overlapped`.
//! - `GameplayService::ai_order_log() -> Vec<AppliedOrder>` lists every order
//!   an in-process seat had applied, in application order, with the tick it
//!   was applied on (`tick: u64`) and its seat (`player: PlayerId`); the type
//!   is `Clone + Debug + PartialEq`.
//! - `aoe_simulation::OrderIssuer::RustAi` labels those orders.
use super::*;
use crate::config::AiRuntime;
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
use aoe_simulation::{BuildingKind, OrderIssuer, UnitKind};

/// Slot 0 is the person (never played in process); slots 1 and 2 are AI.
const SEATS: [bool; 3] = [true, false, false];

async fn match_state(seed: u64) -> AppState {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("address"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = AppState::new(&config, "ai-seats-test").expect("state");
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let rules = MatchRules::default().with_seed(seed);
    let (service, _) = GameplayService::from_match_rules(
        package.clone(),
        None,
        &SEATS,
        &rules,
        &AtomicUsize::new(0),
        &|| false,
    )
    .expect("a small seeded match");
    state
        .map_packages
        .write()
        .await
        .insert(package.content_hash_hex(), package);
    *state.gameplay.write().await = service;
    state
}

/// The seeded match with its seats chosen by `runtime`, stepped `ticks` times.
async fn play(runtime: AiRuntime, seed: u64, ticks: u32, overlapped: bool) -> (AppState, usize) {
    let state = match_state(seed).await;
    let seated = crate::gameplay_match::play_ai_seats(&state, runtime).await;
    let gameplay = state.gameplay.read().await.clone();
    for _ in 0..ticks {
        gameplay.tick_overlapped(overlapped).await;
    }
    (state, seated)
}

#[test]
fn the_runtime_setting_names_rust_or_python_and_defaults_to_rust() {
    use crate::Config;
    assert_eq!(Config::ai_runtime_from(None), Ok(AiRuntime::Rust));
    assert_eq!(Config::ai_runtime_from(Some("")), Ok(AiRuntime::Rust));
    assert_eq!(AiRuntime::default(), AiRuntime::Rust);
    assert_eq!(
        Config::ai_runtime_from(Some("python")),
        Ok(AiRuntime::Python)
    );
    assert_eq!(Config::ai_runtime_from(Some("rust")), Ok(AiRuntime::Rust));
    assert!(Config::ai_runtime_from(Some("lua")).is_err());
}

#[tokio::test]
async fn with_the_python_runtime_no_seat_is_played_in_process() {
    let (state, seated) = play(AiRuntime::Python, 7, 60, false).await;
    assert_eq!(seated, 0);
    let gameplay = state.gameplay.read().await.clone();
    assert!(gameplay.ai_order_log().is_empty());
    let world = gameplay.world.read().await;
    assert_eq!(
        world.order_demand()[OrderIssuer::RustAi as usize].decisions,
        0
    );
}

#[tokio::test]
async fn the_rust_ai_gathers_and_trains_villagers_inside_the_server() {
    let state = match_state(7).await;
    let seated = crate::gameplay_match::play_ai_seats(&state, AiRuntime::Rust).await;
    assert_eq!(seated, 2, "both AI slots, not the person's");
    let gameplay = state.gameplay.read().await.clone();
    let ai = aoe_core::PlayerId(1);
    let mut trained = false;
    for _ in 0..400 {
        gameplay.tick().await;
        let world = gameplay.world.read().await;
        trained |= world.buildings().any(|building| {
            building.player == ai
                && building.kind == BuildingKind::TownCenter
                && !building.training_queue.is_empty()
        });
    }
    let world = gameplay.world.read().await;
    let gatherers = world
        .units()
        .filter(|unit| unit.player == ai && unit.kind == UnitKind::Villager)
        .filter(|unit| world.gathering_kind(unit.id).is_some())
        .count();
    assert!(gatherers > 0, "no villager of the Rust AI is gathering");
    assert!(trained, "the Town Center never trained a villager");
    assert!(world.order_demand()[OrderIssuer::RustAi as usize].decisions > 0);
    assert_eq!(
        world.order_demand()[OrderIssuer::PythonAi as usize].decisions,
        0,
        "in-process orders are not Python's"
    );
    let log = gameplay.ai_order_log();
    assert!(!log.is_empty());
    assert!(
        log.iter()
            .all(|order| order.player != aoe_core::PlayerId(0)),
        "the person's seat is never played"
    );
}

#[tokio::test]
async fn two_runs_of_one_seed_give_identical_order_logs_and_world_hashes() {
    let outcome = |(state, _): (AppState, usize)| async move {
        let gameplay = state.gameplay.read().await.clone();
        let hash = gameplay.world.read().await.canonical_hash();
        (gameplay.ai_order_log(), hash)
    };
    let first = outcome(play(AiRuntime::Rust, 7, 300, false).await).await;
    let second = outcome(play(AiRuntime::Rust, 7, 300, false).await).await;
    assert!(!first.0.is_empty(), "the seats gave no orders");
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
}

#[tokio::test]
async fn a_server_behind_its_clock_applies_the_same_orders_on_the_same_ticks() {
    let outcome = |(state, _): (AppState, usize)| async move {
        let gameplay = state.gameplay.read().await.clone();
        let hash = gameplay.world.read().await.canonical_hash();
        (gameplay.ai_order_log(), hash)
    };
    let steady = outcome(play(AiRuntime::Rust, 7, 300, false).await).await;
    let behind = outcome(play(AiRuntime::Rust, 7, 300, true).await).await;
    assert!(!steady.0.is_empty());
    assert_eq!(
        steady.0, behind.0,
        "orders moved when the server fell behind"
    );
    assert_eq!(steady.1, behind.1);
}

#[tokio::test]
async fn orders_from_one_view_land_on_one_fixed_tick_in_seat_order() {
    let (state, _) = play(AiRuntime::Rust, 7, 300, false).await;
    let gameplay = state.gameplay.read().await.clone();
    // A decision every half game second: `tick_hz / 2` ticks apart.
    let period = u64::from(gameplay.world.read().await.config().tick_hz) / 2;
    let log = gameplay.ai_order_log();
    assert!(!log.is_empty());
    let offsets: std::collections::BTreeSet<_> =
        log.iter().map(|order| order.tick % period).collect();
    assert_eq!(
        offsets.len(),
        1,
        "orders applied at offsets {offsets:?} of the decision period"
    );
    for pair in log.windows(2) {
        assert!(
            (pair[0].tick, pair[0].player) <= (pair[1].tick, pair[1].player),
            "seat order broken: {:?} before {:?}",
            pair[0],
            pair[1]
        );
    }
}

// ----- review findings of slice 1 (#1080): F4 and F2 ---------------------------
//
// More of the interface these tests pin, for the implementer:
// - F4: `crate::maps::terrain_chunk_queued(&AppState, terrain_cache::Key) ->
//   Result<CompactChunk, StatusCode>` returns the same chunk as
//   `maps::terrain_chunk(state, key, false)` but, when the preparation lanes
//   are full, waits its turn in the queue instead of answering 503, and the
//   seats' `fetch` calls it (no retry loop).
// - F2: `rust_ai_perf()["tick_thread"]` keeps `ticks`, `cpu_us_per_tick` and
//   `instructions_per_tick` and gains four sections, `feed`, `chunk_waits`,
//   `join` and `settle`, each `{ "calls": u64, "wall_us_per_tick": f64 }`: the
//   times the tick's thread entered that wait and the wall time spent in it per
//   tick (null before the first tick).

/// Lets the jobs holding the preparation lanes finish, also when a test fails.
struct Release(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Release);
    }
}

/// Fills every background admission slot with a job that waits for `release`
/// (more jobs than the 64 slots: the surplus is refused at once).
async fn fill_preparation(gameplay: &GameplayService) -> Release {
    use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};
    let release = Release(Arc::new(AtomicBool::new(false)));
    for _ in 0..80 {
        let (gameplay, flag) = (gameplay.clone(), release.0.clone());
        tokio::spawn(async move {
            let _ = gameplay
                .prepare_terrain(false, move || {
                    while !flag.load(Ordering::Acquire) {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                })
                .await;
        });
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    release
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_seats_chunk_waits_in_the_queue_when_the_preparation_lanes_are_full() {
    let state = match_state(7).await;
    let gameplay = state.gameplay.read().await.clone();
    let hash = state
        .map_packages
        .read()
        .await
        .keys()
        .next()
        .cloned()
        .expect("the match's map package");
    let key = crate::terrain_cache::Key {
        content_hash: hash,
        x: 0,
        y: 0,
    };
    let release = fill_preparation(&gameplay).await;
    assert_eq!(
        crate::maps::terrain_chunk(&state, key.clone(), false).await,
        Err(axum::http::StatusCode::SERVICE_UNAVAILABLE),
        "the chunk route still refuses a full queue"
    );
    let queued = tokio::spawn({
        let (state, key) = (state.clone(), key.clone());
        async move { crate::maps::terrain_chunk_queued(&state, key).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !queued.is_finished(),
        "a seat's chunk waits for its turn: neither refused nor spinning to a verdict"
    );
    release.0.store(true, std::sync::atomic::Ordering::Release);
    let chunk = tokio::time::timeout(std::time::Duration::from_secs(30), queued)
        .await
        .expect("the queued load completes in bounded time once the lanes drain")
        .expect("the load task")
        .expect("a chunk, not a refusal");
    assert_eq!(
        Ok(chunk),
        crate::maps::terrain_chunk(&state, key, false).await,
        "the same data as the chunk route"
    );
}

#[tokio::test]
async fn the_tick_threads_seat_costs_are_reported_separately() {
    let (state, _) = play(AiRuntime::Rust, 7, 120, false).await;
    let gameplay = state.gameplay.read().await.clone();
    let perf = gameplay.rust_ai_perf();
    let tick_thread = &perf["tick_thread"];
    let ticks = tick_thread["ticks"].as_u64().expect("ticks");
    assert!(ticks > 0);
    for section in ["feed", "chunk_waits", "join", "settle"] {
        let entry = &tick_thread[section];
        assert!(
            entry.is_object(),
            "tick_thread.{section} is missing: {perf}"
        );
        assert!(entry["calls"].is_u64(), "{section}.calls: {perf}");
        assert!(
            entry["wall_us_per_tick"]
                .as_f64()
                .is_some_and(|us| us >= 0.0),
            "{section}.wall_us_per_tick: {perf}"
        );
    }
    assert_eq!(
        tick_thread["feed"]["calls"].as_u64(),
        Some(ticks),
        "every tick applies what was delivered to the seats"
    );
    assert!(
        tick_thread["join"]["calls"].as_u64().is_some_and(|n| n > 0),
        "the decisions' join is counted: {perf}"
    );
    assert!(
        tick_thread["chunk_waits"]["calls"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "the chunk waits are counted: {perf}"
    );
}

#[tokio::test]
async fn a_castle_age_rust_ai_builds_a_monastery_and_banks_a_relic_in_skirmish() {
    use super::super::flat;
    use aoe_simulation::Age;

    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 384,
        seed: 7,
    })
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    crate::map_store::persist_prepared(Some(dir.path()), &package, &pages, &[], &[], &[]).unwrap();
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(dir.path().to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: dir.path().to_owned(),
    };
    let state = AppState::new(&config, "ai-relic-skirmish").unwrap();
    let game = super::super::launch(
        &state,
        package,
        vec![true, false],
        MatchRules::default().with_seed(7),
    )
    .await
    .unwrap()
    .service;
    // Run the ordinary decision and order path on a shorter test clock.
    const HZ: u32 = 4;
    game.world.write().await.set_tick_hz(HZ);
    *state.gameplay.write().await = game.clone();
    assert_eq!(
        crate::gameplay_match::play_ai_seats(&state, AiRuntime::Rust).await,
        1
    );
    let ai = aoe_core::PlayerId(1);
    let mut exposed_relic = false;
    let mut built_monastery = false;
    for _ in 0..45 * 60 * HZ {
        game.tick().await;
        let mut world = game.world.write().await;
        if world.player_economy(ai).unwrap().age < Age::Castle {
            continue;
        }
        if !exposed_relic {
            let center = world
                .buildings()
                .find(|b| b.kind == BuildingKind::TownCenter && b.player == ai)
                .unwrap()
                .footprint
                .min;
            let distance = |at: aoe_core::WorldPosition| {
                let tile = at.tile_floor();
                i64::from(tile.x - center.x).pow(2) + i64::from(tile.y - center.y).pow(2)
            };
            let (_, at) = world
                .ground_relics()
                .min_by_key(|&(_, at)| distance(at))
                .unwrap();
            assert!(
                distance(at) <= 45_i64.pow(2),
                "the fixture needs a nearby relic"
            );
            // Expose a naturally seeded relic once the ordinary AI reaches Castle.
            // An idle cart keeps sight of it without occupying its pickup tile.
            let tile = at.tile_floor();
            let watch = [(-3, 0), (3, 0), (0, -3), (0, 3)]
                .into_iter()
                .map(|(dx, dy)| TileCoord::new(tile.x + dx, tile.y + dy))
                .find(|&tile| world.tile_passable(tile))
                .unwrap();
            world
                .spawn_kind(
                    ai,
                    UnitKind::TradeCart,
                    aoe_core::WorldPosition::from_tile_center(watch).unwrap(),
                )
                .unwrap();
            assert_eq!(
                world.visibility(ai, at.tile_floor()),
                aoe_simulation::TileVisibility::Visible
            );
            exposed_relic = true;
        }
        for monastery in world
            .buildings()
            .filter(|b| b.kind == BuildingKind::Monastery && b.player == ai && b.is_complete())
        {
            built_monastery = true;
            if world.monastery_relic_count(monastery.id) > 0 {
                return;
            }
        }
    }
    assert!(exposed_relic, "the Rust AI did not reach Castle Age");
    assert!(built_monastery, "the Rust AI never finished a Monastery");
    let world = game.world.read().await;
    let orders = game
        .ai_order_log()
        .into_iter()
        .rev()
        .filter(|order| {
            order.player == ai
                && ["Monastery", "Monk", "Relic", "Garrison"]
                    .iter()
                    .any(|kind| order.order.contains(kind))
        })
        .take(8)
        .collect::<Vec<_>>();
    let monks = world
        .units()
        .filter(|unit| unit.player == ai && unit.kind == UnitKind::Monk)
        .map(|monk| {
            (
                monk.position.tile_floor(),
                monk.task,
                world.carried_relic(monk.id),
            )
        })
        .collect::<Vec<_>>();
    panic!(
        "the Rust AI never banked a relic. Monks: {monks:?}, recent orders: {orders:?}, stockpile: {:?}",
        world.player_economy(ai).unwrap().stockpile,
    );
}
