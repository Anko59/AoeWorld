//! Request-time quantiles; recording retains only bounded samples and timestamps.
use super::{TickRecord, TickStats};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);
const WINDOW_RECORDS: usize = 1200;

impl TickStats {
    pub(crate) fn summary(&self, period: Duration) -> Value {
        let now = Instant::now();
        let (records, times, missed_total) = {
            let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
            let pairs = inner
                .records
                .iter()
                .zip(&inner.times)
                .rev()
                .take(WINDOW_RECORDS)
                .take_while(|(_, time)| now.duration_since(**time) < WINDOW)
                .collect::<Vec<_>>();
            (
                pairs
                    .iter()
                    .map(|(record, _)| (*record).clone())
                    .collect::<Vec<_>>(),
                pairs.iter().map(|(_, time)| **time).collect::<Vec<_>>(),
                inner.missed,
            )
        };
        let samples = records.len();
        let mut columns = BTreeMap::<&str, Vec<u64>>::new();
        for TickRecord(_, total, sim, late, _, _, _, planning, phases) in &records {
            for (name, value) in [
                ("total", *total),
                ("advance", *sim),
                ("late", *late),
                ("planning", *planning),
            ] {
                columns.entry(name).or_default().push(value);
            }
            for (name, value) in phases.values() {
                columns.entry(name).or_default().push(value);
            }
        }
        let mut phases = BTreeMap::new();
        for (name, mut values) in columns {
            values.sort_unstable();
            let percentile =
                |percent: usize| values[(values.len() * percent).div_ceil(100).saturating_sub(1)];
            phases.insert(
                name,
                json!({"p50_us":percentile(50),"p95_us":percentile(95),"max_us":values.last()}),
            );
        }
        let mut top = phases
            .iter()
            .filter(|(name, _)| !["total", "advance", "late", "planning"].contains(name))
            .map(|(name, values)| {
                let mut entry = values.clone();
                entry["name"] = json!(name);
                entry
            })
            .collect::<Vec<_>>();
        top.sort_by(|a, b| {
            b["p95_us"]
                .as_u64()
                .cmp(&a["p95_us"].as_u64())
                .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
        });
        top.truncate(5);
        let elapsed = times
            .first()
            .zip(times.last())
            .map_or(0.0, |(last, first)| {
                last.duration_since(*first).as_secs_f64()
            });
        let rate = if elapsed > 0.0 && samples > 1 {
            (samples - 1) as f64 / elapsed
        } else {
            0.0
        };
        json!({"window_seconds":WINDOW.as_secs(),"samples":samples,"tick_hz":1.0/period.as_secs_f64(),
               "tick_rate":rate,"missed_ticks":records.iter().map(|r|u64::from(r.6)).sum::<u64>(),
               "missed_total":missed_total,"phases":phases,"top_phases":top})
    }
}
