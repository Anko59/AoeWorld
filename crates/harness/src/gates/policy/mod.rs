//! Protected-policy preparation. Materialization is NOT execution authority.
mod backend;
mod cli;
pub(crate) use cli::{Options, execute};
mod closure;
mod descriptor;
mod source;
#[cfg(test)]
mod tests;

use crate::gates::registry::{Cadence, Registry};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::{Component, Path, PathBuf},
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
use closure::Materialized;
#[cfg(test)]
use closure::{Closure, Entry};
#[cfg(test)]
use descriptor::Abi;
use descriptor::{Preparation, Status};
pub(crate) use source::{Anchor, ObservedBranch, SourceIdentity};

/// Strict lowercase, full object identifiers. No short IDs, symbolic refs,
/// option-like strings, whitespace, or caller-selected replacement objects.
fn full_oid(value: &str) -> Result<()> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected full lowercase SHA-1/SHA-256 Git object identifier".into());
    }
    Ok(())
}
fn digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("expected lowercase BLAKE3 digest".into());
    }
    Ok(())
}
fn regular(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err("non-root-relative policy path".into());
    }
    let root = plain_absolute(root)?;
    let mut path = root.clone();
    let count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        path.push(component);
        let metadata = fs::symlink_metadata(&path)?;
        if if index + 1 == count {
            !metadata.file_type().is_file()
        } else {
            !metadata.file_type().is_dir()
        } {
            return Err("policy closure contains symlink or unsupported file type".into());
        }
    }
    if !fs::canonicalize(&path)?.starts_with(&root) {
        return Err("policy path escaped materialization".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::metadata(&path)?.nlink() != 1 {
            return Err("policy file has hardlink alias".into());
        }
    }
    Ok(path)
}

fn plain_absolute(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("policy path must be absolute".into());
    }
    let mut observed = PathBuf::new();
    for component in path.components() {
        if matches!(component, Component::ParentDir | Component::CurDir) {
            return Err("non-normal policy path".into());
        }
        observed.push(component);
        if fs::symlink_metadata(&observed)?.file_type().is_symlink() {
            return Err("policy path has symlink ancestor".into());
        }
    }
    Ok(fs::canonicalize(path)?)
}
