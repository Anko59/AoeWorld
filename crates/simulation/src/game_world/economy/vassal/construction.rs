use super::{VassalWork, rules::Budget, snapshot::Snapshot};
use crate::{BuildingKind, GameWorld};
use aoe_core::{EntityId, TileCoord, TileRect};

impl GameWorld {
    /// Fixed-size local rings, no map-wide site searches. Ordinary placement,
    /// fog, costs, path budgets and construction time remain authoritative.
    pub(super) fn vassal_build_near(
        &mut self,
        worker: EntityId,
        kind: BuildingKind,
        near: TileCoord,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
    ) -> bool {
        if !s.complete {
            return false;
        }
        let cost = kind.rules().cost;
        if self.player_economy(s.player).is_none_or(|p| {
            let a = p.stockpile;
            a.food < cost.food || a.wood < cost.wood || a.gold < cost.gold || a.stone < cost.stone
        }) {
            return false;
        }
        let side = kind.rules().side;
        // A pending camp already serves this work area; do not build duplicates.
        if kind != BuildingKind::Farm
            && kind != BuildingKind::House
            && s.buildings.iter().any(|b| {
                b.kind == kind && super::super::dropoff::ring_distance(b.footprint, near) <= 64
            })
        {
            return false;
        }
        let start = s.home.site_after;
        for step in 0..64 {
            let candidate = (start + step) % 64;
            let radius = (candidate / 8) as i32 + 1;
            let offsets = [
                (radius, 0),
                (0, radius),
                (-radius - side, 0),
                (0, -radius - side),
                (radius, radius),
                (-radius - side, radius),
                (radius, -radius - side),
                (-radius - side, -radius - side),
            ];
            let (dx, dy) = offsets[candidate % 8];
            if !budget.site(work) {
                return false;
            }
            s.home.site_after = (candidate + 1) % 64;
            if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                home.site_after = s.home.site_after;
            }
            let tile = TileCoord::new(near.x + dx, near.y + dy);
            let rect = TileRect::from_xywh(tile.x, tile.y, side, kind.rules().depth);
            if !s.home.covers(rect, 0)
                || self.validate_explored_footprint(s.player, rect).is_err()
                || self.validate_building_placement(kind, tile).is_err()
                // The AI never builds on top of its own people.
                || !self.query(rect).0.is_empty()
            {
                continue;
            }
            if !budget.order(work) {
                return false;
            }
            if let Ok(id) = self.issue_build(s.player, &[worker], kind, tile, false) {
                s.home.site_after = 0;
                if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                    home.site_after = 0;
                }
                if let Some(b) = self.building(id) {
                    s.buildings.push(b.clone());
                }
                return true;
            }
        }
        false
    }
    /// Houses must not wait for an idle villager: when the population cap
    /// blocks growth, the first managed worker without a load builds one.
    pub(super) fn vassal_house_for_busy(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        let Some(p) = self.player_economy(s.player) else {
            return;
        };
        if !s.complete
            || usize::from(p.population_used) + s.global_queued + 2 < usize::from(p.population_cap)
            || p.population_cap >= self.limits().population
            || s.buildings
                .iter()
                .any(|b| b.kind == BuildingKind::House && !b.is_complete())
        {
            return;
        }
        let Some((worker, at)) = self.economy.vassal_homes[&s.vassal]
            .workers
            .iter()
            .filter_map(|w| self.unit(*w))
            .find(|u| u.carried.is_none() && u.hp > 0 && !self.embarked(u.id))
            .map(|u| (u.id, u.position.tile_floor()))
        else {
            return;
        };
        if self.vassal_build_near(worker, BuildingKind::House, at, s, budget, work) {
            self.claim_vassal_worker(s.vassal, worker, aoe_map::ResourceKind::Wood);
            changed.push(worker);
        }
    }
}
