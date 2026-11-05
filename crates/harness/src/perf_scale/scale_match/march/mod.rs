//! `--march-probe`: slot 0 played by a scripted human through the SDK's
//! client (the browser's wire protocol). It scatters its army, waits until a
//! given wall minute so the AIs' route demand is real, then selects mixed
//! groups as they stand and sends them `FormationOrder`s, as the browser
//! does, to far destinations that a route probe proved reachable. Every
//! member is followed until the run ends. See `docs/scale/match.md`.
mod orders;
mod table;
mod track;

use super::{BoxError, Options, http};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) use table::print_table;

/// Groups of the first run: (size, straight distance in tiles, with siege).
pub(super) const PLAN: [(usize, i32, bool); 11] = [
    (40, 200, false),
    (150, 200, false),
    (400, 200, false),
    (40, 500, false),
    (150, 500, false),
    (400, 500, false),
    (40, 1000, false),
    (150, 1000, false),
    (400, 1000, false),
    (40, 200, true),
    (40, 500, true),
];

/// Slot 0's seeded army for `PLAN` (`start_units.probe_army`).
pub(super) fn probe_army() -> Value {
    json!({"militia": 820, "archers": 560, "knights": 470, "rams": 40, "mangonels": 40})
}

/// Admin access to the server the probe plays on.
#[derive(Clone)]
pub(super) struct Admin {
    pub address: String,
    pub token: &'static str,
}

impl Admin {
    pub(super) fn get(&self, path: &str) -> Result<Value, BoxError> {
        self.call("GET", path, None)
    }

    pub(super) fn post(&self, path: &str, body: &Value) -> Result<Value, BoxError> {
        self.call("POST", path, Some(serde_json::to_vec(body)?))
    }

    fn call(&self, method: &str, path: &str, body: Option<Vec<u8>>) -> Result<Value, BoxError> {
        let response = http::request(
            &self.address,
            method,
            path,
            &[("X-AoeWorld-Admin-Token", self.token)],
            body.as_deref(),
            Duration::from_secs(120),
        )?;
        if response.status != 200 {
            return Err(format!(
                "{method} {path}: {} {}",
                response.status,
                String::from_utf8_lossy(&response.body)
            )
            .into());
        }
        Ok(serde_json::from_slice(&response.body)?)
    }
}

pub(super) struct Probe {
    handle: JoinHandle<Value>,
    stop: Arc<AtomicBool>,
}

impl Probe {
    /// Ends the observation and returns the probe's report.
    pub(super) fn finish(self) -> Value {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .join()
            .unwrap_or_else(|_| json!({"error": "the march probe panicked"}))
    }
}

pub(super) struct Settings {
    pub url: String,
    pub token: String,
    pub admin: Admin,
    pub march_after: Duration,
    pub scatter: i32,
    pub seed: u64,
    pub ai: bool,
}

pub(super) fn settings(options: &Options, admin: Admin, token: String) -> Settings {
    Settings {
        url: format!("http://{}", admin.address),
        token,
        admin,
        march_after: Duration::from_secs_f64(options.march_minute * 60.0),
        scatter: options.march_scatter,
        seed: options.seed,
        ai: !options.march_no_ai,
    }
}

pub(super) fn start(settings: Settings) -> Probe {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => return json!({"error": error.to_string()}),
        };
        runtime
            .block_on(orders::run(settings, flag))
            .unwrap_or_else(|error| json!({"error": error.to_string()}))
    });
    Probe { handle, stop }
}

/// Seconds since `start`, for the probe's wall stamps.
pub(super) fn wall(start: Instant) -> f64 {
    start.elapsed().as_secs_f64()
}
