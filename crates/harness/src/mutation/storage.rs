//! Retained local allocation, not protected worker storage or cache attestation.
use super::{Result, io};
use crate::gates::scopes::{self, Snapshot};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Eq, PartialEq)]
struct DirectoryIdentity {
    dev: u64,
    ino: u64,
    mode: u32,
    uid: u32,
    gid: u32,
}
#[cfg(unix)]
fn identity(path: &Path) -> Result<DirectoryIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.mode() & 0o077 != 0 {
        return Err("mutation temporary root is not a private normal directory".into());
    }
    Ok(DirectoryIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
        mode: metadata.mode(),
        uid: metadata.uid(),
        gid: metadata.gid(),
    })
}
#[cfg(not(unix))]
fn identity(_: &Path) -> Result<DirectoryIdentity> {
    Err("mutation storage requires Unix identities".into())
}

// Resolve existing ancestors without following a symlink; retain a normal missing suffix.
fn projection(path: &Path) -> Result<PathBuf> {
    let path = io::absolute(path)?;
    let mut resolved = PathBuf::new();
    for component in path.components() {
        resolved.push(component.as_os_str());
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("mutation storage ancestor is not a normal directory".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(path)
}
fn disjoint(left: &Path, right: &Path) -> bool {
    !left.starts_with(right) && !right.starts_with(left)
}
#[cfg(unix)]
fn private_directory(prefix: &str) -> Result<TempDir> {
    use std::os::unix::fs::PermissionsExt;
    Ok(tempfile::Builder::new()
        .prefix(prefix)
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?)
}
#[cfg(not(unix))]
fn private_directory(_: &str) -> Result<TempDir> {
    Err("mutation storage requires Unix private directories".into())
}
pub(super) struct Storage {
    raw: TempDir,
    target: TempDir,
    raw_identity: DirectoryIdentity,
    target_identity: DirectoryIdentity,
    prohibited: Vec<PathBuf>,
}
impl Storage {
    pub(super) fn new(source: &Path, snapshot: &Snapshot) -> Result<Self> {
        let source = projection(source)?;
        let mut prohibited = vec![
            source.clone(),
            projection(snapshot.root())?,
            projection(&source.join(".cache"))?,
            projection(&source.join("target"))?,
        ];
        prohibited.extend(scopes::git_directories(&source)?);
        if let Some(home) = std::env::var_os("CARGO_HOME") {
            prohibited.push(projection(Path::new(&home))?);
        } else if let Some(home) = std::env::var_os("HOME") {
            prohibited.push(projection(&PathBuf::from(home).join(".cargo"))?);
        }
        if let Some(target) = std::env::var_os("CARGO_TARGET_DIR") {
            prohibited.push(projection(Path::new(&target))?);
        }
        let raw = private_directory("aoe-mutation-raw-")?;
        let target = private_directory("aoe-mutation-target-")?;
        let storage = Self {
            raw_identity: identity(raw.path())?,
            target_identity: identity(target.path())?,
            raw,
            target,
            prohibited,
        };
        storage.verify()?;
        Ok(storage)
    }
    pub(super) fn raw(&self) -> &Path {
        self.raw.path()
    }
    pub(super) fn target(&self) -> &Path {
        self.target.path()
    }
    pub(super) fn verify(&self) -> Result<()> {
        for path in &self.prohibited {
            projection(path)?;
        }
        let raw = projection(self.raw.path())?;
        let target = projection(self.target.path())?;
        if !disjoint(&raw, &target)
            || self
                .prohibited
                .iter()
                .any(|path| !disjoint(&raw, path) || !disjoint(&target, path))
            || identity(&raw)? != self.raw_identity
            || identity(&target)? != self.target_identity
        {
            return Err(
                "mutation temporary roots alias protected inputs or changed identity".into(),
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
