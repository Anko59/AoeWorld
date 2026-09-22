use super::*;

#[test]
fn live_lease_survives_recovery_and_drop_removes_only_owned_scratch() {
    let cache = tempfile::tempdir().expect("cache");
    let scratch = Scratch::new(cache.path()).expect("scratch");
    let path = scratch.root.clone();
    fs::write(path.join("page"), b"owned").expect("page");
    fs::write(cache.path().join("source"), b"cached").expect("source");
    recover(cache.path()).expect("recover around active lease");
    assert!(path.join("page").is_file());
    drop(scratch);
    assert!(!path.exists());
    assert_eq!(
        fs::read(cache.path().join("source")).expect("source"),
        b"cached"
    );
}

#[test]
fn worker_shared_lease_protects_data_after_server_lease_disappears() {
    let cache = tempfile::tempdir().expect("cache");
    let (root, registry) = directory(cache.path()).expect("registry");
    let path = root.join("job-crashed-parent");
    fs::create_dir(&path).expect("owned directory");
    let parent = open_lock(&path.join("lease")).expect("parent lease");
    parent.lock_shared().expect("parent shared lock");
    let worker = open_lock(&path.join("lease")).expect("worker lease");
    worker.lock_shared().expect("worker shared lock");
    drop(registry);
    drop(parent);
    fs::write(path.join("partial"), b"unfinished").expect("partial");
    recover(cache.path()).expect("worker still owns scratch");
    assert!(path.join("partial").is_file());
    drop(worker);
    recover(cache.path()).expect("dead worker recovered");
    assert!(!path.exists());
}

#[test]
fn recovery_removes_unleased_allocation_and_preserves_unowned_names() {
    let cache = tempfile::tempdir().expect("cache");
    let (root, registry) = directory(cache.path()).expect("registry");
    fs::create_dir(root.join("job-incomplete-allocation")).expect("allocation");
    fs::create_dir(root.join("unrelated")).expect("unrelated");
    drop(registry);
    recover(cache.path()).expect("recovery");
    assert!(!root.join("job-incomplete-allocation").exists());
    assert!(root.join("unrelated").is_dir());
}

#[cfg(unix)]
#[test]
fn recovery_never_follows_job_directory_or_lease_symlinks() {
    use std::os::unix::fs::symlink;
    let cache = tempfile::tempdir().expect("cache");
    let target = tempfile::tempdir().expect("outside");
    fs::write(target.path().join("keep"), b"keep").expect("outside data");
    let (root, registry) = directory(cache.path()).expect("registry");
    symlink(target.path(), root.join("job-link")).expect("directory link");
    drop(registry);
    recover(cache.path()).expect("directory links skipped");
    assert!(target.path().join("keep").exists());
    let bad = root.join("job-bad-lease");
    fs::create_dir(&bad).expect("job");
    symlink(target.path().join("keep"), bad.join("lease")).expect("lease link");
    assert!(recover(cache.path()).is_err());
    assert_eq!(fs::read(target.path().join("keep")).expect("keep"), b"keep");
}

#[cfg(unix)]
#[test]
fn cancelled_preparation_reaps_worker_before_removing_its_staging() {
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{Arc, atomic::AtomicBool},
        thread,
        time::{Duration, Instant},
    };
    let cache = tempfile::tempdir().expect("cache");
    let worker = cache.path().join("worker");
    fs::write(&worker, b"#!/bin/sh\ncat >/dev/null\nsleep 30 &\nwait\n").expect("worker");
    fs::set_permissions(&worker, fs::Permissions::from_mode(0o700)).expect("executable");
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let root = cache.path().to_owned();
    let preparation = thread::spawn(move || {
        super::super::prepare(
            &worker,
            &root,
            &root.join("output"),
            aoe_map::MapRequest::default(),
            crate::map_jobs::PreparationPlan {
                mode: crate::map_jobs::PreparationMode::Detailed,
                samples_per_axis: 128,
                geographic_millimeters_per_sample: None,
                explanation: "fixture",
            },
            &signal,
            super::super::progress::State::default(),
        )
    });
    let started = Instant::now();
    let staging = loop {
        let found = fs::read_dir(cache.path().join("worker-scratch"))
            .ok()
            .and_then(|entries| {
                entries
                    .filter_map(Result::ok)
                    .find(|entry| entry.file_name().to_string_lossy().starts_with(PREFIX))
                    .map(|entry| entry.path())
            });
        if let Some(path) = found {
            break path;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "worker never allocated scratch"
        );
        thread::sleep(Duration::from_millis(5));
    };
    fs::create_dir_all(staging.join("detailed-staging/pages")).expect("staging");
    fs::write(staging.join("detailed-staging/pages/partial"), b"partial").expect("partial");
    cancelled.store(true, Ordering::Release);
    let error = preparation
        .join()
        .expect("worker thread")
        .expect_err("cancelled");
    assert!(error.contains("cancelled"), "{error}");
    assert!(!staging.exists());
}

#[test]
fn completion_and_recovery_can_run_concurrently() {
    let cache = tempfile::tempdir().expect("cache");
    let scratch = Scratch::new(cache.path()).expect("scratch");
    let root = cache.path().to_owned();
    let recovery = std::thread::spawn(move || {
        for _ in 0..32 {
            recover(&root).expect("concurrent recovery");
        }
    });
    drop(scratch);
    recovery.join().expect("recovery thread");
    recover(cache.path()).expect("final recovery");
}

#[cfg(unix)]
#[test]
fn scratch_directories_and_leases_are_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let cache = tempfile::tempdir().expect("cache");
    let root = cache.path().join("worker-scratch");
    fs::create_dir(&root).expect("existing scratch root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).expect("old permissions");
    let scratch = Scratch::new(cache.path()).expect("private scratch");
    for directory in [&root, &scratch.root] {
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    for file in [root.join(".registry"), scratch.root.join("lease")] {
        assert_eq!(
            fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
