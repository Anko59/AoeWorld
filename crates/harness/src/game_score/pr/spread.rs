//! How much a number of the game score moves when the game has not changed. A game that is
//! deterministic is still chaotic: any change of behaviour moves every number a little, and one
//! seed is one sample. So when two reports differ, a number is judged only against what it does
//! across seeds on the same code: the same game played at eight seeds (`make game-score-spread`),
//! kept in `scenarios/spread.json` beside the scenarios it was measured on, with the commit and the
//! date.
//!
//! A judged number *moved beyond its usual spread* when it moved by more than twice its measured
//! spread (the median absolute deviation over the median across the seeds, in percent), never
//! under 5 percent, and by more than a minimum absolute move: the range it showed across the
//! seeds, and for a small count a floor of its own (`numbers::floor`), so that 2 to 7 is not a
//! finding. A scenario or window nobody measured is judged at 25 percent.
use super::{
    BoxError,
    compare::Row,
    numbers::{self, Better},
};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf, sync::OnceLock};

const COMMITTED: &str = include_str!("../scenarios/spread.json");
/// A move is never a finding under this relative size, whatever the measured spread says.
pub(super) const NEVER_UNDER_PERCENT: f64 = 5.0;
/// A move is a finding beyond this many times the measured spread.
pub(super) const TIMES_SPREAD: f64 = 2.0;
/// A number nobody measured is judged at this relative move.
pub(super) const UNMEASURED_PERCENT: f64 = 25.0;

/// What one number did across the seeds, in its own unit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Entry {
    pub median: f64,
    /// The median absolute deviation over the median, in percent (0 when the median is 0).
    pub mad_percent: f64,
    pub min: f64,
    pub max: f64,
}

impl Entry {
    fn range(&self) -> f64 {
        self.max - self.min
    }
}

/// What the spread was measured on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Measured {
    pub commit: String,
    pub date: String,
    pub seeds: Vec<u64>,
    pub workers: usize,
    /// In words, so a reader of the file knows what `mad_percent` is.
    pub measure: String,
}

type Numbers = BTreeMap<String, Entry>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Spread {
    pub measured: Measured,
    /// scenario, window, number path.
    pub scenarios: BTreeMap<String, BTreeMap<String, Numbers>>,
}

/// How far a number must move before it is a finding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Tolerance {
    pub percent: f64,
    pub min_move: f64,
}

impl Tolerance {
    /// `row` moved by more than the tolerance, in both relative and absolute terms. A move from
    /// zero has no relative size: the absolute one decides. A side without a value is no move.
    pub(super) fn beyond(&self, row: &Row) -> bool {
        let (Some(base), Some(branch)) = (row.base, row.branch) else {
            return false;
        };
        (branch - base).abs() > self.min_move
            && row.percent().is_none_or(|p| p.abs() > self.percent)
    }
}

impl Spread {
    pub(super) fn parse(text: &str) -> Result<Self, BoxError> {
        Ok(serde_json::from_str(text)?)
    }

    /// The spread measured on the scenarios of this code, read once.
    pub(super) fn committed() -> Result<&'static Spread, BoxError> {
        static SPREAD: OnceLock<Result<Spread, String>> = OnceLock::new();
        SPREAD
            .get_or_init(|| Spread::parse(COMMITTED).map_err(|error| error.to_string()))
            .as_ref()
            .map_err(|error| format!("game_score/scenarios/spread.json: {error}").into())
    }

    pub(super) fn entry(&self, scenario: &str, window: &str, path: &str) -> Option<&Entry> {
        self.scenarios.get(scenario)?.get(window)?.get(path)
    }

    pub(super) fn tolerance(&self, scenario: &str, window: &str, path: &str) -> Tolerance {
        let floor = numbers::floor(path);
        match self.entry(scenario, window, path) {
            Some(entry) => Tolerance {
                percent: NEVER_UNDER_PERCENT.max(TIMES_SPREAD * entry.mad_percent),
                min_move: floor.max(entry.range()),
            },
            None => Tolerance {
                percent: UNMEASURED_PERCENT,
                min_move: floor,
            },
        }
    }
}

fn median(sorted: &[f64]) -> f64 {
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[middle]
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    }
}

fn entry_of(mut values: Vec<f64>) -> Entry {
    values.sort_by(f64::total_cmp);
    let middle = median(&values);
    let mut deviations: Vec<f64> = values.iter().map(|v| (v - middle).abs()).collect();
    deviations.sort_by(f64::total_cmp);
    let mad = median(&deviations);
    Entry {
        median: middle,
        mad_percent: if middle == 0.0 {
            0.0
        } else {
            (mad / middle.abs() * 1000.0).round() / 10.0
        },
        min: values[0],
        max: values[values.len() - 1],
    }
}

/// The numbers a pull request is judged on: those the table names with a better side, except the
/// cost per unit of service (judged exactly on instruction counts, never on a spread).
pub(super) fn judged() -> impl Iterator<Item = &'static str> {
    numbers::KNOWN
        .iter()
        .filter(|known| known.better != Better::Neither && !known.path.starts_with("efficiency."))
        .map(|known| known.path)
}

fn value_at(window: &Value, path: &str) -> Option<f64> {
    window
        .pointer(&format!("/service/{}", path.replace('.', "/")))?
        .as_f64()
}

/// The spread of every judged number of every named window, from the reports of the same code at
/// several seeds. `whole` is never judged, so it has no entry.
pub(super) fn measure(reports: &[Value], measured: Measured) -> Result<Spread, BoxError> {
    if reports.len() < 3 {
        return Err("a spread needs the same game at three seeds at least".into());
    }
    let mut scenarios: BTreeMap<String, BTreeMap<String, Numbers>> = BTreeMap::new();
    for scenario in reports[0]["scenarios"].as_array().into_iter().flatten() {
        let id = scenario["id"].as_str().unwrap_or_default();
        for window in scenario["windows"].as_array().into_iter().flatten() {
            let name = window["id"].as_str().unwrap_or_default();
            if name == "whole" {
                continue;
            }
            for path in judged() {
                let values: Vec<f64> = reports
                    .iter()
                    .filter_map(|report| {
                        let scenario = report["scenarios"]
                            .as_array()?
                            .iter()
                            .find(|s| s["id"] == id)?;
                        let window = scenario["windows"]
                            .as_array()?
                            .iter()
                            .find(|w| w["id"] == name)?;
                        value_at(window, path)
                    })
                    .collect();
                if values.len() >= 3 {
                    scenarios
                        .entry(id.to_owned())
                        .or_default()
                        .entry(name.to_owned())
                        .or_default()
                        .insert(path.to_owned(), entry_of(values));
                }
            }
        }
    }
    if scenarios.is_empty() {
        return Err("no judged number has three values in these reports".into());
    }
    Ok(Spread {
        measured,
        scenarios,
    })
}

/// The table of `docs/scale/game-score.md`: one row per judged number and window.
pub(super) fn table(spread: &Spread) -> String {
    let mut out = String::from(
        "| scenario | window | number | median | spread (MAD / median) | range | tolerance |\n|---|---|---|---|---|---|---|\n",
    );
    for (scenario, windows) in &spread.scenarios {
        for (window, entries) in windows {
            for (path, entry) in entries {
                let tolerance = spread.tolerance(scenario, window, path);
                out.push_str(&format!(
                    "| {scenario} | {window} | {} | {} | {:.1}% | {} to {} | {:.0}% and {} |\n",
                    numbers::label(path),
                    numbers::show(path, Some(entry.median)),
                    entry.mad_percent,
                    numbers::show(path, Some(entry.min)),
                    numbers::show(path, Some(entry.max)),
                    tolerance.percent,
                    numbers::show(path, Some(tolerance.min_move)),
                ));
            }
        }
    }
    out
}

#[derive(Args, Debug, Clone)]
pub struct Options {
    /// The directory of the reports, one per seed, named `seed-<n>.json` (`make game-score
    /// GAME_SCORE_ARGS='--seed <n> --workers 1 --parallel --out <directory>/seed-<n>.json'`).
    #[arg(long)]
    reports: PathBuf,
    /// The commit the reports were measured on.
    #[arg(long)]
    commit: String,
    /// The day, `YYYY-MM-DD`.
    #[arg(long)]
    date: String,
    /// Where the spread is written.
    #[arg(
        long,
        default_value = "crates/harness/src/game_score/scenarios/spread.json"
    )]
    out: PathBuf,
}

pub fn run(options: Options) -> Result<(), BoxError> {
    let mut found: Vec<(u64, Value)> = Vec::new();
    for item in fs::read_dir(&options.reports)? {
        let path = item?.path();
        let seed = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| {
                name.strip_prefix("seed-")?
                    .strip_suffix(".json")?
                    .parse()
                    .ok()
            });
        if let Some(seed) = seed {
            found.push((seed, serde_json::from_str(&fs::read_to_string(&path)?)?));
        }
    }
    found.sort_by_key(|(seed, _)| *seed);
    let workers = found
        .first()
        .and_then(|(_, report)| report["scenarios"][0]["measured_at_workers"].as_u64())
        .unwrap_or(1) as usize;
    let measured = Measured {
        commit: options.commit,
        date: options.date,
        seeds: found.iter().map(|(seed, _)| *seed).collect(),
        workers,
        measure: "mad_percent is the median absolute deviation over the median across the seeds, in percent; \
                  min and max are the extremes across the seeds, in the number's own unit"
            .to_owned(),
    };
    let reports: Vec<Value> = found.into_iter().map(|(_, report)| report).collect();
    let spread = measure(&reports, measured)?;
    fs::write(&options.out, serde_json::to_string_pretty(&spread)? + "\n")?;
    print!("{}", table(&spread));
    eprintln!(
        "game score spread: {} seeds, wrote {}",
        spread.measured.seeds.len(),
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
#[path = "tests/spread.rs"]
mod tests;
