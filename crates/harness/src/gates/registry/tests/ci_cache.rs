//! The Rust build caches of the CI jobs and the budget that keeps them under the repository's 10 GB.
use std::path::Path;

fn read(path: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    std::fs::read_to_string(root.join(path)).unwrap_or_else(|error| panic!("{path}: {error}"))
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

/// The steps of a job, one string each (a step starts with `      - `).
fn steps(block: &str) -> Vec<String> {
    let mut steps: Vec<String> = Vec::new();
    for line in block
        .lines()
        .skip_while(|line| line.trim() != "steps:")
        .skip(1)
    {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with("      - ") {
            steps.push(String::new());
        }
        if let Some(step) = steps.last_mut() {
            step.push_str(line);
            step.push('\n');
        }
    }
    steps
}

#[test]
fn every_compiling_job_restores_a_leg_and_the_green_job_saves_it_last() {
    let workflow = read(".github/workflows/ci.yml");
    for (name, leg) in [
        ("browser", "browser"),
        ("e2e", "e2e"),
        ("target-performance", "perf"),
    ] {
        let block = job(&workflow, name);
        let restore = format!("rust-cache-restore, with: {{ leg: {leg} }}");
        let save = format!("rust-cache-save, with: {{ leg: {leg} }}");
        assert!(block.contains(&restore), "{name} restores leg {leg}");
        assert!(block.contains(&save), "{name} saves leg {leg}");
        // The save prunes the workspace artifacts: only artifact uploads may follow it.
        let all = steps(&block);
        let at = all
            .iter()
            .position(|step| step.contains("rust-cache-save"))
            .unwrap();
        for step in &all[at + 1..] {
            assert!(
                step.contains("upload-artifact"),
                "{name}: after the save: {step}"
            );
        }
    }
    // The four shards build the same thing: one of them saves.
    let e2e = job(&workflow, "e2e");
    let save_line = e2e.lines().find(|l| l.contains("rust-cache-save")).unwrap();
    assert!(save_line.contains("matrix.shard == 1"), "{save_line}");
}

#[test]
fn the_save_keeps_dependencies_only() {
    let save = read(".github/actions/rust-cache-save/action.yml");
    let prune = save
        .find("-name 'aoe_*'")
        .expect("workspace artifacts are removed");
    assert!(prune < save.find("actions/cache/save@").unwrap());
    assert!(save.contains("-name 'libaoe_*'") && save.contains("-name 'aoe-*'"));
    assert!(save.contains("-path '*/deps/*' -type f -perm -u+x ! -name '*.*' -delete"));
    assert!(save.contains("github.event_name == 'push' && github.ref == 'refs/heads/dev'"));
}

#[test]
fn only_a_workflow_without_pull_request_code_may_delete_caches() {
    let ci = read(".github/workflows/ci.yml");
    assert!(!ci.contains("actions: write"));
    let trim = read(".github/workflows/cache-trim.yml");
    assert!(trim.contains("actions: write"));
    assert!(trim.contains("workflow_run:") && trim.contains("branches: [dev]"));
    assert!(!trim.contains("pull_request"));
    assert!(trim.contains("ref: dev"));
    let action = read(".github/actions/cache-trim/action.yml");
    assert!(action.contains("--ref refs/heads/dev --key rust-shared-"));
    assert!(action.contains("gh cache delete"));
    // The game map package and the Docker layers are not Rust legs and are never listed.
    assert!(!action.contains("buildkit") && !action.contains("--key game-map"));
}
