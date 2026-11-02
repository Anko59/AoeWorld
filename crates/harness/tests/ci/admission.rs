//! Private live long-job admission for CLI fixtures; short commands need no lock.
use std::{fs, fs::File, process::Command};

pub(crate) struct FixtureLong {
    _lock: File,
    _directory: tempfile::TempDir,
}

pub(crate) fn inherit_or_fixture(command: &mut Command) -> Option<FixtureLong> {
    if ["AOE_LONG_JOB", "AOE_LONG_DIR"]
        .iter()
        .all(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        None
    } else {
        Some(fixture(command))
    }
}

pub(crate) fn fixture(command: &mut Command) -> FixtureLong {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("long.lock");
    let file = File::create(&path).unwrap();
    file.try_lock().unwrap();
    fs::write(
        &path,
        "fixture-nonce\nCLI test on fixture in private test directory (started 0)",
    )
    .unwrap();
    command
        .env("AOE_LONG_JOB", "fixture-nonce")
        .env("AOE_LONG_DIR", directory.path());
    FixtureLong {
        _lock: file,
        _directory: directory,
    }
}
