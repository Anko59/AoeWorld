use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, HistoricalLandUsePage, MapPackage, PotentialBiomePage,
    WaterPage, ordered_page_root,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use thiserror::Error;
const MAX_PACKAGES: usize = 256;
const MAX_PACKAGE_BYTES: u64 = 64 * 1024;
const MAX_PAGE_BYTES: u64 = 128 * 1024;

mod pages;
mod verify;

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
    if !storage_directory_exists(directory)? {
        return Ok(BTreeMap::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
            if paths.len() > MAX_PACKAGES {
                return Err(MapStoreError::TooManyPackages);
            }
        }
    }
    paths.sort();
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
        return persist_prepared(directory, package, &[], &[], &[], &[]);
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
    ensure_storage_directory(directory)?;
    let path = package_path(directory, &package.content_hash_hex());
    if path.exists() {
        return (read_package(&path)?.content_hash == package.content_hash)
            .then_some(())
            .ok_or(MapStoreError::InvalidPackage {
                path,
                reason: "existing package differs from its canonical identity".to_owned(),
            });
    }
    let bytes = serde_json::to_vec(package)?;
    if bytes.len() as u64 > MAX_PACKAGE_BYTES {
        return Err(MapStoreError::InvalidPackage {
            path,
            reason: "serialized package exceeds storage limit".to_owned(),
        });
    }
    publish_immutable(&path, &bytes, || {
        if read_package(&path)?.content_hash != package.content_hash {
            return Err(MapStoreError::InvalidPackage {
                path: path.clone(),
                reason: "existing package differs from its canonical identity".to_owned(),
            });
        }
        Ok(())
    })
}

pub(crate) fn persist_prepared(
    directory: Option<&Path>,
    package: &MapPackage,
    elevation_pages: &[ElevationPage],
    water_pages: &[WaterPage],
    vegetation_pages: &[PotentialBiomePage],
    land_use_pages: &[HistoricalLandUsePage],
) -> Result<(), MapStoreError> {
    let Some(directory) = directory else {
        return Ok(());
    };
    ensure_storage_directory(directory)?;
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: error.to_string(),
        })?;
    verify_pages(
        package,
        elevation_pages,
        water_pages,
        vegetation_pages,
        land_use_pages,
    )
    .map_err(|reason| MapStoreError::InvalidPackage {
        path: directory.to_owned(),
        reason,
    })?;
    let root = elevation_page_root(directory, package);
    ensure_directory_path(&root)?;
    for page in elevation_pages {
        let path = root.join(format!("{}-{}-{}.json", page.level, page.x, page.y));
        write_json(&path, page, MAX_PAGE_BYTES)?;
    }
    if package.environment.water.is_some() {
        pages::persist_water(directory, package, water_pages)?;
    }
    if package.environment.vegetation.is_some() {
        pages::persist_vegetation(directory, package, vegetation_pages)?;
    }
    if package.environment.historical_land_use.is_some() {
        pages::persist_land_use(directory, package, land_use_pages)?;
    }
    verify_environment(directory, package)?;
    persist_manifest(Some(directory), package)
}

fn package_path(directory: &Path, hash: &str) -> PathBuf {
    directory.join(format!("{hash}.json"))
}

pub(super) fn elevation_page_root(directory: &Path, package: &MapPackage) -> PathBuf {
    directory
        .join("pages")
        .join(package.content_hash_hex())
        .join("elevation")
}

#[cfg(test)]
mod tests;

pub(super) fn verify_stored(
    directory: &Path,
    package: &MapPackage,
) -> Result<MapPackage, MapStoreError> {
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: error.to_string(),
        })?;
    let path = package_path(directory, &package.content_hash_hex());
    let stored = read_package(&path)?;
    if stored.content_hash != package.content_hash {
        return Err(MapStoreError::InvalidPackage {
            path,
            reason: "stored manifest differs from the worker result".to_owned(),
        });
    }
    verify_environment(directory, &stored)?;
    // Informational acquisition times are deliberately excluded from identity.
    // Preserve the first published manifest when the same source map is built again.
    Ok(stored)
}

fn verify_environment(directory: &Path, package: &MapPackage) -> Result<(), MapStoreError> {
    verify::environment(directory, package)
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
                let path =
                    elevation_page_root(directory, package).join(format!("{level}-{x}-{y}.json"));
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

pub(super) fn load_water_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<WaterPage>, MapStoreError> {
    pages::load_water(directory, package)
}

pub(super) fn load_vegetation_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<PotentialBiomePage>, MapStoreError> {
    pages::load_vegetation(directory, package)
}

pub(super) fn load_land_use_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<HistoricalLandUsePage>, MapStoreError> {
    pages::load_land_use(directory, package)
}

fn verify_pages(
    package: &MapPackage,
    pages: &[ElevationPage],
    water_pages: &[WaterPage],
    vegetation_pages: &[PotentialBiomePage],
    land_use_pages: &[HistoricalLandUsePage],
) -> Result<(), String> {
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
    pages::verify_water(package, water_pages)?;
    pages::verify_vegetation(package, vegetation_pages)?;
    pages::verify_land_use(package, land_use_pages)?;
    Ok(())
}

fn read_page(path: &Path) -> Result<ElevationPage, MapStoreError> {
    let bytes = read_bounded_file(path, MAX_PAGE_BYTES, "elevation page")?;
    let page: ElevationPage =
        serde_json::from_slice(&bytes).map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    page.validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(page)
}

pub(super) fn write_json(
    path: &Path,
    value: &impl serde::Serialize,
    limit: u64,
) -> Result<(), MapStoreError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() as u64 > limit {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "serialized page exceeds storage limit".to_owned(),
        });
    }
    if path.try_exists()? {
        return same_page_bytes(path, &bytes, limit);
    }
    publish_immutable(path, &bytes, || same_page_bytes(path, &bytes, limit))
}

fn publish_immutable(
    path: &Path,
    bytes: &[u8],
    verify_existing: impl FnOnce() -> Result<(), MapStoreError>,
) -> Result<(), MapStoreError> {
    use std::io::Write;
    static NEXT_TEMPORARY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut attempts = 0;
    let (mut file, temporary) = loop {
        attempts += 1;
        let serial = NEXT_TEMPORARY.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let temporary = path.with_extension(format!("{}-{serial}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => break (file, temporary),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 16 => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        match fs::hard_link(&temporary, path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => verify_existing(),
            Err(error) => Err(error.into()),
        }
    })();
    let _ = fs::remove_file(temporary);
    result
}

fn same_page_bytes(path: &Path, expected: &[u8], limit: u64) -> Result<(), MapStoreError> {
    use std::io::Read;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "existing page is not a bounded regular file".to_owned(),
        });
    }
    let mut actual = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut actual)?;
    if actual != expected {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: "immutable page already exists with different bytes".to_owned(),
        });
    }
    Ok(())
}

fn read_package(path: &Path) -> Result<MapPackage, MapStoreError> {
    let bytes = read_bounded_file(path, MAX_PACKAGE_BYTES, "package")?;
    let package: MapPackage =
        serde_json::from_slice(&bytes).map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    package
        .validate()
        .map_err(|error| MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
    Ok(package)
}

fn validate_existing_ancestors(path: &Path) -> Result<(), MapStoreError> {
    for ancestor in path
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(MapStoreError::InvalidPackage {
                    path: ancestor.to_owned(),
                    reason: "storage path contains a symlink or non-directory".to_owned(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn ensure_directory_path(directory: &Path) -> Result<(), MapStoreError> {
    validate_existing_ancestors(directory)?;
    let mut missing = Vec::new();
    let mut current = directory.to_owned();
    while matches!(fs::symlink_metadata(&current), Err(ref error) if error.kind() == io::ErrorKind::NotFound)
    {
        missing.push(current.clone());
        let Some(parent) = current
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        else {
            break;
        };
        current = parent.to_owned();
    }
    for path in missing.into_iter().rev() {
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        validate_existing_ancestors(&path)?;
    }
    storage_directory_exists(directory)?
        .then_some(())
        .ok_or_else(|| MapStoreError::InvalidPackage {
            path: directory.to_owned(),
            reason: "storage path could not be created as a regular directory".to_owned(),
        })
}

fn storage_directory_exists(directory: &Path) -> Result<bool, MapStoreError> {
    validate_existing_ancestors(directory)?;
    match fs::symlink_metadata(directory) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(MapStoreError::InvalidPackage {
                    path: directory.to_owned(),
                    reason: "storage path is not a regular directory".to_owned(),
                });
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn ensure_storage_directory(directory: &Path) -> Result<(), MapStoreError> {
    ensure_directory_path(directory)
}

pub(super) fn read_bounded_file(
    path: &Path,
    limit: u64,
    kind: &str,
) -> Result<Vec<u8>, MapStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > limit {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: format!("{kind} is not a bounded regular file"),
        });
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(MapStoreError::InvalidPackage {
            path: path.to_owned(),
            reason: format!("{kind} grew beyond the configured bound"),
        });
    }
    Ok(bytes)
}
