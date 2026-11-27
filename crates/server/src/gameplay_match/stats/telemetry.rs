//! Match-local minute records. File I/O runs outside the tick clock.
use super::{TickStats, summary::phase_timings};
use crate::{AppState, GameplayService};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const MINUTE: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MatchTelemetryRow {
    Server {
        elapsed_ms: u64,
        window_ms: u64,
        tick: u64,
        ticks: u32,
        ticks_per_second: f64,
        units: usize,
        phases_us: BTreeMap<String, Value>,
        rust_ai: Value,
    },
    Client {
        elapsed_ms: u64,
        /// Server tick observed when this report arrived.
        tick: u64,
        report: Box<super::super::admin::client_perf::Report>,
    },
}

#[derive(Default)]
pub(crate) struct MatchTelemetry {
    started: OnceLock<Instant>,
    writer: Mutex<()>,
    latest: Mutex<Option<Value>>,
}

impl MatchTelemetry {
    pub(crate) fn latest(&self) -> Option<Value> {
        self.latest
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn directory(root: &Path, world_id: u64) -> PathBuf {
    root.join("matches").join(format!("{world_id:016x}"))
}

fn elapsed_ms(start: Instant, end: Instant) -> u64 {
    end.duration_since(start).as_millis() as u64
}

async fn append(
    state: &AppState,
    gameplay: &GameplayService,
    row: MatchTelemetryRow,
) -> io::Result<()> {
    let Some(root) = &state.map_package_directory else {
        return Ok(());
    };
    let directory = directory(root, gameplay.world_id());
    let telemetry = gameplay.telemetry.clone();
    tokio::task::spawn_blocking(move || {
        let mut line = serde_json::to_vec(&row)?;
        line.push(b'\n');
        let _guard = telemetry.writer.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(&directory)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("telemetry.jsonl"))?;
        file.write_all(&line)
    })
    .await?
}

pub(crate) async fn append_client(
    state: &AppState,
    gameplay: &GameplayService,
    tick: u64,
    report: super::super::admin::client_perf::Report,
) -> io::Result<()> {
    let now = Instant::now();
    let started = *gameplay.telemetry.started.get_or_init(|| now);
    append(
        state,
        gameplay,
        MatchTelemetryRow::Client {
            elapsed_ms: elapsed_ms(started, now),
            tick,
            report: Box::new(report),
        },
    )
    .await
}

impl TickStats {
    fn copy_minute(&self, world_id: u64, start: Instant, end: Instant) -> Vec<super::TickRecord> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.world_id != world_id {
            return Vec::new();
        }
        inner
            .records
            .iter()
            .zip(&inner.times)
            .rev()
            .skip_while(|(_, time)| **time >= end)
            .take_while(|(_, time)| **time >= start)
            .map(|(record, _)| record.clone())
            .collect()
    }
}

async fn write_minute(
    state: &AppState,
    gameplay: &GameplayService,
    start: Instant,
    end: Instant,
) -> io::Result<()> {
    let rows = state
        .tick_stats
        .copy_minute(gameplay.world_id(), start, end);
    let (tick, units) = if let Some(row) = rows.first() {
        (row.0, row.5 as usize)
    } else {
        gameplay.tick_and_units().await
    };
    let ticks = rows.len() as u32;
    let window_ms = elapsed_ms(start, end);
    let started = *gameplay.telemetry.started.get_or_init(|| start);
    let elapsed_ms = elapsed_ms(started, end);
    let ticks_per_second = f64::from(ticks) / end.duration_since(start).as_secs_f64();
    let phases_us = phase_timings(&rows)
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    let row = MatchTelemetryRow::Server {
        elapsed_ms,
        window_ms,
        tick,
        ticks,
        ticks_per_second,
        units,
        phases_us,
        rust_ai: gameplay.rust_ai_perf(),
    };
    let latest = serde_json::to_value(&row)?;
    append(state, gameplay, row).await?;
    *gameplay
        .telemetry
        .latest
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(latest);
    Ok(())
}

impl AppState {
    /// Records completed wall-clock minutes for the currently installed match.
    pub async fn run_match_telemetry(self) {
        if self.map_package_directory.is_none() {
            return;
        }
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut tracked = None;
        loop {
            timer.tick().await;
            let gameplay = self.gameplay.read().await.clone();
            if gameplay.map_hash().is_none() && gameplay.telemetry.started.get().is_none() {
                continue;
            }
            let now = Instant::now();
            let started = *gameplay.telemetry.started.get_or_init(|| now);
            let (world_id, start) = tracked
                .filter(|(id, _)| *id == gameplay.world_id())
                .unwrap_or((gameplay.world_id(), started));
            let end = start + MINUTE;
            if now >= end {
                if let Err(error) = write_minute(&self, &gameplay, start, end).await {
                    tracing::warn!(world_id, %error, "could not append match telemetry minute");
                }
                tracked = Some((world_id, end));
            }
        }
    }

    pub(crate) async fn latest_match_telemetry(&self, gameplay: &GameplayService) -> Value {
        json!({
            "world_id": format!("{:016x}", gameplay.world_id()),
            "server": gameplay.telemetry.latest(),
            "clients": self.client_perf.lock().await.recent(gameplay.world_id()),
        })
    }
}

#[cfg(test)]
mod tests;
