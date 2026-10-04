use super::*;
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};
pub(super) fn file(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let path = checked(path)?;
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum as u64 {
        return Err("input is not a bounded regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err("input has hardlink alias".into());
        }
    }
    let mut bytes = Vec::new();
    fs::File::open(&path)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err("input grew beyond byte limit".into());
    }
    Ok(bytes)
}
pub(super) fn checked(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("input path must be absolute".into());
    }
    let mut actual = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err("non-normal input path".into());
        }
        actual.push(part);
        if fs::symlink_metadata(&actual)?.file_type().is_symlink() {
            return Err("input contains symlink ancestor".into());
        }
    }
    Ok(fs::canonicalize(path)?)
}
