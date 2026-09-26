use super::super::{GeodataError, MAX_HYDROLOGY_SAMPLES_PER_AXIS, PAGE};
use aoe_map::ElevationPage;

pub(in crate::hydrology) struct ElevationGrid {
    axis: u16,
    values: Vec<i32>,
}

impl ElevationGrid {
    pub(super) fn new(pages: &[ElevationPage]) -> Result<Self, GeodataError> {
        if pages.is_empty() || pages.iter().any(|page| page.level != 0) {
            return Err(GeodataError::Preparation(
                "water-model elevation grid is unavailable",
            ));
        }
        let max_pages_per_axis = MAX_HYDROLOGY_SAMPLES_PER_AXIS.div_ceil(PAGE);
        let max_page_index = max_pages_per_axis - 1;
        let max_page_count = usize::from(max_pages_per_axis).pow(2);
        if pages.len() > max_page_count
            || pages
                .iter()
                .any(|page| page.x > max_page_index || page.y > max_page_index)
        {
            return Err(GeodataError::Preparation(
                "water-model elevation grid exceeds its bound",
            ));
        }
        for page in pages {
            page.validate()
                .map_err(|_| GeodataError::Preparation("water-model elevation page is invalid"))?;
        }
        let columns = pages.iter().map(|page| page.x).max().unwrap_or(0) + 1;
        let rows = pages.iter().map(|page| page.y).max().unwrap_or(0) + 1;
        if columns != rows {
            return Err(GeodataError::Preparation(
                "water-model elevation grid is not square",
            ));
        }
        let pages_per_axis = columns;
        if pages.len() != usize::from(pages_per_axis).pow(2) {
            return Err(GeodataError::Preparation(
                "water-model elevation grid is incomplete",
            ));
        }
        let mut axis = pages_per_axis
            .checked_mul(PAGE)
            .ok_or(GeodataError::Preparation(
                "water-model elevation axis overflows",
            ))?;
        axis = pages
            .iter()
            .map(|page| page.x * PAGE + u16::from(page.width))
            .max()
            .unwrap_or(axis);
        if axis == 0 || axis > MAX_HYDROLOGY_SAMPLES_PER_AXIS {
            return Err(GeodataError::Preparation(
                "water-model elevation grid exceeds its bound",
            ));
        }
        if pages
            .iter()
            .map(|page| page.y * PAGE + u16::from(page.height))
            .max()
            != Some(axis)
        {
            return Err(GeodataError::Preparation(
                "water-model elevation grid is not square",
            ));
        }
        for page in pages {
            if page.width != (axis - page.x * PAGE).min(PAGE) as u8
                || page.height != (axis - page.y * PAGE).min(PAGE) as u8
            {
                return Err(GeodataError::Preparation(
                    "water-model elevation page shape is invalid",
                ));
            }
        }
        let mut values = vec![0; usize::from(axis).pow(2)];
        let mut seen = vec![false; usize::from(pages_per_axis).pow(2)];
        for page in pages {
            let slot = usize::from(page.y) * usize::from(pages_per_axis) + usize::from(page.x);
            let Some(present) = seen.get_mut(slot) else {
                return Err(GeodataError::Preparation(
                    "water-model elevation page is out of range",
                ));
            };
            if *present {
                return Err(GeodataError::Preparation(
                    "water-model elevation page is duplicated",
                ));
            }
            *present = true;
            for local_y in 0..u16::from(page.height) {
                for local_x in 0..u16::from(page.width) {
                    let local =
                        usize::from(local_y) * usize::from(page.width) + usize::from(local_x);
                    let index = usize::from(page.y * PAGE + local_y) * usize::from(axis)
                        + usize::from(page.x * PAGE + local_x);
                    values[index] = page.geographic_height_centimeters[local];
                }
            }
        }
        if seen.iter().any(|present| !present) {
            return Err(GeodataError::Preparation(
                "water-model elevation grid is incomplete",
            ));
        }
        Ok(Self { axis, values })
    }

    pub(super) fn target_height(&self, target_axis: u16, x: u16, y: u16) -> i32 {
        let source_x =
            (((u32::from(x) * 2 + 1) * u32::from(self.axis)) / (u32::from(target_axis) * 2)) as u16;
        let source_y =
            (((u32::from(y) * 2 + 1) * u32::from(self.axis)) / (u32::from(target_axis) * 2)) as u16;
        self.values[usize::from(source_y.min(self.axis - 1)) * usize::from(self.axis)
            + usize::from(source_x.min(self.axis - 1))]
    }
}
