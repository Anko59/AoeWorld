use super::{MAX_PAGE_BYTES, MapStoreError};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, FieldPyramid, HistoricalLandUsePage,
    MapPackage, PageLayer, PageRootBuilder, PotentialBiomePage, WaterPage,
};
use serde::de::DeserializeOwned;
use std::{fs, io::Read, path::Path};

pub(super) fn environment(directory: &Path, package: &MapPackage) -> Result<(), MapStoreError> {
    require_directory(directory)?;
    package
        .validate()
        .map_err(|error| invalid(directory, error))?;
    let pages = directory.join("pages");
    let root = pages.join(package.content_hash_hex());
    if package.environment.samples_per_axis == 0 {
        validate_optional_directory(&pages)?;
        validate_optional_directory(&root)?;
        validate_known_layers(&root)?;
        return Ok(());
    }
    for path in [pages, root.clone()] {
        require_directory(&path)?;
    }
    validate_known_layers(&root)?;
    field::<ElevationPage>(&root, PageLayer::Elevation, &package.environment.elevation)?;
    if let Some(index) = &package.environment.water {
        field::<WaterPage>(&root, PageLayer::Water, index)?;
    }
    if let Some(index) = &package.environment.vegetation {
        field::<PotentialBiomePage>(&root, PageLayer::Vegetation, index)?;
    }
    if let Some(index) = &package.environment.historical_land_use {
        field::<HistoricalLandUsePage>(&root, PageLayer::HistoricalLandUse, index)?;
    }
    Ok(())
}

fn field<T: StoredPage>(
    root: &Path,
    layer: PageLayer,
    index: &FieldPyramid,
) -> Result<(), MapStoreError> {
    let directory = root.join(layer.directory_name());
    require_directory(&directory)?;
    let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let mut legacy_index = 0;
    for (level, metadata) in index.levels.iter().enumerate() {
        let count = metadata.samples_per_axis.div_ceil(side);
        let mut digest = PageRootBuilder::new(layer, usize::from(count).pow(2))
            .map_err(|error| invalid(&directory, error))?;
        for y in 0..count {
            for x in 0..count {
                let coordinate_path = directory.join(format!("{level}-{x}-{y}.json"));
                let path = if layer == PageLayer::Elevation || coordinate_path.try_exists()? {
                    coordinate_path
                } else {
                    directory.join(format!("{legacy_index}.json"))
                };
                legacy_index += 1;
                let page: T = read(&path)?;
                let width = (metadata.samples_per_axis - x * side).min(side) as u8;
                let height = (metadata.samples_per_axis - y * side).min(side) as u8;
                if page.coordinates() != (level as u8, x, y, width, height) {
                    return Err(invalid(
                        &path,
                        "page coordinates or dimensions do not match its index",
                    ));
                }
                let hash = page.content_hash().map_err(|error| invalid(&path, error))?;
                digest.push(hash).map_err(|error| invalid(&path, error))?;
            }
        }
        if digest
            .finish()
            .map_err(|error| invalid(&directory, error))?
            != metadata.ordered_page_root
        {
            return Err(invalid(
                &directory,
                "pages do not reproduce the indexed root",
            ));
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), MapStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid(path, "page directory is not a regular directory"));
    }
    Ok(())
}

fn validate_optional_directory(path: &Path) -> Result<(), MapStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            Err(invalid(path, "page directory is not a regular directory"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_known_layers(root: &Path) -> Result<(), MapStoreError> {
    for layer in [
        PageLayer::Elevation,
        PageLayer::Water,
        PageLayer::Vegetation,
        PageLayer::HistoricalLandUse,
    ] {
        validate_optional_directory(&root.join(layer.directory_name()))?;
    }
    Ok(())
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<T, MapStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > MAX_PAGE_BYTES {
        return Err(invalid(path, "page is not a bounded regular file"));
    }
    // At most one page is resident while the level digest is accumulated.
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_PAGE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PAGE_BYTES {
        return Err(invalid(path, "page grew beyond the configured bound"));
    }
    serde_json::from_slice(&bytes).map_err(|error| invalid(path, error))
}

fn invalid(path: &Path, reason: impl std::fmt::Display) -> MapStoreError {
    MapStoreError::InvalidPackage {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}

trait StoredPage: DeserializeOwned {
    fn coordinates(&self) -> (u8, u16, u16, u8, u8);
    fn content_hash(&self) -> Result<[u8; 32], EnvironmentError>;
}

macro_rules! stored_page {
    ($kind:ty) => {
        impl StoredPage for $kind {
            fn coordinates(&self) -> (u8, u16, u16, u8, u8) {
                (self.level, self.x, self.y, self.width, self.height)
            }
            fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
                <$kind>::content_hash(self)
            }
        }
    };
}
stored_page!(ElevationPage);
stored_page!(WaterPage);
stored_page!(PotentialBiomePage);
stored_page!(HistoricalLandUsePage);
