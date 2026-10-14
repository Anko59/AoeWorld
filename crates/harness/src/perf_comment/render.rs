//! The comment body: one compact Markdown table under a hidden marker, so a
//! later CI run rewrites the same comment instead of adding one.
use super::reports::{Pressure, Report, Timing};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(crate) const MARKER: &str = "<!-- aoe-perf-comment -->";
const NOISE_FILE: &str = "crates/harness/src/perf_comment/noise.json";

/// The noise band a timing value must exceed to appear in the table.
#[derive(Deserialize)]
pub(crate) struct Noise {
    percent: u64,
}

impl Noise {
    pub(crate) fn load() -> Result<Self, String> {
        serde_json::from_str(include_str!("noise.json"))
            .map_err(|error| format!("{NOISE_FILE}: {error}"))
    }

    /// Beyond the band: the change is larger than `percent` of the old value.
    /// A zero old value moves as soon as the new value differs.
    pub(crate) fn moved(&self, before: Option<u64>, after: Option<u64>) -> bool {
        match (before, after) {
            (Some(before), Some(after)) if before != after => {
                u128::from(after.abs_diff(before)) * 100
                    > u128::from(before) * u128::from(self.percent)
            }
            _ => false,
        }
    }

    pub(crate) fn percent(&self) -> u64 {
        self.percent
    }
}

struct Row {
    label: String,
    dev: Option<u64>,
    pr: Option<u64>,
    note: String,
}

fn value(value: Option<u64>) -> String {
    value.map_or_else(|| "—".to_owned(), |v| v.to_string())
}

fn delta(before: Option<u64>, after: Option<u64>) -> String {
    let (Some(before), Some(after)) = (before, after) else {
        return "—".to_owned();
    };
    let difference = after as i64 - before as i64;
    let sign = if difference < 0 { '−' } else { '+' };
    let percent = if before > 0 {
        format!(
            " ({sign}{:.1}%)",
            difference.abs() as f64 * 100.0 / before as f64
        )
    } else {
        String::new()
    };
    format!("{sign}{}{percent}", difference.abs())
}

fn deterministic(label: &str, dev: Option<u64>, pr: Option<u64>, note: &str) -> Row {
    Row {
        label: label.to_owned(),
        dev,
        pr,
        note: note.to_owned(),
    }
}

/// A timing row: same shape as a deterministic row, and shown only when the
/// change moved beyond the noise band.
fn timed(noise: &Noise, label: String, dev: Option<u64>, pr: Option<u64>) -> Option<Row> {
    noise.moved(dev, pr).then(|| Row {
        label,
        dev,
        pr,
        note: "timing, noisy runner".to_owned(),
    })
}

type TimingPick = fn(&Timing) -> Option<u64>;
type PressurePick = fn(&Pressure) -> Option<u64>;

fn tick_picks() -> [(&'static str, TimingPick); 2] {
    [
        ("p50", |timing: &Timing| timing.p50),
        ("p95", |timing: &Timing| timing.p95),
    ]
}

fn pressure_picks() -> [(&'static str, PressurePick); 4] {
    [
        ("snapshot latency p50 (µs)", |pressure: &Pressure| {
            pressure.snapshot_latency_us.p50
        }),
        ("snapshot latency p95 (µs)", |pressure: &Pressure| {
            pressure.snapshot_latency_us.p95
        }),
        ("max response backlog", |pressure: &Pressure| {
            pressure.max_response_backlog
        }),
        ("missed offer deadlines", |pressure: &Pressure| {
            pressure.missed_offer_deadlines
        }),
    ]
}

fn rows(dev: Option<&Report>, pr: Option<&Report>, cap: Option<u64>, noise: &Noise) -> Vec<Row> {
    let mut out = Vec::new();
    let note = match cap {
        Some(cap) => format!("deterministic; cap {cap}"),
        None => "deterministic; cap unknown".to_owned(),
    };
    out.push(deterministic(
        "Optimized WASM raw (bytes)",
        dev.and_then(|r| r.wasm_raw),
        pr.and_then(|r| r.wasm_raw),
        "deterministic",
    ));
    out.push(deterministic(
        "Optimized WASM gzip (bytes)",
        dev.and_then(|r| r.wasm_gzip),
        pr.and_then(|r| r.wasm_gzip),
        &note,
    ));
    let empty: BTreeMap<String, Option<u64>> = BTreeMap::new();
    let kernels: BTreeMap<&String, (Option<u64>, Option<u64>)> = pr
        .map(|pr| &pr.instructions)
        .unwrap_or(&empty)
        .iter()
        .map(|(kernel, pr)| {
            (
                kernel,
                (
                    dev.and_then(|d| d.instructions.get(kernel))
                        .and_then(|v| *v),
                    *pr,
                ),
            )
        })
        .collect();
    for (kernel, (dev, pr)) in kernels {
        out.push(deterministic(
            &format!("Instructions: {kernel}"),
            dev,
            pr,
            "deterministic",
        ));
    }
    let empty: BTreeMap<String, Timing> = BTreeMap::new();
    let scenarios: BTreeMap<&String, (&Timing, &Timing)> = pr
        .map(|pr| &pr.ticks)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|(name, pr)| dev?.ticks.get(name).map(|dev| (name, (dev, pr))))
        .collect();
    for (name, (dev, pr)) in scenarios {
        for (share, pick) in tick_picks() {
            if let Some(row) = timed(
                noise,
                format!("Tick {share}, {name} (ns)"),
                pick(dev),
                pick(pr),
            ) {
                out.push(row);
            }
        }
    }
    if let (Some(dev), Some(pr)) = (
        dev.and_then(|report| report.pressure.as_ref()),
        pr.and_then(|report| report.pressure.as_ref()),
    ) {
        for (label, pick) in pressure_picks() {
            if let Some(row) = timed(noise, format!("Pressure {label}"), pick(dev), pick(pr)) {
                out.push(row);
            }
        }
    }
    out
}

fn job_rows(jobs: &[super::Job]) -> String {
    let mut rendered: Vec<String> = Vec::new();
    for job in jobs {
        // A skipped job ran nothing: no duration worth a row.
        if job.conclusion.as_deref() == Some("skipped") {
            continue;
        }
        let Some(seconds) = job.seconds else {
            continue;
        };
        let conclusion = match job.conclusion.as_deref() {
            Some("success") | None => String::new(),
            Some(other) => format!(" ({other})"),
        };
        rendered.push(format!(
            "| {}{conclusion} | {} |\n",
            job.name,
            super::format_duration(seconds)
        ));
    }
    if rendered.is_empty() {
        return String::new();
    }
    let mut table = String::from("| CI job | Duration |\n|---|---:|\n");
    table.push_str(&rendered.concat());
    table
}

/// Everything one comment is built from.
pub(crate) struct Input<'a> {
    pub(crate) repository: &'a str,
    pub(crate) run: u64,
    pub(crate) dev_run: Option<u64>,
    pub(crate) dev: Option<&'a Report>,
    pub(crate) pr: Option<&'a Report>,
    pub(crate) jobs: &'a [super::Job],
    pub(crate) cap: Option<u64>,
    pub(crate) noise: &'a Noise,
}

/// The whole comment: the marker line, the measured table, the job durations
/// of the run, and what these numbers do not measure.
pub(crate) fn comment(input: Input<'_>) -> String {
    let Input {
        repository,
        run,
        dev_run,
        dev,
        pr,
        jobs,
        cap,
        noise,
    } = input;
    let mut text = format!("{MARKER}\n## CI performance numbers (informational)\n\n");
    match (pr, dev_run) {
        (Some(pr), Some(dev_run)) => text.push_str(&format!(
            "The `target-performance` job of [run {run}](https://github.com/{repository}/actions/runs/{run}) \
measured revision `{}` against the latest successful `dev` run [\
{dev_run}](https://github.com/{repository}/actions/runs/{dev_run}) (revision `{}`).\n\n",
            pr.revision,
            dev.map(|dev| dev.revision.as_str()).unwrap_or("unknown"),
        )),
        (Some(_), None) => text.push_str(&format!(
            "The `target-performance` job of [run {run}](https://github.com/{repository}/actions/runs/{run}) \
measured revision `{}`, but no recent successful `dev` run uploaded a `performance-report` \
artifact: `dev` values are missing below and only this pull request's absolute numbers show.\n\n",
            pr.map(|pr| pr.revision.as_str()).unwrap_or("unknown"),
        )),
        (None, _) => text.push_str(&format!(
            "[Run {run}](https://github.com/{repository}/actions/runs/{run}) uploaded no \
`performance-report` artifact (the `target-performance` job may have failed or been skipped); \
nothing was measured for this pull request.\n\n"
        )),
    }
    text.push_str("Informational only: failing checks stay the checks, and this comment never gates a merge.\n\n");
    let measured = pr.is_some().then(|| rows(dev, pr, cap, noise));
    if let Some(measured) = measured.filter(|measured| !measured.is_empty()) {
        text.push_str("| Metric | dev | this PR | Δ | Note |\n|---|---:|---:|---:|---|\n");
        for row in measured {
            text.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                row.label,
                value(row.dev),
                value(row.pr),
                delta(row.dev, row.pr),
                row.note
            ));
        }
        text.push_str(&format!(
            "\nTiming rows appear only when a value moved beyond the ±{}% noise band of \
`{NOISE_FILE}`; tick times and protocol latencies vary with scheduling on shared runners.\n\n",
            noise.percent(),
        ));
    }
    let job_table = job_rows(jobs);
    if !job_table.is_empty() {
        text.push_str(&format!(
            "Job durations of run {run} (setup, compile and gates per job):\n\n{job_table}\n"
        ));
    }
    text.push_str(
        "**Not measured here:** real frame rate. CI renders in software; the 1920×1080 check is \
dedicated-hardware qualification only ([docs/performance.md](docs/performance.md)).\n",
    );
    text
}
