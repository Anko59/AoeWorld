//! Busy long jobs must stop before image work or harness compilation.
mod make_fixture;
use std::{
    fs,
    io::Write,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn busy_long_make_refuses_before_missing_or_stale_harness_bootstrap() {
    let workspace = make_fixture::workspace_named("long-admission-");
    let root = workspace.path();
    let lock_dir = root.join(".git/aoe-ship");
    fs::create_dir_all(&lock_dir).unwrap();
    let mut lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_dir.join("long.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    write!(
        lock,
        "test-proof\n`make coverage` on holder in /holder (started test UTC)"
    )
    .unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let marker = root.join("workload-started");
    fs::write(
        bin.join("docker"),
        format!(
            "#!/bin/sh\nif [ \"$1\" = run ]; then touch '{}'; fi\nexit 1\n",
            marker.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(bin.join("docker"), fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    // Cargo uses this kernel lock for its target directory. Admission must
    // refuse without opening Cargo, even while its build lock is held.
    fs::create_dir_all(root.join("target/debug")).unwrap();
    let cargo_lock = fs::File::create(root.join("target/debug/.cargo-lock")).unwrap();
    cargo_lock.try_lock().unwrap();
    for stale in [false, true] {
        if stale {
            fs::create_dir_all(root.join("target/debug")).unwrap();
            fs::write(root.join("target/debug/aoe-harness"), "stale executable").unwrap();
            assert!(
                Command::new("touch")
                    .args(["-t", "200001010000"])
                    .arg(root.join("target/debug/aoe-harness"))
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let begun = Instant::now();
        let output = Command::new("make")
            .current_dir(root)
            .args(["--no-print-directory", "perf-scale"])
            .env_remove("AOE_LONG_JOB")
            .env_remove("AOE_LONG_DIR")
            .env("HARNESS_TARGET_CACHE", root.join("target"))
            .env("PATH", &path)
            .output()
            .unwrap();
        assert!(begun.elapsed() < Duration::from_secs(1));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(
            stderr.contains("retry later") && stderr.contains("/holder"),
            "{stderr}"
        );
        assert!(!marker.exists(), "{stderr}");
        assert!(!root.join("target/debug/aoe-harness").exists() || stale);
    }
}

#[test]
fn runner_lock_survives_controller_death_and_nested_jobs_validate_ownership() {
    use std::io::{BufRead, BufReader};
    let workspace = make_fixture::workspace_named("long-inheritance-");
    let root = workspace.path();
    let runner = root.join(".cache/resource-runner");
    // The child traverses the runner twice, then replaces itself with sleep.
    let mut parent = Command::new(&runner)
        .current_dir(root)
        .args(["--long", "coverage", "--"])
        .arg(&runner)
        .args([
            "--long",
            "coverage",
            "--",
            "sh",
            "-c",
            "echo READY $$; exec sleep 30",
        ])
        .env_remove("AOE_LONG_JOB")
        .env_remove("AOE_LONG_DIR")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(parent.stdout.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let pid: u32 = line
        .trim()
        .strip_prefix("READY ")
        .expect("nested job started")
        .parse()
        .unwrap();
    parent.kill().unwrap();
    parent.wait().unwrap();
    let refusal = Command::new(&runner)
        .current_dir(root)
        .args(["--long", "coverage", "--", "true"])
        .env_remove("AOE_LONG_JOB")
        .env_remove("AOE_LONG_DIR")
        .output()
        .unwrap();
    assert!(
        Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(refusal.status.code(), Some(75));
    assert!(String::from_utf8_lossy(&refusal.stderr).contains("retry later"));
}

#[test]
fn focused_e2e_skips_long_admission_but_full_e2e_does_not() {
    let workspace = make_fixture::workspace_named("long-focused-");
    let root = workspace.path();
    fs::create_dir_all(root.join(".git/aoe-ship")).unwrap();
    let lock = fs::File::create(root.join(".git/aoe-ship/long.lock")).unwrap();
    lock.try_lock().unwrap();
    for (specs, expected) in [("tests/one.spec.ts", 0), ("", 75)] {
        let result = Command::new(root.join(".cache/resource-runner"))
            .current_dir(root)
            .args(["--long", "test-e2e", "--", "true"])
            .env_remove("AOE_LONG_JOB")
            .env_remove("AOE_LONG_DIR")
            .env("AOE_E2E_SPECS", specs)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(expected));
    }
}
