use crate::ship::{
    postpush::cleanup,
    ship,
    tests::{fixture, offline},
};
use std::{fs, path::Path};

const OUTPUT: [&str; 2] = ["target/debug/incremental", ".cache/harness/snapshots"];

fn seed(root: &Path) {
    for dir in OUTPUT {
        fs::create_dir_all(root.join(dir)).unwrap();
        fs::write(root.join(dir).join("object"), "build").unwrap();
    }
    fs::create_dir_all(root.join("target/debug/deps")).unwrap();
    fs::write(root.join("target/debug/deps/keep"), "dependency").unwrap();
}

fn present(root: &Path, expected: bool) {
    assert_eq!(
        root.join("target/debug/incremental/object").exists(),
        expected,
        "incremental"
    );
    // Snapshots may belong to a concurrent check of this checkout: never removed by ship.
    assert!(root.join(".cache/harness/snapshots/object").is_file());
    assert!(root.join("target/debug/deps/keep").is_file());
}

#[test]
fn disk_hygiene_ship_cleans_only_after_successful_dev_publication() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for base in ["parent", "dev"] {
        seed(root);
        cleanup::after_publication(root, base, || {
            present(root, true);
            Ok(())
        })
        .unwrap();
        present(root, base != "dev");
    }
}

#[test]
fn disk_hygiene_failed_publication_keeps_build_output() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    seed(root);
    let error =
        cleanup::after_publication(root, "dev", || Err("arming failed".into())).unwrap_err();
    assert!(error.to_string().contains("arming failed"));
    present(root, true);
}

#[test]
fn disk_hygiene_ship_gate_failure_and_push_without_pr_keep_build_output() {
    for gate in ["true", "false"] {
        let (_temp, root) = fixture(gate);
        fs::write(root.join(".git/info/exclude"), "target/\n.cache/\n").unwrap();
        seed(&root);
        let result = ship(&root, &offline());
        assert_eq!(result.is_ok(), gate == "true");
        present(&root, true);
    }
}

#[cfg(unix)]
#[test]
fn disk_hygiene_ship_does_not_follow_output_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("work");
    let external = temp.path().join("external");
    fs::create_dir_all(&root).unwrap();
    seed(&external);
    std::os::unix::fs::symlink(external.join("target"), root.join("target")).unwrap();
    std::os::unix::fs::symlink(external.join(".cache"), root.join(".cache")).unwrap();
    cleanup::after_publication(&root, "dev", || Ok(())).unwrap();
    present(&external, true);
}
