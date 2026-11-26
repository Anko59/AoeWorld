//! The decision for each kind of change, against the real crate graph: the jobs that
//! run, the jobs that are skipped and the unit-test scope. `docs/ci-selection.md` shows
//! the same table; a change to the graph or to `gates/ci-rules.json` changes a row here.
use super::*;

/// The jobs a pull request can narrow. `static` always runs; the post-merge jobs never do.
const NARROWED: [&str; 4] = ["browser", "e2e", "target-performance", "fuzz-smoke"];

struct Row {
    change: &'static str,
    paths: &'static [&'static str],
    /// The narrowed jobs that run.
    jobs: &'static [&'static str],
    /// The crates whose unit tests run, or `None` for the whole workspace.
    unit: Option<&'static [&'static str]>,
    /// The legs of the `static` job.
    parts: &'static [&'static str],
    perf_smoke: bool,
}

const ALL_NARROWED: &[&str] = &NARROWED;

const ROWS: &[Row] = &[
    Row {
        change: "docs only",
        paths: &["docs/new-guide.md"],
        jobs: &[],
        unit: Some(&[]),
        parts: &["light"],
        perf_smoke: false,
    },
    Row {
        change: "ai/ Python only",
        paths: &["ai/aoe_ai/client.py", "ai/tests/test_client.py"],
        jobs: &[],
        unit: Some(&[]),
        parts: &["light"],
        perf_smoke: false,
    },
    Row {
        change: "crates/client only",
        paths: &["crates/client/src/lib.rs"],
        jobs: &["browser", "e2e", "target-performance"],
        unit: Some(&["aoe-client"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "crates/rendering only",
        paths: &["crates/rendering/src/lib.rs"],
        jobs: &["browser", "e2e", "target-performance"],
        unit: Some(&["aoe-rendering", "aoe-client", "aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "crates/simulation only",
        paths: &["crates/simulation/src/lib.rs"],
        jobs: ALL_NARROWED,
        unit: Some(&[
            "aoe-simulation",
            "aoe-client",
            "aoe-sdk",
            "aoe-server",
            "aoe-ai",
            "aoe-harness",
        ]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "crates/ai (the Rust AI) only",
        paths: &["crates/ai/src/lib.rs"],
        jobs: &["e2e", "target-performance", "fuzz-smoke"],
        unit: Some(&["aoe-ai", "aoe-server", "aoe-sdk", "aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "crates/server (the bindings' tests run a built server)",
        paths: &["crates/server/src/lib.rs"],
        jobs: &["e2e", "target-performance", "fuzz-smoke"],
        unit: Some(&["aoe-server", "aoe-sdk", "aoe-ai", "aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "crates/protocol",
        paths: &["crates/protocol/src/lib.rs"],
        jobs: ALL_NARROWED,
        unit: Some(&[
            "aoe-protocol",
            "aoe-rendering",
            "aoe-client",
            "aoe-sdk",
            "aoe-server",
            "aoe-ai",
            "aoe-harness",
        ]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "crates/map",
        paths: &["crates/map/src/lib.rs"],
        jobs: ALL_NARROWED,
        unit: Some(&[
            "aoe-map",
            "aoe-geodata",
            "aoe-simulation",
            "aoe-client",
            "aoe-sdk",
            "aoe-server",
            "aoe-ai",
            "aoe-harness",
        ]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "crates/geodata",
        paths: &["crates/geodata/src/lib.rs"],
        jobs: &[],
        unit: Some(&["aoe-geodata"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "crates/assets",
        paths: &["crates/assets/src/lib.rs"],
        jobs: ALL_NARROWED,
        unit: Some(&["aoe-assets", "aoe-client", "aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "crates/sdk-python (outside the workspace)",
        paths: &["crates/sdk-python/src/lib.rs"],
        jobs: &["fuzz-smoke"],
        unit: Some(&[]),
        parts: &["light"],
        perf_smoke: false,
    },
    Row {
        change: "a manifest or lock file of a crate outside the workspace",
        paths: &["crates/sdk-python/Cargo.toml", "fuzz/Cargo.lock"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "fuzz targets (outside the workspace)",
        paths: &["fuzz/fuzz_targets/drs.rs"],
        jobs: &["fuzz-smoke"],
        unit: Some(&[]),
        parts: &["light"],
        perf_smoke: false,
    },
    Row {
        change: "harness perf_comment",
        paths: &["crates/harness/src/perf_comment/render.rs"],
        jobs: &["target-performance"],
        unit: Some(&["aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "harness e2e module",
        paths: &["crates/harness/src/e2e/shard.rs"],
        jobs: &["e2e"],
        unit: Some(&["aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "harness review",
        paths: &["crates/harness/src/review/trusted.rs"],
        jobs: &[],
        unit: Some(&["aoe-harness"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "harness ship (git, gh and JSON helpers that e2e, perf_comment and the unit leg use)",
        paths: &["crates/harness/src/ship/git.rs"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "harness module no rule narrows (selection itself)",
        paths: &["crates/harness/src/gates/select/decide.rs"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: ".github/workflows/ci.yml",
        paths: &[".github/workflows/ci.yml"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "make/, gates/ and the Makefile",
        paths: &["make/jobs.mk", "gates/registry.json", "Makefile"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "Cargo.lock",
        paths: &["Cargo.lock"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "a crate manifest",
        paths: &["crates/geodata/Cargo.toml"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "rust-toolchain.toml",
        paths: &["rust-toolchain.toml"],
        jobs: ALL_NARROWED,
        unit: None,
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "deny.toml",
        paths: &["deny.toml"],
        jobs: &[],
        unit: Some(&[]),
        parts: &["checks"],
        perf_smoke: false,
    },
    Row {
        change: "browser/tests/** only",
        paths: &["browser/tests/village/new.spec.ts"],
        jobs: &["browser", "e2e"],
        unit: Some(&["aoe-harness"]),
        parts: &["light", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "web/ only",
        paths: &["web/zz-probe.js"],
        jobs: &["browser", "e2e"],
        unit: Some(&[]),
        parts: &["light"],
        perf_smoke: false,
    },
    Row {
        change: "instructions only (AGENTS.md)",
        paths: &["AGENTS.md", "crates/map/AGENTS.md"],
        jobs: &[],
        unit: Some(&["aoe-harness"]),
        parts: &["light", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "a golden a protocol test reads",
        paths: &["browser/tests/wire/villager.hex"],
        jobs: &["browser", "e2e"],
        unit: Some(&[
            "aoe-protocol",
            "aoe-rendering",
            "aoe-client",
            "aoe-sdk",
            "aoe-server",
            "aoe-ai",
            "aoe-harness",
        ]),
        parts: &["light", "unit"],
        perf_smoke: true,
    },
    Row {
        change: "mixed: docs, a client file and ai/",
        paths: &[
            "docs/new-guide.md",
            "crates/client/src/lib.rs",
            "ai/aoe_ai/client.py",
        ],
        jobs: &["browser", "e2e", "target-performance"],
        unit: Some(&["aoe-client"]),
        parts: &["checks", "unit"],
        perf_smoke: false,
    },
    Row {
        change: "mixed: client and ai crates",
        paths: &["crates/client/src/lib.rs", "crates/ai/src/lib.rs"],
        jobs: ALL_NARROWED,
        unit: Some(&[
            "aoe-client",
            "aoe-ai",
            "aoe-server",
            "aoe-sdk",
            "aoe-harness",
        ]),
        parts: &["checks", "unit"],
        perf_smoke: true,
    },
];

fn expected_jobs(row: &Row) -> BTreeSet<&'static str> {
    ["static"]
        .into_iter()
        .chain(row.jobs.iter().copied())
        .collect()
}

#[test]
fn each_kind_of_change_runs_only_the_jobs_it_can_break() {
    for row in ROWS {
        let decision = pull_request(row.paths);
        assert_eq!(running(&decision), expected_jobs(row), "{}", row.change);
        let skipped: Vec<_> = decision.jobs.iter().filter(|(_, r)| !**r).collect();
        for (job, _) in skipped {
            assert!(
                decision.reasons[job].starts_with("skipped because"),
                "{}: {job}: {}",
                row.change,
                decision.reasons[job]
            );
        }
        for job in running(&decision) {
            assert!(
                decision.reasons[job].starts_with("runs"),
                "{}: {job}: {}",
                row.change,
                decision.reasons[job]
            );
        }
    }
}

#[test]
fn each_kind_of_change_runs_the_unit_tests_of_the_crates_it_can_break() {
    for row in ROWS {
        let decision = pull_request(row.paths);
        let scope = decision
            .unit
            .selection
            .packages()
            .map(|p| p.into_iter().collect::<BTreeSet<String>>());
        let expected = row.unit.map(|crates| {
            crates
                .iter()
                .map(|c| (*c).to_owned())
                .collect::<BTreeSet<_>>()
        });
        assert_eq!(scope, expected, "{}", row.change);
        assert_eq!(decision.unit.perf_smoke, row.perf_smoke, "{}", row.change);
        let parts: Vec<_> = decision.static_parts.iter().map(String::as_str).collect();
        assert_eq!(parts, row.parts, "{}", row.change);
    }
}

#[test]
fn the_reason_names_the_changed_crate_and_the_path() {
    let decision = pull_request(&["crates/simulation/src/lib.rs"]);
    assert_eq!(
        decision.reasons["browser"],
        "runs because aoe-client builds on aoe-simulation, which changed \
         (crates/simulation/src/lib.rs)"
    );
    let decision = pull_request(&["crates/client/src/lib.rs"]);
    assert_eq!(
        decision.reasons["browser"],
        "runs because aoe-client changed (crates/client/src/lib.rs)"
    );
    assert!(decision.reasons["fuzz-smoke"].starts_with("skipped because no changed crate"));
    let decision = pull_request(&["crates/harness/src/perf/network.rs"]);
    assert!(decision.reasons["target-performance"].contains("crates/harness/src/perf/network.rs"));
}

#[test]
fn a_client_change_does_not_reach_the_simulation_but_a_simulation_change_reaches_the_client() {
    let client = pull_request(&["crates/client/src/lib.rs"]);
    let unit = client.unit.selection.packages().expect("a selection");
    assert!(!unit.contains(&"aoe-simulation".to_owned()), "{unit:?}");
    assert!(!client.jobs["fuzz-smoke"]);
    let simulation = pull_request(&["crates/simulation/src/lib.rs"]);
    assert!(
        simulation.jobs["browser"],
        "the client's closure contains the simulation"
    );
}
