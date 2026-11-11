use super::{JobEnv, guard};
use crate::gates::registry::Registry;
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    time::{Duration, Instant},
};

mod lock;
mod slots;
use lock::SERIAL;

fn registry() -> Registry {
    Registry::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap()
}

#[test]
fn only_benchmarks_full_e2e_coverage_and_their_aggregates_are_long() {
    let registry = registry();
    let expected: BTreeSet<_> = [
        "coverage",
        "test-e2e",
        "perf-smoke",
        "perf-ci",
        "perf-full",
        "perf-instructions",
        "perf-groups",
        "perf-pressure",
        "perf-scale",
        "perf-scale-smoke",
        "perf-soak-10",
        "perf-soak-30",
        "perf-stress",
        "release-build",
        "preflight",
        "preflight-unit",
        "perf-late-game",
        "perf-late-game-instructions",
        "perf-groups-instructions",
        "perf-groups-profile",
        "scale-match",
        "scale-match-smoke",
        "perf-baseline-propose",
    ]
    .into_iter()
    .collect();
    let actual: BTreeSet<_> = registry
        .gates
        .iter()
        .filter(|gate| gate.long)
        .map(|gate| gate.id.as_str())
        .collect();
    assert_eq!(actual, expected);
    let env = JobEnv::default();
    for name in expected.into_iter().chain([
        "preflight",
        "preflight-unit",
        "perf-late-game",
        "scale-match",
        "perf-baseline-propose",
    ]) {
        assert!(guard::is_long(&registry, name, &env), "{name}");
    }
    for name in [
        "lint",
        "build-wasm",
        "test-harness",
        "test-unit",
        "test-wasm",
        "perf-wasm-size",
        "perf-wasm-cap",
        "test-client-memory",
        "build",
    ] {
        assert!(!guard::is_long(&registry, name, &env), "{name}");
    }
    let focused = JobEnv {
        e2e_specs: Some("tests/editor.spec.ts".into()),
        ..env
    };
    assert!(!guard::is_long(&registry, "test-e2e", &focused));
    assert!(guard::is_long(&registry, "coverage", &focused));
    assert!(guard::is_long(&registry, "preflight", &focused));
}

#[test]
fn short_work_admission_never_contends_with_the_long_job_and_nested_long_work_inherits() {
    run_role("short");
}

#[test]
fn aggregate_admission_uses_its_actual_plan_and_focused_e2e_exception() {
    run_role("plan");
}

fn run_role(role: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "jobs::tests::guard_role", "--nocapture"])
        .env("AOE_TEST_JOB_ROLE", role)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn guard_role() {
    let Ok(role) = std::env::var("AOE_TEST_JOB_ROLE") else {
        return;
    };
    assert!(matches!(role.as_str(), "short" | "plan"));

    let _serial = SERIAL.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    fs::create_dir_all(root.join("gates")).unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q", "-b", "feature"])
            .arg(&root)
            .status()
            .unwrap()
            .success()
    );
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../gates/registry.json"),
        root.join("gates/registry.json"),
    )
    .unwrap();
    let env = JobEnv {
        dir: Some(root.join("locks").into_os_string()),
        ..JobEnv::default()
    };
    if role == "plan" {
        let registry = registry();
        let find = |id: &str| registry.gates.iter().find(|gate| gate.id == id).unwrap();
        let dir = guard::lock_dir(&root, &env).unwrap();
        let foreign = (0..3)
            .map(|slot| {
                let path = super::lock::slot_path(&dir, slot);
                super::lock::LongLock::acquire(&path, "other job", "other", None).unwrap()
            })
            .collect::<Vec<_>>();
        let started = Instant::now();
        let short =
            guard::hold_plan(&root, "gate-run", &[find("lint"), find("build-wasm")], &env).unwrap();
        assert!(short.env().is_empty());
        let error = guard::hold_plan(&root, "gate-run", &[find("coverage")], &env).unwrap_err();
        assert_eq!(super::lock::exit_code(error.as_ref()), 75);
        assert!(started.elapsed() < Duration::from_secs(1));
        let focused = JobEnv {
            e2e_specs: Some("tests/focused.spec.ts".into()),
            ..env.clone()
        };
        assert!(
            guard::hold_plan(&root, "gate-run", &[find("test-e2e")], &focused)
                .unwrap()
                .env()
                .is_empty()
        );
        assert!(guard::hold_plan(&root, "gate-run", &[find("coverage")], &focused).is_err());
        drop(foreign);
        let plan = guard::hold_plan(&root, "gate-run", &[find("coverage")], &env).unwrap();
        let nested = guard::hold(&root, "perf-smoke", &env).unwrap();
        assert_eq!(plan.env(), nested.env());
        return;
    }
    assert_ne!(
        guard::lock_dir(&root, &env).unwrap(),
        root.join("locks"),
        "an arbitrary environment directory without live ownership cannot split the lock"
    );
    let long = guard::hold(&root, "coverage", &env).unwrap();
    let begun = Instant::now();
    for name in ["lint", "build-wasm", "test-harness", "test-unit"] {
        let _short = guard::hold(&root, name, &env).unwrap();
    }
    assert!(begun.elapsed() < Duration::from_secs(1));
    let nested = guard::hold(&root, "perf-smoke", &env).unwrap();
    assert_eq!(long.env(), nested.env());
    drop(nested);
    drop(long);
    let dir = guard::lock_dir(&root, &env).unwrap();
    for slot in 0..3 {
        assert!(
            super::lock::status(&super::lock::slot_path(&dir, slot))
                .unwrap()
                .is_none()
        );
    }
}
