//! Untrusted client CPU/cadence observations, kept separately from tick timings.
use crate::AppState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const RATE_WINDOW: Duration = Duration::from_secs(60);
const REPORTS_PER_WINDOW: usize = 4;

// Monotonic client sequence with a random per-page suffix.
type Sequence = (u64, u32);

type Quantiles = [f64; 3];
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    sequence: Sequence,
    pub frames: u32,
    frame_ms: Quantiles,
    pub units_in_view: u32,
    pub still: Cadence,
    pub panning: Cadence,
    stages_ms: Stages,
    draw_calls: Quantiles,
    renderer: String,
    contour: Contour,
}
/// Sparse frame-interval histogram. Bins 0–100 have 1 ms resolution,
/// 101–136 have 25 ms resolution, and 137 covers the remaining tail.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Cadence {
    pub frames: u32,
    pub max_ms: f64,
    pub over_50_ms: u32,
    pub over_100_ms: u32,
    pub histogram: Vec<(u8, u32)>,
}

impl Cadence {
    /// Upper edge of a cadence bucket, limited by the observed maximum.
    pub fn upper_ms(bucket: u8, max: f64) -> f64 {
        let upper = match bucket {
            0..=100 => f64::from(bucket),
            101..=136 => 100.0 + f64::from(bucket - 100) * 25.0,
            _ => max,
        };
        upper.min(max)
    }

    fn valid(&self) -> bool {
        if !self.max_ms.is_finite() || self.max_ms < 0.0 {
            return false;
        }
        let max_bin = if self.max_ms <= 100.0 {
            self.max_ms.ceil() as u8
        } else {
            (100.0 + ((self.max_ms - 100.0) / 25.0).ceil()).min(137.0) as u8
        };
        let mut previous = None;
        let (mut total, mut over_50, mut over_100) = (0_u64, 0_u64, 0_u64);
        for &(bin, count) in &self.histogram {
            if count == 0 || bin > max_bin || previous.is_some_and(|previous| bin <= previous) {
                return false;
            }
            previous = Some(bin);
            total += u64::from(count);
            if bin > 50 {
                over_50 += u64::from(count);
            }
            if bin > 100 {
                over_100 += u64::from(count);
            }
        }
        previous == (total > 0).then_some(max_bin)
            && (total > 0 || self.max_ms == 0.0)
            && total == u64::from(self.frames)
            && over_50 == u64::from(self.over_50_ms)
            && over_100 == u64::from(self.over_100_ms)
    }
}
/// Why a unit silhouette may be missing: cumulative frame counts since page
/// load (effects on vs off), the last frame's eligible/occluder instances and
/// depth span, and WebGL2 context loss/restore counters.
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Contour {
    effect_frames: u32,
    plain_frames: u32,
    eligible_instances: u32,
    occluder_instances: u32,
    depth_span: f64,
    context_lost: u32,
    context_restored: u32,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Stages {
    update: Quantiles,
    interpolation: Quantiles,
    culling: Quantiles,
    terrain: Quantiles,
    sprites: Quantiles,
    ui: Quantiles,
    fog: Quantiles,
}
impl Report {
    pub fn valid(&self) -> bool {
        let s = &self.stages_ms;
        (1..=65_536).contains(&self.frames)
            && self.frames == self.still.frames.saturating_add(self.panning.frames)
            && self.still.valid()
            && self.panning.valid()
            && self.frame_ms[0].is_finite()
            && self.frame_ms[0] >= 0.0
            && self.frame_ms[0] <= self.frame_ms[1]
            && self.frame_ms[1] <= self.frame_ms[2]
            && self.frame_ms[2] == self.still.max_ms.max(self.panning.max_ms)
            && matches!(self.renderer.as_str(), "webgl2" | "webgpu" | "canvas2d")
            && self.contour.depth_span.is_finite()
            && self.contour.depth_span >= 0.0
            && [
                self.draw_calls,
                s.update,
                s.interpolation,
                s.culling,
                s.terrain,
                s.sprites,
                s.ui,
                s.fog,
            ]
            .into_iter()
            .all(|q| {
                q.iter()
                    .all(|n| n.is_finite() && (0.0..=600_000.0).contains(n))
                    && q[0] <= q[1]
                    && q[1] <= q[2]
            })
    }
}
struct Entry {
    at: Instant,
    report: Report,
}

struct Attempt {
    at: Instant,
    pending: Option<Sequence>,
}

#[derive(Default)]
struct SeatReports {
    // Retain accepted identities for this match so a delayed duplicate cannot
    // be journaled again. Only accepted reports consume this long-lived space.
    accepted: HashSet<Sequence>,
    attempts: [Option<Attempt>; REPORTS_PER_WINDOW],
    latest: Option<Entry>,
}

impl SeatReports {
    fn finish_attempt(&mut self, sequence: Sequence) {
        if let Some(attempt) = self
            .attempts
            .iter_mut()
            .flatten()
            .find(|attempt| attempt.pending == Some(sequence))
        {
            attempt.pending = None;
        }
    }
}

#[derive(Default)]
pub(crate) struct Reports {
    world_id: u64,
    // Only authenticated human seat tokens from this match can reach admit().
    by_session: HashMap<[u8; 24], SeatReports>,
}

impl Reports {
    fn admit(
        &mut self,
        token: aoe_protocol::ResumeToken,
        sequence: Sequence,
        world_id: u64,
        now: Instant,
    ) -> Result<(), StatusCode> {
        if self.world_id != world_id {
            *self = Self {
                world_id,
                ..Self::default()
            };
        }
        let seat = self.by_session.entry(token.0).or_default();
        if seat.accepted.contains(&sequence) {
            return Err(StatusCode::CONFLICT);
        }
        if seat
            .attempts
            .iter()
            .flatten()
            .any(|attempt| attempt.pending == Some(sequence))
        {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        let slot = seat
            .attempts
            .iter_mut()
            .find(|slot| {
                slot.as_ref().is_none_or(|attempt| {
                    attempt.pending.is_none() && now.duration_since(attempt.at) >= RATE_WINDOW
                })
            })
            .ok_or(StatusCode::TOO_MANY_REQUESTS)?;
        *slot = Some(Attempt {
            at: now,
            pending: Some(sequence),
        });
        Ok(())
    }

    fn record(
        &mut self,
        token: aoe_protocol::ResumeToken,
        report: Report,
        tick: u64,
        now: Instant,
    ) {
        let Some(seat) = self.by_session.get_mut(&token.0) else {
            return;
        };
        seat.finish_attempt(report.sequence);
        seat.accepted.insert(report.sequence);
        // Never log session secrets or trust client timing as server performance.
        tracing::info!(tick, frames = report.frames, frame_ms = ?report.frame_ms,
            stages_ms = ?serde_json::to_value(&report.stages_ms).ok(),
            draw_calls = ?report.draw_calls, renderer = %report.renderer,
            contour = ?serde_json::to_value(&report.contour).ok(), "client frame summary");
        if seat.latest.as_ref().is_none_or(|entry| entry.at <= now) {
            seat.latest = Some(Entry { at: now, report });
        }
    }

    pub(crate) fn recent(&self, world_id: u64) -> serde_json::Value {
        if self.world_id != world_id {
            return serde_json::json!([]);
        }
        let now = Instant::now();
        let mut entries: Vec<_> = self
            .by_session
            .values()
            .filter_map(|seat| seat.latest.as_ref())
            .filter(|entry| now.duration_since(entry.at) < Duration::from_secs(120))
            .collect();
        entries.sort_unstable_by_key(|entry| entry.at);
        serde_json::to_value(
            entries
                .iter()
                .map(|entry| &entry.report)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default()
    }
}
pub(super) async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(report): Json<Report>,
) -> Result<StatusCode, StatusCode> {
    receive_at(state, headers, report, Instant::now()).await
}

async fn receive_at(
    state: AppState,
    headers: HeaderMap,
    report: Report,
    now: Instant,
) -> Result<StatusCode, StatusCode> {
    let token = crate::maps::controller_token(&headers).ok_or(StatusCode::FORBIDDEN)?;
    let active = state.gameplay.read().await;
    let gameplay = active.clone();
    if !gameplay.is_human_token(token) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !report.valid() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let world_id = gameplay.world_id();
    state
        .client_perf
        .lock()
        .await
        .admit(token, report.sequence, world_id, now)?;
    drop(active);

    // Complete the reservation even if the browser disconnects mid-write.
    tokio::spawn(async move {
        let (tick, _) = gameplay.tick_and_units().await;
        let result = super::super::append_client(&state, &gameplay, tick, report.clone()).await;
        let mut reports = state.client_perf.lock().await;
        match result {
            Ok(()) => {
                if reports.world_id == world_id {
                    reports.record(token, report, tick, now);
                }
                Ok(StatusCode::NO_CONTENT)
            }
            Err(error) => {
                if reports.world_id == world_id
                    && let Some(seat) = reports.by_session.get_mut(&token.0)
                {
                    seat.finish_attempt(report.sequence);
                }
                tracing::warn!(world_id, %error, "could not append client telemetry");
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
}

#[cfg(test)]
mod tests;
