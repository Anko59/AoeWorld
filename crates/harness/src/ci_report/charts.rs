//! The page's charts, drawn from the report's data.
use super::Data;
use super::calendar;
use super::chart::{Axis, Chart, Line, Mark};
use super::format::{grouped, label, si, signed, stamp};
use super::history::EventKind;
use super::series::{DevPoint, segments};
use crate::perf_comment::epoch_seconds;
use std::collections::BTreeMap;

const JOB_LINES: usize = 5;
/// Days without a run of a job (in seconds) after which its line is cut.
const MAX_GAP: f64 = 3.0 * 86_400.0;

/// A group of instruction rows with one chart. The late-game rows are
/// redefined often, so they show the change since the last redefinition.
pub(super) struct Family {
    pub(super) id: &'static str,
    pub(super) title: &'static str,
    pub(super) note: &'static str,
    pub(super) rows: Vec<String>,
    pub(super) relative: bool,
}

fn family_of(row: &str) -> usize {
    let name = row.rsplit("::").next().unwrap_or(row);
    if name.starts_with("late_game") {
        0
    } else if name.ends_with("_ticks") {
        1
    } else {
        2
    }
}

pub(super) fn families(data: &Data) -> Vec<Family> {
    let mut families = vec![
        Family {
            id: "late-game",
            title: "Late-game kernel: change since each row was last redefined",
            note: "A late-game world with a large army. A row whose baseline moved by more than two times between two commits was redefined (more ticks counted, a different world), so its raw count jumps for no change in the game: each line restarts at 0% there, and a lettered marker names the commit. Compare within a stretch, not across a marker. Point at the chart for the raw counts.",
            rows: Vec::new(),
            relative: true,
        },
        Family {
            id: "small-worlds",
            title: "Small worlds: instructions per run",
            note: "Short simulations of a small population, in raw instruction counts.",
            rows: Vec::new(),
            relative: false,
        },
        Family {
            id: "single-operations",
            title: "Single operations: instructions per call",
            note: "One simulation step, one viewport query, one snapshot encoding, one palette decoding.",
            rows: Vec::new(),
            relative: false,
        },
    ];
    let mut rows: Vec<&String> = data
        .dev
        .iter()
        .flat_map(|point| point.rows.keys())
        .filter(|row| !row.starts_with("wasm_"))
        .collect();
    rows.sort();
    rows.dedup();
    for row in rows {
        families[family_of(row)].rows.push(row.clone());
    }
    families
}

fn seconds(at: &str) -> f64 {
    epoch_seconds(at).unwrap_or(0) as f64
}

fn commit_line(point: &DevPoint) -> String {
    let pr = point.pr.map(|pr| format!(" #{pr}")).unwrap_or_default();
    let mut text = format!(
        "{} UTC · {}{pr} {}",
        stamp(&point.at),
        point.rev,
        point.title
    );
    if let Some(n) = point.since_previous.filter(|n| *n > 1) {
        text.push_str(&format!("\n{n} commits since the previous point"));
    }
    text
}

pub(super) fn instruction_chart(data: &Data, family: &Family) -> Chart {
    let mut lines = Vec::new();
    // (point index, row) to the change since the start of its stretch.
    let mut change: BTreeMap<(usize, &str), f64> = BTreeMap::new();
    for row in &family.rows {
        let mut line = Line {
            label: label(row),
            segments: Vec::new(),
        };
        for stretch in segments(&data.dev, row, &data.events) {
            let first = data.dev[stretch[0]].rows[row] as f64;
            let mut points = Vec::new();
            for &index in &stretch {
                let point = &data.dev[index];
                let value = point.rows[row] as f64;
                let shown = if family.relative {
                    (value / first - 1.0) * 100.0
                } else {
                    value
                };
                change.insert((index, row.as_str()), (value / first - 1.0) * 100.0);
                points.push((seconds(&point.at), shown));
            }
            line.segments.push(points);
        }
        lines.push(line);
    }
    let tips = data
        .dev
        .iter()
        .enumerate()
        .filter(|(_, point)| family.rows.iter().any(|row| point.rows.contains_key(row)))
        .map(|(index, point)| {
            let mut text = commit_line(point);
            for row in family
                .rows
                .iter()
                .filter(|row| point.rows.contains_key(*row))
            {
                let value = point.rows[row] as f64;
                let value = match change.get(&(index, row.as_str())) {
                    Some(pct) if family.relative => format!("{} · {}", signed(*pct), si(value)),
                    _ => grouped(point.rows[row]),
                };
                text.push_str(&format!("\n{}: {value}", label(row)));
            }
            (seconds(&point.at), text)
        })
        .collect();
    let marks = data
        .events
        .iter()
        .filter(|event| event.rows.iter().any(|row| family.rows.contains(row)))
        .map(|event| Mark {
            at: seconds(&event.at),
            label: event.id.clone(),
            dashed: event.kind == EventKind::Redefined,
        })
        .collect();
    Chart {
        id: family.id.to_owned(),
        about: format!("Line chart: {}", family.title),
        axis: if family.relative {
            Axis::Percent
        } else {
            Axis::Count
        },
        step: true,
        lines,
        marks,
        guides: Vec::new(),
        tips,
    }
}

pub(super) fn bundle_chart(data: &Data) -> Chart {
    let measured: Vec<(f64, f64)> = data
        .dev
        .iter()
        .filter_map(|point| Some((seconds(&point.at), *point.rows.get("wasm_gzip")? as f64)))
        .collect();
    let (start, end) = (
        measured.first().map_or(0.0, |point| point.0),
        measured.last().map_or(0.0, |point| point.0),
    );
    // The committed limits as steps; those from before the first measurement
    // collapse onto it, the last of them standing.
    let steps = |value: fn(&super::BundleLimit) -> u64| -> Vec<(f64, f64)> {
        let mut steps: Vec<(f64, f64)> = Vec::new();
        if measured.is_empty() {
            return steps;
        }
        for limit in &data.bundle {
            let reached = data
                .dev
                .iter()
                .find(|point| point.position >= limit.position)
                .map_or(end, |point| seconds(&point.at));
            let at = reached.clamp(start, end);
            if steps.last().is_some_and(|last| last.0 == at) {
                steps.pop();
            }
            steps.push((at, value(limit) as f64));
        }
        if let Some(&(_, last)) = steps.last() {
            steps.push((end, last));
        }
        steps
    };
    let tips = data
        .dev
        .iter()
        .filter_map(|point| {
            let bytes = point.rows.get("wasm_gzip")?;
            Some((
                seconds(&point.at),
                format!(
                    "{}\ncompressed: {} bytes",
                    commit_line(point),
                    grouped(*bytes)
                ),
            ))
        })
        .collect();
    let whole = |points: Vec<(f64, f64)>| {
        if points.is_empty() {
            Vec::new()
        } else {
            vec![points]
        }
    };
    Chart {
        id: "bundle".to_owned(),
        about: "Line chart of the compressed client bundle on dev over time, with its baseline and its limit".to_owned(),
        axis: Axis::Count,
        step: true,
        lines: vec![
            Line { label: "Compressed size".to_owned(), segments: whole(measured.clone()) },
            Line { label: "Baseline".to_owned(), segments: whole(steps(|l| l.baseline)) },
            Line { label: "Fails above".to_owned(), segments: whole(steps(|l| l.limit)) },
        ],
        marks: Vec::new(),
        guides: Vec::new(),
        tips,
    }
}

fn day_seconds(day: &str) -> f64 {
    calendar::midnight(day).unwrap_or(0) as f64 + 43_200.0
}

/// Median minutes per day of the longest job of a run and of the jobs that take longest.
pub(super) fn job_chart(data: &Data) -> Chart {
    let mut totals: BTreeMap<&str, f64> = BTreeMap::new();
    for day in &data.ci_days {
        for (job, minutes) in &day.jobs {
            *totals.entry(job).or_default() += minutes;
        }
    }
    let mut names: Vec<(&str, f64)> = totals.into_iter().collect();
    names.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    let mut lines = vec![Line {
        label: "Longest job of a run".to_owned(),
        segments: vec![
            data.ci_days
                .iter()
                .map(|d| (day_seconds(&d.day), d.longest))
                .collect(),
        ],
    }];
    let shown: Vec<&str> = names
        .iter()
        .take(JOB_LINES)
        .map(|(name, _)| *name)
        .collect();
    for name in &shown {
        // A job that did not run for days is not drawn as if it had.
        let mut segments: Vec<Vec<(f64, f64)>> = Vec::new();
        for day in &data.ci_days {
            let Some(&minutes) = day.jobs.get(*name) else {
                continue;
            };
            let at = day_seconds(&day.day);
            match segments.last_mut() {
                Some(last) if at - last.last().map_or(at, |p| p.0) <= MAX_GAP => {
                    last.push((at, minutes))
                }
                _ => segments.push(vec![(at, minutes)]),
            }
        }
        lines.push(Line {
            label: (*name).to_owned(),
            segments,
        });
    }
    let tips = data
        .ci_days
        .iter()
        .map(|day| {
            let mut text = format!(
                "{} · {} runs\nlongest job: {:.1} min",
                super::format::short_date(&day.day),
                day.runs,
                day.longest
            );
            for name in &shown {
                if let Some(minutes) = day.jobs.get(*name) {
                    text.push_str(&format!("\n{name}: {minutes:.1} min"));
                }
            }
            (day_seconds(&day.day), text)
        })
        .collect();
    Chart {
        id: "ci-jobs".to_owned(),
        about: "Line chart of the median duration in minutes of the longest CI jobs on each day"
            .to_owned(),
        axis: Axis::Minutes,
        step: false,
        lines,
        marks: Vec::new(),
        guides: Vec::new(),
        tips,
    }
}

/// Median machine-minutes (every job's time added up) of a run, per day.
pub(super) fn machine_chart(data: &Data) -> Chart {
    let points = data
        .ci_days
        .iter()
        .map(|d| (day_seconds(&d.day), d.total))
        .collect();
    let tips = data
        .ci_days
        .iter()
        .map(|d| {
            let text = format!(
                "{} · {} runs\n{:.1} machine-minutes per run",
                super::format::short_date(&d.day),
                d.runs,
                d.total
            );
            (day_seconds(&d.day), text)
        })
        .collect();
    Chart {
        id: "ci-machine".to_owned(),
        about: "Line chart of the median machine-minutes one CI run used, on each day".to_owned(),
        axis: Axis::Minutes,
        step: false,
        lines: vec![Line {
            label: "Machine-minutes per run".to_owned(),
            segments: vec![points],
        }],
        marks: Vec::new(),
        guides: Vec::new(),
        tips,
    }
}
