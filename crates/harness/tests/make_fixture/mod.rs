//! Current Make inputs and the already Docker-built std-only launcher for fake-Docker tests.
use std::{fs, path::Path};

pub fn workspace_named(prefix: &str) -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workspace = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
    let root = workspace.path();
    fs::copy(source.join("Makefile"), root.join("Makefile")).unwrap();
    fs::create_dir(root.join("make")).unwrap();
    for entry in fs::read_dir(source.join("make")).unwrap().flatten() {
        if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "mk")
        {
            fs::copy(entry.path(), root.join("make").join(entry.file_name())).unwrap();
        }
    }
    for file in [
        "gates/container.args",
        "Cargo.toml",
        "Cargo.lock",
        "crates/harness/src/resources/launcher.rs",
        "crates/harness/src/resources/cpu.rs",
        "crates/harness/src/resources/observer.rs",
        "crates/harness/src/resources/policy.rs",
        "crates/harness/src/resources/long.rs",
        "crates/harness/src/resources/lock.rs",
    ] {
        let destination = root.join(file);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source.join(file), destination).unwrap();
    }
    fs::create_dir(root.join(".cache")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_aoe-resource-runner"),
        root.join(".cache/resource-runner"),
    )
    .expect("Cargo builds the current resource launcher for integration fixtures");
    assert!(
        std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
    workspace
}
