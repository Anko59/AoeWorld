//! What a calibration run measured, computed from the reports alone: per
//! planted-defect area the recall, per persona what it found and what it
//! wrongly raised on clean cases, and how often clean cases were blocked.
use super::seeds::{CLEAN, Case};
use crate::review::{
    Report,
    protocol::{Severity, Status},
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(super) struct Outcome {
    pub(super) id: String,
    pub(super) area: String,
    /// The planted defect, empty for a clean case.
    pub(super) planted: String,
    /// Why the review gave no result (it could not run or was incomplete).
    pub(super) error: Option<String>,
    pub(super) grade: u8,
    pub(super) written_grade: u8,
    /// A complete review that did not pass: blocking finding or grade < merge.
    pub(super) blocked: bool,
    /// Planted defect: a standing finding of at least minor on its file.
    pub(super) found: bool,
    /// Personas that reported it (planted) or that reported a standing major
    /// or critical finding (clean).
    pub(super) raised_by: Vec<String>,
}

impl Outcome {
    pub(super) fn failed(case: &Case, error: String) -> Self {
        Self {
            id: case.id.clone(),
            area: case.area.clone(),
            planted: case.defect().map(|d| d.what.clone()).unwrap_or_default(),
            error: Some(error),
            grade: 0,
            written_grade: 0,
            blocked: false,
            found: false,
            raised_by: Vec::new(),
        }
    }

    pub(super) fn from_report(case: &Case, report: &Report) -> Self {
        let mut outcome = Self::failed(case, String::new());
        outcome.error = (!report.complete()).then(|| report.failures.join("; "));
        outcome.grade = report.grade;
        outcome.written_grade = report.written_grade;
        outcome.blocked = report.complete() && !report.passes();
        let mut raised = Vec::new();
        for finding in report
            .findings
            .iter()
            .filter(|f| f.status != Status::Refuted)
        {
            let r = &finding.reported;
            let counts = match case.defect() {
                Some(defect) => {
                    let on_file =
                        r.file == defect.file || r.file.ends_with(&format!("/{}", defect.file));
                    on_file && r.severity != Severity::Nit
                }
                None => r.severity <= Severity::Major,
            };
            if !counts {
                continue;
            }
            outcome.found = true;
            if let Some(persona) = report.personas.get(finding.reporter) {
                raised.push(persona.clone());
            }
        }
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
    pub(super) found: usize,
    /// Found over answered cases.
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
                let found = ok.iter().filter(|o| o.found).count();
                let grades: Vec<u8> = ok.iter().map(|o| o.grade).collect();
                AreaRow {
                    area: (*name).to_owned(),
                    cases: of.len(),
                    found,
                    recall: ratio(found, ok.len()),
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
            "# Review calibration: tier {}, runtime {} ({}), {} cases\n\nA tool, not a gate. Merge grade {}. Recall counts a standing finding of at least minor on the planted defect's file; a clean case is wrongly blocked when its complete review does not pass.\n\n",
            self.tier,
            self.runtime,
            self.model,
            self.outcomes.len(),
            self.merge_grade
        );
        out.push_str("## Planted defects (recall)\n\n| Area | Cases | Found | Recall | Blocked | Mean grade | No result |\n|---|---|---|---|---|---|---|\n");
        for a in &self.areas {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} |\n",
                a.area,
                a.cases,
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
            let found = match (o.planted(), o.found) {
                (false, _) => "-",
                (true, true) => "yes",
                (true, false) => "no",
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
