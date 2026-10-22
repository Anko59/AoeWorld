//! The AI arena: a match that runs as fast as the machine allows.
//!
//! With `AOE_ARENA=1` the server does not tick on a clock. A driver advances the
//! simulation with `POST /match/step` and decides when each tick happens, so the
//! bots, who still connect through the normal protocol and see the normal
//! fog-filtered view, play in lockstep: the driver steps one decision period,
//! lets every bot decide, then steps again. A run for one seed repeats exactly.
//! Order budgets that guard a real-time server against floods are lifted, since
//! wall-clock rates mean nothing when time is not real.
use crate::{AppState, Config, gameplay_match::ADMIN_TOKEN_HEADER};
use axum::{Json, Router, extract::State, http::HeaderMap, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Orders the sessions have queued since the process started.
static RECEIVED: AtomicU64 = AtomicU64::new(0);
/// `RECEIVED` when the last step ended.
static SETTLED: AtomicU64 = AtomicU64::new(0);
const MAX_STEP_TICKS: u32 = 200;
/// Orders that never arrive (an order the client refused locally) must not
/// stall a step: it goes ahead once the count has been still this long.
const QUIET: Duration = Duration::from_millis(150);

pub(crate) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(Config::arena_from_env)
}

/// Counted by the order queue for every order it is offered.
pub(super) fn note_order() {
    RECEIVED.fetch_add(1, Ordering::SeqCst);
}

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/match/step", post(step))
}

#[derive(Deserialize)]
struct StepRequest {
    /// Ticks to advance.
    ticks: u32,
    /// Orders the bots queued since the previous step: the step waits for them.
    orders: u64,
}

#[derive(Serialize)]
struct StepResponse {
    tick: u64,
    orders_received: u64,
}

async fn step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StepRequest>,
) -> Result<Json<StepResponse>, (StatusCode, String)> {
    let allowed = state.match_admin_token.as_deref().is_some_and(|secret| {
        headers
            .get(ADMIN_TOKEN_HEADER)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|given| given == secret)
    });
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "needs the AOE_MATCH_TOKEN secret".to_owned(),
        ));
    }
    if !enabled() {
        return Err((
            StatusCode::NOT_FOUND,
            "the server is not in arena mode (AOE_ARENA=1)".to_owned(),
        ));
    }
    if request.ticks == 0 || request.ticks > MAX_STEP_TICKS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("ticks must be 1..={MAX_STEP_TICKS}"),
        ));
    }
    let wanted = SETTLED.load(Ordering::SeqCst) + request.orders;
    let (mut seen, mut changed) = (RECEIVED.load(Ordering::SeqCst), Instant::now());
    while seen < wanted && changed.elapsed() < QUIET {
        tokio::time::sleep(Duration::from_millis(1)).await;
        let now = RECEIVED.load(Ordering::SeqCst);
        if now != seen {
            (seen, changed) = (now, Instant::now());
        }
    }
    let gameplay = state.gameplay.read().await.clone();
    for _ in 0..request.ticks {
        gameplay.tick().await;
    }
    let received = RECEIVED.load(Ordering::SeqCst);
    SETTLED.store(received, Ordering::SeqCst);
    Ok(Json(StepResponse {
        tick: gameplay.world.read().await.tick().0,
        orders_received: received,
    }))
}
