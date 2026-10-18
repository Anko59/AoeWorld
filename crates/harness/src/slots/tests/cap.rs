//! Audits of the CPU cap: every target that compiles queues for a slot, the cap comes from the
//! variable the slot holder exports, and the containers the harness starts itself take it too.
use super::makefile::{definitions, make_files, recipe, repository};
use crate::{gates::registry::Registry, slots::guard::cap_args_for};
use std::{collections::BTreeSet, fs, path::Path};

/// Whether a recipe line compiles, tests or drives a browser in a container: the work a slot is for.
/// A `cargo run --release` compiles the release profile of a crate, so it counts; a plain
/// `cargo run` of the harness is the freshness check of the binary `slot-bootstrap` built.
fn heavy_work(line: &str) -> bool {
    let cargo = [
        "cargo build",
        "cargo test",
        "cargo llvm-cov",
        "cargo nextest",
        "cargo bench",
        "cargo run --release",
    ]
    .iter()
    .any(|command| line.contains(command));
    let browser = line.contains("npx ")
        || line.contains("npm run ")
        || line.contains("playwright test")
        || (line.contains("cargo run")
            && line.split(" -- ").nth(1).is_some_and(|rest| {
                rest.split_whitespace()
                    .next()
                    .is_some_and(|command| command.contains("browser"))
            }));
    cargo || browser
}

/// Targets whose heavy work is allowed without a slot wrapper, each with the reason. `agent-hook-build`
/// builds the judge the agent hooks run, which must never queue behind gates; `agent-smoke` only drives
/// the runtimes' CLIs against the harness binary that `slot-bootstrap` already built.
const SLOT_EXEMPT: &[(&str, &str)] = &[
    ("agent-hook-build", "the judge build must never queue"),
    ("agent-smoke", "uses the already bootstrapped binary"),
    (
        "perf-wasm-cap",
        "a host check of the report that the slotted perf-wasm-size wrote; it starts no container",
    ),
];

/// Whether `target` runs its heavy work under the slot wrapper: its own recipe runs `$(SLOT_RUN)`,
/// or a wrapper names it as the `-steps` target it runs.
fn slot_wrapped(files: &[(String, String)], target: &str) -> bool {
    let own = recipe(files, target).is_some_and(|lines| {
        lines
            .iter()
            .any(|line| line.contains(&format!("$(SLOT_RUN) {target} -- ")))
    });
    own || files.iter().flat_map(|(_, text)| text.lines()).any(|line| {
        let words: Vec<&str> = line.split_whitespace().collect();
        line.contains("$(SLOT_RUN) ")
            && words
                .windows(3)
                .any(|run| run == ["make", "--no-print-directory", target])
    })
}

#[test]
fn every_target_that_compiles_or_tests_takes_a_slot() {
    let root = repository();
    let registry = Registry::load(&root).expect("the committed registry");
    let files = make_files(&root);
    let light: BTreeSet<String> = registry
        .gates
        .iter()
        .filter(|gate| !gate.heavy)
        .filter_map(|gate| gate.command.strip_prefix("make ").map(str::to_owned))
        .collect();
    let mut gaps = Vec::new();
    for target in all_targets(&files) {
        let Some(lines) = recipe(&files, &target) else {
            continue;
        };
        if !lines.iter().any(|line| heavy_work(line))
            || light.contains(&target)
            || SLOT_EXEMPT.iter().any(|(name, _)| *name == target)
            || slot_wrapped(&files, &target)
        {
            continue;
        }
        gaps.push(format!(
            "{target}: compiles or tests without a slot; wrap it as `$(SLOT_RUN) {target} -- make --no-print-directory {target}-steps`"
        ));
    }
    assert!(gaps.is_empty(), "{gaps:#?}");
}

/// A registry gate marked heavy starts its first container under its slot, so that container is
/// capped; a gate whose recipe is one harness command would otherwise start it before any slot.
#[test]
fn every_heavy_gate_target_takes_its_slot_before_its_first_container() {
    let root = repository();
    let registry = Registry::load(&root).expect("the committed registry");
    let files = make_files(&root);
    let mut gaps = Vec::new();
    for gate in registry.gates.iter().filter(|gate| gate.heavy) {
        let Some(target) = gate.command.strip_prefix("make ") else {
            continue;
        };
        let exempt = SLOT_EXEMPT.iter().any(|(name, _)| *name == target);
        if !exempt && recipe(&files, target).is_some() && !slot_wrapped(&files, target) {
            gaps.push(target.to_owned());
        }
    }
    assert!(gaps.is_empty(), "heavy gates without a wrapper: {gaps:?}");
}

/// Every rule name in the makefiles, in order, without duplicates.
fn all_targets(files: &[(String, String)]) -> Vec<String> {
    let mut targets: Vec<String> = files
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter(|line| !line.starts_with(['\t', '#', '.']) && !line.contains(":="))
        .filter_map(|line| line.split_once(':').map(|(name, _)| name.trim().to_owned()))
        .filter(|name| !name.contains('$') && !name.contains('%'))
        .collect();
    targets.sort();
    targets.dedup();
    targets
}

#[test]
fn every_container_a_slot_starts_is_capped_to_the_slot_cpus() {
    let files = make_files(&repository());
    let mut gaps = Vec::new();
    for (name, text) in &files {
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if line.contains("docker run")
                && line.contains("$(HEAVY_ENV)")
                && !line.contains("$(SLOT_CAP)")
                && !line.contains("--cpus=")
            {
                gaps.push(format!("{name}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        gaps.is_empty(),
        "these containers are not capped:\n{gaps:#?}"
    );
    let definitions = definitions(&files);
    let cap = &definitions["SLOT_CAP"];
    assert!(cap.contains("--cpus=$(AOE_SLOT_CPUS)"), "{cap}");
    assert!(cap.contains("CARGO_BUILD_JOBS=$(AOE_SLOT_CPUS)"), "{cap}");
    assert!(
        cap.contains("$(and $(AOE_HEAVY_SLOT),$(AOE_SLOT_CPUS))"),
        "no slot or no count must mean no flag, never an empty one: {cap}"
    );
    assert!(
        definitions["HEAVY_ENV"].contains("-e AOE_SLOT_CPUS"),
        "the count must reach the nested make and the harness in the container"
    );
    assert!(
        definitions["SLOT_RUN"].contains("$(SLOT_CAP)"),
        "an inherited slot caps the wrapper too: {}",
        definitions["SLOT_RUN"]
    );
}

/// Make takes the count from the environment the slot holder set. A host binary found by a relative
/// path is absent in a gate's snapshot, where it expanded to an empty `--cpus=`.
#[test]
fn make_never_computes_the_cap_by_running_a_binary() {
    for (name, text) in make_files(&repository()) {
        for line in text.lines() {
            for call in line.split("$(shell").skip(1) {
                let command = call.split(')').next().unwrap_or_default();
                assert!(
                    !command.contains("aoe-harness"),
                    "{name}: a host binary decides the cap: {line}"
                );
            }
        }
    }
}

#[test]
fn the_holders_environment_carries_the_cap_and_unset_means_no_flag() {
    let held = vec![
        ("AOE_HEAVY_SLOT", "ticket:nonce".to_owned()),
        ("AOE_SLOT_CPUS", "6".to_owned()),
    ];
    assert_eq!(cap_args_for(&held), ["--cpus", "6"]);
    assert!(cap_args_for(&Vec::new()).is_empty());
}

/// Files that start a `docker run` themselves, and why one needs no cap. Everything else must
/// take `cap_args()`, so a container started under a slot has the slot's CPU limit.
const OWN_DOCKER_RUN_EXEMPT: &[(&str, &str)] = &[
    ("dev.rs", "the long-lived dev server, not a gate"),
    ("qa_mcp.rs", "the interactive QA server tool"),
    (
        "release_stack.rs",
        "the release rehearsal stack, a release workflow",
    ),
    (
        "devserver/process.rs",
        "the shared dev server: a detached, long-running server of an already built binary, never inside a gate slot",
    ),
    (
        "ship/compare/capture.rs",
        "`make ship` screenshots run by the ship process, never inside a gate slot",
    ),
    (
        "ship/showcase/media.rs",
        "`make ship` video recording run by the ship process, never inside a gate slot",
    ),
];

#[test]
fn every_container_the_harness_starts_itself_takes_the_slot_cap() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![src.clone()];
    let mut gaps = Vec::new();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            if path.ends_with("tests") {
                continue;
            }
            pending.extend(fs::read_dir(&path).unwrap().flatten().map(|e| e.path()));
            continue;
        }
        let name = path.strip_prefix(&src).unwrap().display().to_string();
        if path.extension().is_none_or(|ext| ext != "rs") || name.ends_with("tests.rs") {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        let runs_docker = text.contains("\"docker\"")
            && text.lines().any(|line| {
                let line = line.trim();
                line.starts_with("\"run\"") || line == "\"run\","
            });
        let exempt = OWN_DOCKER_RUN_EXEMPT
            .iter()
            .any(|(file, _)| name.ends_with(file));
        if runs_docker && !exempt && !text.contains("cap_args()") {
            gaps.push(name);
        }
    }
    assert!(gaps.is_empty(), "uncapped `docker run` argv: {gaps:?}");
}

#[test]
fn ci_slots_export_no_cpu_cap() {
    let config = crate::slots::Config::from_values(2, 12, None, None).unwrap();
    assert_eq!(
        crate::slots::guard::cap_for(&config, false).as_deref(),
        Some("6")
    );
    assert_eq!(
        crate::slots::guard::cap_for(&config, true),
        None,
        "a CI job has its runner to itself"
    );
}
