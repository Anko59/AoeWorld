//! Materialize the current ownership raster only when a browser needs it.
use super::{CHUNK_TILES, Walking, field, page};
use crate::{GameWorld, OfficeZone, OfficeZoneChunk};
use aoe_core::{PlayerId, TileCoord};
use std::collections::BTreeMap;
use std::sync::Arc;

impl Walking {
    pub(super) fn build_shape(&self, index: usize) -> Arc<[OfficeZoneChunk]> {
        let (office, _) = self.sources[index];
        let active = &self.fields[index].active;
        let bounds = active.field.terrain.bounds;
        let walking = field::tiles(bounds)
            .zip(self.owned(index))
            .filter_map(|(tile, owned)| owned.then_some(tile));
        // A whole patch can belong to the office even beyond the walk budget.
        let outlying = self
            .patch_owners
            .winning_tiles(office, &active.claims)
            .filter(|&tile| !bounds.contains(tile));
        let mut chunks = BTreeMap::new();
        for tile in walking.chain(outlying) {
            let (x, y) = page(tile, CHUNK_TILES);
            let rows = chunks.entry(TileCoord::new(x, y)).or_insert([0; 32]);
            rows[tile.y.rem_euclid(CHUNK_TILES) as usize] |= 1 << tile.x.rem_euclid(CHUNK_TILES);
        }
        chunks.into_iter().collect()
    }
}

impl GameWorld {
    /// The published walking ownership for this viewer's active offices. The
    /// temporary membership fallback has no geometry to show.
    pub fn office_zones(
        &self,
        viewer: PlayerId,
        vassal: bool,
    ) -> impl Iterator<Item = OfficeZone> + '_ {
        let cache = &self.economy.walking;
        let group = if vassal {
            &cache.vassals
        } else {
            &cache.stewards
        };
        group.shapes(viewer).filter(|(_, shape)| !shape.is_empty())
    }
}
