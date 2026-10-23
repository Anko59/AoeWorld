//! Which owned building a villager carrying a given resource returns to.
use super::{BUILDING_RULES, placement::distance_squared};
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
        self.dropoff_order(player, from, kind).first().copied()
    }

    /// Every completed building of `player` that accepts `kind`, nearest first
    /// (squared tile distance to its ring, ties by id). The one rule behind
    /// `nearest_dropoff` and a returning villager's choice and its fallbacks.
    /// The per-player, per-kind index replaces the scan over every building
    /// of every player; the candidate set and order are unchanged.
    pub(crate) fn dropoff_order(
        &self,
        player: PlayerId,
        from: TileCoord,
        kind: ResourceKind,
    ) -> Vec<EntityId> {
        let mut found = BUILDING_RULES
            .iter()
            .map(|rules| rules.kind)
            .filter(|candidate| candidate.accepts(kind))
            .filter_map(|candidate| self.economy.dropoffs.get(&(player, candidate)))
            .flatten()
            .filter_map(|&id| self.building(id).filter(|building| building.is_complete()))
            .map(|building| (ring_distance(building.footprint, from), building.id))
            .collect::<Vec<_>>();
        found.sort_unstable();
        found.into_iter().map(|(_, id)| id).collect()
    }
}
