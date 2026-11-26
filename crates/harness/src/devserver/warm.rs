//! Warming the maps a start will open. The server computes a map's minimap
//! raster on the first request, under a 30 s budget (`MINIMAP_TIME_BUDGET`,
//! crates/server/src/maps/minimap.rs), and answers 503 when it runs out; the
//! client takes that for a failed raster and never asks again, so a skirmish
//! started on a freshly started or loaded server stalls on the loading screen
//! until its own terrain timeout. Measured on a restarted dev server at load
//! 8: the first request took 21.7 s, the next 2 ms. A map is therefore only
//! ready once its raster has been served, which is what this module does
//! before a take starts or `make dev-server` reports.
use super::process;
use serde_json::Value;
use std::time::{Duration, Instant};

/// Time allowed for one map's raster, over every retry.
pub(crate) const BUDGET: Duration = Duration::from_secs(10 * 60);
/// One request may run as long as the server's own budget plus its slack.
const REQUEST: Duration = Duration::from_secs(120);
pub(crate) const RETRY: Duration = Duration::from_secs(2);

/// The fields that name a map in a request: its centre and its side.
fn key(request: &Value) -> [Option<&Value>; 3] {
    [
        "center_latitude_e7",
        "center_longitude_e7",
        "requested_side_meters",
    ]
    .map(|name| request.get(name))
}

/// The content hash of each completed job in `jobs` (the body of
/// `GET /maps/jobs`) whose request names the same map as `wanted`, once each.
pub(crate) fn completed(jobs: &str, wanted: &Value) -> Vec<String> {
    let jobs: Vec<Value> = serde_json::from_str(jobs).unwrap_or_default();
    let mut hashes: Vec<String> = Vec::new();
    for job in &jobs {
        let done = job.get("state").and_then(Value::as_str) == Some("completed");
        let same = job.get("request").is_some_and(|r| key(r) == key(wanted));
        if let (true, true, Some(hash)) =
            (done, same, job.get("content_hash").and_then(Value::as_str))
            && !hashes.iter().any(|known| known == hash)
        {
            hashes.push(hash.to_owned());
        }
    }
    hashes
}

/// The time the warm-up reads and spends. The tests replace it, so that a slow
/// raster is a jump of the clock and a retry pause costs no real time: what
/// they check then holds on any machine, however loaded.
pub(crate) trait Clock {
    fn now(&self) -> Instant;
    fn pause(&self, time: Duration);
}

pub(crate) struct Wall;

impl Clock for Wall {
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn pause(&self, time: Duration) {
        std::thread::sleep(time);
    }
}

/// Request the raster of one map until the server has served it, retrying a
/// 503 (budget spent) or silence; any other status is final.
pub(crate) fn raster(port: u16, hash: &str, budget: Duration) -> Result<Duration, String> {
    raster_on(&Wall, port, hash, budget)
}

pub(crate) fn raster_on(
    clock: &dyn Clock,
    port: u16,
    hash: &str,
    budget: Duration,
) -> Result<Duration, String> {
    let started = clock.now();
    let elapsed = || clock.now().saturating_duration_since(started);
    let path = format!("/maps/{hash}/minimap");
    let mut last = "no answer".to_owned();
    while elapsed() < budget {
        let patience = REQUEST.min(budget.saturating_sub(elapsed()));
        match process::http_status(port, &path, patience) {
            Some(200) => return Ok(elapsed()),
            Some(503) => last = "503 (its time budget ran out)".into(),
            Some(status) => {
                return Err(format!("map {hash}: GET {path} answered {status}"));
            }
            None => last = "no answer in time".into(),
        }
        clock.pause(RETRY.min(budget.saturating_sub(elapsed())));
    }
    Err(format!(
        "map {hash}: the minimap raster was not served within {} s (last: {last})",
        budget.as_secs()
    ))
}

/// Warm the raster of every completed map `wanted` names on `port`; returns
/// the hashes it warmed, each with how long the first request took. Maps with
/// no completed job yet are left: the start will prepare them itself.
pub(crate) fn maps(
    port: u16,
    wanted: &[Value],
    budget: Duration,
) -> Result<Vec<(String, Duration)>, String> {
    let jobs = process::http_get(port, "/maps/jobs").unwrap_or_default();
    let mut warmed: Vec<(String, Duration)> = Vec::new();
    for want in wanted {
        for hash in completed(&jobs, want) {
            if warmed.iter().all(|(known, _)| *known != hash) {
                let took = raster(port, &hash, budget)?;
                warmed.push((hash, took));
            }
        }
    }
    Ok(warmed)
}
