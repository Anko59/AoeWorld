//! Keep an already usable central clearing on low-relief landscapes. Survey
//! the same bounded whole-map lattice using elevation alone before accepting;
//! mountainous maps still use the connected lowland ranking in `robust`.
use super::*;

pub(super) fn certified_center(
    cache: &mut StartPassabilityCache<'_>,
    center: TileCoord,
) -> Result<bool, EnvironmentPageError> {
    if !clear_starting_area(cache, center)? {
        return Ok(false);
    }
    let Terrain::Map { generator, .. } = cache.terrain else {
        return Ok(false);
    };
    let stride = ((cache.config.width_tiles.max(cache.config.height_tiles) + 63) / 64).max(1);
    let Some(height) = generator.geographic_height_at_with_cancel(center, cache.cancelled)? else {
        return Ok(false);
    };
    let (mut low, mut high) = (height, height);
    for y in (0..cache.config.height_tiles).step_by(stride as usize) {
        for x in (0..cache.config.width_tiles).step_by(stride as usize) {
            let Some(height) = generator
                .geographic_height_at_with_cancel(TileCoord::new(x, y), cache.cancelled)?
            else {
                return Ok(false);
            };
            low = low.min(height);
            high = high.max(height);
            if high.saturating_sub(low) > 50_000 {
                return Ok(false);
            }
        }
    }
    // A beach is established by nearby water, not sand alone: an inland
    // desert clearing remains eligible. Resource blockers still use the full
    // passability certificate below.
    for y in center.y - 4..=center.y + 4 {
        for x in center.x - 4..=center.x + 4 {
            if !generator
                .tile_at_with_cancel(TileCoord::new(x, y), cache.cancelled)?
                .is_some_and(|tile| tile.water == aoe_map::WaterKind::None)
            {
                return Ok(false);
            }
        }
    }
    valid_start(cache, center)
}
