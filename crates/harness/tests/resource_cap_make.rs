//! A fresh snapshot has caps before workspace compilation, independent of long ownership.
mod make_fixture;
use std::process::Command;

#[test]
fn snapshot_containers_always_have_memory_swap_and_cpu_shares_without_quotas() {
    let workspace = make_fixture::workspace_named("resource-cap-");
    assert!(!workspace.path().join("target").exists());
    for target in [
        "test-unit-steps",
        "build-wasm-steps",
        "fuzz-smoke-steps",
        "map-generate-steps",
    ] {
        for inherited in [false, true] {
            let mut command = Command::new("make");
            command
                .current_dir(workspace.path())
                .args(["--dry-run", target]);
            for name in [
                "MAKEFLAGS",
                "MFLAGS",
                "GNUMAKEFLAGS",
                "AOE_LONG_JOB",
                "AOE_LONG_DIR",
            ] {
                command.env_remove(name);
            }
            if inherited {
                command
                    .env("AOE_LONG_JOB", "unverified")
                    .env("AOE_LONG_DIR", "/unused");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(
                text.contains(include_str!("../../../gates/container.args").trim()),
                "{target}: {text}"
            );
            assert!(text.contains("resource-runner' --run"), "{target}: {text}");
            assert!(!text.contains("--cpus"), "{target}: {text}");
        }
    }
    assert!(!workspace.path().join("target").exists());
}
