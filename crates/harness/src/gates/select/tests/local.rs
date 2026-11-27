//! The local checks of `make ship`: the game score runs when the change can affect the game
//! and says why, and it is never a CI job. Real crate graph, real rules.
use super::*;

fn decision(items: &[&str]) -> LocalDecision {
    decide_local(&rules(), graph(), &paths(items))
        .remove("game-score")
        .expect("the rules name the game score")
}

#[test]
fn a_change_in_the_build_closure_of_the_server_runs_the_game_score() {
    for path in [
        "crates/simulation/src/lib.rs",
        "crates/ai/src/lib.rs",
        "crates/map/src/lib.rs",
        "crates/protocol/src/lib.rs",
        "crates/core/src/lib.rs",
        "crates/scenario/src/lib.rs",
        "crates/sdk/src/lib.rs",
        "crates/server/src/lib.rs",
    ] {
        let decision = decision(&[path]);
        assert!(decision.runs, "{path}: {}", decision.reason);
        assert!(
            decision.reason.starts_with("runs because aoe-server"),
            "{path}: {}",
            decision.reason
        );
        assert!(decision.reason.contains(path), "{}", decision.reason);
    }
}

#[test]
fn the_reason_names_the_crate_the_server_builds_on() {
    let decision = decision(&["crates/simulation/src/lib.rs"]);
    assert_eq!(
        decision.reason,
        "runs because aoe-server builds on aoe-simulation, which changed \
         (crates/simulation/src/lib.rs)"
    );
}

#[test]
fn a_change_the_game_cannot_feel_skips_it_and_says_why() {
    for path in [
        "crates/client/src/lib.rs",
        "crates/rendering/src/lib.rs",
        "crates/assets/src/lib.rs",
        "crates/geodata/src/lib.rs",
        "docs/new-guide.md",
        "ai/aoe_ai/client.py",
        "browser/tests/smoke.spec.ts",
        "baselines/perf/micro.json",
        ".github/workflows/ci.yml",
        "gates/registry.json",
        "crates/harness/src/ship/mod.rs",
        "crates/harness/src/perf_comment/mod.rs",
    ] {
        let decision = decision(&[path]);
        assert!(!decision.runs, "{path}: {}", decision.reason);
        assert!(
            decision.reason.starts_with("skipped because"),
            "{path}: {}",
            decision.reason
        );
    }
}

#[test]
fn tests_benchmarks_examples_and_notes_of_a_game_crate_do_not_run_it() {
    for path in [
        "crates/simulation/tests/replay.rs",
        "crates/simulation/benches/tick.rs",
        "crates/ai/examples/demo.rs",
        "crates/simulation/README.md",
    ] {
        assert!(!decision(&[path]).runs, "{path}");
    }
    let mixed = decision(&["crates/simulation/tests/replay.rs", "crates/ai/src/lib.rs"]);
    assert!(
        mixed.runs,
        "one path that counts is enough: {}",
        mixed.reason
    );
}

#[test]
fn the_game_score_itself_runs_it_and_the_rest_of_the_harness_does_not() {
    for path in [
        "crates/harness/src/game_score/scenarios/mod.rs",
        "crates/harness/src/game_score/counters/arrival.rs",
        "crates/harness/Cargo.toml",
    ] {
        let decision = decision(&[path]);
        assert!(decision.runs, "{path}: {}", decision.reason);
        assert_eq!(
            decision.reason,
            format!("runs because {path} is an input of the game score")
        );
    }
}

#[test]
fn the_lock_the_workspace_manifest_and_the_toolchain_run_it() {
    for path in ["Cargo.lock", "Cargo.toml", "rust-toolchain.toml"] {
        assert!(decision(&[path]).runs, "{path}");
    }
}

#[test]
fn no_changed_path_skips_it() {
    let decision = decision(&[]);
    assert!(!decision.runs);
    assert_eq!(decision.reason, "skipped because no path changed");
}

#[test]
fn a_local_check_is_not_a_ci_job() {
    let decision = pull_request(&["crates/simulation/src/lib.rs"]);
    assert!(!decision.jobs.contains_key("game-score"));
    assert!(!decision.reasons.contains_key("game-score"));
    let explained = explain::text(
        &decision.jobs,
        &decision.reasons,
        &decision.static_parts,
        &decision.unit,
    );
    assert!(!explained.contains("game score"), "{explained}");
}

#[test]
fn the_inputs_are_the_closure_of_the_server_and_the_rules_own_list() {
    let inputs = decision(&["Cargo.lock"]).inputs;
    for expected in [
        "crates/server",
        "crates/simulation",
        "crates/ai",
        "crates/map",
        "crates/protocol",
        "crates/core",
        "crates/scenario",
        "crates/sdk",
        "crates/harness/src/game_score",
        "Cargo.lock",
        "rust-toolchain.toml",
    ] {
        assert!(inputs.iter().any(|input| input == expected), "{expected}");
    }
    for outside in ["crates/client", "crates/rendering", "crates/assets"] {
        assert!(!inputs.iter().any(|input| input == outside), "{outside}");
    }
    let mut sorted = inputs.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, inputs, "sorted and without duplicates");
}

#[test]
fn the_rule_names_workspace_crates_and_tracked_inputs() {
    let rules = rules();
    let rule = &rules.local_checks["game-score"];
    for name in &rule.crates {
        assert!(graph().is_workspace(name), "{name}");
    }
    for input in &rule.inputs {
        let listed = Command::new("git")
            .current_dir(root())
            .args(["ls-files", "--", input])
            .output()
            .expect("git");
        assert!(
            !String::from_utf8_lossy(&listed.stdout).trim().is_empty(),
            "{input} is not tracked"
        );
    }
}

#[test]
fn ci_explain_prints_the_local_checks_after_the_jobs() {
    let decisions = decide_local(&rules(), graph(), &paths(&["crates/ai/src/lib.rs"]));
    let text = explain::local(&decisions);
    assert_eq!(
        text,
        "game-score (make ship, not a CI job): runs because aoe-server builds on aoe-ai, \
         which changed (crates/ai/src/lib.rs)\n"
    );
}

#[test]
fn rules_reject_a_local_check_with_a_bad_glob_or_an_unknown_field() {
    let registry = registry();
    let text = std::fs::read_to_string(root().join("gates/ci-rules.json")).expect("rules");
    let parse = |text: &str| Rules::parse(text.as_bytes(), Some(&registry));
    assert!(parse(&text.replace("crates/*/tests/**", "crates/*/tests[1]/**")).is_err());
    assert!(parse(&text.replace("\"local_checks\"", "\"local_checkz\"")).is_err());
}
