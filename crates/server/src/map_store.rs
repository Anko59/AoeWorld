use aoe_map::MapPackage;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_PACKAGES: usize = 256;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Error)]
pub enum MapStoreError {
    #[error("map package storage error: {0}")]
    Io(#[from] io::Error),
    #[error("map package JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("map package storage contains too many packages")]
    TooManyPackages,
    #[error("invalid stored map package {path}: {reason}")]
    InvalidPackage { path: PathBuf, reason: String },
}

pub(crate) fn load(
    directory: Option<&Path>,
) -> Result<BTreeMap<String, MapPackage>, MapStoreError> {
    let Some(directory) = directory else {
        return Ok(BTreeMap::new());
    };
    if !directory.exists() {
        return Ok(BTreeMap::new());
    }
    if !directory.is_dir() {
        return Err(MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: "storage path is not a directory".to_owned(),
        });
    }
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_PACKAGES {
        return Err(MapStoreError::TooManyPackages);
    }
    let mut packages = BTreeMap::new();
    for path in paths {
        let package = read_package(&path)?;
        let hash = package.content_hash_hex();
        let expected = package_path(directory, &hash);
        if path != expected || packages.insert(hash, package).is_some() {
            return Err(MapStoreError::InvalidPackage {
                path,
                reason: "package file name or identity is not canonical".to_owned(),
            });
        }
    }
    Ok(packages)
}

pub(crate) fn persist(directory: Option<&Path>, package: &MapPackage) -> Result<(), MapStoreError> {
    let Some(directory) = directory else {
        return Ok(());
    };
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: error.to_string(),
        })?;
    fs::create_dir_all(directory)?;
    let path = package_path(directory, &package.content_hash_hex());
    if path.exists() {
        return (read_package(&path)? == *package).then_some(()).ok_or(
            MapStoreError::InvalidPackage {
                path,
                reason: "existing package differs from its canonical identity".to_owned(),
            },
        );
    }
    let bytes = serde_json::to_vec(package)?;
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(MapStoreError::InvalidPackage {
            path,
            reason: "serialized package exceeds storage limit".to_owned(),
        });
    }
    let temporary = directory.join(format!(
        ".{}.{}.tmp",
        package.content_hash_hex(),
        std::process::id()
    ));
    fs::write(&temporary, bytes)?;
    match fs::hard_link(&temporary, &path) {
        Ok(()) => fs::remove_file(temporary)?,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::remove_file(temporary)?;
            if read_package(&path)? != *package {
                return Err(MapStoreError::InvalidPackage {
                    path,
                    reason: "existing package differs from its canonical identity".to_owned(),
                });
            }
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
    }
    Ok(())
}

fn package_path(directory: &Path, hash: &str) -> PathBuf {
    directory.join(format!("{hash}.json"))
}

fn read_package(path: &Path) -> Result<MapPackage, MapStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_PACKAGE_BYTES
    {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "package is not a bounded regular file".to_owned(),
        });
    }
    let package: MapPackage = serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    })?;
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_map::MapRequest;

    #[test]
    fn persisted_packages_reload_with_their_canonical_identity() {
        let directory = tempfile::tempdir().expect("package directory");
        let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
        persist(Some(directory.path()), &package).expect("persist");
        let loaded = load(Some(directory.path())).expect("load");
        assert_eq!(loaded.get(&package.content_hash_hex()), Some(&package));
    }

    #[test]
    fn noncanonical_file_names_are_rejected_on_load() {
        let directory = tempfile::tempdir().expect("package directory");
        let package = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
        let path = directory.path().join("wrong.json");
        fs::write(path, serde_json::to_vec(&package).expect("JSON")).expect("fixture");
        assert!(matches!(
            load(Some(directory.path())),
            Err(MapStoreError::InvalidPackage { .. })
        ));
    }
}
