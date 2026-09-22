use crate::{ENVIRONMENT_PAGE_SAMPLES, ElevationPage, Ratio};
use aoe_core::TileCoord;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AxisPosition {
    TileCenter,
    Corner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PreparedElevation {
    pub(super) samples_per_axis: u16,
    pub(super) compression: Ratio,
    pub(super) pages: BTreeMap<(u16, u16), ElevationPage>,
    pub(super) sampling_recipe: u16,
}

impl PreparedElevation {
    pub(super) fn height_at(&self, tile: TileCoord, width_tiles: i32) -> Option<i32> {
        self.sample(tile.x, tile.y, width_tiles, AxisPosition::TileCenter)
    }

    pub(super) fn corner_heights(&self, tile: TileCoord, width_tiles: i32) -> Option<[i32; 4]> {
        Some([
            self.sample(tile.x, tile.y, width_tiles, AxisPosition::Corner)?,
            self.sample(
                tile.x.saturating_add(1),
                tile.y,
                width_tiles,
                AxisPosition::Corner,
            )?,
            self.sample(
                tile.x.saturating_add(1),
                tile.y.saturating_add(1),
                width_tiles,
                AxisPosition::Corner,
            )?,
            self.sample(
                tile.x,
                tile.y.saturating_add(1),
                width_tiles,
                AxisPosition::Corner,
            )?,
        ])
    }

    fn sample(&self, x: i32, y: i32, width_tiles: i32, position: AxisPosition) -> Option<i32> {
        if self.sampling_recipe == crate::GENERATION_RECIPE_VERSION {
            return self.sample_bilinear(x, y, width_tiles, position);
        }
        let source_x = nearest_source_coordinate(x, self.samples_per_axis, width_tiles)?;
        let source_y = nearest_source_coordinate(y, self.samples_per_axis, width_tiles)?;
        self.source_sample(source_x, source_y)
    }

    fn sample_bilinear(
        &self,
        x: i32,
        y: i32,
        width_tiles: i32,
        position: AxisPosition,
    ) -> Option<i32> {
        let (x0, x1, x_remainder, denominator) =
            source_axis_position(x, self.samples_per_axis, width_tiles, position)?;
        let (y0, y1, y_remainder, _) =
            source_axis_position(y, self.samples_per_axis, width_tiles, position)?;
        Some(bilinear_height(
            [
                self.source_sample(x0, y0)?,
                self.source_sample(x1, y0)?,
                self.source_sample(x1, y1)?,
                self.source_sample(x0, y1)?,
            ],
            x_remainder,
            y_remainder,
            denominator,
        ))
    }

    fn source_sample(&self, source_x: u16, source_y: u16) -> Option<i32> {
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

/// Exact rational source-grid coordinate for a cell-centred sample. Tile
/// centres and shared corners use different offsets over the full tile axis.
/// The remainder stays integer until the final height interpolation.
pub(super) fn source_axis_position(
    coordinate: i32,
    samples_per_axis: u16,
    width_tiles: i32,
    position: AxisPosition,
) -> Option<(u16, u16, u64, u64)> {
    let tile_axis = i128::from(width_tiles);
    if tile_axis <= 0 || samples_per_axis == 0 {
        return None;
    }
    let maximum_coordinate = match position {
        AxisPosition::TileCenter => width_tiles.checked_sub(1)?,
        AxisPosition::Corner => width_tiles,
    };
    let coordinate = i128::from(coordinate.clamp(0, maximum_coordinate));
    let sample_axis = i128::from(samples_per_axis);
    let denominator = tile_axis.checked_mul(2)?;
    let doubled_coordinate = match position {
        AxisPosition::TileCenter => coordinate.checked_mul(2)?.checked_add(1)?,
        AxisPosition::Corner => coordinate.checked_mul(2)?,
    };
    let numerator = doubled_coordinate
        .checked_mul(sample_axis)?
        .checked_sub(tile_axis)?;
    let maximum_sample = i128::from(samples_per_axis.checked_sub(1)?);
    let maximum_numerator = maximum_sample.checked_mul(denominator)?;
    let denominator_u64 = u64::try_from(denominator).ok()?;
    if numerator <= 0 {
        return Some((0, 0, 0, denominator_u64));
    }
    if numerator >= maximum_numerator {
        let last = samples_per_axis.checked_sub(1)?;
        return Some((last, last, 0, denominator_u64));
    }
    let lower = u16::try_from(numerator / denominator).ok()?;
    let remainder = u64::try_from(numerator % denominator).ok()?;
    let upper = lower.checked_add(1)?;
    Some((lower, upper, remainder, denominator_u64))
}

pub(super) fn nearest_source_coordinate(
    coordinate: i32,
    samples_per_axis: u16,
    width_tiles: i32,
) -> Option<u16> {
    let tile_axis = u64::try_from(width_tiles.checked_sub(1)?).ok()?;
    let source_axis = u64::from(samples_per_axis.checked_sub(1)?);
    if tile_axis == 0 {
        return Some(0);
    }
    let coordinate = u64::try_from(coordinate.clamp(0, width_tiles.checked_sub(1)?)).ok()?;
    u16::try_from((coordinate * source_axis + tile_axis / 2) / tile_axis).ok()
}

pub(super) fn bilinear_height(
    values: [i32; 4],
    x_remainder: u64,
    y_remainder: u64,
    denominator: u64,
) -> i32 {
    let denominator = i128::from(denominator.max(1));
    let x1 = i128::from(x_remainder);
    let y1 = i128::from(y_remainder);
    let x0 = denominator - x1;
    let y0 = denominator - y1;
    let total = i128::from(values[0]) * x0 * y0
        + i128::from(values[1]) * x1 * y0
        + i128::from(values[2]) * x1 * y1
        + i128::from(values[3]) * x0 * y1;
    let scale = denominator * denominator;
    let rounded = if total >= 0 {
        (total + scale / 2) / scale
    } else {
        (total - scale / 2) / scale
    };
    rounded.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
}
