use super::*;
use std::collections::BTreeMap;

pub(super) type Seal = BTreeMap<PathBuf, String>;

/// Validate types before any private Git call. In particular, never let a
/// gitdir file/symlink redirect verification into the original repository.
pub(super) fn metadata_seal(checkout: &Path, expected: Option<&Seal>) -> Result<Seal> {
    let metadata = checkout.join(".git");
    if !fs::symlink_metadata(&metadata)?.file_type().is_dir() {
        return Err("private Git metadata must remain its own regular directory".into());
    }
    let mut seal = BTreeMap::new();
    fn visit(root: &Path, path: &Path, seal: &mut Seal, expected: Option<&Seal>) -> Result<()> {
        let relative = path.strip_prefix(root)?.to_owned();
        if expected.is_some_and(|expected| !expected.contains_key(&relative)) {
            return Err("gate added private Git metadata".into());
        }
        let metadata = fs::symlink_metadata(path)?;
        let value = if metadata.file_type().is_dir() {
            "directory".to_owned()
        } else if metadata.file_type().is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err("private Git metadata must not contain hard links".into());
                }
            }
            blake3::hash(&fs::read(path)?).to_hex().to_string()
        } else {
            return Err("private Git metadata must not contain symlinks or special files".into());
        };
        seal.insert(relative, value);
        if metadata.file_type().is_dir() {
            for entry in fs::read_dir(path)? {
                visit(root, &entry?.path(), seal, expected)?;
            }
        }
        Ok(())
    }
    visit(&metadata, &metadata, &mut seal, expected)?;
    if expected.is_some_and(|expected| expected != &seal) {
        return Err("gate changed private Git metadata".into());
    }
    Ok(seal)
}
