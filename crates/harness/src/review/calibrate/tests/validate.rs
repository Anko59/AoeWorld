//! The seed-set validation that `make test-harness` runs: every case's
//! repository is a working crate whose tests stay green through the diff.
use super::{repo, repo_root, shipped};
use std::{fs, path::Path, process::Command, sync::Mutex};

/// Builds (and, in the `run` phase, tests) a seed's crate the way a reviewer
/// session would, in the worktree's ignored `.cache/` (never `/tmp`), one at
/// a time.
fn cargo(root: &Path, target: &Path, id: &str, phase: &str, run: bool, probe: bool) {
    let mut command = Command::new("cargo");
    command
        .arg("test")
        .arg("--offline")
        .arg("--quiet")
        .arg("--target-dir")
        .arg(target);
    if !run {
        command.arg("--no-run");
    }
    if probe {
        command.args(["--test", "calibration_probe"]);
    }
    let out = command
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("{id}: cargo test: {e}"));
    assert!(
        out.status.success(),
        "{id}: cargo test {phase} failed:\n{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn every_seed_base_builds_its_tests_run_and_the_diff_keeps_them_green() {
    static CARGOS: Mutex<()> = Mutex::new(());
    let _guard = CARGOS.lock().unwrap();
    let target = repo_root().join(".cache/seed-validation");
    for case in shipped() {
        let dir =
            repo::materialise(&repo_root(), &case).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        let root = dir.path();
        // The diff leaves every defect's lines inside its file.
        for defect in case.defects() {
            let text = fs::read_to_string(root.join(&defect.file))
                .unwrap_or_else(|e| panic!("{}: {}: {e}", case.id, defect.file));
            let lines = text.lines().count() as u32;
            assert!(
                defect.lines[1] <= lines,
                "{}: defect lines {:?} run past {} ({} lines)",
                case.id,
                defect.lines,
                defect.file,
                lines
            );
        }
        // TypeScript seeds have no offline toolchain; their bases are not
        // built here.
        if !case.base.contains_key("Cargo.toml") {
            continue;
        }
        // A defect the seed's own tests catch would not be hard to review:
        // the tests stay green through the diff. The base only has to build
        // and compile its tests — one seed's base fails them on purpose,
        // the failing behaviour its task exists to fix.
        cargo(root, &target, &case.id, "on the diff", true, false);
        let checkout = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["checkout", "-q", "dev"])
            .output()
            .unwrap_or_else(|e| panic!("{}: git checkout dev: {e}", case.id));
        assert!(
            checkout.status.success(),
            "{}: checkout dev failed",
            case.id
        );
        cargo(root, &target, &case.id, "on the base", false, false);
    }
}

#[test]
fn every_rust_seed_runs_obvious_inputs_on_its_base_and_diff() {
    let target = repo_root().join(".cache/seed-probes");
    let mut failures = Vec::new();
    for case in shipped()
        .into_iter()
        .filter(|c| c.base.contains_key("Cargo.toml"))
    {
        let dir = repo::materialise(&repo_root(), &case).unwrap();
        for post in [true, false] {
            let phase = if post { "on the diff" } else { "on the base" };
            if !post {
                assert!(
                    Command::new("git")
                        .current_dir(dir.path())
                        .args(["checkout", "-q", "dev"])
                        .status()
                        .unwrap()
                        .success()
                );
            }
            repo::write(
                dir.path(),
                "tests/calibration_probe.rs",
                &super::probes::source(&case.id, post),
            )
            .unwrap();
            if std::panic::catch_unwind(|| cargo(dir.path(), &target, &case.id, phase, true, true))
                .is_err()
            {
                failures.push(format!("{} {phase}", case.id));
            } else {
                eprintln!("{}: obvious inputs {phase}: PASS", case.id);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "seed input probes failed: {failures:?}"
    );
}
