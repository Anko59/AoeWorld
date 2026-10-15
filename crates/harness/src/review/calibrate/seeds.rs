//! The calibration seed set (`gates/review/seeds/<area>/<case>.json`, harder
//! cases one level deeper under `hard/<area>/`): small self-contained
//! changes, each a base file set, a unified diff, the task sentence and the
//! expected result — clean, or one or more planted defects, each naming its
//! file, its line range in the file as the diff leaves it, and the severity a
//! correct review assigns.
use crate::review::protocol::Severity;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

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
    /// The first and last line (inclusive) of the defect in `file` as the
    /// diff leaves it; a finding must land inside to count as found.
    pub(super) lines: [u32; 2],
    /// The severity a correct review assigns. `nit` is refused: a nit is not
    /// a defect, and recall counts findings of at least this severity.
    pub(super) severity: Severity,
    pub(super) what: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Expect {
    Clean(bool),
    /// One defect, or several of different severity to measure partial
    /// recall.
    Defects(Vec<Defect>),
}

impl Expect {
    fn defects(&self) -> Option<&[Defect]> {
        match self {
            Expect::Defects(defects) => Some(defects),
            Expect::Clean(_) => None,
        }
    }
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
    /// Where the case was read from, relative to the seeds directory.
    #[serde(skip)]
    pub(super) path: PathBuf,
}

impl Case {
    /// The planted defects; empty for a clean case.
    pub(super) fn defects(&self) -> &[Defect] {
        self.expect.defects().unwrap_or(&[])
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
        let defects = self.defects();
        let consistent = match (&self.expect, defects.is_empty()) {
            (Expect::Clean(true), true) => self.area == CLEAN,
            (Expect::Defects(_), false) => {
                self.area != CLEAN && defects.iter().all(|d| d.area == self.area)
            }
            _ => false,
        };
        let lines = defects
            .iter()
            .all(|d| d.lines[0] >= 1 && d.lines[0] <= d.lines[1]);
        let severity = defects.iter().all(|d| d.severity != Severity::Nit);
        let touched = defects
            .iter()
            .all(|d| self.diff.iter().any(|l| l == &format!("+++ b/{}", d.file)));
        if consistent
            && lines
            && severity
            && touched
            && !self.diff.is_empty()
            && !self.base.is_empty()
        {
            Ok(())
        } else {
            Err(format!(
                "seed {}: area, expectation, base, diff, defect lines, severity or defect file is inconsistent",
                self.id
            ))
        }
    }
}

/// The `.json` files of `dir`, as `(area, path)` pairs: an area holds its
/// cases directly, and a grouping directory such as `hard/` holds one area
/// directory of cases per name.
fn cases_under(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let mut found = Vec::new();
    let top: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    for area_dir in top {
        for entry in fs::read_dir(&area_dir)
            .map_err(|e| format!("{}: {e}", area_dir.display()))?
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let json = |p: &Path| {
                p.extension()
                    .is_some_and(|x| x == "json")
                    .then(|| p.to_path_buf())
            };
            match (entry.file_type().is_ok_and(|t| t.is_dir()), json(&path)) {
                (true, _) => {
                    let area = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    for case in fs::read_dir(&path)
                        .map_err(|e| format!("{}: {e}", path.display()))?
                        .filter_map(|e| e.ok())
                    {
                        let file = case.path();
                        if case.file_type().is_ok_and(|t| t.is_dir()) {
                            return Err(format!(
                                "{}: areas nest at most two levels deep",
                                file.display()
                            ));
                        }
                        if let Some(file) = json(&file) {
                            found.push((area.clone(), file));
                        }
                    }
                }
                (false, Some(file)) => {
                    let area = area_dir
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    found.push((area, file));
                }
                (false, None) => {}
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Every case under `dir`, one or two levels of area directories, sorted by
/// area and id.
pub(super) fn load(dir: &Path) -> Result<Vec<Case>, String> {
    let mut cases = Vec::new();
    for (area, file) in cases_under(dir)? {
        let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut case: Case =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        if case.area != area {
            return Err(format!(
                "{}: sits under `{area}` but says area `{}`",
                file.display(),
                case.area
            ));
        }
        case.check()?;
        case.path = file
            .strip_prefix(dir)
            .map_err(|e| format!("{}: {e}", file.display()))?
            .to_path_buf();
        cases.push(case);
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
