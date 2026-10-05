use super::*;
mod qa;
use crate::{
    gates::scopes::{Kind, Snapshot},
    process::{Cancellation, CaptureExit, capture_in},
};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

const TOKEN: &str = "AOE-INTEGRITY-CANARY-v1";
const SUCCESS: &str = "AOE-NATIVE-SUCCESS-OUTPUT-v1";
const TEST_NAME: &str = "tests::a_known_marker_assertion";
const GOOD: &str = "pub fn marker() -> u32 { 1 }\n";
const BAD: &str = "pub fn marker() -> u32 { 2 }\n";
const TESTS: &str = r#"    #[test]
    fn a_known_marker_assertion() {
        if crate::marker() != 1 {
            eprintln!("{}", "N".repeat(70 * 1024));
        }
        assert_eq!(crate::marker(), 1, "AOE-INTEGRITY-CANARY-v1");
    }
    #[test]
    fn z_success_output() { println!("AOE-NATIVE-SUCCESS-OUTPUT-v1"); }
    #[test]
    fn z_valid_marker_type() { let _: u32 = crate::marker(); }
    #[test]
    fn z_independent_arithmetic() { assert_eq!(2_u32.checked_add(3), Some(5)); }
"#;

/// Remove the bounded failure log that `process::retain` writes under this workspace.
struct RetainedProcessLog(std::path::PathBuf);

impl RetainedProcessLog {
    fn new(path: &str) -> Self {
        let path = std::path::PathBuf::from(path);
        let current = std::env::current_dir().unwrap();
        let workspace = current
            .ancestors()
            .find(|ancestor| {
                fs::read_to_string(ancestor.join("Cargo.toml"))
                    .is_ok_and(|manifest| manifest.lines().any(|line| line.trim() == "[workspace]"))
            })
            .unwrap_or(current.as_path());
        let expected_directory = workspace.join("reports/process");
        assert_eq!(path.parent(), Some(expected_directory.as_path()));
        let name = path.file_name().and_then(|name| name.to_str()).unwrap();
        assert!(name.starts_with("cargo-") && name.ends_with(".log"));
        Self(path)
    }
}

impl Drop for RetainedProcessLog {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn git(root: &Path, args: &[&str], input: Option<&[u8]>) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["--no-replace-objects", "-c", "core.fsmonitor=false"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0");
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn().unwrap();
    if let Some(bytes) = input {
        child.stdin.take().unwrap().write_all(bytes).unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn fixture() -> (tempfile::TempDir, String) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    git(root, &["init", "-q"], None);
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join(".gitignore"), ".cache/\ntarget/\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"aoe-native-canary\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    )
    .unwrap();
    fs::write(
        root.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"aoe-native-canary\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), GOOD).unwrap();
    fs::write(root.join("src/tests.rs"), TESTS).unwrap();
    // Keep test bytes independent of the production-only staged mutation.
    fs::write(
        root.join("src/lib.rs"),
        format!("{GOOD}#[cfg(test)] mod tests;\n"),
    )
    .unwrap();
    git(root, &["add", "."], None);
    let tree = git(root, &["write-tree"], None);
    let raw = format!(
        "tree {tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nnative canary\n"
    );
    let oid = git(
        root,
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    );
    git(root, &["update-ref", "HEAD", &oid], None);
    (owner, oid)
}

#[test]
fn fixed_policy_preserves_workspace_locked_no_fail_fast_and_six_hundred_seconds() {
    assert_eq!(
        ARGS,
        &[
            "nextest",
            "run",
            "--workspace",
            "--locked",
            "--no-fail-fast",
            "--failure-output",
            "final",
            "--success-output",
            "never"
        ]
    );
    assert_eq!(BUDGET, Duration::from_secs(600));
}

#[test]
fn real_assertion_canary_rejects_compiled_staged_mutation_and_retains_noisy_failure() {
    let (owner, oid) = fixture();
    let root = owner.path();
    let target = tempfile::tempdir().unwrap();
    let target_path = target.path().to_str().unwrap();
    let mut environment = [
        ("CARGO_TARGET_DIR", target_path),
        ("CARGO_NET_OFFLINE", "true"),
        ("NEXTEST_SUCCESS_OUTPUT", "immediate"),
        ("NEXTEST_FAILURE_OUTPUT", "never"),
    ];
    let source = fs::read(root.join("src/lib.rs")).unwrap();
    let tests = fs::read(root.join("src/tests.rs")).unwrap();
    let lock = fs::read(root.join("Cargo.lock")).unwrap();
    let baseline = Snapshot::prepare_independent(root, Kind::Commit(oid)).unwrap();
    baseline
        .run_checked(|checkout| {
            run_in(checkout, &environment)?;
            Ok(())
        })
        .expect("actual passing baseline is required before counting a canary catch");
    let positive = baseline
        .run_checked(|checkout| {
            Ok(capture_in(
                checkout,
                "cargo",
                ARGS,
                &environment,
                BUDGET,
                &Cancellation::default(),
            ))
        })
        .unwrap();
    assert!(matches!(positive.exit, CaptureExit::Success));
    assert!(!String::from_utf8_lossy(&positive.stdout).contains(SUCCESS));
    assert!(!String::from_utf8_lossy(&positive.stderr).contains(SUCCESS));
    baseline.content_witness().unwrap();

    fs::write(
        root.join("src/lib.rs"),
        format!("{BAD}#[cfg(test)] mod tests;\n"),
    )
    .unwrap();
    git(root, &["add", "src/lib.rs"], None);
    fs::write(root.join("src/lib.rs"), &source).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    let before = snapshot.content_witness().unwrap();
    // Distinct source projections must not borrow each other's build artifacts.
    let mutated_target = tempfile::tempdir().unwrap();
    environment[0].1 = mutated_target.path().to_str().unwrap();
    let error = snapshot
        .run_checked(|checkout| {
            run_in(checkout, &environment)?;
            Ok(())
        })
        .unwrap_err();
    let error = error
        .downcast_ref::<ProcessError>()
        .expect("real test command must fail with an exit, not an unavailable tool");
    let log = match error {
        ProcessError::Exit { log, .. }
        | ProcessError::Deadline { log, .. }
        | ProcessError::Cancelled { log, .. } => log,
        _ => panic!("native canary did not retain captured process output"),
    };
    let retained_log = RetainedProcessLog::new(log);
    let retained_path = retained_log.0.clone();
    let ProcessError::Exit { code, .. } = error else {
        panic!("not an actual native test exit")
    };
    assert!(code.is_some_and(|code| code != 0));
    let retained = fs::read(&retained_log.0).unwrap();
    let text = String::from_utf8_lossy(&retained);
    assert!(text.contains("[output capture truncated]"));
    assert!(text.contains(TOKEN));
    assert!(text.contains(TEST_NAME));
    assert!(text.contains("assertion `left == right` failed"));
    assert!(!text.contains("could not compile"));
    assert!(!text.contains(SUCCESS));
    assert!(retained.len() <= 2 * 64 * 1024 + 128);

    let captured = snapshot
        .run_checked(|checkout| {
            Ok(capture_in(
                checkout,
                "cargo",
                ARGS,
                &environment,
                BUDGET,
                &Cancellation::default(),
            ))
        })
        .unwrap();
    assert!(matches!(captured.exit, CaptureExit::Failed(Some(code)) if code != 0));
    assert!(captured.truncated);
    assert!(captured.stdout.len() <= 64 * 1024);
    assert!(captured.stderr.len() <= 64 * 1024);
    let tail = String::from_utf8_lossy(&captured.stderr);
    assert!(tail.contains(TOKEN));
    assert!(tail.contains(TEST_NAME));
    assert!(tail.contains("assertion `left == right` failed"));
    assert_eq!(snapshot.content_witness().unwrap(), before);
    snapshot.verify_source().unwrap();
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("src/lib.rs")).unwrap(), source);
    assert_eq!(fs::read(root.join("src/tests.rs")).unwrap(), tests);
    assert_eq!(fs::read(root.join("Cargo.lock")).unwrap(), lock);
    let working_target = tempfile::tempdir().unwrap();
    environment[0].1 = working_target.path().to_str().unwrap();
    run_in(root, &environment).expect("unmutated working source remains a passing control");
    assert_eq!(snapshot.content_witness().unwrap(), before);
    drop(retained_log);
    assert!(!retained_path.exists());
}
