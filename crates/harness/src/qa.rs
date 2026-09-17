//! Provider-neutral exploratory QA report contract and validation.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, error::Error, fs, path::Path};

pub const REQUIRED: [&str; 6] = [
    "startup",
    "camera-and-zoom",
    "multiple-clients",
    "reconnect",
    "invalid-configuration",
    "capability-failure",
];

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Pass,
    Findings,
    Blocked,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Journey {
    pub name: String,
    pub completed: bool,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Finding {
    pub title: String,
    pub reproduction: String,
    pub expected: String,
    pub actual: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub version: u16,
    pub budget: String,
    pub build: String,
    pub scenario: String,
    pub status: Status,
    pub journeys: Vec<Journey>,
    pub findings: Vec<Finding>,
}

pub fn validate(report: &Report) -> Result<(), String> {
    if report.version != 1 {
        return Err("unsupported QA report version".into());
    }
    if !["fast", "full", "extended"].contains(&report.budget.as_str()) {
        return Err("invalid QA budget".into());
    }
    if report.build.trim().is_empty() || report.scenario.trim().is_empty() {
        return Err("build and scenario are required".into());
    }
    let mut seen = BTreeSet::new();
    for journey in &report.journeys {
        if !seen.insert(journey.name.as_str()) {
            return Err(format!("duplicate journey {}", journey.name));
        }
        if journey.evidence.iter().any(|item| item.trim().is_empty()) {
            return Err("empty journey evidence".into());
        }
    }
    for finding in &report.findings {
        if finding.title.trim().is_empty()
            || finding.reproduction.trim().is_empty()
            || finding.expected.trim().is_empty()
            || finding.actual.trim().is_empty()
            || finding.evidence.is_empty()
        {
            return Err("finding lacks reproduction, expected/actual, or evidence".into());
        }
    }
    match report.status {
        Status::Pass => {
            if !report.findings.is_empty() {
                return Err("PASS report contains findings".into());
            }
            for name in REQUIRED {
                let Some(journey) = report.journeys.iter().find(|item| item.name == name) else {
                    return Err(format!("required journey {name} is missing"));
                };
                if !journey.completed || journey.evidence.is_empty() {
                    return Err(format!(
                        "required journey {name} lacks completion or evidence"
                    ));
                }
            }
        }
        Status::Findings if report.findings.is_empty() => {
            return Err("FINDINGS report has no findings".into());
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_file(path: &Path) -> Result<(), Box<dyn Error>> {
    validate_file_at(path, Path::new("reports/qa"))
}

fn validate_file_at(path: &Path, evidence_root: &Path) -> Result<(), Box<dyn Error>> {
    let report: Report = serde_json::from_slice(&fs::read(path)?)?;
    validate(&report).map_err(|error| -> Box<dyn Error> { error.into() })?;
    for evidence in report
        .journeys
        .iter()
        .flat_map(|journey| &journey.evidence)
        .chain(report.findings.iter().flat_map(|finding| &finding.evidence))
    {
        validate_evidence_at(evidence_root, Path::new(evidence))
            .map_err(|error| -> Box<dyn Error> { error.into() })?;
    }
    println!(
        "validated QA report: {} ({:?})",
        path.display(),
        report.status
    );
    Ok(())
}

pub(crate) fn validate_evidence_at(root: &Path, path: &Path) -> Result<(), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let candidate = path.canonicalize().map_err(|error| error.to_string())?;
    if !candidate.starts_with(root) || !candidate.is_file() {
        return Err("QA evidence must be an existing file under reports/qa".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(status: Status) -> Report {
        Report {
            version: 1,
            budget: "fast".into(),
            build: "abc".into(),
            scenario: "smoke".into(),
            status,
            journeys: REQUIRED
                .iter()
                .map(|name| Journey {
                    name: (*name).into(),
                    completed: true,
                    evidence: vec!["screen.png".into()],
                })
                .collect(),
            findings: Vec::new(),
        }
    }

    #[test]
    fn incomplete_qa_cannot_pass() {
        let mut item = report(Status::Pass);
        assert!(validate(&item).is_ok());
        item.journeys[2].completed = false;
        assert!(validate(&item).is_err());
        item.journeys[2].completed = true;
        item.journeys[3].evidence.clear();
        assert!(validate(&item).is_err());
        item.journeys[3].evidence.push("shot.png".into());
        item.findings.push(Finding {
            title: "bug".into(),
            reproduction: "steps".into(),
            expected: "yes".into(),
            actual: "no".into(),
            evidence: vec!["shot.png".into()],
        });
        assert!(validate(&item).is_err());
    }

    #[test]
    fn report_status_and_evidence_validation_rejects_ambiguous_claims() {
        let mut item = report(Status::Pass);
        item.version = 2;
        assert!(validate(&item).is_err());
        item.version = 1;
        item.budget = "infinite".into();
        assert!(validate(&item).is_err());
        item.budget = "fast".into();
        item.journeys.push(Journey {
            name: REQUIRED[0].into(),
            completed: true,
            evidence: vec!["screen.png".into()],
        });
        assert!(validate(&item).is_err());
        item.journeys.pop();
        item.journeys.remove(0);
        assert!(validate(&item).is_err());
        item.status = Status::Findings;
        assert!(validate(&item).is_err());
        item.status = Status::Blocked;
        assert!(validate(&item).is_ok());

        let temp = tempfile::tempdir().expect("directory");
        let root = temp.path().join("reports/qa");
        fs::create_dir_all(&root).expect("evidence directory");
        let inside = root.join("screen.png");
        let outside = temp.path().join("outside.png");
        fs::write(&inside, b"screen").expect("inside");
        fs::write(&outside, b"outside").expect("outside");
        assert!(validate_evidence_at(&root, &inside).is_ok());
        assert!(validate_evidence_at(&root, &outside).is_err());
        assert!(validate_evidence_at(&root, &root.join("missing.png")).is_err());
    }

    #[test]
    fn report_file_requires_existing_evidence_within_the_qa_directory() {
        let temp = tempfile::tempdir().expect("directory");
        let root = temp.path().join("qa");
        fs::create_dir_all(&root).expect("evidence directory");
        let inside = root.join("screen.png");
        let outside = temp.path().join("outside.png");
        fs::write(&inside, b"screen").expect("inside");
        fs::write(&outside, b"outside").expect("outside");
        let path = root.join("session.json");
        let mut item = report(Status::Pass);
        for journey in &mut item.journeys {
            journey.evidence = vec![inside.to_string_lossy().into_owned()];
        }
        fs::write(&path, serde_json::to_vec(&item).expect("JSON")).expect("report");
        validate_file_at(&path, &root).expect("valid evidence");
        item.journeys[0].evidence = vec![outside.to_string_lossy().into_owned()];
        fs::write(&path, serde_json::to_vec(&item).expect("JSON")).expect("report");
        assert!(validate_file_at(&path, &root).is_err());
        item.journeys[0].evidence = vec!["missing.png".into()];
        fs::write(&path, serde_json::to_vec(&item).expect("JSON")).expect("report");
        assert!(validate_file_at(&path, &root).is_err());
        item.journeys[0].evidence.clear();
        fs::write(&path, serde_json::to_vec(&item).expect("JSON")).expect("report");
        assert!(validate_file_at(&path, &root).is_err());
    }
}
