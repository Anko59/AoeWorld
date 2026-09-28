use crate::{ENVIRONMENT_PAGE_SAMPLES, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey};
use std::sync::Arc;

use super::MapChunkGenerator;

pub(super) fn load_page(
    generator: &MapChunkGenerator,
    key: EnvironmentPageKey,
    cancelled: &dyn Fn() -> bool,
) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
    if cancelled() {
        return Err(EnvironmentPageError::Cancelled);
    }
    let provider = generator
        .provider
        .as_ref()
        .ok_or(EnvironmentPageError::Invalid)?;
    let page = provider.page(key, cancelled)?;
    if page.key() != key {
        return Err(EnvironmentPageError::Corrupt);
    }
    Ok(page)
}

pub(super) fn source_coordinate(
    x: i32,
    y: i32,
    samples: u16,
    width_tiles: i32,
) -> Result<(u16, u16), EnvironmentPageError> {
    let tile_axis = u64::try_from(
        width_tiles
            .checked_sub(1)
            .ok_or(EnvironmentPageError::Invalid)?,
    )
    .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_axis = u64::from(
        samples
            .checked_sub(1)
            .ok_or(EnvironmentPageError::Invalid)?,
    );
    let x = u64::try_from(x.clamp(0, width_tiles.saturating_sub(1)))
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let y = u64::try_from(y.clamp(0, width_tiles.saturating_sub(1)))
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_x = u16::try_from((x * source_axis + tile_axis / 2) / tile_axis)
        .map_err(|_| EnvironmentPageError::Invalid)?;
    let source_y = u16::try_from((y * source_axis + tile_axis / 2) / tile_axis)
        .map_err(|_| EnvironmentPageError::Invalid)?;
    Ok((source_x, source_y))
}

pub(super) fn elevation_value(
    page: &Arc<EnvironmentPage>,
    source_x: u16,
    source_y: u16,
) -> Result<i32, EnvironmentPageError> {
    let page = match page.as_ref() {
        EnvironmentPage::Elevation(page) => page,
        _ => return Err(EnvironmentPageError::Corrupt),
    };
    let index = page_index(page.width, page.height, source_x, source_y)?;
    Ok(page.geographic_height_centimeters[index])
}

pub(super) fn page_index(
    width: u8,
    height: u8,
    source_x: u16,
    source_y: u16,
) -> Result<usize, EnvironmentPageError> {
    let local_x = usize::from(source_x % u16::from(ENVIRONMENT_PAGE_SAMPLES));
    let local_y = usize::from(source_y % u16::from(ENVIRONMENT_PAGE_SAMPLES));
    (local_x < usize::from(width) && local_y < usize::from(height))
        .then_some(local_y * usize::from(width) + local_x)
        .ok_or(EnvironmentPageError::Corrupt)
}
