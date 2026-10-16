use super::*;

#[test]
fn transitive_only_job_membership_cannot_hide_a_selected_gate() {
    let mut value = proposal();
    value["jobs"]["target-performance"] = json!(["perf-ci", "perf-pressure", "perf-wasm-size"]);
    // perf-ci still depends on perf-instructions, but direct selection must
    // nevertheless identify the job that executes the selected prerequisite.
    assert!(
        parse(&value)
            .unwrap_err()
            .to_string()
            .contains("no job coverage")
    );
}

#[test]
fn independent_prerequisite_selects_its_executing_job_without_broadening_suites() {
    let mut value = proposal();
    gate_mut(&mut value, "perf-instructions")["suites"] = json!(["static"]);
    let registry = parse(&value).expect("valid covered gate");
    let impact = registry.classify(&paths(&["docs/testing.md"]));
    let plan = registry.plan(Cadence::Ci, &impact.suites).expect("CI plan");
    assert_eq!(plan.suites, suite_set(&["docs", "static"]));
    assert!(plan.gates.contains(&"perf-instructions".into()));
    assert!(plan.jobs["target-performance"]);
    assert!(!plan.jobs["native-coverage"]);
}

#[test]
fn registry_evidence_change_invalidates_same_revision_selection() {
    use crate::gates::{check_selection, selection};
    let original = registry();
    let mut changed = original.clone();
    changed.gates[0].evidence.push_str(" reviewed change");
    let paths = paths(&["docs/testing.md"]);
    let old = selection(&original, "a".repeat(40), None, paths.clone(), false).unwrap();
    let new = selection(&changed, "a".repeat(40), None, paths, false).unwrap();
    assert_eq!(old.gates, new.gates);
    assert_ne!(old.registry_hash, new.registry_hash);
    let mut results = BTreeMap::from([("select".to_owned(), "success")]);
    results.extend(
        old.jobs
            .iter()
            .map(|(job, selected)| (job.clone(), if *selected { "success" } else { "skipped" })),
    );
    let results = serde_json::to_string(&results).unwrap();
    let manifest = serde_json::to_string(&old).unwrap();
    check_selection(&manifest, &results, &old).expect("passing control");
    assert!(check_selection(&manifest, &results, &new).is_err());
}

fn workflow() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows/ci.yml"),
    )
    .expect("CI workflow")
}

/// The text of one top-level job of the workflow, up to the next job.
fn workflow_job<'a>(workflow: &'a str, job: &str) -> &'a str {
    let start = workflow
        .find(&format!("\n  {job}:\n"))
        .unwrap_or_else(|| panic!("workflow job {job}"))
        + 1;
    let rest = &workflow[start..];
    let next_job = |line: &&str| {
        line.starts_with("  ") && !line.starts_with("   ") && !line.trim_start().starts_with('#')
    };
    let length: usize = rest
        .split_inclusive('\n')
        .enumerate()
        .take_while(|(index, line)| *index == 0 || !next_job(line))
        .map(|(_, line)| line.len())
        .sum();
    &rest[..length]
}

#[test]
fn every_registry_job_is_a_workflow_job_the_aggregate_requires() {
    let registry = registry();
    let workflow = workflow();
    let required = workflow_job(&workflow, "required");
    for job in registry.jobs.keys() {
        let id = job.replace('-', "_");
        let block = workflow_job(&workflow, job);
        if job != "static" {
            assert!(
                block.contains(&format!("needs.select.outputs.{id} == 'true'")),
                "{job} runs when its selection output is true"
            );
            assert!(
                workflow.contains(&format!("{id}: ${{{{ steps.manifest.outputs.{id} }}}}")),
                "select exports {job}"
            );
        }
        assert!(
            required.contains(&format!("{job}, ")) || required.contains(&format!(" {job}]")),
            "{job} in required needs"
        );
        assert!(
            required.contains(&format!("\"{job}\":\"${{{{ needs['{job}'].result }}}}\""))
                || required.contains(&format!("\"{job}\":\"${{{{ needs.{job}.result }}}}\"")),
            "{job} result reaches ci-check"
        );
    }
}

#[test]
fn the_e2e_job_runs_each_shard_of_one_split_and_nothing_else_runs_the_suite_there() {
    let registry = registry();
    let workflow = workflow();
    assert_eq!(registry.jobs["e2e"], ["test-e2e"]);
    let block = workflow_job(&workflow, "e2e");
    let matrix = block
        .lines()
        .find_map(|line| line.trim().strip_prefix("shard: ["))
        .and_then(|rest| rest.strip_suffix(']'))
        .expect("shard matrix");
    let shards: Vec<&str> = matrix.split(", ").collect();
    let count = shards.len();
    assert!(count > 1);
    for (offset, shard) in shards.iter().enumerate() {
        assert_eq!(
            shard.parse::<usize>().ok(),
            Some(offset + 1),
            "shards are 1..n"
        );
    }
    assert!(block.contains("run: make test-e2e"));
    assert!(block.contains(&format!("AOE_E2E_SHARD: ${{{{ matrix.shard }}}}/{count}")));
    assert!(block.contains("name: e2e-${{ matrix.shard }}-diagnostics"));
    assert!(block.contains("fail-fast: false"));
    // The browser job keeps its checks but no longer runs the suite, and the
    // coverage job runs it whole.
    assert!(!workflow_job(&workflow, "browser").contains("test-e2e"));
    assert!(!workflow_job(&workflow, "native-coverage").contains("AOE_E2E_SHARD"));
}
