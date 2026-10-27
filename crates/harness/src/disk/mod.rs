//! Disk admission, and disposable output sizing/cleanup without following symlinks.
use std::{fs, io, path::Path};
pub(crate) mod precheck;
pub(crate) mod stale;

#[cfg(test)]
mod tests;

/// File bytes, excluding symlink destinations. Missing output occupies no bytes.
pub(crate) fn bytes(path: &Path) -> io::Result<u64> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }
    fs::read_dir(path)?.try_fold(0u64, |total, entry| {
        Ok(total.saturating_add(bytes(&entry?.path())?))
    })
}

/// Only remove a directory reached through regular directories beneath `root`.
pub(crate) fn remove(root: &Path, relative: &Path) -> io::Result<u64> {
    let mut path = root.to_owned();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(io::Error::other(
                "cleanup needs a relative output directory",
            ));
        }
        path.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => (),
            Ok(_) => return Ok(0),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(error),
        }
    }
    let size = bytes(&path)?;
    fs::remove_dir_all(path)?;
    Ok(size)
}
