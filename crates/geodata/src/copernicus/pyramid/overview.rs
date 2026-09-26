use super::{GeodataError, PAGE};

pub(crate) fn coarse_coordinate(axis: u16, value: u16) -> u16 {
    coarse_coordinate_for_axis(axis, value, 128)
}

pub(crate) fn coarse_coordinate_for_axis(axis: u16, value: u16, source_axis: u16) -> u16 {
    let numerator = (u32::from(value) * 2 + 1) * u32::from(source_axis);
    let denominator = u32::from(axis) * 2;
    (numerator / denominator).min(u32::from(source_axis.saturating_sub(1))) as u16
}

pub(super) fn coarse_vegetation(
    overview: &crate::PreparedOverview,
    axis: u16,
    x: u16,
    y: u16,
) -> Result<u8, GeodataError> {
    let source_axis = overview.environment.samples_per_axis;
    let x = coarse_coordinate_for_axis(axis, x, source_axis);
    let y = coarse_coordinate_for_axis(axis, y, source_axis);
    let page = overview
        .vegetation_pages
        .iter()
        .find(|page| page.level == 0 && page.x == x / PAGE && page.y == y / PAGE)
        .ok_or(GeodataError::Preparation(
            "overview vegetation page is missing",
        ))?;
    Ok(page.potential_biome_class
        [usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE)])
}

pub(super) fn coarse_historical(
    overview: &crate::PreparedOverview,
    axis: u16,
    x: u16,
    y: u16,
) -> Result<(u8, u8, u16), GeodataError> {
    let historical_axis = overview
        .environment
        .historical_land_use
        .as_ref()
        .and_then(|field| field.levels.first())
        .map(|level| level.samples_per_axis)
        .or_else(|| {
            overview
                .historical_land_use_pages
                .iter()
                .filter(|page| page.level == 0)
                .map(|page| {
                    let x_extent = page
                        .x
                        .saturating_mul(PAGE)
                        .saturating_add(u16::from(page.width));
                    let y_extent = page
                        .y
                        .saturating_mul(PAGE)
                        .saturating_add(u16::from(page.height));
                    x_extent.max(y_extent)
                })
                .max()
        })
        .ok_or(GeodataError::Preparation(
            "overview historical field is missing",
        ))?;
    let x = coarse_coordinate_for_axis(axis, x, historical_axis);
    let y = coarse_coordinate_for_axis(axis, y, historical_axis);
    let page = overview
        .historical_land_use_pages
        .iter()
        .find(|page| page.level == 0 && page.x == x / PAGE && page.y == y / PAGE)
        .ok_or(GeodataError::Preparation(
            "overview historical page is missing",
        ))?;
    let index = usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE);
    Ok((
        page.crop_percent[index],
        page.grazing_percent[index],
        page.population_pressure_per_square_kilometer[index],
    ))
}
