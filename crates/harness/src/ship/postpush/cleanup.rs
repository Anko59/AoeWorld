//! Reclaim rebuildable worktree output after publication and auto-merge arming.
use crate::{disk, ship::Result};
use std::path::Path;

pub(in crate::ship) fn after_publication(
    root: &Path,
    base: &str,
    publish: impl FnOnce() -> Result<()>,
) -> Result<()> {
    publish()?;
    if base != crate::review::base::DEV {
        return Ok(());
    }
    // Only incremental output: `.cache/harness/snapshots` may hold the snapshot of a check
    // another run of this checkout is still using; each run removes its own snapshots.
    let relative = "target/debug/incremental";
    let freed = match disk::remove(root, Path::new(relative)) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("ship: could not clean {relative}: {error}");
            0
        }
    };
    eprintln!("ship: freed {freed} bytes of rebuildable output");
    let free = disk::precheck::free_bytes(root).ok();
    if let Some(line) = free.and_then(disk::stale::hint) {
        eprintln!("{line}");
    }
    Ok(())
}
