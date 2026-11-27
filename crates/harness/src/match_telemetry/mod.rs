//! Pool observed counts in ten-minute windows of match wall time.
use aoe_server::{FrameCadence, MatchTelemetryRow};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

const WINDOW_MS: u64 = 600_000;

#[derive(Default)]
struct Window {
    server_ms: u64,
    ticks: u64,
    latest_units: Option<(u64, usize)>,
    client_reports: u64,
    still: FrameTotals,
    panning: FrameTotals,
}

#[derive(Default)]
struct FrameTotals {
    bins: BTreeMap<u8, u64>,
    frames: u64,
    max_ms: f64,
    over_50_ms: u64,
    over_100_ms: u64,
}

#[derive(Debug, Serialize)]
struct FrameSummary {
    frames: u64,
    p99_ms: Option<f64>,
    max_ms: Option<f64>,
    over_50_ms: u64,
    over_100_ms: u64,
}

impl FrameTotals {
    fn add(&mut self, report: FrameCadence) {
        self.frames += u64::from(report.frames);
        self.max_ms = self.max_ms.max(report.max_ms);
        self.over_50_ms += u64::from(report.over_50_ms);
        self.over_100_ms += u64::from(report.over_100_ms);
        for (bucket, count) in report.histogram {
            *self.bins.entry(bucket).or_default() += u64::from(count);
        }
    }

    fn summary(&self) -> FrameSummary {
        let threshold = self.frames - self.frames / 100;
        let mut cumulative = 0;
        let p99_ms = self.bins.iter().find_map(|(&bucket, &count)| {
            cumulative += count;
            (cumulative >= threshold).then(|| FrameCadence::upper_ms(bucket, self.max_ms))
        });
        FrameSummary {
            frames: self.frames,
            p99_ms,
            max_ms: (self.frames > 0).then_some(self.max_ms),
            over_50_ms: self.over_50_ms,
            over_100_ms: self.over_100_ms,
        }
    }
}

#[derive(Debug, Serialize)]
struct Summary {
    start_minute: u64,
    end_minute: u64,
    server_seconds: f64,
    ticks_per_second: Option<f64>,
    units: Option<usize>,
    client_reports: u64,
    still: FrameSummary,
    panning: FrameSummary,
}

fn summarize(reader: impl BufRead) -> io::Result<Vec<Summary>> {
    let mut windows = BTreeMap::<u64, Window>::new();
    for (index, line) in reader.lines().enumerate() {
        let invalid = |msg: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {}: {msg}", index + 1),
            )
        };
        let row: MatchTelemetryRow =
            serde_json::from_str(&line?).map_err(|error| invalid(&error.to_string()))?;
        let elapsed_ms = match &row {
            MatchTelemetryRow::Server { elapsed_ms, .. }
            | MatchTelemetryRow::Client { elapsed_ms, .. } => *elapsed_ms,
        };
        let window = windows
            .entry(elapsed_ms.saturating_sub(1) / WINDOW_MS)
            .or_default();
        match row {
            MatchTelemetryRow::Server {
                window_ms,
                ticks,
                units,
                ..
            } => {
                if window_ms == 0 || window_ms > elapsed_ms || window_ms > 60_000 {
                    return Err(invalid("invalid server sampling duration"));
                }
                window.server_ms += window_ms;
                window.ticks += u64::from(ticks);
                if window
                    .latest_units
                    .is_none_or(|(latest, _)| elapsed_ms >= latest)
                {
                    window.latest_units = Some((elapsed_ms, units));
                }
            }
            MatchTelemetryRow::Client { report, .. } => {
                if !report.valid() {
                    return Err(invalid("invalid client timing report"));
                }
                window.client_reports += 1;
                window.still.add(report.still);
                window.panning.add(report.panning);
            }
        }
    }
    Ok(windows
        .into_iter()
        .map(|(index, window)| {
            let server_seconds = window.server_ms as f64 / 1000.0;
            Summary {
                start_minute: index * 10,
                end_minute: (index + 1) * 10,
                server_seconds,
                ticks_per_second: (window.server_ms > 0)
                    .then(|| window.ticks as f64 / server_seconds),
                units: window.latest_units.map(|(_, units)| units),
                client_reports: window.client_reports,
                still: window.still.summary(),
                panning: window.panning.summary(),
            }
        })
        .collect())
}

pub(crate) fn run(path: &Path) -> io::Result<()> {
    let windows = summarize(BufReader::new(File::open(path)?))?;
    serde_json::to_writer_pretty(io::stdout().lock(), &windows)?;
    println!();
    Ok(())
}

#[cfg(test)]
mod tests;
