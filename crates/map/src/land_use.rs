use crate::{ENVIRONMENT_PAGE_SAMPLES, EnvironmentError, FieldPyramid};
use aoe_core::TileCoord;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One bounded historical land-use page. Crop and grazing are fractions of
/// valid land area, while population remains a pressure signal only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoricalLandUsePage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub crop_percent: Vec<u8>,
    pub grazing_percent: Vec<u8>,
    pub population_pressure_per_square_kilometer: Vec<u16>,
}

impl HistoricalLandUsePage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        let samples = usize::from(self.width) * usize::from(self.height);
        if self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.crop_percent.len() != samples
            || self.grazing_percent.len() != samples
            || self.population_pressure_per_square_kilometer.len() != samples
            || self
                .crop_percent
                .iter()
                .zip(&self.grazing_percent)
                .any(|(&crop, &grazing)| u16::from(crop) + u16::from(grazing) > 100)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-historical-land-use-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        hash.update(&self.crop_percent);
        hash.update(&self.grazing_percent);
        for population in &self.population_pressure_per_square_kilometer {
            hash.update(&population.to_le_bytes());
        }
        Ok(*hash.finalize().as_bytes())
    }
}

pub fn ordered_land_use_page_root(
    pages: &[HistoricalLandUsePage],
) -> Result<[u8; 32], EnvironmentError> {
    if pages.is_empty() {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut ordered = pages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|page| (page.y, page.x));
    if ordered
        .windows(2)
        .any(|pair| (pair[0].x, pair[0].y) == (pair[1].x, pair[1].y))
    {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut root = crate::PageRootBuilder::new(crate::PageLayer::HistoricalLandUse, ordered.len())?;
    for page in ordered {
        root.push(page.content_hash()?)?;
    }
    root.finish()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HistoricalLandUse {
    samples_per_axis: u16,
    pages: BTreeMap<(u16, u16), HistoricalLandUsePage>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LandUseSample {
    pub crop_percent: u8,
    pub grazing_percent: u8,
    pub population_pressure_per_square_kilometer: u16,
}

pub(crate) fn level_zero_land_use_pages(
    field: &FieldPyramid,
    pages: Vec<HistoricalLandUsePage>,
) -> Result<BTreeMap<(u16, u16), HistoricalLandUsePage>, EnvironmentError> {
    let mut levels = (0..field.levels.len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<HistoricalLandUsePage>>>();
    for page in pages {
        let level = usize::from(page.level);
        let metadata = field
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
    for (level, (metadata, level_pages)) in field.levels.iter().zip(levels).enumerate() {
        let count = metadata
            .samples_per_axis
            .div_ceil(u16::from(ENVIRONMENT_PAGE_SAMPLES));
        if level_pages.len() != usize::from(count).pow(2)
            || ordered_land_use_page_root(&level_pages)? != metadata.ordered_page_root
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

impl HistoricalLandUse {
    pub(crate) fn new(
        samples_per_axis: u16,
        pages: BTreeMap<(u16, u16), HistoricalLandUsePage>,
    ) -> Self {
        Self {
            samples_per_axis,
            pages,
        }
    }

    pub(crate) fn at(&self, tile: TileCoord, width_tiles: i32) -> Option<LandUseSample> {
        let tile_axis = u64::try_from(width_tiles.checked_sub(1)?).ok()?;
        let source_axis = u64::from(self.samples_per_axis.checked_sub(1)?);
        let x =
            u16::try_from((u64::try_from(tile.x).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let y =
            u16::try_from((u64::try_from(tile.y).ok()? * source_axis + tile_axis / 2) / tile_axis)
                .ok()?;
        let page_size = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self.pages.get(&(x / page_size, y / page_size))?;
        let local_x = usize::from(x % page_size);
        let local_y = usize::from(y % page_size);
        (local_x < usize::from(page.width) && local_y < usize::from(page.height)).then(|| {
            let index = local_y * usize::from(page.width) + local_x;
            LandUseSample {
                crop_percent: page.crop_percent[index],
                grazing_percent: page.grazing_percent[index],
                population_pressure_per_square_kilometer: page
                    .population_pressure_per_square_kilometer[index],
            }
        })
    }
}
