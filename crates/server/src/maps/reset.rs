//! Local scenario reset adapters.
use super::*;

#[derive(serde::Deserialize)]
pub(crate) struct ResetOptions {
    scenario: Option<String>,
}

pub(crate) async fn reset(
    State(state): State<AppState>,
    Query(options): Query<ResetOptions>,
) -> StatusCode {
    let service = match options.scenario.as_deref() {
        Some("defensive-volley") => {
            match GameplayService::volley_demo(state.diagnostic_scenario.seed) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        None => GameplayService::new(state.diagnostic_scenario.seed),
        Some(
            name @ ("behaviour-tc" | "behaviour-wolf" | "behaviour-deer" | "behaviour-hunt"
            | "behaviour-camps" | "behaviour-bridge"),
        ) => {
            use aoe_simulation::BehaviourScene;
            let scene = match name {
                "behaviour-tc" => BehaviourScene::TownCenter,
                "behaviour-wolf" => BehaviourScene::Wolf,
                "behaviour-deer" => BehaviourScene::Deer,
                "behaviour-hunt" => BehaviourScene::Hunt,
                "behaviour-bridge" => BehaviourScene::Bridge,
                _ => BehaviourScene::Camps,
            };
            let Ok((package, elevation, water)) = aoe_simulation::GameWorld::behaviour_environment(
                state.diagnostic_scenario.seed,
                scene,
            ) else {
                return StatusCode::INTERNAL_SERVER_ERROR;
            };
            let stored = package.clone();
            let directory = state.map_package_directory.clone();
            if !tokio::task::spawn_blocking(move || {
                crate::map_store::persist_prepared(
                    directory.as_deref(),
                    &stored,
                    &elevation,
                    &water,
                    &[],
                    &[],
                )
            })
            .await
            .is_ok_and(|result| result.is_ok())
            {
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            state
                .map_packages
                .write()
                .await
                .insert(package.content_hash_hex(), package);
            match GameplayService::behaviour_demo(state.diagnostic_scenario.seed, scene) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Some("naval-battle") => {
            let seed = state.diagnostic_scenario.seed;
            let Ok((package, elevation, water)) =
                aoe_simulation::GameWorld::naval_battle_environment(seed)
            else {
                return StatusCode::INTERNAL_SERVER_ERROR;
            };
            let stored = package.clone();
            let directory = state.map_package_directory.clone();
            if !tokio::task::spawn_blocking(move || {
                crate::map_store::persist_prepared(
                    directory.as_deref(),
                    &stored,
                    &elevation,
                    &water,
                    &[],
                    &[],
                )
            })
            .await
            .is_ok_and(|result| result.is_ok())
            {
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
            state
                .map_packages
                .write()
                .await
                .insert(package.content_hash_hex(), package);
            match GameplayService::naval_battle_demo(seed) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Some("movement-lane") => {
            match GameplayService::movement_lane_demo(state.diagnostic_scenario.seed) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Some(name @ ("movement-first-sight" | "movement-retained-target")) => {
            match GameplayService::movement_recovery_demo(
                state.diagnostic_scenario.seed,
                name == "movement-retained-target",
            ) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Some("vassal-river") => {
            let active = state.gameplay.read().await.clone();
            match active.prepare_vassal_river_demo().await {
                Ok(service) => service,
                Err(reason) => {
                    tracing::warn!(%reason, "vassal river setup failed");
                    return StatusCode::BAD_REQUEST;
                }
            }
        }
        Some("general-river") => {
            let active = state.gameplay.read().await.clone();
            match active.prepare_general_river_demo().await {
                Ok(service) => service,
                Err(reason) => {
                    tracing::warn!(%reason, "source river recording setup failed");
                    return StatusCode::BAD_REQUEST;
                }
            }
        }
        Some(
            name @ ("roads" | "vassal-defense" | "feudal-economy" | "steward-economy"
            | "steward-territory" | "steward-farming" | "steward-kingdom"
            | "feudal-military" | "general-army" | "general-appearance" | "imperial-army"
            | "trade-wonder" | "steward-fishing" | "steward-throughput"
            | "steward-stability" | "walk-crowd"),
        ) => {
            let constructor = match name {
                "roads" => GameplayService::roads_demo,
                "vassal-defense" => GameplayService::vassal_defense_demo,
                "feudal-economy" => GameplayService::feudal_demo,
                "steward-economy" => GameplayService::steward_demo,
                "steward-territory" => GameplayService::steward_territory_demo,
                "steward-farming" => GameplayService::steward_farming_demo,
                "steward-kingdom" => GameplayService::steward_kingdom_demo,
                "steward-stability" => GameplayService::steward_stability_demo,
                "steward-fishing" => GameplayService::steward_fishing_demo,
                "steward-throughput" => GameplayService::steward_throughput_demo,
                "walk-crowd" => GameplayService::walk_crowd_demo,
                "general-army" => GameplayService::general_demo,
                "imperial-army" => GameplayService::imperial_demo,
                "trade-wonder" => GameplayService::trade_demo,
                "general-appearance" => GameplayService::general_appearance_demo,
                _ => GameplayService::military_demo,
            };
            match constructor(state.diagnostic_scenario.seed) {
                Ok(service) => service,
                Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Some(_) => return StatusCode::BAD_REQUEST,
    };
    *state.gameplay.write().await = service;
    state.generation.fetch_add(1, Ordering::SeqCst);
    StatusCode::NO_CONTENT
}
