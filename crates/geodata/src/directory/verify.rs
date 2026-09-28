use super::{
    DirectoryLayer, GeodataError, MAX_DIRECTORY_MANIFEST_BYTES, MAX_DIRECTORY_PAGE_BYTES, PageKey,
    PageValue, invalid, page_file, read_bounded_file,
};
use aoe_map::{ENVIRONMENT_PAGE_SAMPLES, MapPackage, PageLayer, PageRootBuilder, PyramidLevel};
use std::{fs, path::Path};

pub(super) fn read_manifest(
    directory: &Path,
    package_hash: &str,
) -> Result<MapPackage, GeodataError> {
    super::validate_existing_directories(directory)?;
    if package_hash.len() != 64
        || !package_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid("package content hash is not canonical"));
    }
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("map storage root is not a regular directory"));
    }
    let path = directory.join(format!("{package_hash}.json"));
    let bytes = read_bounded_file(&path, MAX_DIRECTORY_MANIFEST_BYTES)?;
    let package: MapPackage = serde_json::from_slice(&bytes).map_err(json_error)?;
    if package.content_hash_hex() != package_hash {
        return Err(invalid("directory manifest identity or schema is invalid"));
    }
    Ok(package)
}

pub(super) fn verify_manifest(
    directory: &Path,
    package_hash: &str,
    package: &MapPackage,
) -> Result<(), GeodataError> {
    package.validate()?;
    verify_files(directory, package_hash, package)?;
    let mut count = 0_usize;
    count += verify_layer(
        directory,
        package_hash,
        DirectoryLayer::Elevation,
        Some(&package.environment.elevation.levels),
    )?;
    count += verify_layer(
        directory,
        package_hash,
        DirectoryLayer::Water,
        package
            .environment
            .water
            .as_ref()
            .map(|field| field.levels.as_slice()),
    )?;
    if let Some(index) = &package.environment.hydrology_evidence {
        count += verify_evidence_layer(
            directory,
            package_hash,
            DirectoryLayer::HydrologyEvidence,
            index.samples_per_axis,
            PageLayer::HydrologyEvidence,
            index.hydrology_page_root,
        )?;
        count += verify_evidence_layer(
            directory,
            package_hash,
            DirectoryLayer::ModernLandCover,
            index.samples_per_axis,
            PageLayer::ModernLandCover,
            index.modern_land_cover_page_root,
        )?;
    }
    count += verify_layer(
        directory,
        package_hash,
        DirectoryLayer::Vegetation,
        package
            .environment
            .vegetation
            .as_ref()
            .map(|field| field.levels.as_slice()),
    )?;
    count += verify_layer(
        directory,
        package_hash,
        DirectoryLayer::HistoricalLandUse,
        package
            .environment
            .historical_land_use
            .as_ref()
            .map(|field| field.levels.as_slice()),
    )?;
    if count == 0 && package.environment.samples_per_axis != 0 {
        return Err(invalid("prepared package has no indexed pages"));
    }
    Ok(())
}

fn verify_evidence_layer(
    directory: &Path,
    package_hash: &str,
    layer: DirectoryLayer,
    axis: u16,
    page_layer: PageLayer,
    expected_root: [u8; 32],
) -> Result<usize, GeodataError> {
    let side = usize::from(ENVIRONMENT_PAGE_SAMPLES);
    let count = usize::from(axis).div_ceil(side);
    let mut root = PageRootBuilder::new(page_layer, count.saturating_mul(count))?;
    for y in 0..count {
        for x in 0..count {
            let key = page_key(layer, 0, x, y)?;
            let page = read_page(directory, package_hash, key)?;
            if page.key() != key {
                return Err(invalid("typed page coordinates do not match its path"));
            }
            let width = (usize::from(axis) - x * side).min(side) as u8;
            let height = (usize::from(axis) - y * side).min(side) as u8;
            if page.dimensions() != (width, height) {
                return Err(invalid("typed page dimensions do not match its grid"));
            }
            root.push(page.content_hash()?)?;
        }
    }
    if root.finish()? != expected_root {
        return Err(invalid("typed pages do not reproduce the indexed root"));
    }
    Ok(count.saturating_mul(count))
}

fn verify_layer(
    directory: &Path,
    package_hash: &str,
    layer: DirectoryLayer,
    levels: Option<&[PyramidLevel]>,
) -> Result<usize, GeodataError> {
    let Some(levels) = levels else {
        return Ok(0);
    };
    let mut total = 0_usize;
    for (level, metadata) in levels.iter().enumerate() {
        let count = page_count(metadata);
        let mut root = PageRootBuilder::new(page_layer(layer), count.saturating_mul(count))?;
        for y in 0..count {
            for x in 0..count {
                let key = page_key(layer, level, x, y)?;
                let page = read_page(directory, package_hash, key)?;
                if page.key() != key {
                    return Err(invalid("directory page coordinates do not match its path"));
                }
                let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
                let width = metadata
                    .samples_per_axis
                    .saturating_sub(key.x.saturating_mul(side))
                    .min(side) as u8;
                let height = metadata
                    .samples_per_axis
                    .saturating_sub(key.y.saturating_mul(side))
                    .min(side) as u8;
                if page.dimensions() != (width, height) {
                    return Err(invalid("directory page dimensions do not match its grid"));
                }
                root.push(page.content_hash()?)?;
            }
        }
        if root.finish()? != metadata.ordered_page_root {
            return Err(invalid("directory pages do not reproduce the indexed root"));
        }
        total = total.saturating_add(count.saturating_mul(count));
    }
    Ok(total)
}

fn page_layer(layer: DirectoryLayer) -> PageLayer {
    match layer {
        DirectoryLayer::Elevation => PageLayer::Elevation,
        DirectoryLayer::Water => PageLayer::Water,
        DirectoryLayer::Vegetation => PageLayer::Vegetation,
        DirectoryLayer::HistoricalLandUse => PageLayer::HistoricalLandUse,
        DirectoryLayer::HydrologyEvidence => PageLayer::HydrologyEvidence,
        DirectoryLayer::ModernLandCover => PageLayer::ModernLandCover,
    }
}

pub(super) fn read_pages(
    directory: &Path,
    package_hash: &str,
    package: &MapPackage,
) -> Result<Vec<PageValue>, GeodataError> {
    let mut pages = Vec::new();
    read_layer_pages(
        directory,
        package_hash,
        DirectoryLayer::Elevation,
        Some(&package.environment.elevation.levels),
        &mut pages,
    )?;
    read_layer_pages(
        directory,
        package_hash,
        DirectoryLayer::Water,
        package
            .environment
            .water
            .as_ref()
            .map(|field| field.levels.as_slice()),
        &mut pages,
    )?;
    read_layer_pages(
        directory,
        package_hash,
        DirectoryLayer::Vegetation,
        package
            .environment
            .vegetation
            .as_ref()
            .map(|field| field.levels.as_slice()),
        &mut pages,
    )?;
    read_layer_pages(
        directory,
        package_hash,
        DirectoryLayer::HistoricalLandUse,
        package
            .environment
            .historical_land_use
            .as_ref()
            .map(|field| field.levels.as_slice()),
        &mut pages,
    )?;
    if let Some(index) = &package.environment.hydrology_evidence {
        read_evidence_pages(
            directory,
            package_hash,
            DirectoryLayer::HydrologyEvidence,
            index.samples_per_axis,
            &mut pages,
        )?;
        read_evidence_pages(
            directory,
            package_hash,
            DirectoryLayer::ModernLandCover,
            index.samples_per_axis,
            &mut pages,
        )?;
    }
    Ok(pages)
}

fn read_evidence_pages(
    directory: &Path,
    package_hash: &str,
    layer: DirectoryLayer,
    axis: u16,
    pages: &mut Vec<PageValue>,
) -> Result<(), GeodataError> {
    let count = usize::from(axis.div_ceil(ENVIRONMENT_PAGE_SAMPLES as u16));
    for y in 0..count {
        for x in 0..count {
            pages.push(read_page(
                directory,
                package_hash,
                page_key(layer, 0, x, y)?,
            )?);
        }
    }
    Ok(())
}

fn read_layer_pages(
    directory: &Path,
    package_hash: &str,
    layer: DirectoryLayer,
    levels: Option<&[PyramidLevel]>,
    pages: &mut Vec<PageValue>,
) -> Result<(), GeodataError> {
    let Some(levels) = levels else {
        return Ok(());
    };
    for (level, metadata) in levels.iter().enumerate() {
        let count = page_count(metadata);
        for y in 0..count {
            for x in 0..count {
                pages.push(read_page(
                    directory,
                    package_hash,
                    page_key(layer, level, x, y)?,
                )?);
            }
        }
    }
    Ok(())
}

fn page_count(metadata: &PyramidLevel) -> usize {
    usize::from(
        metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES)),
    )
}

fn page_key(
    layer: DirectoryLayer,
    level: usize,
    x: usize,
    y: usize,
) -> Result<PageKey, GeodataError> {
    Ok(PageKey {
        layer,
        level: u8::try_from(level).map_err(|_| invalid("page level overflows"))?,
        x: u16::try_from(x).map_err(|_| invalid("page coordinate overflows"))?,
        y: u16::try_from(y).map_err(|_| invalid("page coordinate overflows"))?,
    })
}

fn read_page(
    directory: &Path,
    package_hash: &str,
    key: PageKey,
) -> Result<PageValue, GeodataError> {
    let bytes = read_bounded_file(
        &directory.join(page_file(package_hash, key)),
        MAX_DIRECTORY_PAGE_BYTES,
    )?;
    let page = match key.layer {
        DirectoryLayer::Elevation => {
            PageValue::Elevation(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
        DirectoryLayer::Water => {
            PageValue::Water(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
        DirectoryLayer::Vegetation => {
            PageValue::Vegetation(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
        DirectoryLayer::HistoricalLandUse => {
            PageValue::HistoricalLandUse(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
        DirectoryLayer::HydrologyEvidence => {
            PageValue::HydrologyEvidence(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
        DirectoryLayer::ModernLandCover => {
            PageValue::ModernLandCover(serde_json::from_slice(&bytes).map_err(json_error)?)
        }
    };
    match &page {
        PageValue::Elevation(page) => page.validate()?,
        PageValue::Water(page) => page.validate()?,
        PageValue::Vegetation(page) => page.validate()?,
        PageValue::HistoricalLandUse(page) => page.validate()?,
        PageValue::HydrologyEvidence(page) => page.validate()?,
        PageValue::ModernLandCover(page) => page.validate()?,
    }
    Ok(page)
}

fn verify_files(
    directory: &Path,
    package_hash: &str,
    package: &MapPackage,
) -> Result<(), GeodataError> {
    let pages_root = directory.join("pages");
    if package.environment.samples_per_axis != 0 {
        require_directory(&pages_root)?;
        require_directory(&pages_root.join(package_hash))?;
    } else if let Ok(metadata) = fs::symlink_metadata(&pages_root)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        return Err(invalid("directory pages root is not a regular directory"));
    }
    for layer in [
        DirectoryLayer::Elevation,
        DirectoryLayer::Water,
        DirectoryLayer::Vegetation,
        DirectoryLayer::HistoricalLandUse,
        DirectoryLayer::HydrologyEvidence,
        DirectoryLayer::ModernLandCover,
    ] {
        let path = pages_root.join(package_hash).join(layer.name());
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && (metadata.file_type().is_symlink() || !metadata.is_dir())
        {
            return Err(invalid("directory page layer is not a regular directory"));
        }
    }
    Ok(())
}

fn require_directory(path: &Path) -> Result<(), GeodataError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(invalid("directory page path is not a regular directory"));
    }
    Ok(())
}

fn json_error(error: serde_json::Error) -> GeodataError {
    GeodataError::Directory(error.to_string())
}
