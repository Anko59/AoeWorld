//! Which owned building a villager carrying a given resource returns to.
use super::placement::distance_squared;
use crate::GameWorld;
use aoe_core::{EntityId, PlayerId, TileCoord, TileRect};
use aoe_map::ResourceKind;

/// Squared distance from `tile` to the closest tile of the ring around `rect`.
fn ring_distance(rect: TileRect, tile: TileCoord) -> u64 {
    let ring = TileCoord::new(
        tile.x.clamp(rect.min.x - 1, rect.max.x),
        tile.y.clamp(rect.min.y - 1, rect.max.y),
    );
    distance_squared(tile, ring)
}

impl GameWorld {
    /// The nearest completed building of `player` that accepts `kind`:
    /// the Town Center takes everything, a Lumber Camp wood, a Mining Camp
    /// gold and stone, a Mill food. Ties break toward the lower id.
    pub fn nearest_dropoff(
        &self,
        player: PlayerId,
        from: TileCoord,
        kind: ResourceKind,
    ) -> Option<EntityId> {
        self.buildings()
            .filter(|b| b.player == player && b.is_complete() && b.kind.accepts(kind))
            .min_by_key(|b| (ring_distance(b.footprint, from), b.id))
            .map(|b| b.id)
    }
}
