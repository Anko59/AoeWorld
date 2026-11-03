use super::*;
use crate::BuildingKind;
use aoe_core::{TileCoord, TileRect};
use aoe_map::{ResourceKind, WaterKind};

impl GameWorld {
    pub(super) fn vassal_docks(
        &mut self,
        s: &mut Snapshot,
        budget: &mut Budget,
        work: &mut VassalWork,
        changed: &mut Vec<EntityId>,
    ) {
        if !s.complete || s.home.navy.fish.is_empty() {
            return;
        }
        // The census already restricts buildings to this owned territory.
        // Reuse a paid foundation too; never buy a second Dock while it builds.
        if let Some(dock) = s
            .buildings
            .iter()
            .find(|b| b.kind == BuildingKind::Dock)
            .cloned()
        {
            if !dock.is_complete() {
                let attended = s.units.iter().any(|u| {
                    self.economy
                        .builders
                        .get(&u.id)
                        .is_some_and(|task| task.building == dock.id)
                });
                if !attended
                    && let Some(worker) = self.vassal_dock_builder(s)
                    && budget.order(work)
                    && self.issue_construct(s.player, &[worker], dock.id).is_ok()
                {
                    self.claim_vassal_worker(s.vassal, worker, ResourceKind::Food);
                    changed.push(worker);
                }
                return;
            }
            let queued: usize = s
                .buildings
                .iter()
                .flat_map(|b| &b.training_queue)
                .filter(|e| e.kind == UnitKind::FishingShip)
                .count();
            let Some(player) = self.player_economy(s.player) else {
                return;
            };
            if s.home.settings.training
                && self
                    .vassal_priority(s.player, s.home.settings.weights)
                    .is_some_and(|p| p.kinds()[0] == ResourceKind::Food)
                && s.boats.len() + queued < VASSAL_RULES.max_villagers
                && dock.training_queue.is_empty()
                && dock.research.is_none()
                && player.stockpile.wood >= 75
                && budget.order(work)
            {
                let _ = self.issue_train(s.player, dock.id, UnitKind::FishingShip);
            }
            return;
        }
        let Some(worker) = self.vassal_dock_builder(s) else {
            return;
        };
        if self
            .player_economy(s.player)
            .is_none_or(|p| p.stockpile.wood < BuildingKind::Dock.rules().cost.wood)
        {
            return;
        }
        // A known fish-to-Castle ray supplies a bounded shore anchor; ordinary
        // explored placement and the builder's route admission decide legality.
        let Some(near) = s
            .home
            .navy
            .fish
            .iter()
            .find_map(|n| self.vassal_shore(n.tile, s))
        else {
            return;
        };
        let start = s.home.navy.site_after;
        for step in 0..81 {
            if !budget.site(work) {
                break;
            }
            let candidate = (start + step) % 81;
            let next = (candidate + 1) % 81;
            s.home.navy.site_after = next;
            if let Some(home) = self.economy.vassal_homes.get_mut(&s.vassal) {
                home.navy.site_after = next;
            }
            let tile = TileCoord::new(
                near.x + (candidate % 9) as i32 - 4,
                near.y + (candidate / 9) as i32 - 4,
            );
            let rect = TileRect::from_xywh(tile.x, tile.y, 3, 3);
            if !s.home.covers(rect, 1)
                || self.validate_explored_footprint(s.player, rect).is_err()
                || self
                    .validate_building_placement(BuildingKind::Dock, tile)
                    .is_err()
            {
                continue;
            }
            if !budget.order(work) {
                break;
            }
            if let Ok(id) = self.issue_build(s.player, &[worker], BuildingKind::Dock, tile, false) {
                self.claim_vassal_worker(s.vassal, worker, ResourceKind::Food);
                changed.push(worker);
                if let Some(building) = self.building(id) {
                    s.buildings.push(building.clone());
                }
                break;
            }
        }
    }

    fn vassal_dock_builder(&self, s: &Snapshot) -> Option<EntityId> {
        s.units
            .iter()
            .filter_map(|u| self.unit(u.id))
            .find(|u| {
                !self.economy.vassals.manual.contains(&u.id)
                    && !self.economy.builders.contains_key(&u.id)
                    && (self.vassal_worker_idle(u.id, s.vassal)
                        || (self.vassal_controller(u.id) == Some(s.vassal) && u.carried.is_none()))
            })
            .map(|u| u.id)
    }

    fn vassal_shore(&self, fish: TileCoord, s: &Snapshot) -> Option<TileCoord> {
        let Terrain::Map { generator, .. } = &self.terrain else {
            return None;
        };
        let center = s.home.center.tile_floor();
        let (dx, dy) = (center.x - fish.x, center.y - fish.y);
        let steps = dx.abs().max(dy.abs()).min(2 * VASSAL_RULES.radius_tiles);
        for i in 1..=steps {
            let tile = TileCoord::new(fish.x + dx * i / steps, fish.y + dy * i / steps);
            if generator
                .tile_at(tile)
                .is_some_and(|t| t.water == WaterKind::None && t.passable)
            {
                return Some(tile);
            }
        }
        None
    }
}
