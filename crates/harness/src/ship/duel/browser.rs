//! Prepare the locked browser dependencies once, shared with baseline exports.
use super::runner;
use crate::{
    process::{Cancellation, CaptureExit, capture_command},
    ship::git,
};
use std::{fs, path::Path, process::Command, time::Duration};

pub(super) fn reset(root: &Path) -> git::Result<()> {
    let marker = root.join(".cache/duel/browser-deps-ready");
    if marker.exists() {
        fs::remove_file(marker).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn dependencies(root: &Path, cache: &Path) -> git::Result<()> {
    let original = cache
        .parent()
        .and_then(Path::parent)
        .ok_or("duel cache has no checkout")?;
    let modules = original.join("browser/node_modules");
    let marker = cache.join("browser-deps-ready");
    if !marker.is_file() || !modules.is_dir() {
        let mut command = Command::new("make");
        command.arg("browser-deps").current_dir(original);
        let captured =
            capture_command(command, Duration::from_secs(3600), &Cancellation::default());
        if !matches!(captured.exit, CaptureExit::Success) {
            return Err(format!(
                "browser dependencies unavailable: {:?}\n{}",
                captured.exit,
                runner::tail(&captured)
            ));
        }
        if !modules.is_dir() {
            return Err("browser dependencies unavailable: node_modules missing".into());
        }
        fs::write(&marker, b"ready\n").map_err(|e| e.to_string())?;
    }
    if root != original && !root.join("browser/node_modules").exists() {
        std::os::unix::fs::symlink(modules, root.join("browser/node_modules"))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
