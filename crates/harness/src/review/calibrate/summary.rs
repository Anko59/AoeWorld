//! What a calibration run measured, computed from the reports alone: per
//! planted-defect area the recall, per persona what it found and what it
//! wrongly raised on clean cases, and how often clean cases were blocked.
use super::seeds::{CLEAN, Case, Defect};
use crate::review::{
    Report,
    protocol::{Reported, Severity, Status},
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(super) struct Outcome {
    pub(super) id: String,
    pub(super) area: String,
    /// The planted defects joined with `; `, empty for a clean case.
    pub(super) planted: String,
    /// Why the review gave no result (it could not run or was incomplete).
    pub(super) error: Option<String>,
    pub(super) grade: u8,
    pub(super) written_grade: u8,
    /// A complete review that did not pass: blocking finding or grade < merge.
    pub(super) blocked: bool,
    /// How many planted defects the review found (see [`Defect`]): a
    /// standing finding of at least the planted severity, on its file within
    /// its lines. Clean cases have none planted.
    pub(super) defects: usize,
    pub(super) found: usize,
    /// Personas that reported it (planted) or that reported a standing major
    /// or critical finding (clean).
    pub(super) raised_by: Vec<String>,
}

impl Defect {
    /// Whether `finding`, standing, is this defect being found.
    fn reported_by(&self, reported: &Reported) -> bool {
        let on_file =
            reported.file == self.file || reported.file.ends_with(&format!("/{}", self.file));
        let in_lines = reported
            .line
            .is_some_and(|l| l >= self.lines[0] && l <= self.lines[1]);
        on_file && in_lines && reported.severity <= self.severity
    }
}

impl Outcome {
    pub(super) fn failed(case: &Case, error: String) -> Self {
        Self {
            id: case.id.clone(),
            area: case.area.clone(),
            planted: case
                .defects()
                .iter()
                .map(|d| d.what.as_str())
                .collect::<Vec<_>>()
                .join("; "),
            error: Some(error),
            grade: 0,
            written_grade: 0,
            blocked: false,
            defects: case.defects().len(),
            found: 0,
            raised_by: Vec::new(),
        }
    }

    pub(super) fn from_report(case: &Case, report: &Report) -> Self {
        let mut outcome = Self::failed(case, String::new());
        outcome.error = (!report.complete()).then(|| report.failures.join("; "));
        outcome.grade = report.grade;
        outcome.written_grade = report.written_grade;
        outcome.blocked = report.complete() && !report.passes();
        let defects = case.defects();
        let mut hit = vec![false; defects.len()];
        let mut raised = Vec::new();
        for finding in report
            .findings
            .iter()
            .filter(|f| f.status != Status::Refuted)
        {
            let r = &finding.reported;
            let counts = if defects.is_empty() {
                r.severity <= Severity::Major
            } else {
                let mut any = false;
                for (i, defect) in defects.iter().enumerate() {
                    if defect.reported_by(r) {
                        hit[i] = true;
                        any = true;
                    }
                }
                any
            };
            if !counts {
                continue;
            }
            if let Some(persona) = report.personas.get(finding.reporter) {
                raised.push(persona.clone());
            }
        }
        outcome.found = hit.iter().filter(|hit| **hit).count();
        raised.sort();
        raised.dedup();
        outcome.raised_by = raised;
        outcome
    }

    fn planted(&self) -> bool {
        self.area != CLEAN
    }

    fn answered(&self) -> bool {
        self.error.is_none()
    }
}

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct AreaRow {
    pub(super) area: String,
    pub(super) cases: usize,
    /// Defects planted in the answered cases (a case may plant several).
    pub(super) defects: usize,
    /// Defects found, so a case with two planted defects can be half found.
    pub(super) found: usize,
    /// Defects found over defects planted in answered cases.
    pub(super) recall: Option<f64>,
    pub(super) blocked: usize,
    pub(super) mean_grade: Option<f64>,
    pub(super) errors: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct PersonaRow {
    pub(super) persona: String,
    /// Planted defects it reported.
    pub(super) found: usize,
    /// Clean cases on which it raised a standing major or critical finding.
    pub(super) false_alarms: usize,
}

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct CleanRow {
    pub(super) cases: usize,
    pub(super) wrongly_blocked: usize,
    pub(super) false_positive_rate: Option<f64>,
    /// Clean cases graded at least the merge grade.
    pub(super) graded_to_merge: usize,
    pub(super) mean_grade: Option<f64>,
    pub(super) errors: usize,
}

#[derive(Debug, Serialize)]
pub(super) struct Summary {
    pub(super) tier: String,
    pub(super) runtime: String,
    pub(super) model: String,
    pub(super) merge_grade: u8,
    pub(super) outcomes: Vec<Outcome>,
    pub(super) areas: Vec<AreaRow>,
    pub(super) personas: Vec<PersonaRow>,
    pub(super) clean: CleanRow,
}

fn mean(grades: &[u8]) -> Option<f64> {
    (!grades.is_empty())
        .then(|| grades.iter().map(|g| f64::from(*g)).sum::<f64>() / grades.len() as f64)
}

fn ratio(part: usize, whole: usize) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

impl Summary {
    pub(super) fn new(
        tier: &str,
        runtime: &str,
        model: &str,
        merge_grade: u8,
        outcomes: Vec<Outcome>,
    ) -> Self {
        let mut names: Vec<&str> = outcomes
            .iter()
            .filter(|o| o.planted())
            .map(|o| o.area.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        let areas = names
            .iter()
            .map(|name| {
                let of: Vec<&Outcome> = outcomes.iter().filter(|o| o.area == *name).collect();
                let ok: Vec<&&Outcome> = of.iter().filter(|o| o.answered()).collect();
                let defects: usize = ok.iter().map(|o| o.defects).sum();
                let found: usize = ok.iter().map(|o| o.found).sum();
                let grades: Vec<u8> = ok.iter().map(|o| o.grade).collect();
                AreaRow {
                    area: (*name).to_owned(),
                    cases: of.len(),
                    defects,
                    found,
                    recall: ratio(found, defects),
                    blocked: ok.iter().filter(|o| o.blocked).count(),
                    mean_grade: mean(&grades),
                    errors: of.len() - ok.len(),
                }
            })
            .collect();
        let clean_of: Vec<&Outcome> = outcomes.iter().filter(|o| !o.planted()).collect();
        let clean_ok: Vec<&&Outcome> = clean_of.iter().filter(|o| o.answered()).collect();
        let wrongly_blocked = clean_ok.iter().filter(|o| o.blocked).count();
        let grades: Vec<u8> = clean_ok.iter().map(|o| o.grade).collect();
        let clean = CleanRow {
            cases: clean_of.len(),
            wrongly_blocked,
            false_positive_rate: ratio(wrongly_blocked, clean_ok.len()),
            graded_to_merge: grades.iter().filter(|g| **g >= merge_grade).count(),
            mean_grade: mean(&grades),
            errors: clean_of.len() - clean_ok.len(),
        };
        let mut personas: Vec<String> = outcomes
            .iter()
            .flat_map(|o| o.raised_by.iter().cloned())
            .collect();
        personas.sort();
        personas.dedup();
        let personas = personas
            .into_iter()
            .map(|persona| {
                let by = |planted: bool| {
                    outcomes
                        .iter()
                        .filter(|o| o.planted() == planted && o.answered())
                        .filter(|o| o.raised_by.contains(&persona))
                        .count()
                };
                PersonaRow {
                    found: by(true),
                    false_alarms: by(false),
                    persona,
                }
            })
            .collect();
        Self {
            tier: tier.to_owned(),
            runtime: runtime.to_owned(),
            model: model.to_owned(),
            merge_grade,
            outcomes,
            areas,
            personas,
            clean,
        }
    }

    pub(super) fn markdown(&self) -> String {
        let pct = |v: Option<f64>| v.map_or("n/a".to_owned(), |v| format!("{:.0}%", v * 100.0));
        let grade = |v: Option<f64>| v.map_or("n/a".to_owned(), |v| format!("{v:.1}"));
        let mut out = format!(
            "# Review calibration: tier {}, runtime {} ({}), {} cases\n\nA tool, not a gate. Merge grade {}. Recall counts a standing finding of at least the planted severity, on the defect's file within its lines, so a case with two planted defects can be half found; a clean case is wrongly blocked when its complete review does not pass.\n\n",
            self.tier,
            self.runtime,
            self.model,
            self.outcomes.len(),
            self.merge_grade
        );
        out.push_str("## Planted defects (recall)\n\n| Area | Cases | Defects | Found | Recall | Blocked | Mean grade | No result |\n|---|---|---|---|---|---|---|---|\n");
        for a in &self.areas {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
                a.area,
                a.cases,
                a.defects,
                a.found,
                pct(a.recall),
                a.blocked,
                grade(a.mean_grade),
                a.errors
            ));
        }
        let c = &self.clean;
        out.push_str(&format!(
            "\n## Clean cases (false positives)\n\n| Cases | Wrongly blocked | False-positive rate | Graded {}+ | Mean grade | No result |\n|---|---|---|---|---|---|\n| {} | {} | {} | {} | {} | {} |\n",
            self.merge_grade, c.cases, c.wrongly_blocked, pct(c.false_positive_rate), c.graded_to_merge, grade(c.mean_grade), c.errors
        ));
        out.push_str("\n## Personas\n\n| Persona | Planted defects reported | Clean cases with a major finding |\n|---|---|---|\n");
        for p in &self.personas {
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                p.persona, p.found, p.false_alarms
            ));
        }
        out.push_str(
            "\n## Cases\n\n| Case | Planted | Found | Grade | Result |\n|---|---|---|---|---|\n",
        );
        for o in &self.outcomes {
            let result = match (&o.error, o.blocked) {
                (Some(error), _) => {
                    format!("no result: {}", error.chars().take(80).collect::<String>())
                }
                (None, true) => "blocked".to_owned(),
                (None, false) => "passed".to_owned(),
            };
            let found = match o.planted() {
                false => "-".to_owned(),
                true => format!("{}/{}", o.found, o.defects),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {}/10 | {} |\n",
                o.id,
                if o.planted() { "yes" } else { "no" },
                found,
                o.grade,
                result
            ));
        }
        out
    }
}
