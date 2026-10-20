//! Refuse local work before slot admission; diagnostics inventory the host's worktrees.
use crate::{hooks::common_directory, ship::git};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) const GIB: u64 = 1024 * 1024 * 1024;

/// Direct commands and Make's slot wrapper share the same admission policy.
pub(crate) fn before_command(command: &crate::Command, gate: &str, ci: bool) -> Result<()> {
    let gate = match command {
        crate::Command::Slot { gate, .. } => gate.as_str(),
        _ => gate,
    };
    if matches!(
        gate,
        "disk-precheck" | "ship" | "test-unit" | "test-wasm" | "test-e2e" | "preflight"
    ) {
        let floor = std::env::var("AOE_DISK_FLOOR_GIB").ok();
        // Disabled and CI runs need neither Git nor filesystem access.
        if floor_bytes(floor.as_deref(), ci)? == 0 {
            return Ok(());
        }
        let root = PathBuf::from(git::git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
        // Gate containers see the actual cache bound at root/target. Host entry points
        // instead need its host path, which need not exist inside those containers.
        let target = matches!(gate, "disk-precheck" | "ship")
            .then(|| std::env::var_os("HARNESS_TARGET_CACHE").map(PathBuf::from))
            .flatten();
        check_at(
            &root,
            floor.as_deref(),
            ci,
            [
                target,
                std::env::var_os("HARNESS_CARGO_CACHE").map(PathBuf::from),
            ],
            free_bytes,
        )?;
    }
    Ok(())
}

fn floor_bytes(value: Option<&str>, ci: bool) -> Result<u64> {
    if ci {
        return Ok(0);
    }
    value
        .filter(|value| !value.is_empty())
        .unwrap_or("15")
        .parse::<u64>()
        .ok()
        .and_then(|gib| gib.checked_mul(GIB))
        .ok_or_else(|| "disk pre-check: AOE_DISK_FLOOR_GIB must be a whole GiB count (0 disables it), within the byte-count range".into())
}

pub(super) fn check_at(
    root: &Path,
    floor: Option<&str>,
    ci: bool,
    overrides: [Option<PathBuf>; 2],
    mut measure: impl FnMut(&Path) -> Result<u64>,
) -> Result<()> {
    let floor = floor_bytes(floor, ci)?;
    if floor == 0 {
        return Ok(());
    }
    let [target, cache] = overrides;
    let mut paths = vec![
        target.unwrap_or_else(|| root.join("target")),
        common_directory(root)?.join("aoe-cargo"),
    ];
    if let Some(cache) = cache.filter(|cache| !paths.contains(cache)) {
        paths.push(cache);
    }
    let samples = paths
        .iter()
        .map(|path| {
            measure(path).map(|free| (path, free)).map_err(|error| {
                format!("disk pre-check: cannot measure {}: {error}", path.display())
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if samples.iter().all(|(_, free)| *free >= floor) {
        return Ok(());
    }
    let mut message = format!(
        "disk pre-check: refusing to start; floor {} GiB ({floor} bytes)\n",
        floor / GIB
    );
    for (path, free) in samples {
        message.push_str(&format!(
            "  {:.2} GiB free ({free} bytes): {}\n",
            free as f64 / GIB as f64,
            path.display()
        ));
    }
    message.push_str("Five largest worktree target/ directories (file bytes, largest first):\n");
    match largest_targets(root) {
        Ok(targets) if targets.is_empty() => message.push_str("  (none)\n"),
        Ok(targets) => {
            for (bytes, path) in targets {
                message.push_str(&format!(
                    "  {bytes} bytes ({:.2} GiB): {}\n",
                    bytes as f64 / GIB as f64,
                    path.display()
                ));
            }
        }
        Err(error) => message.push_str(&format!("  cannot inventory targets: {error}\n")),
    }
    message.push_str("Free disposable output after checking no run uses it, then retry. AOE_DISK_FLOOR_GIB=0 disables this check.");
    Err(message.into())
}

/// Available to this user, including when output has not been created yet.
pub(super) fn free_bytes(path: &Path) -> Result<u64> {
    let absolute = std::path::absolute(path)?;
    let mut existing = absolute.as_path();
    loop {
        match nix::sys::statvfs::statvfs(existing) {
            Ok(stat) => return Ok(stat.blocks_available().saturating_mul(stat.fragment_size())),
            Err(nix::errno::Errno::ENOENT) => {
                existing = existing.parent().ok_or("no existing filesystem ancestor")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn largest_targets(root: &Path) -> Result<Vec<(u64, PathBuf)>> {
    let worktrees = git::git_raw(root, &["worktree", "list", "--porcelain", "-z"])?;
    let mut targets = worktrees
        .split('\0')
        .filter_map(|field| field.strip_prefix("worktree "))
        .map(|path| PathBuf::from(path).join("target"))
        .filter(|path| path.is_dir())
        .map(|path| super::bytes(&path).map(|bytes| (bytes, path)))
        .collect::<std::io::Result<Vec<_>>>()?;
    targets.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    targets.truncate(5);
    Ok(targets)
}
