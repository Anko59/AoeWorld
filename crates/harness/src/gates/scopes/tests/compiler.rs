use super::*;
use std::time::Duration;

#[test]
fn real_clippy_rejects_staged_type_error_despite_valid_working_source() {
    let repo = repo();
    let root = repo.path();
    put(
        root,
        "Cargo.toml",
        "[package]\nname=\"aoe-scope-fixture\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace]\n",
    );
    put(root, "src/lib.rs", "pub fn sample() -> u32 { 1 }\n");
    crate::process::run_in(
        root,
        "cargo",
        &["generate-lockfile", "--offline"],
        &[],
        Duration::from_secs(30),
    )
    .unwrap();
    run(root, &["add", "Cargo.toml", "Cargo.lock", "src/lib.rs"]);
    run(root, &["commit", "--quiet", "-m", "minimal Rust package"]);
    put(
        root,
        "src/lib.rs",
        "pub fn sample() -> u32 { \"wrong type\" }\n",
    );
    run(root, &["add", "src/lib.rs"]);
    put(root, "src/lib.rs", "pub fn sample() -> u32 { 1 }\n");
    let target = root.join(".cache/check-target");
    let target = target.to_str().unwrap();
    let args = [
        "clippy",
        "--workspace",
        "--all-targets",
        "--locked",
        "--",
        "-D",
        "warnings",
    ];
    crate::process::run_in(
        root,
        "cargo",
        &args,
        &[("CARGO_TARGET_DIR", target)],
        Duration::from_secs(60),
    )
    .expect("valid working control");
    let before = index_bytes(root);
    let snapshot = Snapshot::prepare(root, Kind::Index).unwrap();
    let error = snapshot
        .run_checked(|checkout| {
            crate::process::run_in(
                checkout,
                "cargo",
                &args,
                &[("CARGO_TARGET_DIR", target)],
                Duration::from_secs(60),
            )?;
            Ok(())
        })
        .unwrap_err();
    let crate::process::ProcessError::Exit { log, .. } = error
        .downcast_ref::<crate::process::ProcessError>()
        .expect("actual Clippy failure")
    else {
        panic!("not a compiler exit")
    };
    assert!(
        fs::read_to_string(log)
            .unwrap()
            .contains("mismatched types")
    );
    assert_eq!(index_bytes(root), before);
    assert_eq!(
        fs::read_to_string(root.join("src/lib.rs")).unwrap(),
        "pub fn sample() -> u32 { 1 }\n"
    );
}
