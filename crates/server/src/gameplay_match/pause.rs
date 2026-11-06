//! Pause: the person stops the match clock from the game menu. The server
//! stops stepping the match, so AI processes get no frames and their orders
//! only fill their bounded queues; nothing advances until the person resumes.
use super::skirmish;
use crate::{AppState, GameplayService};
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;

impl GameplayService {
    /// Whether the match clock is stopped.
    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PauseRequest {
    paused: bool,
}

#[derive(Serialize)]
struct PauseState {
    paused: bool,
}

/// `POST /match/pause`: the human player pauses or resumes their match.
async fn set_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PauseRequest>,
) -> Result<Json<PauseState>, (StatusCode, String)> {
    skirmish::authorize(&state, &headers).await?;
    let service = state.gameplay.read().await.clone();
    service.set_paused(request.paused);
    tracing::info!(paused = request.paused, "match pause changed");
    Ok(Json(PauseState {
        paused: request.paused,
    }))
}

pub(super) fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/match/pause", axum::routing::post(set_pause))
}
