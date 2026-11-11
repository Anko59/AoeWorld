//! Untrusted client CPU/cadence observations, kept separately from tick timings.
use crate::AppState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

type Quantiles = [f64; 3];
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Report {
    frames: u32,
    frame_ms: Quantiles,
    stages_ms: Stages,
    draw_calls: Quantiles,
    renderer: String,
    contour: Contour,
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
    fn valid(&self) -> bool {
        let s = &self.stages_ms;
        (1..=2048).contains(&self.frames)
            && matches!(self.renderer.as_str(), "webgl2" | "webgpu" | "canvas2d")
            && self.contour.depth_span.is_finite()
            && self.contour.depth_span >= 0.0
            && [
                self.frame_ms,
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
    token: aoe_protocol::ResumeToken,
    at: Instant,
    report: Report,
}
#[derive(Default)]
pub(crate) struct Reports(VecDeque<Entry>);
impl Reports {
    fn record(
        &mut self,
        token: aoe_protocol::ResumeToken,
        report: Report,
        tick: u64,
    ) -> Result<(), StatusCode> {
        let now = Instant::now();
        if let Some(index) = self.0.iter().position(|e| e.token == token) {
            if now.duration_since(self.0[index].at) < Duration::from_secs(2) {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            self.0.remove(index);
        }
        if self.0.len() == 64 {
            // Do not evict a token while its rate window is still live:
            // cycling 65 sessions must not bypass the per-token limit.
            if self
                .0
                .front()
                .is_some_and(|e| now.duration_since(e.at) < Duration::from_secs(2))
            {
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
            self.0.pop_front();
        }
        // Never log session secrets or trust client timing as server performance.
        tracing::info!(tick, frames = report.frames, frame_ms = ?report.frame_ms,
            stages_ms = ?serde_json::to_value(&report.stages_ms).ok(),
            draw_calls = ?report.draw_calls, renderer = %report.renderer,
            contour = ?serde_json::to_value(&report.contour).ok(), "client frame summary");
        self.0.push_back(Entry {
            token,
            at: now,
            report,
        });
        Ok(())
    }
    pub(crate) fn recent(&self) -> serde_json::Value {
        serde_json::to_value(
            self.0
                .iter()
                .filter(|e| e.at.elapsed() < Duration::from_secs(30))
                .map(|e| &e.report)
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
    let token = crate::maps::controller_token(&headers).ok_or(StatusCode::FORBIDDEN)?;
    let gameplay = state.gameplay.read().await.clone();
    if !gameplay.is_human_token(token) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !report.valid() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (tick, _) = gameplay.tick_and_units().await;
    state.client_perf.lock().await.record(token, report, tick)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
