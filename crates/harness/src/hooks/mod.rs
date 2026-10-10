//! Canonical Git hook dispatchers: verify bytes, not shell substrings.

use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const HOOKS: [(&str, &[u8]); 2] = [
    ("pre-commit", b"#!/bin/sh\nexec make pre-commit\n"),
    ("pre-push", b"#!/bin/sh\nexec make pre-push\n"),
];

/// Earlier canonical dispatchers that `install` may replace in place.
const SUPERSEDED: [(&str, &[u8]); 1] = [("pre-push", b"#!/bin/sh\nexec make preflight\n")];

/// Install the same mandatory Make dispatchers used by the local gates.
pub fn install(root: &Path) -> Result<()> {
    let mut planned = Vec::new();
    for (name, expected) in HOOKS {
        let path = hook_path(root, name)?;
        let parent = path
            .parent()
            .ok_or_else(|| format!("{name} hook parent is missing"))?;
        fs::create_dir_all(parent)?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(format!(
                "Git hooks directory must be a real directory: {}",
                parent.display()
            )
            .into());
        }
        let exists = match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(
                    format!("{name} hook must be a regular file: {}", path.display()).into(),
                );
            }
            Ok(_) => {
                let current = fs::read(&path)?;
                if current == expected {
                    true
                } else if SUPERSEDED.contains(&(name, current.as_slice())) {
                    // Our own earlier dispatcher: replace it atomically below.
                    false
                } else {
                    return Err(format!(
                        "existing {name} hook differs; refusing to overwrite: {}",
                        path.display()
                    )
                    .into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        planned.push((expected, path, exists));
    }
    for (expected, path, exists) in planned {
        if !exists {
            // Written beside the hook and renamed over a superseded dispatcher.
            let temporary = path.with_extension("aoe-new");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(expected)?;
            drop(file);
            fs::rename(&temporary, &path)?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    check(root)
}

/// Reject added, commented, unreachable, or otherwise altered dispatchers.
pub fn check(root: &Path) -> Result<()> {
    for (name, expected) in HOOKS {
        let path = hook_path(root, name)?;
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(format!("{name} hook must be a regular file: {}", path.display()).into());
        }
        if fs::read(&path)? != expected {
            return Err(format!("{name} hook differs from expected dispatcher").into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o7777 != 0o755 {
                return Err(format!("{name} hook must have permissions 0755").into());
            }
        }
    }
    Ok(())
}

pub(crate) fn common_directory(root: &Path) -> Result<PathBuf> {
    rev_parse_path(
        root,
        "common repository",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
}

fn hook_path(root: &Path, name: &str) -> Result<PathBuf> {
    reject_git_config_overrides(name)?;
    // Do not honor an arbitrary `core.hooksPath`, which may point at shared user
    // hooks outside this repository. The only managed location is Git's common
    // repository-local hooks directory (also correct for linked worktrees).
    let common = common_directory(root)?;
    let configured = rev_parse_path(
        root,
        name,
        &["rev-parse", "--path-format=absolute", "--git-path", "hooks"],
    )?;
    let expected = common.join("hooks");
    if configured != expected {
        return Err(format!(
            "refusing noncanonical Git hooks path: {} (expected {})",
            configured.display(),
            expected.display()
        )
        .into());
    }
    Ok(expected.join(name))
}

fn reject_git_config_overrides(hook: &str) -> Result<()> {
    if let Some((name, _)) = env::vars_os().find(|(name, _)| {
        let name = name.to_string_lossy();
        name == "GIT_CONFIG" || name.starts_with("GIT_CONFIG_")
    }) {
        return Err(format!(
            "refusing to manage {hook} hook while Git config override {name:?} is set"
        )
        .into());
    }
    Ok(())
}

fn rev_parse_path(root: &Path, hook: &str, args: &[&str]) -> Result<PathBuf> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_NAMESPACE")
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "cannot locate Git hook {hook}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )
        .into());
    }
    // Git appends one newline. Preserve whitespace that belongs to the path.
    let output = String::from_utf8(result.stdout)?;
    let path = PathBuf::from(output.strip_suffix('\n').unwrap_or(&output));
    if !path.is_absolute() {
        return Err(format!("Git returned a non-absolute hook path: {}", path.display()).into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests;
