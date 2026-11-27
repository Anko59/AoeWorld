//! Untrusted client CPU/cadence observations, kept separately from tick timings.
use crate::AppState;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const REPORT_RATE: Duration = Duration::from_secs(60);

type Quantiles = [f64; 3];
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
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

#[derive(Default)]
pub(crate) struct Reports {
    world_id: u64,
    // Only authenticated human seat tokens from this match can reach record().
    by_session: HashMap<[u8; 24], Entry>,
}

impl Reports {
    fn record(
        &mut self,
        token: aoe_protocol::ResumeToken,
        report: Report,
        tick: u64,
        world_id: u64,
    ) -> Result<(), StatusCode> {
        let now = Instant::now();
        if self.world_id != world_id {
            *self = Self {
                world_id,
                ..Self::default()
            };
        }
        if self
            .by_session
            .get(&token.0)
            .is_some_and(|entry| now.duration_since(entry.at) < REPORT_RATE)
        {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        // Never log session secrets or trust client timing as server performance.
        tracing::info!(tick, frames = report.frames, frame_ms = ?report.frame_ms,
            stages_ms = ?serde_json::to_value(&report.stages_ms).ok(),
            draw_calls = ?report.draw_calls, renderer = %report.renderer,
            contour = ?serde_json::to_value(&report.contour).ok(), "client frame summary");
        self.by_session.insert(token.0, Entry { at: now, report });
        Ok(())
    }

    pub(crate) fn recent(&self, world_id: u64) -> serde_json::Value {
        if self.world_id != world_id {
            return serde_json::json!([]);
        }
        let now = Instant::now();
        let mut entries: Vec<_> = self
            .by_session
            .values()
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
    let token = crate::maps::controller_token(&headers).ok_or(StatusCode::FORBIDDEN)?;
    let active = state.gameplay.read().await;
    let gameplay = active.clone();
    if !gameplay.is_human_token(token) {
        return Err(StatusCode::FORBIDDEN);
    }
    if !report.valid() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (tick, _) = gameplay.tick_and_units().await;
    state
        .client_perf
        .lock()
        .await
        .record(token, report.clone(), tick, gameplay.world_id())?;
    drop(active);
    if let Err(error) = super::super::append_client(&state, &gameplay, tick, report).await {
        tracing::warn!(world_id = gameplay.world_id(), %error, "could not append client telemetry");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
