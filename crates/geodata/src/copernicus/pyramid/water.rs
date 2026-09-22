use super::{GeodataError, PAGE, coarse_coordinate};

pub(super) fn coarse_water(
    hydrology: &crate::PreparedHydrology,
    overview: &crate::PreparedOverview,
    axis: u16,
    x: u16,
    y: u16,
) -> Result<(u8, u8), GeodataError> {
    if let Some(water) = hydrology.modern_water_override_at(axis, x, y)? {
        return Ok(water);
    }
    let x = coarse_coordinate(axis, x);
    let y = coarse_coordinate(axis, y);
    let page = overview
        .water_pages
        .iter()
        .find(|page| page.level == 0 && page.x == x / PAGE && page.y == y / PAGE)
        .ok_or(GeodataError::Preparation("overview water page is missing"))?;
    let index = usize::from(y % PAGE) * usize::from(page.width) + usize::from(x % PAGE);
    Ok((
        page.ocean_coverage_percent[index],
        page.inland_coverage_percent[index],
    ))
}
