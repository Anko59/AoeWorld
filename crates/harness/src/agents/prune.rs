//! Post-build cleanup, called in the tool image while harness.sh holds build.lock.
use crate::disk;
use std::{fs, io, path::Path, process::Command, time::SystemTime};

const BINARY: &str = "aoe-harness";
const TARGET_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

fn judge_name(name: &str) -> bool {
    let hash = name.strip_prefix("bootstrap-").unwrap_or(name);
    hash.len() == 40 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Keep the freshly installed judge and the newest other completed judge.
/// An invalid home/keep, a symlink or an unrelated entry is never removed.
pub(crate) fn built(home: &Path, keep: &str) -> io::Result<usize> {
    if home.file_name().is_none_or(|n| n != "aoe-agent-hook")
        || !judge_name(keep)
        || !home.join(keep).join(BINARY).is_file()
        || !fs::symlink_metadata(home).is_ok_and(|m| m.is_dir())
    {
        return Ok(0);
    }
    let mut others: Vec<(SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in fs::read_dir(home)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str().filter(|n| judge_name(n) && *n != keep) else {
            continue;
        };
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = home.join(name);
        if let Ok(metadata) = fs::metadata(dir.join(BINARY)) {
            others.push((metadata.modified()?, dir));
        }
    }
    others.sort_by(|a, b| b.cmp(a));
    let mut removed = 0;
    for (_, dir) in others.into_iter().skip(1) {
        fs::remove_dir_all(dir)?;
        removed += 1;
    }
    Ok(removed)
}

pub(crate) fn clean_target_with(
    target: &Path,
    limit: u64,
    clean: &mut impl FnMut() -> io::Result<()>,
) -> io::Result<()> {
    if !fs::symlink_metadata(target).is_ok_and(|m| m.is_dir()) {
        return Ok(());
    }
    for entry in fs::read_dir(target)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            disk::remove(target, &Path::new(&entry.file_name()).join("incremental"))?;
        }
    }
    if disk::bytes(target)? > limit {
        clean()?;
        if disk::bytes(target)? > limit {
            return Err(io::Error::other(
                "hook target still exceeds its bound after cargo clean",
            ));
        }
    }
    Ok(())
}

/// Dispatched after install, inside the launcher's flock and pinned tool image.
pub(crate) fn execute(home: &Path, keep: &str) -> io::Result<()> {
    if home.file_name().is_none_or(|n| n != "aoe-agent-hook")
        || !judge_name(keep)
        || !home.join(keep).join(BINARY).is_file()
        || !fs::symlink_metadata(home).is_ok_and(|m| m.is_dir())
    {
        return Err(io::Error::other("invalid hook build output directory"));
    }
    built(home, keep)?;
    let target = home.join("target");
    let before = disk::bytes(&target)?;
    clean_target_with(&target, TARGET_LIMIT, &mut || {
        let status = Command::new("cargo")
            .args(["clean", "--target-dir"])
            .arg(&target)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("hook cargo clean failed"))
        }
    })?;
    let after = disk::bytes(&target)?;
    eprintln!(
        "hook: target {before} -> {after} bytes; freed {} bytes (limit {TARGET_LIMIT})",
        before.saturating_sub(after)
    );
    Ok(())
}
