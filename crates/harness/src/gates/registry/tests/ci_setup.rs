//! What the CI jobs do not repeat: the setup a job does not use, and the job a step can replace.
use std::path::Path;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// The text of one top-level job of `ci.yml`, up to the next job.
fn job(workflow: &str, name: &str) -> String {
    let start = workflow
        .find(&format!("\n  {name}:\n"))
        .unwrap_or_else(|| panic!("workflow job {name}"))
        + 1;
    let mut text = String::new();
    for (index, line) in workflow[start..].lines().enumerate() {
        let next_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim_start().starts_with('#');
        if index > 0 && next_job {
            break;
        }
        text.push_str(line);
        text.push('\n');
    }
    text
}

#[test]
fn only_the_release_build_runs_make_bootstrap() {
    // `make bootstrap` builds the browser image and installs the npm packages
    // whether or not a job opens a browser, and installs git hooks nobody
    // runs in CI. Each goal builds the images it needs as prerequisites.
    let workflow = read(".github/workflows/ci.yml");
    assert_eq!(workflow.matches("make bootstrap").count(), 1);
    assert!(job(&workflow, "dev-artifacts").contains("run: make bootstrap"));
}

#[test]
fn goals_that_run_in_an_image_build_it_themselves() {
    let makefile = read("Makefile");
    for goal in ["perf-dev-reference", "perf-comment"] {
        let head = makefile
            .lines()
            .find(|line| line.starts_with(&format!("{goal}:")))
            .unwrap_or_else(|| panic!("{goal} rule"));
        assert!(head.contains("orchestrator-tools"), "{head}");
    }
}

#[test]
fn the_performance_comment_is_the_last_step_of_the_performance_job() {
    let workflow = read(".github/workflows/ci.yml");
    assert!(!workflow.contains("\n  perf-comment:\n"));
    let block = job(&workflow, "target-performance");
    let comment = block
        .lines()
        .find(|line| line.contains("run: make perf-comment"))
        .expect("the comment step");
    // Informational: a failed comment never fails the job.
    assert!(comment.contains("continue-on-error: true"), "{comment}");
    assert!(comment.contains("github.event_name == 'pull_request'"));
    assert!(comment.contains("!cancelled()"));
    assert!(block.contains("pull-requests: write"));
    // The token that may write to the pull request is not left in the checkout.
    assert!(block.contains("persist-credentials: false"));
    let upload = block.find("name: performance-report").unwrap();
    assert!(upload < block.find("run: make perf-comment").unwrap());
    // Only the cache save follows: it prunes the build output the comment's harness run needs.
    let after = block.split_once("run: make perf-comment").unwrap().1;
    let steps: Vec<_> = after
        .lines()
        .filter(|line| line.starts_with("      - ") && !line.contains("env:"))
        .collect();
    assert!(
        steps.iter().all(|line| line.contains("rust-cache-save")),
        "{steps:?}"
    );
}

#[test]
fn the_workflow_leaves_room_for_the_game_score_job() {
    // The score job needs about 15 lines and the file stays within 500.
    let lines = read(".github/workflows/ci.yml").lines().count();
    assert!(lines <= 480, "ci.yml has {lines} lines");
}
