//! Rust AI seats played inside the server (`AOE_AI_RUNTIME=rust`).
//!
//! The interface these tests pin, for the implementer:
//! - `crate::config::AiRuntime { Python, Rust }` (`Debug`, `PartialEq`, `Copy`)
//!   and `Config::ai_runtime_from(Option<&str>) -> Result<AiRuntime, String>`,
//!   the parser behind `Config::ai_runtime_from_env`; unset means `Python`.
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
fn the_runtime_setting_names_rust_or_python_and_defaults_to_python() {
    use crate::Config;
    assert_eq!(Config::ai_runtime_from(None), Ok(AiRuntime::Python));
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
