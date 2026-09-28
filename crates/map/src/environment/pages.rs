use super::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, PotentialBiomePage,
    PreparedEnvironment, WaterPage,
};
use std::collections::BTreeMap;

fn ordered_root<T>(
    pages: &[T],
    layer: crate::PageLayer,
    hash: impl Fn(&T) -> Result<[u8; 32], EnvironmentError>,
    coordinates: impl Fn(&T) -> (u16, u16),
) -> Result<[u8; 32], EnvironmentError> {
    if pages.is_empty() {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut ordered = pages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|page| {
        let (x, y) = coordinates(page);
        (y, x)
    });
    if ordered
        .windows(2)
        .any(|pair| coordinates(pair[0]) == coordinates(pair[1]))
    {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut root = crate::PageRootBuilder::new(layer, ordered.len())?;
    for page in ordered {
        root.push(hash(page)?)?;
    }
    root.finish()
}

pub fn ordered_page_root(pages: &[ElevationPage]) -> Result<[u8; 32], EnvironmentError> {
    ordered_root(
        pages,
        crate::PageLayer::Elevation,
        ElevationPage::content_hash,
        |page| (page.x, page.y),
    )
}

pub fn ordered_water_page_root(pages: &[WaterPage]) -> Result<[u8; 32], EnvironmentError> {
    ordered_root(
        pages,
        crate::PageLayer::Water,
        WaterPage::content_hash,
        |page| (page.x, page.y),
    )
}

pub fn ordered_biome_page_root(pages: &[PotentialBiomePage]) -> Result<[u8; 32], EnvironmentError> {
    ordered_root(
        pages,
        crate::PageLayer::Vegetation,
        PotentialBiomePage::content_hash,
        |page| (page.x, page.y),
    )
}

pub(crate) fn level_zero_pages(
    environment: &PreparedEnvironment,
    pages: Vec<ElevationPage>,
) -> Result<BTreeMap<(u16, u16), ElevationPage>, EnvironmentError> {
    environment.validate()?;
    let mut levels = (0..environment.elevation.levels.len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<ElevationPage>>>();
    for page in pages {
        let level = usize::from(page.level);
        let metadata = environment
            .elevation
            .levels
            .get(level)
            .ok_or(EnvironmentError::InvalidPyramid)?;
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        page.validate()?;
        if page.x >= count || page.y >= count {
            return Err(EnvironmentError::InvalidPyramid);
        }
        levels[level].push(page);
    }
    let mut level_zero = BTreeMap::new();
    for (level, (metadata, level_pages)) in
        environment.elevation.levels.iter().zip(levels).enumerate()
    {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_page_root(&level_pages)? != metadata.ordered_page_root
        {
            return Err(EnvironmentError::InvalidPyramid);
        }
        if level == 0 {
            for page in level_pages {
                if level_zero.insert((page.x, page.y), page).is_some() {
                    return Err(EnvironmentError::InvalidPyramid);
                }
            }
        }
    }
    Ok(level_zero)
}
