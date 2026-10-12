use super::{
    Reviewer, SEEDS, parse, repo, run,
    seeds::{self, CLEAN, Case},
    summary::{Outcome, Summary},
    write_run,
};
use crate::{
    agents::Runtime,
    review::{Ask, Plan, Report, Tier, review_with},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shipped() -> Vec<Case> {
    seeds::load(&repo_root().join(SEEDS)).expect("the shipped seeds load")
}

/// Reports one finding on the defect file (or on `src/lib.rs` of a clean
/// case) of the given severity, then grades 9.
struct Flags {
    on_planted: &'static str,
    on_clean: &'static str,
    clean: bool,
    file: String,
}

impl Ask for Flags {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        prompts
            .iter()
            .map(|p| {
                if p.contains("# Grader") {
                    return Ok(r#"{"grade": 9, "summary": "ok"}"#.into());
                }
                let severity = if self.clean { self.on_clean } else { self.on_planted };
                Ok(if severity == "none" {
                    r#"{"findings": []}"#.into()
                } else {
                    format!(
                        r#"{{"findings": [{{"file": "{}", "line": 1, "severity": "{severity}", "category": "correctness", "claim": "c", "trigger": "t", "expected_vs_actual": "e", "evidence": "v"}}]}}"#,
                        self.file
                    )
                })
            })
            .collect()
    }
}

fn reviewer(
    on_planted: &'static str,
    on_clean: &'static str,
) -> impl Fn(&Path, &Case) -> Result<Report, String> {
    move |dir, case| {
        let file = case
            .defect()
            .map_or("src/lib.rs".to_owned(), |d| d.file.clone());
        let clean = case.defect().is_none();
        let ask = Flags {
            on_planted,
            on_clean,
            clean,
            file,
        };
        review_with(
            dir,
            Tier::Low,
            Runtime::Claude,
            &case.task,
            &Plan::Full,
            &ask,
        )
        .map_err(|e| e.to_string())
    }
}

#[test]
fn the_shipped_seeds_cover_every_area_and_are_a_third_clean() {
    let cases = shipped();
    assert!(
        (28..=34).contains(&cases.len()),
        "about 30 cases, got {}",
        cases.len()
    );
    let clean = cases.iter().filter(|c| c.area == CLEAN).count();
    assert!(
        clean * 3 >= cases.len() && clean * 2 <= cases.len(),
        "{clean} clean"
    );
    for area in [
        "correctness",
        "test-integrity",
        "spec",
        "hostile-input",
        "performance",
    ] {
        assert!(cases.iter().any(|c| c.area == area), "no {area} case");
    }
    let mut ids: Vec<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), cases.len(), "ids are unique");
    for case in &cases {
        if let Some(defect) = case.defect() {
            let touched = case
                .diff
                .iter()
                .any(|l| l == &format!("+++ b/{}", defect.file));
            assert!(
                touched,
                "{}: the diff does not touch {}",
                case.id, defect.file
            );
        }
    }
}

#[test]
fn every_seed_materialises_and_the_normal_review_runs_on_it() {
    let ok = reviewer("none", "none");
    for case in shipped() {
        let dir =
            repo::materialise(&repo_root(), &case).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        let report = ok(dir.path(), &case).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        assert!(report.passes(), "{}: {}", case.id, report.markdown());
        assert_eq!(
            report.floor, "low",
            "{}: seeds never raise the floor",
            case.id
        );
    }
}

#[test]
fn a_sample_covers_every_area_before_repeating_one() {
    let all = shipped();
    let sample = seeds::select(all.clone(), Some(6));
    let mut areas: Vec<&str> = sample.iter().map(|c| c.area.as_str()).collect();
    areas.sort_unstable();
    areas.dedup();
    assert_eq!(areas.len(), 6);
    assert_eq!(seeds::select(all.clone(), None).len(), all.len());
    assert_eq!(seeds::select(all.clone(), Some(1000)).len(), all.len());
}

#[test]
fn recall_and_false_positives_are_measured_from_the_reports() {
    let cases = seeds::select(shipped(), Some(6));
    let source = repo_root();
    // A reviewer that flags the planted defect and a clean change as major.
    let strict = run(&source, &cases, &reviewer("major", "major"));
    let summary = Summary::new("low", "claude", "m", 8, strict);
    assert!(
        summary
            .areas
            .iter()
            .all(|a| a.recall == Some(1.0) && a.blocked == 1)
    );
    assert_eq!(summary.clean.cases, 1);
    assert_eq!(summary.clean.wrongly_blocked, 1);
    assert_eq!(summary.clean.false_positive_rate, Some(1.0));
    assert_eq!(
        summary.clean.graded_to_merge, 0,
        "a major caps the grade at 7"
    );
    assert_eq!(summary.personas.len(), 1);
    assert_eq!(
        (summary.personas[0].found, summary.personas[0].false_alarms),
        (5, 1)
    );
    // One that finds nothing.
    let lax = run(&source, &cases, &reviewer("none", "none"));
    let summary = Summary::new("low", "claude", "m", 8, lax);
    assert!(
        summary
            .areas
            .iter()
            .all(|a| a.recall == Some(0.0) && a.blocked == 0)
    );
    assert_eq!(summary.clean.wrongly_blocked, 0);
    assert_eq!(summary.clean.graded_to_merge, 1);
    assert!(summary.personas.is_empty());
    // A minor finding on a clean case does not block it.
    let minor = run(&source, &cases, &reviewer("minor", "minor"));
    assert_eq!(
        Summary::new("low", "claude", "m", 8, minor)
            .clean
            .wrongly_blocked,
        0
    );
}

#[test]
fn cases_that_cannot_run_are_results_and_leave_the_rates_alone() {
    let cases = seeds::select(shipped(), Some(2));
    let outcomes = run(&repo_root(), &cases, &|_, _| {
        Err("sessions unavailable".into())
    });
    assert!(
        outcomes
            .iter()
            .all(|o| o.error.as_deref() == Some("sessions unavailable"))
    );
    let summary = Summary::new("low", "claude", "m", 8, outcomes);
    assert_eq!(summary.areas[0].recall, None);
    assert_eq!(summary.clean.false_positive_rate, None);
    let table = summary.markdown();
    assert!(
        table.contains("no result: sessions unavailable") && table.contains("n/a"),
        "{table}"
    );
    let incomplete = Outcome::failed(&cases[0], "round 1: timeout".into());
    assert!(!incomplete.blocked && !incomplete.found);
}

#[test]
fn the_run_writes_json_and_a_markdown_table() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("gates")).unwrap();
    fs::copy(
        repo_root().join("gates/review.json"),
        root.join("gates/review.json"),
    )
    .unwrap();
    let seeds_dir = root.join(SEEDS);
    for case in seeds::select(shipped(), Some(3)) {
        let dir = seeds_dir.join(&case.area);
        fs::create_dir_all(&dir).unwrap();
        fs::copy(
            repo_root()
                .join(SEEDS)
                .join(&case.area)
                .join(format!("{}.json", case.id)),
            dir.join(format!("{}.json", case.id)),
        )
        .unwrap();
    }
    let strict = reviewer("major", "major");
    let review: Reviewer = &strict;
    write_run(root, Tier::Low, Runtime::Claude, None, review).unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("reports/review/calibration.json")).unwrap())
            .unwrap();
    assert_eq!(json["tier"], "low");
    assert_eq!(json["outcomes"].as_array().unwrap().len(), 3);
    let table = fs::read_to_string(root.join("reports/review/calibration.md")).unwrap();
    assert!(
        table.contains("## Planted defects (recall)") && table.contains("## Clean cases"),
        "{table}"
    );
    let empty = tempfile::tempdir().unwrap();
    fs::create_dir_all(empty.path().join(SEEDS)).unwrap();
    assert!(write_run(empty.path(), Tier::Low, Runtime::Claude, None, review).is_err());
}

#[test]
fn the_environment_is_parsed_strictly() {
    let some = |s: &str| Some(s.to_owned());
    let (tier, runtime, limit) = parse(None, None, None).unwrap();
    assert_eq!((tier, runtime, limit), (Tier::Low, Runtime::Claude, None));
    let (tier, runtime, limit) = parse(some("high"), some("codex"), some("6")).unwrap();
    assert_eq!(
        (tier, runtime, limit),
        (Tier::High, Runtime::Codex, Some(6))
    );
    assert!(parse(some("huge"), None, None).is_err());
    assert!(parse(None, some("gemini"), None).is_err());
    assert!(parse(None, None, some("0")).is_err());
    assert!(parse(None, None, some("many")).is_err());
}

#[test]
fn a_broken_seed_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("clean");
    fs::create_dir_all(&dir).unwrap();
    // Says it is clean but sits under a defect area.
    fs::write(
        dir.join("x.json"),
        r#"{"id": "x", "area": "spec", "task": "t", "base": {"a": ["b"]}, "diff": ["d"], "expect": {"clean": true}}"#,
    )
    .unwrap();
    assert!(
        seeds::load(temp.path())
            .unwrap_err()
            .contains("inconsistent")
    );
    fs::write(dir.join("x.json"), "{").unwrap();
    assert!(seeds::load(temp.path()).is_err());
    assert!(seeds::load(&temp.path().join("missing")).is_err());
}
