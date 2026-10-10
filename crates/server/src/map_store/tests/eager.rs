use super::super::{MAX_PAGE_BYTES, MapStoreError, elevation_page_root};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, HistoricalLandUsePage, MapPackage, PotentialBiomePage,
    WaterPage, ordered_page_root,
};
use std::path::Path;

pub(super) fn load_elevation_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<ElevationPage>, MapStoreError> {
    if package.environment.samples_per_axis == 0 {
        return Ok(Vec::new());
    }
    let directory = directory.ok_or_else(|| MapStoreError::InvalidPackage {
        path: std::path::PathBuf::from("prepared-environment"),
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
    super::super::pages::load_water(directory, package)
}

pub(super) fn load_vegetation_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<PotentialBiomePage>, MapStoreError> {
    super::super::pages::load_vegetation(directory, package)
}

pub(super) fn load_land_use_pages(
    directory: Option<&Path>,
    package: &MapPackage,
) -> Result<Vec<HistoricalLandUsePage>, MapStoreError> {
    super::super::pages::load_land_use(directory, package)
}

fn read_page(path: &Path) -> Result<ElevationPage, MapStoreError> {
    let bytes = super::super::read_bounded_file(path, MAX_PAGE_BYTES, "elevation page")?;
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
