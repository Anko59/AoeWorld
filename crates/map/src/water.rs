use crate::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentError, FieldPyramid, WaterPage, ordered_water_page_root,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PreparedWater {
    samples_per_axis: u16,
    pages: BTreeMap<(u16, u16), WaterPage>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WaterCoverage {
    pub ocean_percent: u8,
    pub inland_percent: u8,
}

pub(crate) fn level_zero_water_pages(
    field: &FieldPyramid,
    pages: Vec<WaterPage>,
) -> Result<BTreeMap<(u16, u16), WaterPage>, EnvironmentError> {
    let mut levels = (0..field.levels.len())
        .map(|_| Vec::new())
        .collect::<Vec<Vec<WaterPage>>>();
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
            || ordered_water_page_root(&level_pages)? != metadata.ordered_page_root
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

impl PreparedWater {
    pub(crate) fn new(samples_per_axis: u16, pages: BTreeMap<(u16, u16), WaterPage>) -> Self {
        Self {
            samples_per_axis,
            pages,
        }
    }

    pub(crate) fn samples_per_axis(&self) -> u16 {
        self.samples_per_axis
    }

    /// Coverage of one level-zero source cell.
    pub(crate) fn cell(&self, x: i64, y: i64) -> Option<WaterCoverage> {
        let (x, y) = (u16::try_from(x).ok()?, u16::try_from(y).ok()?);
        let page_size = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self.pages.get(&(x / page_size, y / page_size))?;
        let local_x = usize::from(x % page_size);
        let local_y = usize::from(y % page_size);
        (local_x < usize::from(page.width) && local_y < usize::from(page.height)).then(|| {
            let index = local_y * usize::from(page.width) + local_x;
            WaterCoverage {
                ocean_percent: page.ocean_coverage_percent[index],
                inland_percent: page.inland_coverage_percent[index],
            }
        })
    }
}
