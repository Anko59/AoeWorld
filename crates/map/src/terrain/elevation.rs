use crate::{ENVIRONMENT_PAGE_SAMPLES, ElevationPage, Ratio};
use aoe_core::TileCoord;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedElevation {
    pub(super) samples_per_axis: u16,
    pub(super) compression: Ratio,
    pub(super) pages: BTreeMap<(u16, u16), ElevationPage>,
}

impl PreparedElevation {
    pub(super) fn height_at(&self, tile: TileCoord, width_tiles: i32) -> Option<i32> {
        self.sample(tile.x, tile.y, width_tiles)
    }

    pub(super) fn corner_heights(&self, tile: TileCoord, width_tiles: i32) -> Option<[i32; 4]> {
        Some([
            self.sample(tile.x, tile.y, width_tiles)?,
            self.sample(tile.x.saturating_add(1), tile.y, width_tiles)?,
            self.sample(
                tile.x.saturating_add(1),
                tile.y.saturating_add(1),
                width_tiles,
            )?,
            self.sample(tile.x, tile.y.saturating_add(1), width_tiles)?,
        ])
    }

    fn sample(&self, x: i32, y: i32, width_tiles: i32) -> Option<i32> {
        let tile_axis = u64::try_from(width_tiles.checked_sub(1)?).ok()?;
        let source_axis = u64::from(self.samples_per_axis.checked_sub(1)?);
        let x = u64::try_from(x.clamp(0, width_tiles.checked_sub(1)?)).ok()?;
        let y = u64::try_from(y.clamp(0, width_tiles.checked_sub(1)?)).ok()?;
        let source_x = u16::try_from((x * source_axis + tile_axis / 2) / tile_axis).ok()?;
        let source_y = u16::try_from((y * source_axis + tile_axis / 2) / tile_axis).ok()?;
        let page_size = u16::from(ENVIRONMENT_PAGE_SAMPLES);
        let page = self
            .pages
            .get(&(source_x / page_size, source_y / page_size))?;
        let local_x = usize::from(source_x % page_size);
        let local_y = usize::from(source_y % page_size);
        (local_x < usize::from(page.width) && local_y < usize::from(page.height)).then(|| {
            page.geographic_height_centimeters[local_y * usize::from(page.width) + local_x]
        })
    }
}
