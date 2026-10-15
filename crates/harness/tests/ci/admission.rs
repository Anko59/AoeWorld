//! Admission fixtures exist only in this integration-test binary.
use nix::fcntl::{Flock, FlockArg};
use std::{fs, fs::File, process::Command};

pub(crate) struct FixtureSlot {
    _lock: Flock<File>,
    _queue: tempfile::TempDir,
}

/// Ordinary CLI tests inherit the slot held by make test-harness. Standalone
/// test runs use a private queue with an admitted fixture ticket instead.
pub(crate) fn inherit_or_fixture(command: &mut Command) -> Option<FixtureSlot> {
    if ["AOE_HEAVY_SLOT", "AOE_HEAVY_DIR"]
        .iter()
        .all(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        None
    } else {
        Some(fixture(command))
    }
}

/// Keep the actual OS lock alive until the command finishes. Production code
/// must validate both the live ticket and its nonce through its normal path.
pub(crate) fn fixture(command: &mut Command) -> FixtureSlot {
    let cache = super::super::root().join(".cache/ppf-ci2/tmp");
    fs::create_dir_all(&cache).unwrap();
    let queue = tempfile::tempdir_in(cache).unwrap();
    let ticket = queue.path().join("admitted");
    let file = File::create(&ticket).unwrap();
    let lock = Flock::lock(file, FlockArg::LockExclusiveNonblock).unwrap();
    fs::write(
        &ticket,
        serde_json::json!({
            "worktree": queue.path(), "branch": "fixture", "command": "admitted test",
            "started": 0, "nonce": "fixture-nonce"
        })
        .to_string(),
    )
    .unwrap();
    command
        .env("AOE_HEAVY_SLOT", "admitted:fixture-nonce")
        .env("AOE_HEAVY_DIR", queue.path())
        .env("AOE_HEAVY_SLOTS", "1")
        .env("AOE_HEAVY_WAIT_MINUTES", "0");
    FixtureSlot {
        _lock: lock,
        _queue: queue,
    }
}
