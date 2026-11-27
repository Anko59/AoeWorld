//! The merge base against this branch, number by number. `service` numbers are whole numbers
//! that depend on the game alone, so they are compared exactly: a number that differs is listed
//! with both values and its change, the ones that do not are counted. Wall time and memory
//! (`cost`) sit beside, labelled noisy and never judged; a cost per unit of service (`efficiency`)
//! joins the compared numbers only when both sides counted instructions, and is left out on wall
//! time (it is the seconds of `cost` divided by a count).
use super::numbers::{self, Better};
use serde_json::Value;
use std::collections::BTreeMap;

/// One number of one window, on both sides. `None`: the report holds no value (`null`, or the
/// number does not exist on that side).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub path: String,
    pub base: Option<f64>,
    pub branch: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Direction {
    Same,
    Better,
    Worse,
    /// Changed, but the number has no better side, or one side holds no value.
    Neither,
}

impl Row {
    pub(super) fn label(&self) -> String {
        numbers::label(&self.path)
    }

    /// The change against the merge base in percent; `None` without a base value or from zero.
    pub(super) fn percent(&self) -> Option<f64> {
        match (self.base, self.branch) {
            (Some(base), Some(branch)) if base != 0.0 => Some((branch - base) / base * 100.0),
            _ => None,
        }
    }

    pub(super) fn direction(&self) -> Direction {
        let (Some(base), Some(branch)) = (self.base, self.branch) else {
            return if self.base == self.branch {
                Direction::Same
            } else {
                Direction::Neither
            };
        };
        match (numbers::better(&self.path), branch.total_cmp(&base)) {
            (_, std::cmp::Ordering::Equal) => Direction::Same,
            (Better::Higher, std::cmp::Ordering::Greater)
            | (Better::Lower, std::cmp::Ordering::Less) => Direction::Better,
            (Better::Higher, _) | (Better::Lower, _) => Direction::Worse,
            (Better::Neither, _) => Direction::Neither,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct WindowDiff {
    pub id: String,
    pub from_tick: u64,
    pub to_tick: u64,
    /// The `service` numbers (and instruction-counted costs per unit of service) that differ.
    pub changed: Vec<Row>,
    /// How many numbers have the same value on both sides.
    pub unchanged: usize,
    /// Wall time and memory: noisy, shown, never judged.
    pub noisy: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Status {
    Compared,
    /// Nothing to compare with: the numbers shown are the branch's alone.
    NoBase(String),
    /// The merge base played another game: the numbers are not comparable.
    NotComparable(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ScenarioDiff {
    pub id: String,
    pub status: Status,
    pub windows: Vec<WindowDiff>,
}

fn flatten(value: &Value, prefix: &str, out: &mut BTreeMap<String, Option<f64>>) {
    match value {
        Value::Object(map) => {
            for (name, inner) in map {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}.{name}")
                };
                flatten(inner, &path, out);
            }
        }
        Value::Number(number) => {
            out.insert(prefix.to_owned(), number.as_f64());
        }
        Value::Null => {
            out.insert(prefix.to_owned(), None);
        }
        _ => {}
    }
}

fn numbers_of(window: &Value, section: &str, prefix: &str) -> BTreeMap<String, Option<f64>> {
    let mut out = BTreeMap::new();
    if let Some(value) = window.get(section) {
        flatten(value, prefix, &mut out);
    }
    out
}

/// Known numbers first, in the table's order, then the rest by path.
fn order(path: &str) -> (usize, String) {
    let index = numbers::KNOWN
        .iter()
        .position(|known| known.path == path)
        .unwrap_or(numbers::KNOWN.len());
    (index, path.to_owned())
}

fn rows(base: &BTreeMap<String, Option<f64>>, branch: &BTreeMap<String, Option<f64>>) -> Vec<Row> {
    let paths: std::collections::BTreeSet<&String> = base.keys().chain(branch.keys()).collect();
    let mut rows: Vec<Row> = paths
        .into_iter()
        .map(|path| Row {
            path: path.clone(),
            base: base.get(path).copied().flatten(),
            branch: branch.get(path).copied().flatten(),
        })
        .collect();
    rows.sort_by_key(|row| order(&row.path));
    rows
}

const NOISY: [&str; 4] = [
    "cost.tick_seconds",
    "cost.tick_ms.p95",
    "cost.peak_memory_mib",
    "cost.instructions",
];

fn basis(window: Option<&Value>) -> Option<&str> {
    window?.pointer("/efficiency/basis")?.as_str()
}

fn diff_window(base: Option<&Value>, branch: &Value) -> WindowDiff {
    let empty = Value::Null;
    let base_window = base.unwrap_or(&empty);
    let counted = base.is_some()
        && basis(base) == Some("instructions")
        && basis(Some(branch)) == Some("instructions");
    let mut service = rows(
        &numbers_of(base_window, "service", ""),
        &numbers_of(branch, "service", ""),
    );
    let efficiency = rows(
        &numbers_of(base_window, "efficiency", "efficiency"),
        &numbers_of(branch, "efficiency", "efficiency"),
    );
    let cost = rows(
        &numbers_of(base_window, "cost", "cost"),
        &numbers_of(branch, "cost", "cost"),
    );
    // Wall time and memory, in the order of `NOISY`. A cost per unit of service on wall time is
    // those seconds divided by a count: it says nothing more, so it is not shown.
    let noisy: Vec<Row> = NOISY
        .iter()
        .filter_map(|path| cost.iter().find(|row| row.path == *path))
        .filter(|row| row.base.is_some() || row.branch.is_some())
        .cloned()
        .collect();
    if counted {
        service.extend(efficiency);
    }
    let (changed, same): (Vec<Row>, Vec<Row>) =
        service.into_iter().partition(|row| row.base != row.branch);
    WindowDiff {
        id: branch["id"].as_str().unwrap_or_default().to_owned(),
        from_tick: branch["from_tick"].as_u64().unwrap_or(0),
        to_tick: branch["to_tick"].as_u64().unwrap_or(0),
        changed,
        unchanged: same.iter().filter(|row| row.base.is_some()).count(),
        noisy,
    }
}

/// What makes a scenario's numbers a different game, as the first difference in words.
fn definition_difference(base: &Value, branch: &Value) -> Option<String> {
    for key in ["seats", "seed", "ticks", "sample_every", "start"] {
        if base[key] != branch[key] {
            return Some(format!(
                "{key} is {} on the merge base and {} on this branch",
                base[key], branch[key]
            ));
        }
    }
    let spans = |scenario: &Value| -> Vec<(String, u64, u64)> {
        scenario["windows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|window| {
                (
                    window["id"].as_str().unwrap_or_default().to_owned(),
                    window["from_tick"].as_u64().unwrap_or(0),
                    window["to_tick"].as_u64().unwrap_or(0),
                )
            })
            .collect()
    };
    (spans(base) != spans(branch)).then(|| "the windows differ".to_owned())
}

fn scenario_of<'a>(report: &'a Value, id: &str) -> Option<&'a Value> {
    report["scenarios"]
        .as_array()?
        .iter()
        .find(|scenario| scenario["id"] == id)
}

/// The numbers of the branch alone, for a scenario nothing can be compared with: the
/// headline ones (`numbers::KNOWN`), so the table stays short.
fn alone(branch: &Value) -> WindowDiff {
    let mut window = diff_window(None, branch);
    let numbers = std::mem::take(&mut window.changed);
    window.changed = numbers
        .into_iter()
        .filter(|row| {
            numbers::find(&row.path).is_some_and(|known| known.headline) && row.branch.is_some()
        })
        .collect();
    window.unchanged = 0;
    window
}

pub(super) fn compare(base: Option<&Value>, branch: &Value) -> Vec<ScenarioDiff> {
    let different_map = base
        .map(|base| (&base["header"]["map_hash"], &branch["header"]["map_hash"]))
        .filter(|(base, branch)| base != branch)
        .map(|_| "the merge base was played on another map".to_owned());
    branch["scenarios"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|scenario| {
            let id = scenario["id"].as_str().unwrap_or_default().to_owned();
            let windows = || scenario["windows"].as_array().into_iter().flatten();
            let counterpart = base.and_then(|base| scenario_of(base, &id));
            let refusal = match (base, counterpart) {
                (None, _) => Some(Status::NoBase("no numbers for the merge base".to_owned())),
                (Some(_), None) => Some(Status::NoBase(format!(
                    "the merge base has no scenario {id}"
                ))),
                (Some(_), Some(other)) => different_map
                    .clone()
                    .or_else(|| definition_difference(other, scenario))
                    .map(Status::NotComparable),
            };
            match (refusal, counterpart) {
                (Some(status), _) => ScenarioDiff {
                    id,
                    status,
                    windows: windows().map(alone).collect(),
                },
                (None, other) => ScenarioDiff {
                    status: Status::Compared,
                    windows: windows()
                        .map(|window| {
                            let same = other.and_then(|other| {
                                other["windows"]
                                    .as_array()?
                                    .iter()
                                    .find(|candidate| candidate["id"] == window["id"])
                            });
                            diff_window(same, window)
                        })
                        .collect(),
                    id,
                },
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/compare.rs"]
mod tests;
