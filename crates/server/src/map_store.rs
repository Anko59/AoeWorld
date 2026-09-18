use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, ElevationPage, MapPackage, ordered_page_root};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_PACKAGES: usize = 256;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024;
const MAX_PAGE_BYTES: u64 = 128 * 1024;

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
        verify_environment(directory, &package)?;
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
    if package.environment.samples_per_axis != 0 {
        return persist_prepared(directory, package, &[]);
    }
    persist_manifest(directory, package)
}

fn persist_manifest(directory: Option<&Path>, package: &MapPackage) -> Result<(), MapStoreError> {
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

pub(crate) fn persist_prepared(
    directory: Option<&Path>,
    package: &MapPackage,
    pages: &[ElevationPage],
) -> Result<(), MapStoreError> {
    let Some(directory) = directory else {
        return Ok(());
    };
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: error.to_string(),
        })?;
    verify_pages(package, pages).map_err(|reason| MapStoreError::InvalidPackage {
        path: directory.to_owned(),
        reason,
    })?;
    let root = page_root(directory, package);
    fs::create_dir_all(&root)?;
    for page in pages {
        let path = root.join(format!("{}-{}-{}.json", page.level, page.x, page.y));
        write_json(&path, page, MAX_PAGE_BYTES)?;
    }
    persist_manifest(Some(directory), package)
}

fn package_path(directory: &Path, hash: &str) -> PathBuf {
    directory.join(format!("{hash}.json"))
}

fn page_root(directory: &Path, package: &MapPackage) -> PathBuf {
    directory
        .join("pages")
        .join(package.content_hash_hex())
        .join("elevation")
}

fn verify_environment(directory: &Path, package: &MapPackage) -> Result<(), MapStoreError> {
    load_elevation_pages(Some(directory), package).map(|_| ())
}

pub(super) fn load_elevation_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<ElevationPage>, MapStoreError> {
    if package.environment.samples_per_axis == 0 {
        return Ok(Vec::new());
    }
    let directory = directory.ok_or_else(|| MapStoreError::InvalidPackage {
        path: PathBuf::from("prepared-environment"),
        reason: "prepared package requires a page directory".to_owned(),
    })?;
    let mut pages = Vec::new();
    for (level, metadata) in package.environment.elevation.levels.iter().enumerate() {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        for y in 0..count {
            for x in 0..count {
                let path = page_root(directory, package).join(format!("{level}-{x}-{y}.json"));
                pages.push(read_page(&path)?);
            }
        }
        let level_pages = pages
            .iter()
            .filter(|page| usize::from(page.level) == level)
            .cloned()
            .collect::<Vec<_>>();
        if ordered_page_root(&level_pages).ok() != Some(metadata.ordered_page_root) {
            return Err(MapStoreError::InvalidPackage {
                path: directory.to_owned(),
                reason: "elevation pages do not reproduce the indexed root".to_owned(),
            });
        }
    }
    Ok(pages)
}

fn verify_pages(package: &MapPackage, pages: &[ElevationPage]) -> Result<(), String> {
    let levels = &package.environment.elevation.levels;
    if levels.is_empty() || pages.iter().any(|page| page.validate().is_err()) {
        return Err("prepared package has invalid elevation pages".to_owned());
    }
    for (level, metadata) in levels.iter().enumerate() {
        let level_pages = pages
            .iter()
            .filter(|page| usize::from(page.level) == level)
            .cloned()
            .collect::<Vec<_>>();
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_page_root(&level_pages).ok() != Some(metadata.ordered_page_root)
        {
            return Err("prepared package page index is incomplete".to_owned());
        }
    }
    Ok(())
}

fn read_page(path: &Path) -> Result<ElevationPage, MapStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_PAGE_BYTES {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "elevation page is not a bounded regular file".to_owned(),
        });
    }
    let page: ElevationPage = serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    })?;
    page.validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(page)
}

fn write_json(path: &Path, value: &impl serde::Serialize, limit: u64) -> Result<(), MapStoreError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > limit {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "serialized page exceeds storage limit".to_owned(),
        });
    }
    fs::write(path, bytes)?;
    Ok(())
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
    use aoe_map::{
        EnvironmentalProvenance, FieldPyramid, MapRequest, PreparedEnvironment, ProjectionMetadata,
        PyramidLevel,
    };

    fn prepared() -> (MapPackage, Vec<ElevationPage>) {
        let first = ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![1, 2, 3, 4],
        };
        let second = ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![3],
        };
        let environment = PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![
                    PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&first))
                            .expect("root"),
                    },
                    PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&second))
                            .expect("root"),
                    },
                ],
            },
        };
        let package = MapPackage::with_prepared_environment(
            1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            environment,
        )
        .expect("package");
        (package, vec![first, second])
    }

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

    #[test]
    fn prepared_pages_must_persist_with_their_package() {
        let directory = tempfile::tempdir().expect("package directory");
        let (package, pages) = prepared();
        persist_prepared(Some(directory.path()), &package, &pages).expect("persist");
        assert_eq!(load(Some(directory.path())).expect("load").len(), 1);
        assert_eq!(
            load_elevation_pages(Some(directory.path()), &package).expect("pages"),
            pages
        );
        let missing = page_root(directory.path(), &package).join("0-0-0.json");
        fs::remove_file(missing).expect("remove page");
        assert!(matches!(
            load(Some(directory.path())),
            Err(MapStoreError::Io(_))
        ));
    }
}
