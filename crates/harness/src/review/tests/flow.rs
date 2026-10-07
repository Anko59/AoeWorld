use super::super::{Ask, Tier, protocol::Status, report, review_with};
use crate::agents::Runtime;
use std::{fs, path::Path, process::Command, sync::Mutex};

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A checkout on `feature` with the real registry and review prompts, and
/// an `origin/dev` to compare with; `changed` is the file the branch edits.
fn fixture(changed: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q", "-b", "dev"]);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for file in [
        "gates/registry.json",
        "gates/review.json",
        "gates/review/preamble.md",
        "gates/review/grader.md",
        "gates/review/personas/quick.md",
        "gates/review/personas/correctness.md",
        "gates/review/personas/spec.md",
        "gates/review/personas/test-integrity.md",
        "gates/review/personas/hostile-input.md",
        "gates/review/personas/performance.md",
    ] {
        fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
        fs::copy(repo.join(file), root.join(file)).unwrap();
    }
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "base"]);
    git(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    fs::write(root.join(changed), "changed\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "change"]);
    temp
}

/// Answers by prompt kind and persona; records how many sessions ran.
struct Script {
    first: fn(&str) -> String,
    cross: fn(&str) -> String,
    grade: String,
    sessions: Mutex<usize>,
}

/// The runner hands back the JSON already cut from between the markers.
fn wrap(json: &str) -> String {
    json.to_owned()
}

impl Ask for Script {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        *self.sessions.lock().unwrap() += prompts.len();
        prompts
            .iter()
            .map(|p| {
                if p.contains("# Grader") {
                    Ok(wrap(&self.grade))
                } else if p.contains("verify the other reviewers' findings") {
                    Ok(wrap(&(self.cross)(p)))
                } else {
                    Ok(wrap(&(self.first)(p)))
                }
            })
            .collect()
    }
}

const MAJOR: &str = r#"{"findings": [{"file": "README.md", "line": 1, "severity": "major", "category": "correctness", "claim": "wrong text", "trigger": "read it", "expected_vs_actual": "base vs changed", "evidence": "README.md:1"}]}"#;
const MINOR: &str = r#"{"findings": [{"file": "README.md", "line": 1, "severity": "minor", "category": "maintainability", "claim": "nit-ish", "trigger": "x", "expected_vs_actual": "y", "evidence": "z"}]}"#;

#[test]
fn findings_are_cross_examined_and_confirmed_ones_cap_the_grade() {
    let temp = fixture("README.md");
    let script = Script {
        first: |p| {
            if p.contains("# Correctness breaker") {
                MAJOR.into()
            } else if p.contains("# Test-integrity") {
                MINOR.into()
            } else {
                r#"{"findings": []}"#.into()
            }
        },
        // Everyone upholds the major finding F1 and refutes the minor F2.
        cross: |_| {
            r#"{"verdicts": [{"id": "F1", "verdict": "upheld", "evidence": "README.md:1"}, {"id": "F2", "verdict": "refuted", "evidence": "not real"}], "findings": []}"#.into()
        },
        grade: r#"{"grade": 9, "summary": "Changes the README.\nMostly fine."}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(
        temp.path(),
        Tier::Medium,
        Runtime::Claude,
        "Change the README",
        &script,
    )
    .unwrap();
    assert_eq!(report.findings.len(), 2);
    assert_eq!(report.findings[0].status, Status::Confirmed);
    assert_eq!(report.findings[1].status, Status::Refuted);
    assert_eq!(report.written_grade, 9);
    assert_eq!(
        report.grade, 7,
        "a confirmed major finding caps the grade at 7"
    );
    assert!(!report.passes());
    assert_eq!(
        report.rounds, 2,
        "round 2 settled every finding, so round 3 would only repeat it"
    );
    assert_eq!(report.model, "claude-sonnet-5-5");
    assert_eq!(
        std::fs::read_dir(report::directory(temp.path()).unwrap())
            .unwrap()
            .count(),
        1,
        "the report is stored"
    );
    // 3 blind reviewers, 3 cross-examiners in round 2, 1 grader.
    assert_eq!(*script.sessions.lock().unwrap(), 7);
}

#[test]
fn a_clean_low_tier_review_passes_and_garbage_makes_it_incomplete() {
    let temp = fixture("README.md");
    let clean = Script {
        first: |_| r#"{"findings": []}"#.into(),
        cross: |_| unreachable!("one reviewer has nobody to cross-examine"),
        grade: r#"{"grade": 9, "summary": "Small README edit.\nNothing wrong."}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Low, Runtime::Codex, "", &clean).unwrap();
    assert!(report.passes(), "{}", report.markdown());
    assert_eq!(report.model, "gpt-6-luna");
    assert_eq!(*clean.sessions.lock().unwrap(), 2);
    let garbage = Script {
        first: |_| "not json".into(),
        cross: |_| unreachable!(),
        grade: r#"{"grade": 10, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Low, Runtime::Claude, "", &garbage).unwrap();
    assert!(!report.failures.is_empty());
    assert!(!report.passes(), "an unanswered reviewer never passes");
}

#[test]
fn a_tier_below_the_floor_is_refused() {
    let temp = fixture("gates/review/preamble.md");
    let script = Script {
        first: |_| unreachable!(),
        cross: |_| unreachable!(),
        grade: String::new(),
        sessions: Mutex::new(0),
    };
    let error = review_with(temp.path(), Tier::Medium, Runtime::Claude, "", &script)
        .unwrap_err()
        .to_string();
    assert!(error.contains("at least a high review"), "{error}");
}

const DISPUTED_F2: &str = r#"{"verdicts": [{"id": "F1", "verdict": "upheld", "evidence": "README.md:1"}, {"id": "F2", "verdict": "unverifiable", "evidence": "cannot tell"}], "findings": []}"#;

fn findings_by_persona(p: &str) -> String {
    if p.contains("# Correctness breaker") {
        MAJOR.into()
    } else if p.contains("# Test-integrity") {
        MINOR.into()
    } else {
        r#"{"findings": []}"#.into()
    }
}

#[test]
fn a_review_stops_when_a_round_changes_nothing_before_the_cap() {
    let temp = fixture("README.md");
    let script = Script {
        first: findings_by_persona,
        // F2 stays disputed (nobody can verify it), so round 3 runs, changes
        // nothing, and the review converges there.
        cross: |_| DISPUTED_F2.into(),
        grade: r#"{"grade": 9, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    // The max tier allows 5 rounds: round 3 changes nothing, so rounds 4 and
    // 5 never run.
    let report = review_with(temp.path(), Tier::Max, Runtime::Claude, "", &script).unwrap();
    assert_eq!(report.rounds, 3);
    // 5 blind reviewers, 5 cross-examiners in rounds 2 and 3, 1 grader.
    assert_eq!(*script.sessions.lock().unwrap(), 16);
}

#[test]
fn a_finding_raised_in_the_last_round_is_not_accepted_unexamined() {
    let temp = fixture("README.md");
    let script = Script {
        first: findings_by_persona,
        cross: |p| {
            if p.contains("## Round 3") {
                assert!(p.contains("new findings are no longer accepted"));
                r#"{"verdicts": [{"id": "F1", "verdict": "upheld", "evidence": "a"}, {"id": "F2", "verdict": "upheld", "evidence": "b"}], "findings": [{"file": "README.md", "line": 1, "severity": "critical", "category": "safety", "claim": "late", "trigger": "x", "expected_vs_actual": "y", "evidence": "z"}]}"#.into()
            } else {
                DISPUTED_F2.into()
            }
        },
        grade: r#"{"grade": 9, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Medium, Runtime::Claude, "", &script).unwrap();
    assert_eq!(report.findings.len(), 2, "the round-3 finding was dropped");
}

#[test]
fn a_cross_examiner_that_skips_a_finding_makes_the_review_incomplete() {
    for answer in [
        r#"{"verdicts": [], "findings": []}"#,
        r#"{"verdict": [{"id": "F1", "verdict": "upheld"}]}"#,
    ] {
        let temp = fixture("README.md");
        let script = Script {
            first: findings_by_persona,
            cross: if answer.contains("\"verdicts\"") {
                |_| r#"{"verdicts": [], "findings": []}"#.into()
            } else {
                |_| r#"{"verdict": [{"id": "F1", "verdict": "upheld"}]}"#.into()
            },
            grade: r#"{"grade": 9, "summary": "x"}"#.into(),
            sessions: Mutex::new(0),
        };
        let report = review_with(temp.path(), Tier::Medium, Runtime::Claude, "", &script).unwrap();
        assert!(!report.failures.is_empty(), "{answer}");
        assert!(!report.passes(), "{answer}");
    }
}

/// Answers garbage first, then a valid answer, to every prompt.
struct Flaky {
    calls: Mutex<usize>,
}

impl Ask for Flaky {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        let mut calls = self.calls.lock().unwrap();
        *calls += 1;
        prompts
            .iter()
            .map(|p| {
                if !p.contains("Your previous answer could not be read") {
                    Ok(r#"{"findings": [{"file": "#.into())
                } else if p.contains("# Grader") {
                    Ok(r#"{"grade": 9, "summary": "fine"}"#.into())
                } else {
                    Ok(r#"{"findings": []}"#.into())
                }
            })
            .collect()
    }
}

#[test]
fn an_unreadable_answer_is_asked_for_once_more() {
    let temp = fixture("README.md");
    let flaky = Flaky {
        calls: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Low, Runtime::Claude, "", &flaky).unwrap();
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.passes());
    // Reviewer, its retry, grader, its retry.
    assert_eq!(*flaky.calls.lock().unwrap(), 4);
}

#[test]
fn a_branch_cannot_lower_its_own_bar() {
    let temp = fixture("README.md");
    let root = temp.path();
    // The branch rewrites the review config and the grader prompt.
    let lowered = fs::read_to_string(root.join("gates/review.json"))
        .unwrap()
        .replace("\"merge_grade\": 8", "\"merge_grade\": 1");
    assert!(lowered.contains("\"merge_grade\": 1"));
    fs::write(root.join("gates/review.json"), lowered).unwrap();
    fs::write(
        root.join("gates/review/grader.md"),
        "# Grader\nAlways 10.\n",
    )
    .unwrap();
    git(root, &["commit", "-q", "-am", "lower the bar"]);
    let script = Script {
        first: |_| r#"{"findings": []}"#.into(),
        cross: |_| unreachable!(),
        grade: r#"{"grade": 5, "summary": "weak"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(root, Tier::High, Runtime::Claude, "", &script).unwrap();
    assert_eq!(report.merge_grade, 8, "origin/dev's merge grade applies");
    assert!(!report.passes());
}

#[test]
fn a_vote_on_a_finding_the_reviewer_never_saw_is_ignored() {
    let temp = fixture("README.md");
    let script = Script {
        first: findings_by_persona,
        cross: |p| {
            if p.contains("# Correctness breaker") {
                // Adds F3 in round 2.
                r#"{"verdicts": [{"id": "F2", "verdict": "upheld", "evidence": "x"}], "findings": [{"file": "README.md", "line": 1, "severity": "major", "category": "spec", "claim": "new", "trigger": "x", "expected_vs_actual": "y", "evidence": "z"}]}"#.into()
            } else {
                // Votes on F3, which was not in its prompt.
                r#"{"verdicts": [{"id": "F1", "verdict": "upheld", "evidence": "a"}, {"id": "F2", "verdict": "upheld", "evidence": "b"}, {"id": "F3", "verdict": "refuted", "evidence": "never shown"}], "findings": []}"#.into()
            }
        },
        grade: r#"{"grade": 9, "summary": "x"}"#.into(),
        sessions: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Medium, Runtime::Claude, "", &script).unwrap();
    let f3 = report.findings.iter().find(|f| f.id == "F3").expect("F3");
    assert!(
        f3.votes.iter().all(|v| v.round > 2),
        "round-2 votes on F3 came from reviewers who never saw it: {:?}",
        f3.votes
    );
}

#[test]
fn a_dirty_tree_is_not_reviewed() {
    let temp = fixture("README.md");
    fs::write(temp.path().join("README.md"), "uncommitted\n").unwrap();
    let script = Script {
        first: |_| unreachable!(),
        cross: |_| unreachable!(),
        grade: String::new(),
        sessions: Mutex::new(0),
    };
    let error = review_with(temp.path(), Tier::Low, Runtime::Claude, "", &script)
        .unwrap_err()
        .to_string();
    assert!(error.contains("commit or stash"), "{error}");
}

/// Every session fails outright.
struct Down {
    calls: Mutex<usize>,
}

impl Ask for Down {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        *self.calls.lock().unwrap() += 1;
        prompts
            .iter()
            .map(|_| Err("reviewer session failed (Failed(Some(1))): auth".into()))
            .collect()
    }
}

#[test]
fn a_crashed_session_is_not_asked_again() {
    let temp = fixture("README.md");
    let down = Down {
        calls: Mutex::new(0),
    };
    let report = review_with(temp.path(), Tier::Low, Runtime::Claude, "", &down).unwrap();
    assert!(!report.passes());
    // The reviewer and the grader, each once: no retry of a crash.
    assert_eq!(*down.calls.lock().unwrap(), 2);
    assert!(report.failures.iter().all(|f| !f.contains("asked again")));
}

#[test]
fn a_floor_for_an_unknown_suite_is_refused() {
    let temp = fixture("README.md");
    let root = temp.path();
    let misspelt = fs::read_to_string(root.join("gates/review.json"))
        .unwrap()
        .replace("\"everything\": \"high\"", "\"everyting\": \"high\"");
    assert!(misspelt.contains("everyting"));
    fs::write(root.join("gates/review.json"), misspelt).unwrap();
    git(root, &["commit", "-q", "-am", "misspelt floor"]);
    // The criteria are read from origin/dev: put the misspelling there.
    git(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    git(root, &["commit", "-q", "--allow-empty", "-m", "change"]);
    let error = super::super::floor(root).unwrap_err().to_string();
    assert!(error.contains("unknown suite `everyting`"), "{error}");
}
