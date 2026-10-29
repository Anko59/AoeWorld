//! The comment body: one compact Markdown table under a hidden marker, so a
//! later CI run rewrites the same comment instead of adding one.
use super::reports::{Pressure, Report, Timing, WasmRule};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(crate) const MARKER: &str = "<!-- aoe-perf-comment -->";
const NOISE_FILE: &str = "crates/harness/src/perf_comment/noise.json";
/// Timing rows need at least this many dev reports: fewer cannot describe
/// dev's own variance, and a fixed band would flag pure runner noise.
const MIN_DEV_REPORTS: usize = 3;

/// What `dev` itself varies, from `noise.json`: how many dev runs with passed
/// performance jobs the band is drawn from and how much their range widens.
#[derive(Deserialize)]
pub(crate) struct Noise {
    widening_percent: u64,
    dev_runs: usize,
}

impl Noise {
    pub(crate) fn load() -> Result<Self, String> {
        serde_json::from_str(include_str!("noise.json"))
            .map_err(|error| format!("{NOISE_FILE}: {error}"))
    }

    pub(crate) fn widening_percent(&self) -> u64 {
        self.widening_percent
    }

    pub(crate) fn dev_runs(&self) -> usize {
        self.dev_runs
    }

    /// The band dev's own values for one metric span: `[min, max]` widened by
    /// `widening_percent` of that range — or of the median when every value
    /// is equal. `None` when no dev run measured the metric.
    pub(crate) fn band(&self, values: &[u64]) -> Option<(f64, f64)> {
        let min = *values.iter().min()? as f64;
        let max = *values.iter().max()? as f64;
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        let middle = sorted.len() / 2;
        let median = if sorted.len() % 2 == 1 {
            sorted[middle] as f64
        } else {
            (sorted[middle - 1] + sorted[middle]) as f64 / 2.0
        };
        let range = max - min;
        let width = if range > 0.0 { range } else { median } * self.widening_percent as f64 / 100.0;
        Some((min - width, max + width))
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

/// A timing row: shown only when the PR value falls outside the band dev's
/// own runs span. `dev_values` are that metric's values across the dev
/// reports (newest first); `latest` is the newest dev value for display.
fn timed(
    noise: &Noise,
    label: String,
    dev_values: Vec<u64>,
    latest: Option<u64>,
    pr: Option<u64>,
) -> Option<Row> {
    let band = noise.band(&dev_values)?;
    let pr = pr?;
    let value = pr as f64;
    (value < band.0 || value > band.1).then(|| Row {
        label,
        dev: latest,
        pr: Some(pr),
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

fn wasm_note(wasm: Option<WasmRule>, pr_gzip: Option<u64>) -> String {
    let Some(rule) = wasm else {
        return "deterministic; cap unknown".to_owned();
    };
    let note = format!(
        "deterministic; baseline {}, cap {}",
        rule.baseline, rule.cap
    );
    if pr_gzip.is_some_and(|gzip| gzip > rule.cap) {
        format!("{note}; PR over cap")
    } else {
        note
    }
}

fn rows(devs: &[&Report], pr: Option<&Report>, wasm: Option<WasmRule>, noise: &Noise) -> Vec<Row> {
    let mut out = Vec::new();
    let latest = devs.first();
    let pr_report = pr;
    out.push(deterministic(
        "Optimized WASM raw (bytes)",
        latest.and_then(|r| r.wasm_raw),
        pr.and_then(|r| r.wasm_raw),
        "deterministic",
    ));
    out.push(deterministic(
        "Optimized WASM gzip (bytes)",
        latest.and_then(|r| r.wasm_gzip),
        pr.and_then(|r| r.wasm_gzip),
        &wasm_note(wasm, pr.and_then(|r| r.wasm_gzip)),
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
                    latest
                        .and_then(|d| d.instructions.get(kernel))
                        .and_then(|v| *v),
                    *pr,
                ),
            )
        })
        .collect();
    for (kernel, (dev, pr)) in kernels {
        let note = match pr_report.and_then(|r| r.instruction_notes.get(kernel)) {
            Some(gate) => format!("deterministic; {gate}"),
            None => "deterministic".to_owned(),
        };
        out.push(deterministic(
            &format!("Instructions: {kernel}"),
            dev,
            pr,
            &note,
        ));
    }
    let Some(latest) = latest else {
        return out;
    };
    // Timing rows compare against dev's own variance: without enough dev
    // reports there is no honest band, so none show.
    if devs.len() < MIN_DEV_REPORTS {
        return out;
    }
    let empty: BTreeMap<String, Timing> = BTreeMap::new();
    let pr_ticks = pr.map(|pr| &pr.ticks).unwrap_or(&empty);
    for (name, pr_timing) in pr_ticks {
        let dev_timing = latest.ticks.get(name);
        for (share, pick) in tick_picks() {
            let values: Vec<u64> = devs
                .iter()
                .filter_map(|d| d.ticks.get(name).and_then(pick))
                .collect();
            if let Some(row) = timed(
                noise,
                format!("Tick {share}, {name} (ns)"),
                values,
                dev_timing.and_then(pick),
                pick(pr_timing),
            ) {
                out.push(row);
            }
        }
    }
    if let (Some(dev_pressure), Some(pr_pressure)) = (
        latest.pressure.as_ref(),
        pr.and_then(|pr| pr.pressure.as_ref()),
    ) {
        for (label, pick) in pressure_picks() {
            let values: Vec<u64> = devs
                .iter()
                .filter_map(|d| d.pressure.as_ref().and_then(pick))
                .collect();
            if let Some(row) = timed(
                noise,
                format!("Pressure {label}"),
                values,
                pick(dev_pressure),
                pick(pr_pressure),
            ) {
                out.push(row);
            }
        }
    }
    out
}

/// The pinned instruction counts dev and this PR observed are identical:
/// deterministic evidence that the measured code paths did not change.
fn instructions_unchanged(dev: Option<&Report>, pr: Option<&Report>) -> bool {
    let (Some(dev), Some(pr)) = (dev, pr) else {
        return false;
    };
    !dev.instructions.is_empty()
        && dev.instructions.len() == pr.instructions.len()
        && dev.instructions.iter().all(|(kernel, dev_value)| {
            match (pr.instructions.get(kernel), dev_value) {
                (Some(Some(pr_value)), Some(dev_value)) => pr_value == dev_value,
                _ => false,
            }
        })
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

/// Everything one comment is built from. `devs` are the dev reports of the
/// latest dev runs whose performance job passed, newest first.
pub(crate) struct Input<'a> {
    pub(crate) repository: &'a str,
    pub(crate) run: u64,
    pub(crate) devs: &'a [(u64, Report)],
    pub(crate) pr: Option<&'a Report>,
    pub(crate) jobs: &'a [super::Job],
    pub(crate) wasm: Option<WasmRule>,
    pub(crate) noise: &'a Noise,
}

/// The whole comment: the marker line, the measured table, the timing-band
/// footnotes, and the job durations of the run.
pub(crate) fn comment(input: Input<'_>) -> String {
    let Input {
        repository,
        run,
        devs,
        pr,
        jobs,
        wasm,
        noise,
    } = input;
    let dev_run = devs.first().map(|(run, _)| *run);
    let mut text = format!("{MARKER}\n## CI performance numbers (informational)\n\n");
    match (pr, dev_run) {
        (Some(pr), Some(dev_run)) => text.push_str(&format!(
            "The `target-performance` job of [run {run}](https://github.com/{repository}/actions/runs/{run}) \
measured revision `{}` against the latest `dev` run whose `target-performance` job passed [\
{dev_run}](https://github.com/{repository}/actions/runs/{dev_run}) (revision `{}`).\n\n",
            pr.revision,
            devs.first()
                .map(|(_, report)| report.revision.as_str())
                .unwrap_or("unknown"),
        )),
        (Some(_), None) => text.push_str(&format!(
            "The `target-performance` job of [run {run}](https://github.com/{repository}/actions/runs/{run}) \
measured revision `{}`, but no recent `dev` run whose `target-performance` job passed uploaded a `performance-report` \
artifact: `dev` values are missing below and only this pull request's absolute numbers show.\n\n",
            pr.map(|pr| pr.revision.as_str()).unwrap_or("unknown"),
        )),
        (None, _) => text.push_str(&format!(
            "[Run {run}](https://github.com/{repository}/actions/runs/{run}) uploaded no \
`performance-report` artifact (the `target-performance` job may have failed or been skipped); \
nothing was measured for this pull request.\n\n"
        )),
    }
    text.push_str("This comment never gates a merge. `make perf-ci` does: instruction rows warn above +1% of their baseline and fail above +3% (or +10% of the fixed reference); timing rows are informational.\n\n");
    if let Some(measured) = pr.is_some().then(|| {
        let reports: Vec<&Report> = devs.iter().map(|(_, report)| report).collect();
        rows(&reports, pr, wasm, noise)
    }) {
        if !measured.is_empty() {
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
            text.push('\n');
        }
        text.push_str(&if devs.len() >= MIN_DEV_REPORTS {
            format!(
                "Timing rows appear only when this PR's value lies outside the band spanned by \
`dev` itself across its last {} runs whose `target-performance` job passed — their [min, max] widened by {}% of that \
range (`{NOISE_FILE}`); tick times and protocol latencies vary with scheduling on shared runners.\n\n",
                devs.len(),
                noise.widening_percent(),
            )
        } else {
            format!(
                "No timing rows: only {} `dev` run{} whose `target-performance` job passed and which have a \
`performance-report` artifact found, and comparing against dev's own variance needs at least {MIN_DEV_REPORTS}. \
A fixed band would flag pure runner noise.\n\n",
                devs.len(),
                if devs.len() == 1 { "" } else { "s" },
            )
        });
        if instructions_unchanged(devs.first().map(|(_, report)| report), pr) {
            text.push_str(
                "instruction counts unchanged: timing differences are most likely runner noise\n\n",
            );
        }
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
