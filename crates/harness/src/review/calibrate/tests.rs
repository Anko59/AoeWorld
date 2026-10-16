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

mod matching;
mod probes;
mod validate;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shipped() -> Vec<Case> {
    seeds::load(&repo_root().join(SEEDS)).expect("the shipped seeds load")
}

/// Reports one finding on the planted defects' files (or on `src/lib.rs` of a
/// clean case) of the given severity, then grades 9. `planted` findings are
/// placed on each defect's own first line, so recall can be measured per
/// defect.
struct Flags {
    planted: Vec<(String, u32)>,
    clean: bool,
    on_planted: &'static str,
    on_clean: &'static str,
}

impl Flags {
    fn finding(file: &str, line: u32, severity: &str) -> String {
        format!(
            r#"{{"file": "{file}", "line": {line}, "severity": "{severity}", "category": "correctness", "claim": "c", "trigger": "t", "expected_vs_actual": "e", "evidence": "v"}}"#
        )
    }
}

impl Ask for Flags {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        prompts
            .iter()
            .map(|p| {
                if p.contains("# Grader") {
                    return Ok(r#"{"grade": 9, "summary": "ok"}"#.into());
                }
                let severity = if self.clean {
                    self.on_clean
                } else {
                    self.on_planted
                };
                let findings: Vec<String> = if severity == "none" {
                    Vec::new()
                } else if self.clean {
                    vec![Self::finding("src/lib.rs", 1, severity)]
                } else {
                    self.planted
                        .iter()
                        .map(|(file, line)| Self::finding(file, *line, severity))
                        .collect()
                };
                Ok(format!(r#"{{"findings": [{}]}}"#, findings.join(",")))
            })
            .collect()
    }
}

fn reviewer(
    on_planted: &'static str,
    on_clean: &'static str,
) -> impl Fn(&Path, &Case) -> Result<Report, String> {
    move |dir, case| {
        let ask = Flags {
            planted: case
                .defects()
                .iter()
                .map(|d| (d.file.clone(), d.lines[0]))
                .collect(),
            clean: case.defects().is_empty(),
            on_planted,
            on_clean,
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
        (38..=50).contains(&cases.len()),
        "about 44 cases, got {}",
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
        for defect in case.defects() {
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
fn the_harder_seeds_plant_what_issue_377_asks_for() {
    let cases = shipped();
    let harder = |c: &Case| c.id.starts_with("hard-") || c.id.starts_with("clean-hard-");
    let hard: Vec<&Case> = cases.iter().filter(|c| harder(c)).collect();
    assert_eq!(hard.len(), 14, "fourteen harder seeds, got {}", hard.len());
    // Three of them plant two defects of different severity, to measure
    // partial recall.
    let two: Vec<&Case> = hard
        .iter()
        .copied()
        .filter(|c| c.defects().len() == 2)
        .collect();
    assert!(two.len() >= 3, "{} two-defect cases", two.len());
    for case in &two {
        let mut severities: Vec<_> = case.defects().iter().map(|d| d.severity).collect();
        severities.sort();
        severities.dedup();
        assert!(
            severities.len() == 2,
            "{}: its two defects must differ in severity",
            case.id
        );
    }
    // Five of them are clean changes that look suspicious.
    let clean = hard
        .iter()
        .copied()
        .filter(|c| c.defects().is_empty())
        .count();
    assert_eq!(clean, 5, "{clean} clean harder seeds");
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
    areas.sort();
    areas.dedup();
    assert_eq!(areas.len(), 6);
    assert_eq!(seeds::select(all.clone(), None).len(), all.len());
    assert_eq!(seeds::select(all.clone(), Some(1000)).len(), all.len());
}

#[test]
fn recall_counts_findings_of_at_least_the_planted_severity_per_defect() {
    let all = shipped();
    let two: Vec<Case> = all.into_iter().filter(|c| c.defects().len() == 2).collect();
    assert!(!two.is_empty(), "no two-defect cases shipped");
    // A reviewer that reports every defect as minor finds only the minor
    // ones: half of each two-defect case, and none of the majors.
    let minor = run(&repo_root(), &two, &reviewer("minor", "none"));
    for outcome in &minor {
        let minors = two
            .iter()
            .find(|c| c.id == outcome.id)
            .unwrap()
            .defects()
            .iter()
            .filter(|d| matches!(d.severity, crate::review::protocol::Severity::Minor))
            .count();
        assert_eq!(
            (outcome.defects, outcome.found),
            (2, minors),
            "{}: partial recall not measured",
            outcome.id
        );
    }
    let summary = Summary::new("low", "claude", "m", 8, minor);
    assert!(
        summary.areas.iter().all(|a| a.recall < Some(1.0)),
        "a minor-only reviewer cannot find every planted defect"
    );
    // Reported below the planted severity, a defect counts as missed.
    let nit = run(&repo_root(), &two, &reviewer("nit", "none"));
    assert!(nit.iter().all(|o| o.found == 0));
}

#[test]
fn recall_and_false_positives_are_measured_from_the_reports() {
    let cases = seeds::select(shipped(), Some(6));
    let source = repo_root();
    // A reviewer that flags every planted defect as critical and a clean
    // change as major.
    let strict = run(&source, &cases, &reviewer("critical", "major"));
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
        (7, 1)
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
    assert!(!incomplete.blocked && incomplete.found == 0);
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
        let to = seeds_dir.join(&case.path);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(repo_root().join(SEEDS).join(&case.path), &to).unwrap();
    }
    let strict = reviewer("critical", "major");
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
    let cases = [
        (
            "clean/x.json",
            r#"{"id": "x", "area": "spec", "task": "t", "base": {"a": ["b"]}, "diff": ["d"], "expect": {"clean": true}}"#,
            "sits under",
        ),
        (
            "spec/x.json",
            r#"{"id": "x", "area": "spec", "task": "t", "base": {"a": ["b"]}, "diff": ["d"], "expect": {"clean": true}}"#,
            "inconsistent",
        ),
        (
            "spec/y.json",
            r#"{"id": "y", "area": "spec", "task": "t", "base": {"a": ["b"]}, "diff": ["+++ b/a"], "expect": {"defects": [{"area": "spec", "file": "a", "what": "w", "severity": "major"}]}}"#,
            "missing field",
        ),
        (
            "spec/z.json",
            r#"{"id": "z", "area": "correctness", "task": "t", "base": {"a": ["b"]}, "diff": ["+++ b/a"], "expect": {"defects": [{"area": "correctness", "file": "a", "lines": [1, 1], "severity": "minor", "what": "w"}]}}"#,
            "sits under",
        ),
        ("clean/x.json", "{", "EOF"),
    ];
    for (file, contents, reason) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        let error = seeds::load(temp.path()).unwrap_err();
        assert!(error.contains(file), "{file}: {error}");
        assert!(error.contains(reason), "{file}: {error}");
    }
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let error = seeds::load(&missing).unwrap_err();
    assert!(error.contains(&missing.display().to_string()), "{error}");
}

#[test]
fn a_case_repository_carries_every_runtimes_role_hooks_to_the_source_judge() {
    let case = shipped().remove(0);
    let dir = repo::materialise(&repo_root(), &case).unwrap();
    for relative in super::hooks::CONFIG {
        assert!(dir.path().join(relative).is_file(), "{relative} missing");
    }
    for relative in [
        ".claude/settings.json",
        ".codex/hooks.json",
        ".dsh/hooks.json",
    ] {
        let text = fs::read_to_string(dir.path().join(relative)).unwrap();
        assert!(text.contains(".agents/hooks/harness.sh"), "{relative}");
    }
    // Codex's inline hooks are read from the case repository, so it can start.
    assert!(crate::agents::launch::codex_hooks(dir.path()).is_ok());
    // The launcher judges the case repository with the source checkout's launcher.
    let source = tempfile::tempdir().unwrap();
    for relative in super::hooks::CONFIG {
        let to = source.path().join(relative);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(repo_root().join(relative), to).unwrap();
    }
    let fake = source.path().join(super::hooks::LAUNCHER);
    fs::create_dir_all(fake.parent().unwrap()).unwrap();
    fs::write(&fake, "#!/bin/sh\necho \"$AOE_AGENT_HOOK_ROOT $*\"\n").unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let target = tempfile::tempdir().unwrap();
    super::hooks::install(source.path(), target.path()).unwrap();
    let out = std::process::Command::new(target.path().join(super::hooks::LAUNCHER))
        .args(["claude", "pre-tool-use"])
        .output()
        .unwrap();
    let judged = target.path().canonicalize().unwrap();
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        format!("{} claude pre-tool-use", judged.display())
    );
    // A source without the configuration cannot enforce the role: refused.
    let bare = tempfile::tempdir().unwrap();
    assert!(super::hooks::install(bare.path(), target.path()).is_err());
}
