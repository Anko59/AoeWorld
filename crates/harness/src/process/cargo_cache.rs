//! Shared downloads; compiler artifacts remain scoped to their checkout.
use super::*;

mod command;

pub(crate) fn home(root: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = match std::env::var_os("HARNESS_CARGO_CACHE") {
        Some(path) => PathBuf::from(path),
        None => crate::hooks::common_directory(root)?.join("aoe-cargo"),
    };
    fs::create_dir_all(&home)?;
    Ok(fs::canonicalize(home)?)
}

fn missing(captured: &Captured) -> bool {
    if !matches!(captured.exit, CaptureExit::Failed(Some(1 | 101 | 102))) || captured.truncated {
        return false;
    }
    let text = String::from_utf8_lossy(&captured.stderr);
    (text.contains("--offline was specified") || text.contains("offline mode"))
        && [
            "failed to download",
            "no matching package named",
            "failed to get",
            "failed to select a version",
        ]
        .iter()
        .any(|reason| text.contains(reason))
}

/// The common capture choke point: callers choose output retention, never recovery.
/// `attempt` is the raw single-command runner; fetches cannot recursively recover.
pub(super) fn capture(
    command: Command,
    deadline: Duration,
    mut attempt: impl FnMut(Command, Duration, bool) -> Captured,
) -> Captured {
    let started = Instant::now();
    let recovery = command::fetch(&command).map(|fetch| (command::copy(&command), fetch));
    let captured = attempt(command, deadline.saturating_sub(started.elapsed()), true);
    if !missing(&captured) {
        return captured;
    }
    let Some((original, fetch)) = recovery else {
        return captured;
    };
    eprintln!(
        "Cargo offline cache miss: {}\nRunning cargo fetch --locked with network, then retrying once.",
        String::from_utf8_lossy(&captured.stderr).trim()
    );
    // Fetch output must not contaminate a retained WASM runner transcript or metadata JSON.
    let mut fetched = attempt(fetch, deadline.saturating_sub(started.elapsed()), false);
    if !matches!(fetched.exit, CaptureExit::Success) {
        let fix = "Cargo cache recovery failed: enable network access and run make cargo-fetch in the failing checkout, then rerun the gate.";
        eprintln!("{fix}");
        if let CaptureExit::Failed(code) = fetched.exit {
            fetched.exit = CaptureExit::CargoFetch(code);
        }
        fetched.duration = started.elapsed();
        return fetched;
    }
    let mut retry = attempt(original, deadline.saturating_sub(started.elapsed()), true);
    retry.duration = started.elapsed();
    retry
}

#[cfg(test)]
#[path = "tests/cargo_cache_command.rs"]
mod tests;
