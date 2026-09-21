use super::{MAX_PAGE_BYTES, MapStoreError, ensure_directory_path, write_json};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, HistoricalLandUsePage, MapPackage, PotentialBiomePage, WaterPage,
    ordered_biome_page_root, ordered_land_use_page_root, ordered_water_page_root,
};
use std::path::{Path, PathBuf};

pub(super) fn persist_water(
    directory: &Path,
    package: &MapPackage,
    pages: &[WaterPage],
) -> Result<(), MapStoreError> {
    persist(directory, package, "water", pages, |page| {
        (page.level, page.x, page.y)
    })
}

pub(super) fn persist_vegetation(
    directory: &Path,
    package: &MapPackage,
    pages: &[PotentialBiomePage],
) -> Result<(), MapStoreError> {
    persist(directory, package, "vegetation", pages, |page| {
        (page.level, page.x, page.y)
    })
}

pub(super) fn persist_land_use(
    directory: &Path,
    package: &MapPackage,
    pages: &[HistoricalLandUsePage],
) -> Result<(), MapStoreError> {
    persist(directory, package, "historical-land-use", pages, |page| {
        (page.level, page.x, page.y)
    })
}

pub(super) fn load_water(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<WaterPage>, MapStoreError> {
    let Some(levels) = package
        .environment
        .water
        .as_ref()
        .map(|field| &field.levels)
    else {
        return Ok(Vec::new());
    };
    let directory = prepared_directory(directory, "water")?;
    let pages = load(directory, package, "water", levels, read_water)?;
    verify_water(package, &pages).map_err(|reason| invalid(directory, reason))?;
    Ok(pages)
}

pub(super) fn load_vegetation(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<PotentialBiomePage>, MapStoreError> {
    let Some(levels) = package
        .environment
        .vegetation
        .as_ref()
        .map(|field| &field.levels)
    else {
        return Ok(Vec::new());
    };
    let directory = prepared_directory(directory, "vegetation")?;
    let pages = load(directory, package, "vegetation", levels, read_vegetation)?;
    verify_vegetation(package, &pages).map_err(|reason| invalid(directory, reason))?;
    Ok(pages)
}

pub(super) fn load_land_use(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<HistoricalLandUsePage>, MapStoreError> {
    let Some(levels) = package
        .environment
        .historical_land_use
        .as_ref()
        .map(|field| &field.levels)
    else {
        return Ok(Vec::new());
    };
    let directory = prepared_directory(directory, "historical land-use")?;
    let pages = load(
        directory,
        package,
        "historical-land-use",
        levels,
        read_land_use,
    )?;
    verify_land_use(package, &pages).map_err(|reason| invalid(directory, reason))?;
    Ok(pages)
}

pub(super) fn verify_water(package: &MapPackage, pages: &[WaterPage]) -> Result<(), String> {
    let Some(field) = &package.environment.water else {
        return pages
            .is_empty()
            .then_some(())
            .ok_or_else(|| "water pages require a water index".to_owned());
    };
    if pages.iter().any(|page| page.validate().is_err()) {
        return Err("prepared package has invalid water pages".to_owned());
    }
    for (level, metadata) in field.levels.iter().enumerate() {
        let level_pages = pages
            .iter()
            .filter(|page| usize::from(page.level) == level)
            .cloned()
            .collect::<Vec<_>>();
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_water_page_root(&level_pages).ok() != Some(metadata.ordered_page_root)
        {
            return Err("prepared package water page index is incomplete".to_owned());
        }
    }
    Ok(())
}

pub(super) fn verify_vegetation(
    package: &MapPackage,
    pages: &[PotentialBiomePage],
) -> Result<(), String> {
    let Some(field) = &package.environment.vegetation else {
        return pages
            .is_empty()
            .then_some(())
            .ok_or_else(|| "vegetation pages require a vegetation index".to_owned());
    };
    if pages.iter().any(|page| page.validate().is_err()) {
        return Err("prepared package has invalid vegetation pages".to_owned());
    }
    for (level, metadata) in field.levels.iter().enumerate() {
        let level_pages = pages
            .iter()
            .filter(|page| usize::from(page.level) == level)
            .cloned()
            .collect::<Vec<_>>();
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_biome_page_root(&level_pages).ok() != Some(metadata.ordered_page_root)
        {
            return Err("prepared package vegetation page index is incomplete".to_owned());
        }
    }
    Ok(())
}

pub(super) fn verify_land_use(
    package: &MapPackage,
    pages: &[HistoricalLandUsePage],
) -> Result<(), String> {
    let Some(field) = &package.environment.historical_land_use else {
        return pages
            .is_empty()
            .then_some(())
            .ok_or_else(|| "historical land-use pages require an index".to_owned());
    };
    if pages.iter().any(|page| page.validate().is_err()) {
        return Err("prepared package has invalid historical land-use pages".to_owned());
    }
    for (level, metadata) in field.levels.iter().enumerate() {
        let level_pages = pages
            .iter()
            .filter(|page| usize::from(page.level) == level)
            .cloned()
            .collect::<Vec<_>>();
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_land_use_page_root(&level_pages).ok() != Some(metadata.ordered_page_root)
        {
            return Err("prepared package historical land-use index is incomplete".to_owned());
        }
    }
    Ok(())
}

fn persist<T: serde::Serialize>(
    directory: &Path,
    package: &MapPackage,
    layer: &str,
    pages: &[T],
    coordinates: impl Fn(&T) -> (u8, u16, u16),
) -> Result<(), MapStoreError> {
    let root = root(directory, package, layer);
    ensure_directory_path(&root)?;
    for page in pages {
        let (level, x, y) = coordinates(page);
        write_json(
            &root.join(format!("{level}-{x}-{y}.json")),
            page,
            MAX_PAGE_BYTES,
        )?;
    }
    Ok(())
}

fn load<T>(
    directory: &Path,
    package: &MapPackage,
    layer: &str,
    levels: &[aoe_map::PyramidLevel],
    read: impl Fn(&Path) -> Result<T, MapStoreError>,
) -> Result<Vec<T>, MapStoreError> {
    let root = root(directory, package, layer);
    let mut pages = Vec::new();
    let mut index = 0;
    for (level, metadata) in levels.iter().enumerate() {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        for y in 0..count {
            for x in 0..count {
                let coordinate_path = root.join(format!("{level}-{x}-{y}.json"));
                let path = if coordinate_path.try_exists()? {
                    coordinate_path
                } else {
                    root.join(format!("{index}.json"))
                };
                pages.push(read(&path)?);
                index += 1;
            }
        }
    }
    Ok(pages)
}

fn root(directory: &Path, package: &MapPackage, layer: &str) -> PathBuf {
    directory
        .join("pages")
        .join(package.content_hash_hex())
        .join(layer)
}

fn prepared_directory<'a>(
    directory: Option<&'a Path>,
    layer: &str,
) -> Result<&'a Path, MapStoreError> {
    directory.ok_or_else(|| {
        invalid(
            Path::new("prepared-environment"),
            format!("prepared {layer} requires a page directory"),
        )
    })
}

fn invalid(path: &Path, reason: impl Into<String>) -> MapStoreError {
    MapStoreError::InvalidPackage {
        path: path.to_owned(),
        reason: reason.into(),
    }
}

fn read_water(path: &Path) -> Result<WaterPage, MapStoreError> {
    read(path, "water")
}

fn read_vegetation(path: &Path) -> Result<PotentialBiomePage, MapStoreError> {
    read(path, "vegetation")
}

fn read_land_use(path: &Path) -> Result<HistoricalLandUsePage, MapStoreError> {
    read(path, "historical land-use")
}

fn read<T: serde::de::DeserializeOwned>(path: &Path, layer: &str) -> Result<T, MapStoreError> {
    let bytes = super::read_bounded_file(path, MAX_PAGE_BYTES, &format!("{layer} page"))?;
    serde_json::from_slice(&bytes).map_err(|error| invalid(path, error.to_string()))
}
