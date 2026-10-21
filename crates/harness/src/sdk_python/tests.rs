use super::*;

#[test]
fn the_newest_aoeworld_wheel_is_chosen_and_other_files_are_ignored() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        newest_wheel(directory.path()).is_err(),
        "no wheel is an error"
    );
    let old = directory
        .path()
        .join("aoeworld-0.0.9-cp39-abi3-linux_x86_64.whl");
    std::fs::write(&old, b"old").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let new = directory
        .path()
        .join("aoeworld-0.1.0-cp39-abi3-linux_x86_64.whl");
    std::fs::write(&new, b"new").unwrap();
    std::fs::write(directory.path().join("other-1.0.whl"), b"x").unwrap();
    std::fs::write(directory.path().join("aoeworld-0.2.0.tar.gz"), b"x").unwrap();
    assert_eq!(newest_wheel(directory.path()).unwrap(), new);
}

#[test]
fn bindings_paths_match_the_repository_layout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut paths = vec![MANIFEST, AI_SOURCE];
    for (tests, project) in TESTS {
        paths.extend([tests, project]);
    }
    for path in paths {
        assert!(root.join(path).exists(), "{path} is missing");
    }
}
