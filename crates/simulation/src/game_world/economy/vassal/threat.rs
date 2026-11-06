//! Attacks by structures and by units just outside the territory still raise
//! an alert. Read only recent hit sources on the protected units and plan sites.
use super::*;
use crate::GameUnit;
impl GameWorld {
    pub(super) fn vassal_recent_attacker(
        &self,
        id: EntityId,
        units: &[GameUnit],
    ) -> Option<(EntityId, WorldPosition)> {
        let d = &self.economy.vassals.domains[&id];
        // The cached plan: re-planning here would scan the whole domain.
        let structures = std::iter::once(d.castle)
            .chain(d.plan.iter().filter_map(|site| self.building_at(site.tile)));
        let hits = units
            .iter()
            .filter(|u| u.player == d.player && u.hp > 0 && inside(d.center, u.position))
            .filter_map(|u| u.last_hit)
            .chain(structures.filter_map(|id| {
                self.building(id)
                    .filter(|b| b.player == d.player)
                    .and_then(|b| b.last_hit)
            }));
        hits.filter(|hit| {
            self.tick.0.saturating_sub(hit.tick.0) <= u64::from(self.config.tick_hz) * 10 + 20
        })
        .filter_map(|hit| {
            if let Some(u) = self.unit(hit.attacker).filter(|u| {
                u.player != d.player && u.hp > 0 && self.unit_visible_to(d.player, u.id)
            }) {
                return Some((u.id, u.position));
            }
            let b = self.building(hit.attacker).filter(|b| {
                b.player != d.player && b.hp > 0 && self.building_visible_to(d.player, b.footprint)
            })?;
            // An ordinary march needs a walkable destination, not the
            // blocked centre of the attacking building.
            let tile = super::super::placement::perimeter(b.footprint)
                .into_iter()
                .filter(|tile| self.tile_passable(*tile))
                .min_by_key(|tile| {
                    (
                        super::super::placement::distance_squared(*tile, d.center),
                        tile.y,
                        tile.x,
                    )
                })?;
            Some((b.id, WorldPosition::from_tile_center(tile).ok()?))
        })
        .min_by_key(|(attacker, _)| *attacker)
    }
}
