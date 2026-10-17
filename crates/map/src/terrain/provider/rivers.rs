//! Direct line queries; no evidence-cell filling or invented cell connectors.
use super::helpers::load_page;
use crate::{EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, PageLayer};
use aoe_core::TileCoord;
use std::sync::Arc;
#[path = "rivers/index.rs"]
mod index;
pub(in crate::terrain) use index::Cache;

/// What a river query measures.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    /// The warped, noise-varied channel that becomes water tiles.
    Physical,
    /// The nominal centreline footprint of channels at least this wide.
    Footprint { min_width_units: u16 },
}

pub(super) fn sample(
    generator: &super::MapChunkGenerator,
    tile: TileCoord,
    radius: i32,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<bool>, EnvironmentPageError> {
    query(generator, tile, radius, Mode::Physical, cancelled)
}

fn query(
    generator: &super::MapChunkGenerator,
    tile: TileCoord,
    radius: i32,
    mode: Mode,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<bool>, EnvironmentPageError> {
    if cancelled() {
        return Err(EnvironmentPageError::Cancelled);
    }
    if tile.x < 0
        || tile.y < 0
        || tile.x >= generator.width_tiles
        || tile.y >= generator.width_tiles
    {
        return Ok(None);
    }
    let Some(index) = generator
        .provider_environment
        .as_ref()
        .and_then(|e| e.hydrology_evidence.as_ref())
    else {
        return Ok(None);
    };
    let width = i64::from(generator.width_tiles);
    let axis = i64::from(index.samples_per_axis);
    let cell = |tile: i64| (tile.clamp(0, width - 1) * axis / width).min(axis - 1) as u16 / 64;
    let radius = i64::from(radius.max(0));
    // Read neighboring pages when the channel's half width crosses a page
    // edge. The expanded query still samples the original physical width.
    let physical = mode == Mode::Physical;
    let query_radius = if physical { radius * 2 } else { radius };
    let discovery = query_radius + 10;
    let point = [i64::from(tile.x) * 256 + 128, i64::from(tile.y) * 256 + 128];
    let variation = |domain, lattice| {
        crate::terrain::shoreline::variation(generator.shoreline_key(), domain, tile, lattice)
    };
    let mut shaped = None;
    let mut best = None;
    for y in cell(i64::from(tile.y) - discovery)..=cell(i64::from(tile.y) + discovery) {
        for x in cell(i64::from(tile.x) - discovery)..=cell(i64::from(tile.x) + discovery) {
            let key = EnvironmentPageKey {
                layer: PageLayer::HydrologyEvidence,
                level: 0,
                x,
                y,
            };
            if cancelled() {
                return Err(EnvironmentPageError::Cancelled);
            }
            let cached = generator
                .river_queries
                .lock()
                .map_err(|_| EnvironmentPageError::Unavailable)?
                .get(key);
            let indexed = if let Some(index) = cached {
                index
            } else {
                let page = load_page(generator, key, cancelled)?;
                let EnvironmentPage::HydrologyEvidence(page) = page.as_ref() else {
                    return Err(EnvironmentPageError::Corrupt);
                };
                let extent = width * 64 * 256 / axis;
                let origin =
                    [i64::from(x), i64::from(y)].map(|cell| cell * width * 64 * 256 / axis);
                let index = Arc::new(index::Index::new(&page.river_segments, origin, extent));
                generator
                    .river_queries
                    .lock()
                    .map_err(|_| EnvironmentPageError::Unavailable)?
                    .insert(key, index.clone());
                index
            };
            let hit = if physical {
                // The conservative bins include the corner-cut bound; add
                // both warp components before proving that a page cannot hit.
                if !indexed.possible(point, query_radius * 256 + 2 * 256) {
                    continue;
                }
                let (point, width_noise) = *shaped.get_or_insert_with(|| {
                    // At most 1.5 tiles per axis on a smooth world lattice.
                    // One inverse warp moves every intersecting reach together.
                    let point = [0, 1]
                        .map(|axis| point[axis] + variation(20 + axis as u64, 32) * 384 / 1024);
                    (point, variation(22, 9) * 64 / 1024)
                });
                indexed.physical(point, query_radius * 256, width_noise)
            } else {
                let Mode::Footprint { min_width_units } = mode else {
                    unreachable!("physical queries returned above");
                };
                indexed.sample(point, radius * 256, min_width_units)
            };
            if let Some(hit) = hit
                && best.is_none_or(|(distance, _)| hit.0 < distance)
            {
                best = Some(hit);
            }
        }
    }
    Ok(best.map(|(_, ford)| ford))
}

impl super::MapChunkGenerator {
    /// Conservative footprint query for strategic rasterization. It reads
    /// persisted centreline geometry, never expands the physical channel.
    /// Only channels at least `min_width_units` wide count, so a minimap can
    /// show rivers by importance.
    pub fn river_within_with_cancel(
        &self,
        tile: TileCoord,
        radius: i32,
        min_width_units: u16,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        let mode = Mode::Footprint { min_width_units };
        query(self, tile, radius, mode, cancelled).map(|hit| hit.is_some())
    }
}
