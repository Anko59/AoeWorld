//! Compile the actual QA held-file reader, then remove only its staged link guard.
use super::{Kind, Path, ProcessError, Snapshot, git, run_in};
use crate::process;
use std::{fs, time::Duration};

const READER: &str = "src/observation/io.rs";
const GUARD: &str = " || metadata.nlink() != 1";
const TOKEN: &str = "AOE-QA-HARDLINK-COMPILED-CANARY-v1";
const TEST_NAME: &str = "observation::tests::b_hardlinked_qa_evidence_is_rejected";
const DATA_ENV: &str = "AOE_QA_CANARY_ROOT";
const TESTS: &str = r#"use std::{fs, os::unix::fs::MetadataExt, path::PathBuf};
fn root() -> PathBuf {
    let root = PathBuf::from(std::env::var("AOE_QA_CANARY_ROOT").unwrap());
    assert!(root.is_absolute());
    root
}
#[test]
fn a_single_link_qa_file_is_observed() {
    let path = root().join("single.bin");
    assert_eq!(fs::metadata(&path).unwrap().nlink(), 1);
    let mut held = super::io::Held::open(&path, 256).unwrap();
    assert_eq!(held.read().unwrap(), b"actual single-link QA control\x00\xff");
    let expected = blake3::hash(b"actual single-link QA control\x00\xff").to_hex().to_string();
    held.recheck(&expected).unwrap();
}
#[test]
fn b_hardlinked_qa_evidence_is_rejected() {
    let path = root().join("hard-a.bin");
    assert_eq!(fs::metadata(&path).unwrap().nlink(), 2);
    assert!(super::io::Held::open(&path, 256).is_err(),
        "assertion failed: QA single-link guard accepted hardlink: AOE-QA-HARDLINK-COMPILED-CANARY-v1");
}
"#;

fn locked_version(lock: &str, name: &str) -> String {
    // Only the generated lock's fixed nix/blake3 package records are needed here;
    // this fixture helper is not a general TOML parser or admission policy.
    let named = format!("name = \"{name}\"");
    let matching: Vec<_> = lock
        .split("[[package]]")
        .skip(1)
        .filter(|package| package.lines().any(|line| line.trim() == named))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "canary dependency requires one actual root lock version"
    );
    let quoted = matching[0]
        .lines()
        .find_map(|line| line.trim().strip_prefix("version = "))
        .expect("actual generated lock package version missing");
    let version: String = serde_json::from_str(quoted)
        .expect("generated lock version must be a quoted numeric version");
    assert!(
        version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    );
    version
}

fn fixture() -> (tempfile::TempDir, String, Vec<u8>) {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path();
    let harness = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = fs::read(harness.join("src/qa/observation/io.rs")).unwrap();
    let text = std::str::from_utf8(&source).unwrap();
    assert_eq!(
        text.matches(GUARD).count(),
        1,
        "production guard changed; review this canary"
    );
    let lock = fs::read_to_string(harness.join("../../Cargo.lock")).unwrap();
    let blake3 = locked_version(&lock, "blake3");
    let nix = locked_version(&lock, "nix");
    git(root, &["init", "-q"], None);
    fs::create_dir_all(root.join("src/observation")).unwrap();
    fs::write(root.join(".gitignore"), ".cache/\ntarget/\n").unwrap();
    fs::write(root.join("Cargo.toml"), format!(
        "[package]\nname=\"aoe-qa-compiled-canary\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\nblake3=\"={blake3}\"\nnix=\"={nix}\"\n"
    )).unwrap();
    fs::write(root.join("src/lib.rs"), "mod observation;\n").unwrap();
    fs::write(root.join("src/observation/mod.rs"),
        "type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;\nmod io;\n#[cfg(test)] mod tests;\n"
    ).unwrap();
    fs::write(root.join(READER), &source).unwrap();
    fs::write(root.join("src/observation/tests.rs"), TESTS).unwrap();
    process::run_in(
        root,
        "cargo",
        &["generate-lockfile", "--offline"],
        &[("CARGO_NET_OFFLINE", "true")],
        Duration::from_secs(600),
    )
    .expect("offline fixture dependency resolution must succeed before any canary catch");
    git(root, &["add", "."], None);
    let tree = git(root, &["write-tree"], None);
    let raw = format!(
        "tree {tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\ncompiled QA reader canary\n"
    );
    let oid = git(
        root,
        &["hash-object", "-t", "commit", "-w", "--stdin"],
        Some(raw.as_bytes()),
    );
    git(root, &["update-ref", "HEAD", &oid], None);
    (owner, oid, source)
}

fn native_and_docs(root: &Path, environment: &[(&str, &str)]) -> Result<(), ProcessError> {
    run_in(root, environment)?;
    process::run_in(
        root,
        "cargo",
        &["test", "--workspace", "--doc", "--locked"],
        environment,
        Duration::from_secs(600),
    )
}

#[test]
fn actual_qa_reader_rejects_compiled_staged_hardlink_guard_mutation() {
    let (owner, oid, source) = fixture();
    let root = owner.path();
    let data = tempfile::tempdir().unwrap();
    fs::write(
        data.path().join("single.bin"),
        b"actual single-link QA control\x00\xff",
    )
    .unwrap();
    fs::write(
        data.path().join("hard-a.bin"),
        b"actual hardlinked QA input",
    )
    .unwrap();
    fs::hard_link(
        data.path().join("hard-a.bin"),
        data.path().join("hard-b.bin"),
    )
    .unwrap();
    let target = tempfile::tempdir().unwrap();
    let mutated_target = tempfile::tempdir().unwrap();
    let working_target = tempfile::tempdir().unwrap();
    let mut environment = [
        ("CARGO_TARGET_DIR", target.path().to_str().unwrap()),
        ("CARGO_NET_OFFLINE", "true"),
        ("NEXTEST_FAILURE_OUTPUT", "never"),
        ("NEXTEST_SUCCESS_OUTPUT", "immediate"),
        (DATA_ENV, data.path().to_str().unwrap()),
    ];
    let tests = fs::read(root.join("src/observation/tests.rs")).unwrap();
    let lock = fs::read(root.join("Cargo.lock")).unwrap();
    assert_eq!(fs::read(root.join(READER)).unwrap(), source);
    let baseline = Snapshot::prepare_independent(root, Kind::Commit(oid)).unwrap();
    let baseline_witness = baseline.content_witness().unwrap();
    baseline
        .run_checked(|checkout| {
            native_and_docs(checkout, &environment)?;
            Ok(())
        })
        .expect("actual compiled QA baseline and doctests must pass before counting a catch");
    assert_eq!(baseline.content_witness().unwrap(), baseline_witness);

    let bad = std::str::from_utf8(&source).unwrap().replacen(GUARD, "", 1);
    fs::write(root.join(READER), bad).unwrap();
    git(root, &["add", READER], None);
    fs::write(root.join(READER), &source).unwrap();
    assert!(
        baseline.verify_source().is_err(),
        "old source endpoints must not refresh after staging"
    );
    let index = fs::read(root.join(".git/index")).unwrap();
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    let before = snapshot.content_witness().unwrap();
    environment[0].1 = mutated_target.path().to_str().unwrap();
    let error = snapshot
        .run_checked(|checkout| {
            native_and_docs(checkout, &environment)?;
            Ok(())
        })
        .unwrap_err();
    let ProcessError::Exit { code, log, .. } = error
        .downcast_ref::<ProcessError>()
        .expect("must fail as an actual test exit, never as a missing tool or monitor")
    else {
        panic!("compiled QA mutant did not produce an actual assertion exit")
    };
    assert!(code.is_some_and(|code| code != 0));
    let retained = fs::read(log).unwrap();
    let text = String::from_utf8_lossy(&retained);
    assert!(text.contains(TEST_NAME));
    assert!(text.contains(TOKEN));
    assert!(text.contains("assertion failed: QA single-link guard accepted hardlink"));
    assert!(!text.contains("could not compile"));
    assert!(retained.len() <= 2 * 64 * 1024 + 128);
    assert_eq!(snapshot.content_witness().unwrap(), before);
    snapshot.verify_source().unwrap();
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(READER)).unwrap(), source);
    assert_eq!(
        fs::read(root.join("src/observation/tests.rs")).unwrap(),
        tests
    );
    assert_eq!(fs::read(root.join("Cargo.lock")).unwrap(), lock);
    environment[0].1 = working_target.path().to_str().unwrap();
    native_and_docs(root, &environment)
        .expect("the restored working production reader and doctests remain a passing control");
    assert_eq!(snapshot.content_witness().unwrap(), before);
    snapshot.verify_source().unwrap();
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join(READER)).unwrap(), source);
    assert_eq!(
        fs::read(root.join("src/observation/tests.rs")).unwrap(),
        tests
    );
    assert_eq!(fs::read(root.join("Cargo.lock")).unwrap(), lock);
}
