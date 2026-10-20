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

#[test]
fn the_static_job_runs_both_preflight_halves_and_only_dev_pushes_save_the_cache() {
    use crate::native::part::PreflightPart;
    use clap::ValueEnum;
    let workflow = workflow();
    let block = workflow_job(&workflow, "static");
    let matrix = block
        .split_once("part: [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(legs, _)| legs)
        .expect("part matrix");
    let mut legs: Vec<&str> = matrix.split(", ").collect();
    legs.sort_unstable();
    // The legs are exactly the halves `preflight --part` knows besides `all`.
    let mut halves: Vec<String> = PreflightPart::value_variants()
        .iter()
        .filter(|part| **part != PreflightPart::All)
        .map(|part| part.to_possible_value().unwrap().get_name().to_owned())
        .collect();
    halves.sort_unstable();
    assert_eq!(legs, halves);
    assert!(block.contains("run: make preflight-${{ matrix.part }}"));
    assert!(block.contains("fail-fast: false"));
    let makefile = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile"),
    )
    .expect("Makefile");
    for leg in &legs {
        assert!(
            makefile.contains(&format!("\npreflight-{leg}:"))
                && makefile.contains(&format!("preflight --part {leg}\n")),
            "make preflight-{leg} runs its half"
        );
    }
    // Registry membership is unchanged: one job, so ci-check still sees one result.
    for gate in ["lint", "deny", "build-wasm", "test-unit", "perf-smoke"] {
        assert!(
            registry().jobs["static"]
                .iter()
                .any(|member| member == gate)
        );
    }
    // Pull requests read what dev saved and never write, so they cannot evict it.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/actions");
    let save = std::fs::read_to_string(root.join("rust-cache-save/action.yml")).expect("save");
    let restore =
        std::fs::read_to_string(root.join("rust-cache-restore/action.yml")).expect("restore");
    assert!(save.contains("github.event_name == 'push' && github.ref == 'refs/heads/dev'"));
    assert!(save.contains("env.RUST_CACHE_EXACT != 'true'"));
    assert!(
        restore.contains("actions/cache/restore@")
            && restore.contains("rust-shared-${{ inputs.leg }}-")
    );
    for action in [&save, &restore] {
        assert!(!action.contains("uses: actions/cache@"));
        assert!(action.contains("key: rust-shared-${{ inputs.leg }}-"));
        assert!(action.contains(".git/aoe-cargo/registry"));
        assert!(action.contains(".git/aoe-cargo/git"));
        assert!(!action.contains(".cache/cargo"));
    }
    for name in ["rust-cache-restore", "rust-cache-save"] {
        assert!(block.contains(&format!("./.github/actions/{name}")));
    }
    assert!(block.contains("leg: '${{ matrix.part }}'"));
}

#[test]
fn the_qualification_job_runs_after_merge_and_never_on_pull_requests() {
    let registry = registry();
    let impact = registry.classify(&paths(&["crates/server/src/lib.rs"]));
    let pr = registry.plan(Cadence::Pr, &impact.suites).unwrap();
    assert!(!pr.jobs["qualification"], "pull requests skip the tier");
    assert!(!pr.gates.contains(&"test-source-qualification".into()));
    let ci = registry.plan(Cadence::Ci, &impact.suites).unwrap();
    assert!(ci.jobs["qualification"], "pushes to dev run the tier");
    assert_eq!(
        registry.jobs["qualification"],
        ["test-source-qualification"]
    );
    let nightly = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.github/workflows/nightly.yml"),
    )
    .expect("nightly workflow");
    assert!(nightly.contains("run: make test-source-qualification"));
}

#[test]
fn the_exhaustive_job_runs_after_merge_and_never_on_pull_requests() {
    let registry = registry();
    let impact = registry.classify(&paths(&["crates/map/src/lib.rs"]));
    let pr = registry.plan(Cadence::Pr, &impact.suites).unwrap();
    assert!(!pr.jobs["exhaustive"], "pull requests skip the tier");
    assert!(!pr.gates.contains(&"test-exhaustive".into()));
    let ci = registry.plan(Cadence::Ci, &impact.suites).unwrap();
    assert!(ci.jobs["exhaustive"], "pushes to dev run the tier");
    assert_eq!(registry.jobs["exhaustive"], ["test-exhaustive"]);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let nightly = std::fs::read_to_string(root.join(".github/workflows/nightly.yml")).unwrap();
    assert!(nightly.contains("run: make test-exhaustive"));
    let workflow = std::fs::read_to_string(root.join(".github/workflows/ci.yml")).unwrap();
    assert!(workflow.contains("run: make test-exhaustive"));
}

#[test]
fn running_dev_builds_and_main_promotions_finish_while_prs_cancel() {
    let workflow = workflow();
    let concurrency = workflow
        .split_once("\nconcurrency:\n")
        .unwrap()
        .1
        .split_once("\njobs:\n")
        .unwrap()
        .0;
    assert!(concurrency.contains("group: validation-${{ github.workflow }}-${{ github.ref }}"));
    // PRs use refs/pull/<number>/merge, including release/* -> main PRs.
    // Pushes only trigger for dev/main; a release/* dispatch still cancels.
    assert!(concurrency.contains(
        "cancel-in-progress: ${{ github.ref != 'refs/heads/dev' && github.ref != 'refs/heads/main' }}"
    ));
    assert!(workflow.contains("branches: [dev, main]"));
}

#[test]
fn dev_artifacts_reuses_required_evidence_and_only_builds_verifies_and_publishes() {
    let workflow = workflow();
    let dev = workflow_job(&workflow, "dev-artifacts");
    assert!(dev.contains("needs: required"));
    assert!(dev.contains("github.event_name == 'push' && github.ref == 'refs/heads/dev'"));
    let commands: Vec<_> = dev
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("run: make ")
                .or_else(|| line.trim().strip_prefix("- run: make "))
        })
        .collect();
    assert_eq!(
        commands,
        [
            "bootstrap",
            "release-build",
            "release-verify",
            "release-publish"
        ]
    );

    let coverage = workflow_job(&workflow, "native-coverage");
    assert!(coverage.contains("run: make coverage"));
    assert!(coverage.contains("name: native-coverage-report, path: reports/coverage/"));
    let upload = coverage
        .split_once("name: release-e2e-evidence,")
        .unwrap()
        .1;
    assert!(upload.contains("path: reports/e2e/pass.json"));
    assert!(upload.contains("if-no-files-found: error"));
    assert!(coverage.contains("github.event_name == 'push' && github.ref == 'refs/heads/dev'"));
    let performance = workflow_job(&workflow, "target-performance");
    assert!(performance.contains("run: make perf-ci"));
    assert!(performance.contains("name: performance-report, path: reports/perf/"));
    assert_eq!(dev.matches("uses: actions/download-artifact@").count(), 2);
    for (artifact, path) in [
        ("release-e2e-evidence", "reports/e2e/"),
        ("performance-report", "reports/perf/"),
    ] {
        let transfer = format!("name: {artifact}, path: {path}");
        assert!(dev.find(&transfer).unwrap() < dev.find("run: make release-build").unwrap());
    }
    // download-artifact defaults to the current run/repository. No cross-run
    // fallback may turn missing current evidence into a passing release.
    for forbidden in [
        "run-id:",
        "repository:",
        "github-token:",
        "continue-on-error:",
    ] {
        assert!(!dev.contains(forbidden), "{forbidden}");
    }
    for output in [
        "name: Server SBOM",
        "name: Browser SBOM",
        "name: Static bundle SBOM",
        "name: Attest server provenance",
        "name: Attest browser provenance",
        "name: Attest server SBOM",
        "name: Attest browser SBOM",
        "name: Attest static bundle",
        "name: Attest published manifest",
        "gh release create",
        "name: dev-release-diagnostics",
    ] {
        assert!(dev.contains(output), "{output}");
    }
}
