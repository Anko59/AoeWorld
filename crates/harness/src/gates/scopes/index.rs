use super::*;

/// Git's commit hook may use index.lock or next-index-PID.lock, not index.
/// Only the captured source context gets this index; private Git never does.
pub(super) fn source_index(root: &Path, requested: Option<&Path>) -> Result<PathBuf> {
    let directory = fs::canonicalize(line(git(
        root,
        &["rev-parse", "--absolute-git-dir"],
        None,
    )?)?)?;
    let index = match requested {
        Some(path) if path.is_absolute() => path.to_owned(),
        Some(path) => root.join(path),
        None => PathBuf::from(line(git(
            root,
            &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            None,
        )?)?),
    };
    if !fs::symlink_metadata(&index)?.file_type().is_file() {
        return Err("effective source index must be a regular file, never a symlink".into());
    }
    let parent = index.parent().ok_or("index has no parent")?;
    if fs::canonicalize(parent)? != directory
        || fs::symlink_metadata(parent)?.file_type().is_symlink()
    {
        return Err(
            "effective source index must belong to this checkout's Git metadata directory".into(),
        );
    }
    Ok(fs::canonicalize(index)?)
}

pub(super) fn export_parent(source: &Path) -> Result<PathBuf> {
    let mut path = source.to_owned();
    for name in [".cache", "harness", "snapshots"] {
        path.push(name);
        if !path.exists() {
            fs::create_dir(&path)?;
        }
        if !fs::symlink_metadata(&path)?.file_type().is_dir() {
            return Err("snapshot parent must contain regular directories, never symlinks".into());
        }
    }
    Ok(path)
}
