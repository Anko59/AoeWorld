//! A match played without HTTP, a socket or a clock: the way in for the game
//! score (`aoe-harness game-score`, docs/scale/game-score.md).
//!
//! It does what `POST /match` does for a known map package, through the same
//! `launch` and `install`, and leaves the loop to the caller: every call of
//! `HeadlessMatch::tick` is one `GameplayService::tick`, whenever the caller
//! wants it. Every slot is an AI seat played by the Rust AI in process.
use super::{MatchRules, StartUnits, install, launch};
use crate::{AiRuntime, AppState, Config, GameplayService};
use aoe_map::{EnvironmentPageProvider, MapPackage};
use aoe_simulation::GameWorld;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

/// How a headless match starts.
#[derive(Clone, Debug)]
pub struct HeadlessPlan {
    /// The directory that holds the map package (`<hash>.json` and its pages).
    pub package_directory: PathBuf,
    /// The package to play on, by its content hash (64 hex digits).
    pub map_hash: String,
    /// Seats, all played by the Rust AI.
    pub seats: usize,
    /// Seeds the village layout, as `seed` of `POST /match`.
    pub seed: u64,
    /// The late-game recipe (`late_game.rs`) at this scale in percent (0 and
    /// 100 keep the owner's shapes), seeded around every village before the
    /// first tick. `None` is a fresh start.
    pub late_game_percent: Option<u32>,
    /// Keep the orders the seats had applied (`HeadlessMatch::order_log`). It
    /// costs one serialization per order on the tick path: off, a run is timed
    /// as a live server runs.
    pub record_orders: bool,
}

#[derive(Debug, thiserror::Error)]
#[error("headless match: {0}")]
pub struct HeadlessError(String);

/// A running match with nobody connected.
pub struct HeadlessMatch {
    /// Held for the seats, which fetch their map chunks through it.
    _state: AppState,
    gameplay: GameplayService,
}

/// The state of a server that serves `directory`, and the package `hash` in it.
async fn open(directory: &Path, hash: &str) -> Result<(AppState, MapPackage), HeadlessError> {
    let config = Config {
        bind: ([127, 0, 0, 1], 0).into(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: Config::DEFAULT_SIM_HZ,
        asset_pack: None,
        map_package_directory: Some(directory.to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: Config::DEFAULT_GEODATA_CACHE_DIRECTORY.into(),
    };
    let state =
        AppState::new(&config, "headless").map_err(|error| HeadlessError(error.to_string()))?;
    let package = state
        .map_packages
        .read()
        .await
        .get(hash)
        .cloned()
        .ok_or_else(|| HeadlessError(format!("no map package {hash}")))?;
    Ok((state, package))
}

impl HeadlessMatch {
    /// The world of a package exactly as a match loads it (its page residency, its prepared
    /// navigation, the simulation's clock rate), with no village, no unit, no seat and no AI.
    /// For a scenario that stages its own units and ticks the world itself with
    /// `GameWorld::advance`, which is what one `GameplayService::tick` does for a world
    /// nobody sends orders to (the game score's `march` and `battle`).
    pub async fn bare_world(directory: &Path, map_hash: &str) -> Result<GameWorld, HeadlessError> {
        let (state, package) = open(directory, map_hash).await?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let provider = crate::maps::page_residency(&state, &package, cancelled)
            .await
            .map_err(|(_, message)| HeadlessError(message))?;
        let (hash, directory) = (package.content_hash, directory.to_owned());
        tokio::task::spawn_blocking(move || {
            let mut world = match provider {
                Some(provider) => GameWorld::from_page_provider(
                    package,
                    provider as Arc<dyn EnvironmentPageProvider>,
                ),
                None => GameWorld::from_map(package),
            }
            .map_err(|error| HeadlessError(error.to_string()))?;
            world.set_tick_hz(Config::sim_hz_from_env());
            crate::map_store::navigation::prepare_world(&world, hash, Some(&directory), &|| false)
                .map_err(HeadlessError)?;
            world
                .prepare_navigation(&|| false)
                .map_err(|error| HeadlessError(error.to_string()))?;
            Ok(world)
        })
        .await
        .map_err(|_| HeadlessError("the world task failed".to_owned()))?
    }

    /// Loads the package directory, builds the match and seats its AIs.
    pub async fn start(plan: &HeadlessPlan) -> Result<Self, HeadlessError> {
        let (state, package) = open(&plan.package_directory, &plan.map_hash).await?;
        let rules = match plan.late_game_percent {
            None => MatchRules::default(),
            Some(percent) => MatchRules::new(Some(StartUnits {
                late_game: true,
                late_game_scale: percent,
                ..StartUnits::default()
            }))
            .map_err(|error| HeadlessError(error.to_string()))?,
        }
        .with_seed(plan.seed);
        let launched = launch(&state, package, vec![false; plan.seats], rules)
            .await
            .map_err(|(_, message)| HeadlessError(message))?;
        launched.service.ai_seats.record_orders(plan.record_orders);
        let gameplay = launched.service.clone();
        install(&state, launched.service, AiRuntime::Rust).await;
        Ok(Self {
            _state: state,
            gameplay,
        })
    }

    /// One tick, as the server's clock would call it.
    pub async fn tick(&self) {
        self.gameplay.tick().await;
    }

    /// The match, to read through `inspect_scale_world`.
    pub fn gameplay(&self) -> &GameplayService {
        &self.gameplay
    }

    /// One line per order the seats had applied, in application order: tick,
    /// player, sequence, result and the order itself. Empty unless the plan
    /// asked for the orders to be recorded.
    pub fn order_log(&self) -> Vec<String> {
        self.gameplay
            .ai_order_log()
            .into_iter()
            .map(|order| {
                format!(
                    "{} {} {} {:?} {}",
                    order.tick, order.player.0, order.sequence, order.result, order.order
                )
            })
            .collect()
    }
}
