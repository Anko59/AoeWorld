use super::*;

#[test]
fn invalid_or_unknown_boot_map_reports_hash_and_directory_without_launching_docker() {
    let checkout = built_checkout();
    let directory = checkout
        .path()
        .join(aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY)
        .canonicalize()
        .unwrap();
    std::fs::create_dir(directory.join(format!("{}.json", "ef".repeat(32))))
        .expect("directory is not a stored package file");
    for (hash, reason) in [
        ("invalid-hash".to_owned(), "invalid content hash"),
        (String::new(), "invalid content hash"),
        ("bad\nhash".to_owned(), "invalid content hash"),
        ("../escape".to_owned(), "invalid content hash"),
        ("AB".repeat(32), "invalid content hash"),
        ("cd".repeat(32), "unknown stored map package"),
        ("ef".repeat(32), "unknown stored map package"),
    ] {
        // Model an immediately exiting --rm server, whose logs cannot be read.
        let runtime = FakeRuntime {
            dies_on_start: true,
            ..Default::default()
        };
        let error = start_with(
            &runtime,
            checkout.path(),
            "smoke",
            "revision",
            None,
            Some(&hash),
            8080,
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            format!(
                "AOE_OPEN_MAP_PACKAGE {hash:?} in {}: {reason}",
                directory.display()
            )
        );
        assert_eq!(error.lines().count(), 1);
        assert!(runtime.probed.borrow().is_empty());
        assert!(runtime.calls.borrow().iter().all(|args| args[0] != "run"));
    }
}

#[cfg(unix)]
#[test]
fn boot_map_uses_the_canonical_package_directory_mounted_for_the_server() {
    let checkout = built_checkout();
    let requested = checkout
        .path()
        .join(aoe_server::Config::DEFAULT_MAP_PACKAGE_DIRECTORY);
    let external = tempfile::tempdir().expect("external packages");
    std::fs::rename(&requested, external.path().join("packages")).expect("move packages");
    let directory = external.path().join("packages").canonicalize().unwrap();
    std::os::unix::fs::symlink(&directory, &requested).expect("package symlink");
    let runtime = FakeRuntime::default();
    let missing_hash = "cd".repeat(32);
    let error = start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        None,
        Some(&missing_hash),
        8080,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        error,
        format!(
            "AOE_OPEN_MAP_PACKAGE {missing_hash:?} in {}: unknown stored map package",
            directory.display()
        )
    );
    assert!(runtime.calls.borrow().iter().all(|args| args[0] != "run"));
    runtime.healthy.set(true);
    start_with(
        &runtime,
        checkout.path(),
        "smoke",
        "revision",
        None,
        Some(&"ab".repeat(32)),
        8080,
    )
    .expect("stored package in mounted directory");
    assert!(runtime.calls.borrow().iter().any(|args| {
        args[0] == "run"
            && args.contains(&format!(
                "{}:{}:rw",
                directory.display(),
                directory.display()
            ))
    }));
}
