//! The calibration seed set (`gates/review/seeds/<area>/<case>.json`): small
//! self-contained changes, each a base file set, a unified diff, the task
//! sentence and the expected result (clean, or one planted defect).
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

/// The area of cases with no planted defect.
pub(super) const CLEAN: &str = "clean";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct Defect {
    /// The persona area it belongs to: correctness, test-integrity, spec,
    /// hostile-input or performance.
    pub(super) area: String,
    /// The file a finding must be about to count as found.
    pub(super) file: String,
    pub(super) what: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Expect {
    Clean(bool),
    Defect(Defect),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Case {
    pub(super) id: String,
    pub(super) area: String,
    pub(super) task: String,
    /// Path to the file's lines.
    pub(super) base: BTreeMap<String, Vec<String>>,
    /// A unified diff, one line per item, for `git apply`.
    pub(super) diff: Vec<String>,
    pub(super) expect: Expect,
}

impl Case {
    /// The planted defect; `None` for a clean case.
    pub(super) fn defect(&self) -> Option<&Defect> {
        match &self.expect {
            Expect::Defect(defect) => Some(defect),
            Expect::Clean(_) => None,
        }
    }

    pub(super) fn patch(&self) -> String {
        format!("{}\n", self.diff.join("\n"))
    }

    pub(super) fn files(&self) -> impl Iterator<Item = (&String, String)> {
        self.base
            .iter()
            .map(|(p, l)| (p, format!("{}\n", l.join("\n"))))
    }

    fn check(&self) -> Result<(), String> {
        let consistent = match &self.expect {
            Expect::Clean(true) => self.area == CLEAN,
            Expect::Clean(false) => false,
            Expect::Defect(defect) => self.area == defect.area && self.area != CLEAN,
        };
        if consistent && !self.diff.is_empty() && !self.base.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "seed {}: area, expectation, base or diff is inconsistent",
                self.id
            ))
        }
    }
}

/// Every case under `dir`, one subdirectory per area, sorted by area and id.
pub(super) fn load(dir: &Path) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    let mut areas: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    areas.sort();
    for area in areas {
        let mut files: Vec<_> = fs::read_dir(&area)
            .map_err(|e| format!("{}: {e}", area.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let case: Case =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            case.check()?;
            cases.push(case);
        }
    }
    Ok(cases)
}

/// The first `limit` cases, taken round-robin across the areas so a small
/// sample still covers every one; all of them without a limit.
pub(super) fn select(cases: Vec<Case>, limit: Option<usize>) -> Vec<Case> {
    let Some(limit) = limit else { return cases };
    let mut areas: Vec<(String, Vec<Case>)> = Vec::new();
    for case in cases {
        match areas.iter_mut().find(|(area, _)| *area == case.area) {
            Some((_, list)) => list.push(case),
            None => areas.push((case.area.clone(), vec![case])),
        }
    }
    let mut picked = Vec::new();
    let mut round = 0;
    while picked.len() < limit && areas.iter().any(|(_, list)| round < list.len()) {
        for (_, list) in &areas {
            if picked.len() < limit && round < list.len() {
                picked.push(list[round].clone());
            }
        }
        round += 1;
    }
    picked
}
