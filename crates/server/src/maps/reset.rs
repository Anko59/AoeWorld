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
        None => GameplayService::new(state.diagnostic_scenario.seed),
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
            name @ ("feudal-economy" | "vassal-economy" | "vassal-territory" | "vassal-farming"
            | "vassal-kingdom" | "feudal-military" | "general-army" | "general-appearance"
            | "imperial-army" | "vassal-fishing" | "vassal-throughput" | "vassal-stability"),
        ) => {
            let constructor = match name {
                "feudal-economy" => GameplayService::feudal_demo,
                "vassal-economy" => GameplayService::vassal_demo,
                "vassal-territory" => GameplayService::vassal_territory_demo,
                "vassal-farming" => GameplayService::vassal_farming_demo,
                "vassal-kingdom" => GameplayService::vassal_kingdom_demo,
                "vassal-stability" => GameplayService::vassal_stability_demo,
                "vassal-fishing" => GameplayService::vassal_fishing_demo,
                "vassal-throughput" => GameplayService::vassal_throughput_demo,
                "general-army" => GameplayService::general_demo,
                "imperial-army" => GameplayService::imperial_demo,
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
