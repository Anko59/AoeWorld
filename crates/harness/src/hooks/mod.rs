//! Canonical Git hook dispatchers: verify bytes, not shell substrings.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const HOOKS: [(&str, &[u8]); 2] = [
    ("pre-commit", b"#!/bin/sh\nexec make pre-commit\n"),
    ("pre-push", b"#!/bin/sh\nexec make preflight\n"),
];

/// Install the same mandatory Make dispatchers used by the local gates.
pub fn install(root: &Path) -> Result<()> {
    for (name, expected) in HOOKS {
        let path = hook_path(root, name)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Do not follow an existing symlink and overwrite a different file.
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.file_type().is_file() => {
                return Err(
                    format!("{name} hook must be a regular file: {}", path.display()).into(),
                );
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::write(&path, expected)?;
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

fn hook_path(root: &Path, name: &str) -> Result<PathBuf> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        // Git canonicalizes absolute paths, including a hook's final symlink.
        // Resolve only its directory so metadata checks see the hook itself.
        .args(["rev-parse", "--path-format=absolute", "--git-path", "hooks"])
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "cannot locate Git hook {name}: {}",
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
    Ok(path.join(name))
}

#[cfg(test)]
mod tests;
