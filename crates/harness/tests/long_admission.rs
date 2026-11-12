//! Busy long jobs must stop before image work or harness compilation.
mod make_fixture;
use std::{
    fs,
    io::Write,
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn long_busy_make_refuses_before_missing_or_stale_runner_bootstrap() {
    let workspace = make_fixture::workspace_named("long-admission-");
    let root = workspace.path();
    let lock_dir = root.join(".git/aoe-ship");
    fs::create_dir_all(&lock_dir).unwrap();
    let mut lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(lock_dir.join("long-0.lock"))
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
            "#!/bin/sh\nif [ \"$1\" = run ] || [ \"$1\" = build ]; then touch '{}'; fi\nexit 1\n",
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
        let runner = root.join(".cache/resource-runner");
        if stale {
            fs::write(&runner, "stale runner").unwrap();
            assert!(
                Command::new("touch")
                    .args(["-t", "200001010000"])
                    .arg(&runner)
                    .status()
                    .unwrap()
                    .success()
            );
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
        } else {
            fs::remove_file(&runner).unwrap();
        }
        let begun = Instant::now();
        let output = Command::new("make")
            .current_dir(root)
            .args(["--no-print-directory", "-j4", "coverage"])
            .env_remove("AOE_LONG_JOB")
            .env_remove("AOE_LONG_DIR")
            .env("AOE_LONG_SLOTS", "1")
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
fn long_lock_does_not_block_short_make_target() {
    let workspace = make_fixture::workspace_named("long-short-");
    let root = workspace.path();
    fs::create_dir_all(root.join(".git/aoe-ship")).unwrap();
    let lock = fs::File::create(root.join(".git/aoe-ship/long-0.lock")).unwrap();
    lock.try_lock().unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    let marker = root.join("short-started");
    fs::write(
        bin.join("docker"),
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(bin.join("docker"), fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let output = Command::new("make")
        .current_dir(root)
        .env("PATH", path)
        .args(["--no-print-directory", "fmt-check"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(marker.exists());
}

#[test]
fn long_make_precheck_allows_valid_inheritance_and_focused_e2e() {
    let workspace = make_fixture::workspace_named("long-nested-");
    let root = workspace.path();
    let dir = root.join(".git/aoe-ship");
    fs::create_dir_all(&dir).unwrap();
    let mut lock = fs::File::create(dir.join("long-0.lock")).unwrap();
    lock.try_lock().unwrap();
    write!(lock, "proof\nheld job").unwrap();
    let runner = root.join(".cache/resource-runner");
    fs::write(&runner, "#!/bin/sh\nexit 0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&runner, fs::Permissions::from_mode(0o755)).unwrap();
    for (target, token, specs, expected) in [
        ("coverage", "proof", "", true),
        ("coverage", "wrong", "", false),
        ("test-e2e", "", "tests/one.spec.ts", true),
    ] {
        let output = Command::new("make")
            .current_dir(root)
            .args(["--no-print-directory", target])
            .env("AOE_LONG_DIR", &dir)
            .env("AOE_LONG_JOB", token)
            .env("AOE_LONG_SLOTS", "1")
            .env("AOE_E2E_SPECS", specs)
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            expected,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
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
        .env("AOE_LONG_SLOTS", "1")
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
        .env("AOE_LONG_SLOTS", "1")
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
    let lock = fs::File::create(root.join(".git/aoe-ship/long-0.lock")).unwrap();
    lock.try_lock().unwrap();
    for (specs, expected) in [("tests/one.spec.ts", 0), ("", 75)] {
        let result = Command::new(root.join(".cache/resource-runner"))
            .current_dir(root)
            .args(["--long", "test-e2e", "--", "true"])
            .env_remove("AOE_LONG_JOB")
            .env_remove("AOE_LONG_DIR")
            .env("AOE_LONG_SLOTS", "1")
            .env("AOE_E2E_SPECS", specs)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(expected));
    }
}

#[test]
fn runner_takes_the_first_free_slot_and_refuses_only_when_all_are_held() {
    let workspace = make_fixture::workspace_named("long-slots-");
    let root = workspace.path();
    fs::create_dir_all(root.join(".git/aoe-ship")).unwrap();
    let mut held = Vec::new();
    for slot in 0..3 {
        let mut lock =
            fs::File::create(root.join(format!(".git/aoe-ship/long-{slot}.lock"))).unwrap();
        let admitted = Command::new(root.join(".cache/resource-runner"))
            .current_dir(root)
            .args(["--long", "coverage", "--", "true"])
            .env_remove("AOE_LONG_JOB")
            .env_remove("AOE_LONG_DIR")
            .output()
            .unwrap();
        assert_eq!(
            admitted.status.code(),
            Some(0),
            "slot {slot} was still free"
        );
        lock.try_lock().unwrap();
        write!(
            lock,
            "proof{slot}\n`make coverage` on holder{slot} in /holder{slot}"
        )
        .unwrap();
        held.push(lock);
    }
    let refusal = Command::new(root.join(".cache/resource-runner"))
        .current_dir(root)
        .args(["--long", "coverage", "--", "true"])
        .env_remove("AOE_LONG_JOB")
        .env_remove("AOE_LONG_DIR")
        .output()
        .unwrap();
    assert_eq!(refusal.status.code(), Some(75));
    let stderr = String::from_utf8_lossy(&refusal.stderr);
    for slot in 0..3 {
        assert!(
            stderr.contains(&format!("slot {slot}: `make coverage` on holder{slot}")),
            "{stderr}"
        );
    }
}
