use crate::{GeneratedMap, GeodataError};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, HistoricalLandUsePage, MapPackage, PotentialBiomePage,
    WaterPage,
};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub const DIRECTORY_SCHEMA_VERSION: u16 = 1;
pub const MAX_DIRECTORY_MANIFEST_BYTES: u64 = 64 * 1024;
pub const MAX_DIRECTORY_PAGE_BYTES: u64 = 128 * 1024;

mod verify;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum DirectoryLayer {
    Elevation,
    Water,
    Vegetation,
    HistoricalLandUse,
}

impl DirectoryLayer {
    fn name(self) -> &'static str {
        match self {
            Self::Elevation => "elevation",
            Self::Water => "water",
            Self::Vegetation => "vegetation",
            Self::HistoricalLandUse => "historical-land-use",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PageKey {
    layer: DirectoryLayer,
    level: u8,
    x: u16,
    y: u16,
}

enum PageRef<'a> {
    Elevation(&'a ElevationPage),
    Water(&'a WaterPage),
    Vegetation(&'a PotentialBiomePage),
    HistoricalLandUse(&'a HistoricalLandUsePage),
}

impl PageRef<'_> {
    fn layer(&self) -> DirectoryLayer {
        match self {
            Self::Elevation(_) => DirectoryLayer::Elevation,
            Self::Water(_) => DirectoryLayer::Water,
            Self::Vegetation(_) => DirectoryLayer::Vegetation,
            Self::HistoricalLandUse(_) => DirectoryLayer::HistoricalLandUse,
        }
    }

    fn key(&self) -> PageKey {
        let (level, x, y) = match self {
            Self::Elevation(page) => (page.level, page.x, page.y),
            Self::Water(page) => (page.level, page.x, page.y),
            Self::Vegetation(page) => (page.level, page.x, page.y),
            Self::HistoricalLandUse(page) => (page.level, page.x, page.y),
        };
        PageKey {
            layer: self.layer(),
            level,
            x,
            y,
        }
    }

    fn serialized(&self) -> Result<(PageKey, Vec<u8>), GeodataError> {
        let bytes = match self {
            Self::Elevation(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::Water(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::Vegetation(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::HistoricalLandUse(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
        };
        if bytes.is_empty() || bytes.len() as u64 > MAX_DIRECTORY_PAGE_BYTES {
            return Err(invalid("serialized page exceeds the directory page limit"));
        }
        Ok((self.key(), bytes))
    }
}

enum PageValue {
    Elevation(ElevationPage),
    Water(WaterPage),
    Vegetation(PotentialBiomePage),
    HistoricalLandUse(HistoricalLandUsePage),
}

impl PageValue {
    fn key(&self) -> PageKey {
        match self {
            Self::Elevation(page) => PageKey {
                layer: DirectoryLayer::Elevation,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::Water(page) => PageKey {
                layer: DirectoryLayer::Water,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::Vegetation(page) => PageKey {
                layer: DirectoryLayer::Vegetation,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::HistoricalLandUse(page) => PageKey {
                layer: DirectoryLayer::HistoricalLandUse,
                level: page.level,
                x: page.x,
                y: page.y,
            },
        }
    }

    fn content_hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(match self {
            Self::Elevation(page) => page.content_hash()?,
            Self::Water(page) => page.content_hash()?,
            Self::Vegetation(page) => page.content_hash()?,
            Self::HistoricalLandUse(page) => page.content_hash()?,
        })
    }

    fn dimensions(&self) -> (u8, u8) {
        match self {
            Self::Elevation(page) => (page.width, page.height),
            Self::Water(page) => (page.width, page.height),
            Self::Vegetation(page) => (page.width, page.height),
            Self::HistoricalLandUse(page) => (page.width, page.height),
        }
    }
}

impl GeneratedMap {
    /// Publishes one immutable package into a shared map directory.
    pub fn write_directory(&self, directory: &Path) -> Result<(), GeodataError> {
        self.package.validate()?;
        ensure_directory(directory)?;
        let hash = self.package.content_hash_hex();
        let manifest_path = directory.join(format!("{hash}.json"));
        if fs::symlink_metadata(&manifest_path).is_ok() {
            let existing = verify::read_manifest(directory, &hash)?;
            return verify::verify_manifest(directory, &hash, &existing);
        }
        let result = write_directory_contents(self, directory);
        result?;
        verify::verify_manifest(directory, &hash, &self.package)?;
        let bytes = serde_json::to_vec(&self.package).map_err(json_error)?;
        if bytes.len() as u64 > MAX_DIRECTORY_MANIFEST_BYTES {
            return Err(invalid("directory manifest exceeds its byte limit"));
        }
        let temporary = write_temporary(&manifest_path, &bytes)?;
        match fs::hard_link(&temporary, &manifest_path) {
            Ok(()) => fs::remove_file(&temporary).map_err(GeodataError::from),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = fs::remove_file(&temporary);
                let existing = verify::read_manifest(directory, &hash)?;
                verify::verify_manifest(directory, &hash, &existing)
            }
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                Err(error.into())
            }
        }
    }

    /// Loads and validates every page from a published map directory.
    pub fn read_directory(directory: &Path, package_hash: &str) -> Result<Self, GeodataError> {
        let package = verify::read_manifest(directory, package_hash)?;
        verify::verify_manifest(directory, package_hash, &package)?;
        let mut generated = GeneratedMap {
            package: package.clone(),
            elevation_pages: Vec::new(),
            water_pages: Vec::new(),
            vegetation_pages: Vec::new(),
            historical_land_use_pages: Vec::new(),
        };
        for page in verify::read_pages(directory, package_hash, &package)? {
            match page {
                PageValue::Elevation(page) => generated.elevation_pages.push(page),
                PageValue::Water(page) => generated.water_pages.push(page),
                PageValue::Vegetation(page) => generated.vegetation_pages.push(page),
                PageValue::HistoricalLandUse(page) => {
                    generated.historical_land_use_pages.push(page)
                }
            }
        }
        generated.validate()?;
        Ok(generated)
    }

    /// Verifies a published map directory one bounded page at a time.
    pub fn verify_directory(directory: &Path, package_hash: &str) -> Result<(), GeodataError> {
        let package = verify::read_manifest(directory, package_hash)?;
        verify::verify_manifest(directory, package_hash, &package)
    }
}

fn write_directory_contents(map: &GeneratedMap, directory: &Path) -> Result<(), GeodataError> {
    let package_hash = map.package.content_hash_hex();
    let mut written = 0_usize;
    for page in page_refs(map) {
        let (key, bytes) = page.serialized()?;
        let path = directory.join(page_file(&package_hash, key));
        let parent = path
            .parent()
            .ok_or_else(|| invalid("directory page path has no parent"))?;
        ensure_directory(parent)?;
        write_page_immutable(&path, &bytes)?;
        written = written.saturating_add(1);
    }
    if written != package_page_count(&map.package) {
        return Err(invalid("generated pages do not match the package index"));
    }
    Ok(())
}

fn package_page_count(package: &MapPackage) -> usize {
    let mut count = field_page_count(&package.environment.elevation.levels);
    for field in [
        package.environment.water.as_ref(),
        package.environment.vegetation.as_ref(),
        package.environment.historical_land_use.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        count = count.saturating_add(field_page_count(&field.levels));
    }
    count
}

fn field_page_count(levels: &[aoe_map::PyramidLevel]) -> usize {
    levels.iter().fold(0, |count, level| {
        let side = usize::from(
            level
                .samples_per_axis
                .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES)),
        );
        count.saturating_add(side.saturating_mul(side))
    })
}

fn page_refs(map: &GeneratedMap) -> impl Iterator<Item = PageRef<'_>> {
    map.elevation_pages
        .iter()
        .map(PageRef::Elevation)
        .chain(map.water_pages.iter().map(PageRef::Water))
        .chain(map.vegetation_pages.iter().map(PageRef::Vegetation))
        .chain(
            map.historical_land_use_pages
                .iter()
                .map(PageRef::HistoricalLandUse),
        )
}

fn write_page_immutable(path: &Path, bytes: &[u8]) -> Result<(), GeodataError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_DIRECTORY_PAGE_BYTES
                || read_bounded_file(path, MAX_DIRECTORY_PAGE_BYTES)? != bytes
            {
                return Err(invalid("existing page differs from its canonical identity"));
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let temporary = write_temporary(path, bytes)?;
            match fs::hard_link(&temporary, path) {
                Ok(()) => fs::remove_file(&temporary).map_err(GeodataError::from),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let _ = fs::remove_file(&temporary);
                    write_page_immutable(path, bytes)
                }
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    Err(error.into())
                }
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn write_temporary(path: &Path, bytes: &[u8]) -> Result<std::path::PathBuf, GeodataError> {
    static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    for _ in 0..16 {
        let serial = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let temporary = path.with_file_name(format!(
            ".{name}.{}.{}.{serial}.tmp",
            std::process::id(),
            timestamp
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
                    let _ = fs::remove_file(&temporary);
                    return Err(error.into());
                }
                return Ok(temporary);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(invalid(
        "could not allocate a unique temporary package file",
    ))
}

fn read_bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>, GeodataError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(invalid("directory package file is not a regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(invalid("directory package file exceeds its byte limit"));
    }
    Ok(bytes)
}

fn ensure_directory(path: &Path) -> Result<(), GeodataError> {
    validate_existing_directories(path)?;
    fs::create_dir_all(path)?;
    validate_existing_directories(path)
}

fn validate_existing_directories(path: &Path) -> Result<(), GeodataError> {
    for ancestor in path
        .ancestors()
        .filter(|ancestor| !ancestor.as_os_str().is_empty())
    {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(invalid(
                "directory package path contains a symlink or non-directory",
            ));
        }
    }
    Ok(())
}

fn page_file(package_hash: &str, key: PageKey) -> String {
    format!(
        "pages/{package_hash}/{}/{}-{}-{}.json",
        key.layer.name(),
        key.level,
        key.x,
        key.y
    )
}

fn json_error(error: serde_json::Error) -> GeodataError {
    GeodataError::Directory(error.to_string())
}

fn invalid(reason: &str) -> GeodataError {
    GeodataError::Directory(reason.to_owned())
}

#[cfg(test)]
mod tests;
