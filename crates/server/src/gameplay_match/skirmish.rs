//! A single-player skirmish against AI opponents, started from the browser's
//! setup screen once its map is prepared. The server builds the match (slot 0
//! is the person, the rest AI), swaps it in, starts the AI host processes and
//! reports each phase with real counts: villages placed, AI players connected.
//! The AIs are separate processes on the same protocol and fog view as the
//! person. They stop when the match is replaced, cancelled or quit, or when
//! nobody has held the human seat for a while.
use super::{MAX_MATCH_SLOTS, MatchRules};
use crate::{AppState, GameplayService, maps};
use aoe_map::EnvironmentPageProvider;
use aoe_protocol::ResumeToken;
use axum::{Json, extract::State, http::HeaderMap, http::StatusCode};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

mod fleet;
use fleet::Launch;

/// Opponents beside the person: the match's seats minus one, at most 99.
pub(crate) const MAX_OPPONENTS: usize = if MAX_MATCH_SLOTS > 100 {
    99
} else {
    MAX_MATCH_SLOTS - 1
};
const POLL: Duration = Duration::from_millis(250);
/// Every AI must hold its slot within this time after the hosts start.
const CONNECT_LIMIT: Duration = Duration::from_secs(10 * 60);
/// The match is over when nobody has held the human seat this long.
const ABANDONED_AFTER: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    #[default]
    Idle,
    OpeningMap,
    PlacingVillages,
    StartingAi,
    ConnectingAi,
    Ready,
    Failed,
    Cancelled,
    Stopped,
}

impl Phase {
    fn active(self) -> bool {
        matches!(
            self,
            Self::OpeningMap | Self::PlacingVillages | Self::StartingAi | Self::ConnectingAi
        )
    }
}

/// `GET /skirmish`: what the loading screen shows. Counts are measured, never
/// estimated; a phase without one leaves them out.
#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct Status {
    id: u64,
    phase: Phase,
    ai_available: bool,
    max_opponents: usize,
    opponents: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    completed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// AI host processes running for the current match.
    ai_hosts: usize,
}

#[derive(Default)]
struct Inner {
    status: Status,
    started_by: Option<ResumeToken>,
    starter_world: Option<u64>,
    cancelled: Arc<AtomicBool>,
    placed: Arc<AtomicUsize>,
    fleet: Option<fleet::Fleet>,
}

/// The server's one skirmish: its phase and its AI processes.
#[derive(Default)]
pub(crate) struct Skirmish {
    launch: Option<Launch>,
    inner: std::sync::Mutex<Inner>,
}

impl Skirmish {
    pub(crate) fn new(launch: Option<Launch>) -> Self {
        Self {
            launch,
            inner: std::sync::Mutex::default(),
        }
    }

    /// AI hosts from `AOE_AI_COMMAND`, joining the server on its own port.
    pub(crate) fn from_env(port: u16) -> Self {
        Self::new(crate::Config::ai_command_from_env().map(|command| Launch {
            command,
            url: format!("http://127.0.0.1:{port}"),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Records `phase` if skirmish `id` is still the current one and running.
    fn advance(&self, id: u64, phase: Phase) -> bool {
        let mut inner = self.lock();
        let current = inner.status.id == id && inner.status.phase.active();
        if current {
            inner.status.phase = phase;
        }
        current
    }

    fn fail(&self, id: u64, phase: Phase, error: Option<String>) {
        let fleet = {
            let mut inner = self.lock();
            if inner.status.id != id || !inner.status.phase.active() {
                return;
            }
            inner.status.phase = phase;
            inner.status.error = error;
            inner.fleet.take()
        };
        drop(fleet);
    }

    /// Stops the AI processes and any match still being made.
    fn stop(&self) -> Status {
        let fleet = {
            let mut inner = self.lock();
            inner.cancelled.store(true, Ordering::Release);
            inner.status.phase = match inner.status.phase {
                phase if phase.active() => Phase::Cancelled,
                Phase::Ready => Phase::Stopped,
                phase => phase,
            };
            inner.fleet.take()
        };
        drop(fleet);
        self.lock().status.clone()
    }
}

async fn status(state: &AppState) -> Status {
    let gameplay = state.gameplay.read().await.clone();
    let (live_ai, _, _) = gameplay.presence().await;
    let skirmish = &state.skirmish;
    let mut inner = skirmish.lock();
    let mut status = inner.status.clone();
    status.ai_available = skirmish.launch.is_some();
    status.max_opponents = MAX_OPPONENTS;
    status.ai_hosts = inner.fleet.as_mut().map_or(0, |fleet| fleet.hosts());
    match status.phase {
        Phase::PlacingVillages => {
            status.completed = Some(inner.placed.load(Ordering::Acquire));
            status.total = Some(status.opponents + 1);
            status.unit = Some("villages");
        }
        Phase::ConnectingAi | Phase::Ready
            if inner.fleet.as_ref().map(|f| f.world_id) == Some(gameplay.world_id()) =>
        {
            status.completed = Some(live_ai);
            status.total = Some(status.opponents);
            status.unit = Some("AI players");
        }
        _ => {}
    }
    status
}

/// The token of a request made by the current controller, or by whoever
/// started the skirmish (who is not connected to the new world yet).
async fn authorize(state: &AppState, headers: &HeaderMap) -> Result<u64, (StatusCode, String)> {
    let refused = || {
        (
            StatusCode::FORBIDDEN,
            "current gameplay controller authorization is required".to_owned(),
        )
    };
    let token = maps::controller_token(headers).ok_or_else(refused)?;
    let gameplay = state.gameplay.read().await.clone();
    {
        let inner = state.skirmish.lock();
        if inner.started_by == Some(token) && inner.starter_world == Some(gameplay.world_id()) {
            return Ok(gameplay.world_id());
        }
    }
    gameplay
        .is_controller(token)
        .await
        .then_some(gameplay.world_id())
        .ok_or_else(refused)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartRequest {
    map_hash: String,
    opponents: usize,
}

async fn get_status(State(state): State<AppState>) -> Json<Status> {
    Json(status(&state).await)
}

async fn stop(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Status>, (StatusCode, String)> {
    authorize(&state, &headers).await?;
    state.skirmish.stop();
    Ok(Json(status(&state).await))
}

async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartRequest>,
) -> Result<(StatusCode, Json<Status>), (StatusCode, String)> {
    let starter_world = authorize(&state, &headers).await?;
    let token = maps::controller_token(&headers);
    if request.opponents > MAX_OPPONENTS {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("choose 0 to {MAX_OPPONENTS} opponents"),
        ));
    }
    if request.opponents > 0 && state.skirmish.launch.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "AI opponents are not available on this server (start it with `make dev`)".to_owned(),
        ));
    }
    let package = state
        .map_packages
        .read()
        .await
        .get(&request.map_hash)
        .cloned()
        .ok_or((StatusCode::NOT_FOUND, "unknown map package".to_owned()))?;
    let (id, cancelled, placed, previous) = {
        let mut inner = state.skirmish.lock();
        if inner.status.phase.active() {
            return Err((
                StatusCode::CONFLICT,
                "a skirmish is already being prepared".to_owned(),
            ));
        }
        let id = inner.status.id + 1;
        inner.cancelled = Arc::new(AtomicBool::new(false));
        inner.placed = Arc::new(AtomicUsize::new(0));
        inner.started_by = token;
        inner.starter_world = Some(starter_world);
        inner.status = Status {
            id,
            phase: Phase::OpeningMap,
            opponents: request.opponents,
            content_hash: Some(request.map_hash.clone()),
            ..Status::default()
        };
        let previous = inner.fleet.take();
        (id, inner.cancelled.clone(), inner.placed.clone(), previous)
    };
    drop(previous);
    let task_state = state.clone();
    tokio::spawn(async move {
        let result = prepare(
            &task_state,
            id,
            package,
            request.opponents,
            &cancelled,
            &placed,
        )
        .await;
        let skirmish = &task_state.skirmish;
        match result {
            _ if cancelled.load(Ordering::Acquire) => skirmish.fail(id, Phase::Cancelled, None),
            Ok(()) => watch(&task_state, id).await,
            Err(error) => {
                tracing::warn!(%error, "skirmish failed");
                skirmish.fail(id, Phase::Failed, Some(error));
            }
        }
    });
    Ok((StatusCode::ACCEPTED, Json(status(&state).await)))
}

/// Makes the match, swaps it in and starts the AI hosts.
async fn prepare(
    state: &AppState,
    id: u64,
    package: aoe_map::MapPackage,
    opponents: usize,
    cancelled: &Arc<AtomicBool>,
    placed: &Arc<AtomicUsize>,
) -> Result<(), String> {
    let provider = maps::page_residency(state, &package, cancelled.clone())
        .await
        .map_err(|(_, error)| error)?;
    if !state.skirmish.advance(id, Phase::PlacingVillages) {
        return Ok(());
    }
    let mut humans = vec![false; opponents + 1];
    humans[0] = true;
    let seed = super::fresh_seed();
    tracing::info!(seed, "village layout seed");
    let started = Instant::now();
    let (cancel, count) = (cancelled.clone(), placed.clone());
    let directory = state.map_package_directory.clone();
    let service = tokio::task::spawn_blocking(move || {
        GameplayService::from_match_rules_stored(
            package,
            provider.map(|value| value as Arc<dyn EnvironmentPageProvider>),
            &humans,
            &MatchRules::default().with_seed(seed),
            &count,
            &|| cancel.load(Ordering::Acquire),
            directory.as_deref(),
        )
    })
    .await
    .map_err(|_| "match task failed".to_owned())?
    .map_err(|error| error.to_string())?
    .0;
    tracing::info!(
        seats = opponents + 1,
        seconds = started.elapsed().as_secs_f64(),
        "skirmish match created"
    );
    if !state.skirmish.advance(id, Phase::StartingAi) {
        return Ok(());
    }
    let tokens: Vec<String> = service
        .slot_infos()
        .await
        .into_iter()
        .filter(|slot| !slot.human)
        .map(|slot| super::hex(slot.token))
        .collect();
    let world_id = service.world_id();
    let previous = {
        let mut gameplay = state.gameplay.write().await;
        let mut inner = state.skirmish.lock();
        if inner.status.id != id
            || !inner.status.phase.active()
            || inner.starter_world != Some(gameplay.world_id())
        {
            return Err("the starting world was replaced".to_owned());
        }
        inner.starter_world = Some(world_id);
        std::mem::replace(&mut *gameplay, service)
    };
    previous.retire(world_id).await;
    state.generation.fetch_add(1, Ordering::SeqCst);
    if let Some(launch) = state.skirmish.launch.as_ref().filter(|_| opponents > 0) {
        let fleet = launch.spawn(world_id, &tokens)?;
        let mut inner = state.skirmish.lock();
        if inner.status.id == id && inner.status.phase.active() {
            inner.fleet = Some(fleet);
        }
    }
    state.skirmish.advance(id, Phase::ConnectingAi);
    Ok(())
}

/// Waits for every AI to hold its slot, then keeps the hosts while the match
/// lasts: until it is replaced, stopped, or abandoned by the person.
async fn watch(state: &AppState, id: u64) {
    let started = Instant::now();
    let mut absent_since: Option<Instant> = None;
    loop {
        tokio::time::sleep(POLL).await;
        let gameplay = state.gameplay.read().await.clone();
        let (live_ai, ai, human) = gameplay.presence().await;
        let skirmish = &state.skirmish;
        let (phase, finished, world) = {
            let mut inner = skirmish.lock();
            if inner.status.id != id {
                return;
            }
            let world = inner.fleet.as_ref().map(|fleet| fleet.world_id);
            let finished = inner.fleet.as_mut().is_some_and(fleet::Fleet::finished);
            (inner.status.phase, finished, world)
        };
        match phase {
            Phase::ConnectingAi if live_ai == ai => {
                tracing::info!(
                    ai,
                    seconds = started.elapsed().as_secs_f64(),
                    "skirmish AI connected"
                );
                skirmish.advance(id, Phase::Ready);
            }
            Phase::ConnectingAi if finished => {
                skirmish.fail(
                    id,
                    Phase::Failed,
                    Some("the AI hosts exited before every AI player connected".to_owned()),
                );
                return;
            }
            Phase::ConnectingAi if started.elapsed() > CONNECT_LIMIT => {
                let error = format!(
                    "{live_ai} of {ai} AI players connected within {} minutes",
                    CONNECT_LIMIT.as_secs() / 60
                );
                skirmish.fail(id, Phase::Failed, Some(error));
                return;
            }
            Phase::ConnectingAi => {}
            // Without opponents there is nothing to stop.
            Phase::Ready if world.is_none() => return,
            Phase::Ready => {
                absent_since = if human {
                    None
                } else {
                    absent_since.or(Some(Instant::now()))
                };
                let replaced = world.is_some_and(|world| world != gameplay.world_id());
                let abandoned = absent_since.is_some_and(|since| since.elapsed() > ABANDONED_AFTER);
                if replaced || abandoned || finished {
                    tracing::info!(
                        replaced,
                        abandoned,
                        finished,
                        "skirmish over; stopping its AI"
                    );
                    let fleet = {
                        let mut inner = skirmish.lock();
                        if inner.status.id == id && inner.status.phase == Phase::Ready {
                            inner.status.phase = Phase::Stopped;
                        }
                        inner.fleet.take()
                    };
                    drop(fleet);
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            _ => return,
        }
    }
}

pub(super) fn routes() -> axum::Router<AppState> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route("/skirmish", get(get_status).post(start))
        .route("/skirmish/stop", post(stop))
}

#[cfg(test)]
mod tests;
